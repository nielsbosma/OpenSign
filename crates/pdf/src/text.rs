//! Where text and lines are drawn on a page, for text tags and field detection. Reads the page's
//! own content stream only: text inside form XObjects is not seen. Uploads are hostile, so every
//! loop is bounded and anything unreadable is skipped rather than trusted.

use std::collections::HashMap;

use lopdf::content::{Content, Operation};
use lopdf::{Dictionary, Document, Object, ObjectId};

use crate::geometry::{PageBox, page_box};
use crate::{PdfError, Result};

const MAX_OPERATIONS: usize = 1_000_000;
const MAX_GLYPHS: usize = 300_000;
const MAX_SEGMENTS: usize = 100_000;
const MAX_STACK: usize = 64;
const MAX_CMAP_BYTES: usize = 2 * 1024 * 1024;
const MAX_CMAP_ENTRIES: usize = 70_000;
pub(crate) const MAX_CONTENT_BYTES: usize = 64 * 1024 * 1024;

type Matrix = [f64; 6];
const IDENTITY: Matrix = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

fn mul(a: &Matrix, b: &Matrix) -> Matrix {
    [
        a[0] * b[0] + a[1] * b[2],
        a[0] * b[1] + a[1] * b[3],
        a[2] * b[0] + a[3] * b[2],
        a[2] * b[1] + a[3] * b[3],
        a[4] * b[0] + a[5] * b[2] + b[4],
        a[4] * b[1] + a[5] * b[3] + b[5],
    ]
}

fn apply(m: &Matrix, x: f64, y: f64) -> (f64, f64) {
    (x * m[0] + y * m[2] + m[4], x * m[1] + y * m[3] + m[5])
}

fn translate(x: f64, y: f64) -> Matrix {
    [1.0, 0.0, 0.0, 1.0, x, y]
}

fn number(object: &Object) -> Option<f64> {
    match object {
        Object::Integer(i) => Some(*i as f64),
        Object::Real(r) => Some(*r as f64),
        _ => None,
    }
    .filter(|n| n.is_finite())
}

fn numbers(operands: &[Object]) -> Vec<f64> {
    operands.iter().filter_map(number).collect()
}

/// One character code drawn on the page, in displayed page points (origin top-left, y down).
#[derive(Debug, Clone)]
pub(crate) struct Glyph {
    pub text: String,
    /// Where it sits in the content: operation, element of a TJ array, and byte range.
    pub op: usize,
    pub part: usize,
    pub bytes: (usize, usize),
    /// Baseline start and end, and the vector from the baseline to the top of the em box.
    pub origin: (f64, f64),
    pub end: (f64, f64),
    pub up: (f64, f64),
    /// The TJ adjustment that moves as far as this glyph did, so removing it leaves the rest in place.
    pub adjustment: Option<f64>,
}

impl Glyph {
    pub fn size(&self) -> f64 {
        self.up.0.hypot(self.up.1)
    }

    /// Runs left to right on the displayed page.
    pub fn is_horizontal(&self) -> bool {
        let (dx, dy) = (self.end.0 - self.origin.0, self.end.1 - self.origin.1);
        dx >= 0.0 && dy.abs() <= dx.abs() * 0.2 + 0.01 && self.up.1 < 0.0
    }

    /// Bounding box `[x0, y0, x1, y1]` of the em box, from a little below the baseline.
    pub fn bounds(&self) -> [f64; 4] {
        let (ux, uy) = self.up;
        let corners = [
            (self.origin.0 - ux * 0.22, self.origin.1 - uy * 0.22),
            (self.end.0 - ux * 0.22, self.end.1 - uy * 0.22),
            (self.origin.0 + ux * 0.78, self.origin.1 + uy * 0.78),
            (self.end.0 + ux * 0.78, self.end.1 + uy * 0.78),
        ];
        let xs = corners.iter().map(|c| c.0);
        let ys = corners.iter().map(|c| c.1);
        [
            xs.clone().fold(f64::INFINITY, f64::min),
            ys.clone().fold(f64::INFINITY, f64::min),
            xs.fold(f64::NEG_INFINITY, f64::max),
            ys.fold(f64::NEG_INFINITY, f64::max),
        ]
    }
}

/// A straight line segment or an outlined rectangle drawn on the page, in displayed points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Shape {
    Line { from: (f64, f64), to: (f64, f64) },
    Rect { x0: f64, y0: f64, x1: f64, y1: f64, filled: bool },
}

#[derive(Debug, Default)]
pub(crate) struct PageLayout {
    pub width: f64,
    pub height: f64,
    pub glyphs: Vec<Glyph>,
    pub shapes: Vec<Shape>,
}

struct Font {
    two_byte: bool,
    widths: HashMap<u32, f64>,
    default_width: f64,
    /// Glyph space to text space, 1/1000 except for Type 3 fonts.
    scale: f64,
    unicode: HashMap<u32, String>,
    differences: HashMap<u32, char>,
    symbolic: Symbolic,
}

#[derive(Clone, Copy, PartialEq)]
enum Symbolic {
    No,
    Dingbats,
    Wingdings,
}

fn resolve<'a>(doc: &'a Document, object: &'a Object) -> Option<&'a Object> {
    doc.dereference(object).ok().map(|(_, o)| o)
}

fn dict<'a>(doc: &'a Document, object: &'a Object) -> Option<&'a Dictionary> {
    match resolve(doc, object)? {
        Object::Dictionary(d) => Some(d),
        Object::Stream(s) => Some(&s.dict),
        _ => None,
    }
}

fn name<'a>(doc: &'a Document, d: &'a Dictionary, key: &[u8]) -> Option<&'a [u8]> {
    resolve(doc, d.get(key).ok()?)?.as_name().ok()
}

fn hex_value(bytes: &[u8]) -> Option<u32> {
    if bytes.is_empty() || bytes.len() > 8 {
        return None;
    }
    bytes.iter().try_fold(0u32, |acc, b| Some(acc << 8 | *b as u32))
}

fn utf16(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes.chunks(2).map(|c| u16::from_be_bytes([c[0], *c.get(1).unwrap_or(&0)])).collect();
    String::from_utf16_lossy(&units)
}

/// The bfchar and bfrange entries of a ToUnicode CMap, and whether its codes are one byte long.
fn parse_cmap(data: &[u8]) -> (HashMap<u32, String>, Option<bool>) {
    let mut map = HashMap::new();
    let mut one_byte = None;
    let mut tokens: Vec<Token> = Vec::new();
    let mut i = 0;
    while i < data.len() {
        match data[i] {
            b'<' => {
                let end = data[i + 1..].iter().position(|b| *b == b'>').map_or(data.len(), |p| i + 1 + p);
                let digits: Vec<u8> = data[i + 1..end].iter().copied().filter(u8::is_ascii_hexdigit).collect();
                let mut bytes = Vec::with_capacity(digits.len() / 2 + 1);
                for pair in digits.chunks(2) {
                    let hex = |d: u8| (d as char).to_digit(16).unwrap_or(0) as u8;
                    bytes.push(hex(pair[0]) << 4 | pair.get(1).map_or(0, |d| hex(*d)));
                }
                tokens.push(Token::Hex(bytes));
                i = end + 1;
            }
            b'[' => {
                tokens.push(Token::Open);
                i += 1;
            }
            b']' => {
                tokens.push(Token::Close);
                i += 1;
            }
            b'%' => {
                while i < data.len() && data[i] != b'\n' && data[i] != b'\r' {
                    i += 1;
                }
            }
            b if b.is_ascii_alphabetic() => {
                let start = i;
                while i < data.len() && data[i].is_ascii_alphanumeric() {
                    i += 1;
                }
                tokens.push(Token::Word(data[start..i].to_vec()));
            }
            _ => i += 1,
        }
    }
    let mut section = Section::None;
    let mut t = 0;
    while t < tokens.len() && map.len() < MAX_CMAP_ENTRIES {
        match &tokens[t] {
            Token::Word(w) => {
                section = match w.as_slice() {
                    b"begincodespacerange" => Section::Codespace,
                    b"beginbfchar" => Section::Char,
                    b"beginbfrange" => Section::Range,
                    _ => Section::None,
                };
                t += 1;
            }
            Token::Hex(src) => match section {
                Section::Codespace => {
                    one_byte.get_or_insert(src.len() == 1);
                    t += 2;
                }
                Section::Char => {
                    if let (Some(code), Some(Token::Hex(dst))) = (hex_value(src), tokens.get(t + 1)) {
                        map.insert(code, utf16(dst));
                    }
                    t += 2;
                }
                Section::Range => {
                    let (Some(lo), Some(Token::Hex(hi))) = (hex_value(src), tokens.get(t + 1)) else {
                        t += 1;
                        continue;
                    };
                    let hi = hex_value(hi).unwrap_or(lo).min(lo.saturating_add(MAX_CMAP_ENTRIES as u32));
                    match tokens.get(t + 2) {
                        Some(Token::Hex(dst)) if dst.len() >= 2 => {
                            let mut units: Vec<u16> = dst.chunks(2).map(|c| u16::from_be_bytes([c[0], *c.get(1).unwrap_or(&0)])).collect();
                            for code in lo..=hi {
                                if map.len() >= MAX_CMAP_ENTRIES {
                                    break;
                                }
                                map.insert(code, String::from_utf16_lossy(&units));
                                if let Some(last) = units.last_mut() {
                                    *last = last.wrapping_add(1);
                                }
                            }
                            t += 3;
                        }
                        Some(Token::Open) => {
                            let mut code = lo;
                            t += 3;
                            while let Some(Token::Hex(dst)) = tokens.get(t) {
                                if code <= hi {
                                    map.insert(code, utf16(dst));
                                }
                                code = code.saturating_add(1);
                                t += 1;
                            }
                            t += 1;
                        }
                        _ => t += 3,
                    }
                }
                Section::None => t += 1,
            },
            _ => t += 1,
        }
    }
    (map, one_byte)
}

enum Token {
    Hex(Vec<u8>),
    Word(Vec<u8>),
    Open,
    Close,
}

#[derive(Clone, Copy)]
enum Section {
    None,
    Codespace,
    Char,
    Range,
}

/// Glyph names that matter for tags and labels, beyond single letters and `uniXXXX`.
fn glyph_char(name: &[u8]) -> Option<char> {
    let named = match name {
        b"space" => ' ',
        b"braceleft" => '{',
        b"braceright" => '}',
        b"semicolon" => ';',
        b"equal" => '=',
        b"underscore" => '_',
        b"bar" => '|',
        b"comma" => ',',
        b"period" => '.',
        b"colon" => ':',
        b"hyphen" | b"minus" => '-',
        b"slash" => '/',
        b"zero" => '0',
        b"one" => '1',
        b"two" => '2',
        b"three" => '3',
        b"four" => '4',
        b"five" => '5',
        b"six" => '6',
        b"seven" => '7',
        b"eight" => '8',
        b"nine" => '9',
        b"aring" => 'å',
        b"adieresis" => 'ä',
        b"odieresis" => 'ö',
        b"Aring" => 'Å',
        b"Adieresis" => 'Ä',
        b"Odieresis" => 'Ö',
        _ if name.len() == 1 && name[0].is_ascii_alphabetic() => name[0] as char,
        _ if name.len() == 7 && name.starts_with(b"uni") => {
            return std::str::from_utf8(&name[3..]).ok().and_then(|h| u32::from_str_radix(h, 16).ok()).and_then(char::from_u32);
        }
        _ => return None,
    };
    Some(named)
}

const WINANSI_HIGH: [char; 32] = [
    '€', '\u{fffd}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{fffd}', 'Ž', '\u{fffd}', '\u{fffd}', '‘', '’', '“', '”',
    '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{fffd}', 'ž', 'Ÿ',
];

impl Font {
    fn load(doc: &Document, font: &Dictionary) -> Font {
        let subtype = name(doc, font, b"Subtype").unwrap_or_default();
        let base = name(doc, font, b"BaseFont").unwrap_or_default();
        let contains = |needle: &[u8]| base.windows(needle.len()).any(|w| w.eq_ignore_ascii_case(needle));
        let symbolic = if contains(b"Dingbats") {
            Symbolic::Dingbats
        } else if contains(b"Wingdings") {
            Symbolic::Wingdings
        } else {
            Symbolic::No
        };
        let (unicode, one_byte) = font
            .get(b"ToUnicode")
            .ok()
            .and_then(|o| resolve(doc, o))
            .and_then(|o| o.as_stream().ok())
            .and_then(|s| s.decompressed_content_with_limit(MAX_CMAP_BYTES).ok().or_else(|| Some(s.content.clone())))
            .filter(|data| data.len() <= MAX_CMAP_BYTES)
            .map(|data| parse_cmap(&data))
            .unwrap_or_default();
        let mut widths = HashMap::new();
        let mut default_width = 500.0;
        let two_byte = subtype == b"Type0" && one_byte != Some(true);
        if subtype == b"Type0" {
            let descendant = font
                .get(b"DescendantFonts")
                .ok()
                .and_then(|o| resolve(doc, o))
                .and_then(|o| o.as_array().ok())
                .and_then(|a| a.first())
                .and_then(|o| dict(doc, o));
            if let Some(cid) = descendant {
                default_width = cid.get(b"DW").ok().and_then(|o| resolve(doc, o)).and_then(number).unwrap_or(1000.0);
                if let Some(w) = cid.get(b"W").ok().and_then(|o| resolve(doc, o)).and_then(|o| o.as_array().ok()) {
                    let mut i = 0;
                    while i < w.len() && widths.len() < MAX_CMAP_ENTRIES {
                        let first = resolve(doc, &w[i]).and_then(number).unwrap_or(0.0).max(0.0) as u32;
                        match w.get(i + 1).and_then(|o| resolve(doc, o)) {
                            Some(Object::Array(list)) => {
                                for (k, item) in list.iter().enumerate().take(MAX_CMAP_ENTRIES) {
                                    if let Some(width) = resolve(doc, item).and_then(number) {
                                        widths.insert(first.saturating_add(k as u32), width);
                                    }
                                }
                                i += 2;
                            }
                            Some(last) => {
                                let last = number(last).unwrap_or(0.0).max(0.0) as u32;
                                let width = w.get(i + 2).and_then(|o| resolve(doc, o)).and_then(number).unwrap_or(default_width);
                                for code in first..=last.min(first.saturating_add(MAX_CMAP_ENTRIES as u32)) {
                                    widths.insert(code, width);
                                }
                                i += 3;
                            }
                            None => break,
                        }
                    }
                }
            }
        } else {
            let first = font.get(b"FirstChar").ok().and_then(|o| resolve(doc, o)).and_then(number).unwrap_or(0.0).max(0.0) as u32;
            if let Some(list) = font.get(b"Widths").ok().and_then(|o| resolve(doc, o)).and_then(|o| o.as_array().ok()) {
                for (k, item) in list.iter().enumerate().take(256) {
                    if let Some(width) = resolve(doc, item).and_then(number) {
                        widths.insert(first + k as u32, width);
                    }
                }
            }
            if contains(b"Courier") {
                default_width = 600.0;
            }
        }
        let scale = if subtype == b"Type3" {
            font.get(b"FontMatrix")
                .ok()
                .and_then(|o| resolve(doc, o))
                .and_then(|o| o.as_array().ok())
                .and_then(|a| a.first())
                .and_then(number)
                .filter(|s| *s != 0.0 && s.abs() < 1.0)
                .unwrap_or(0.001)
        } else {
            0.001
        };
        let mut differences = HashMap::new();
        if let Some(encoding) = font.get(b"Encoding").ok().and_then(|o| dict(doc, o))
            && let Some(list) = encoding.get(b"Differences").ok().and_then(|o| resolve(doc, o)).and_then(|o| o.as_array().ok())
        {
            let mut code = 0u32;
            for item in list.iter().take(1024) {
                match item {
                    Object::Integer(i) => code = (*i).clamp(0, 255) as u32,
                    Object::Name(n) => {
                        if let Some(c) = glyph_char(n) {
                            differences.insert(code, c);
                        }
                        code += 1;
                    }
                    _ => {}
                }
            }
        }
        Font { two_byte, widths, default_width, scale, unicode, differences, symbolic }
    }

    fn width(&self, code: u32) -> f64 {
        self.widths.get(&code).copied().unwrap_or(self.default_width) * self.scale
    }

    fn text(&self, code: u32) -> String {
        if let Some(s) = self.unicode.get(&code) {
            return s.clone();
        }
        if self.two_byte {
            return String::new();
        }
        if let Some(c) = self.differences.get(&code) {
            return c.to_string();
        }
        let box_glyph = match (self.symbolic, code) {
            (Symbolic::Dingbats, 0x6F..=0x72) | (Symbolic::Wingdings, 0x6F | 0x71 | 0xA8) => Some('☐'),
            (Symbolic::No, _) => None,
            _ => return String::new(),
        };
        if let Some(c) = box_glyph {
            return c.to_string();
        }
        match code {
            0x20..=0x7E | 0xA0..=0xFF => char::from_u32(code).map(String::from).unwrap_or_default(),
            0x80..=0x9F => WINANSI_HIGH[(code - 0x80) as usize].to_string(),
            _ => String::new(),
        }
    }

    fn codes(&self, bytes: &[u8]) -> Vec<(u32, usize, usize)> {
        if self.two_byte {
            bytes.chunks(2).enumerate().map(|(i, c)| (hex_value(c).unwrap_or(0), i * 2, i * 2 + c.len())).collect()
        } else {
            bytes.iter().enumerate().map(|(i, b)| (*b as u32, i, i + 1)).collect()
        }
    }
}

#[derive(Clone)]
struct State {
    ctm: Matrix,
    char_spacing: f64,
    word_spacing: f64,
    scale: f64,
    leading: f64,
    font: Vec<u8>,
    size: f64,
    rise: f64,
}

struct Walker<'a> {
    doc: &'a Document,
    page: PageBox,
    fonts: HashMap<Vec<u8>, Font>,
    font_dicts: std::collections::BTreeMap<Vec<u8>, &'a Dictionary>,
    state: State,
    stack: Vec<State>,
    tm: Matrix,
    tlm: Matrix,
    path: Vec<Shape>,
    start: (f64, f64),
    current: (f64, f64),
    out: PageLayout,
}

impl<'a> Walker<'a> {
    fn display(&self, m: &Matrix, x: f64, y: f64) -> (f64, f64) {
        let (px, py) = apply(m, x, y);
        self.page.to_display(px, py)
    }

    fn show(&mut self, op: usize, part: usize, bytes: &[u8]) {
        if !self.fonts.contains_key(&self.state.font) {
            let font = self.font_dicts.get(&self.state.font).map(|d| Font::load(self.doc, d));
            let Some(font) = font else { return };
            self.fonts.insert(self.state.font.clone(), font);
        }
        let Some(font) = self.fonts.get(&self.state.font) else { return };
        let s = &self.state;
        let trm_base = [s.size * s.scale, 0.0, 0.0, s.size, 0.0, s.rise];
        let mut glyphs = Vec::new();
        let mut tm = self.tm;
        for (code, from, to) in font.codes(bytes) {
            let w0 = font.width(code);
            let spacing = s.char_spacing + if !font.two_byte && code == 32 { s.word_spacing } else { 0.0 };
            let trm = mul(&mul(&trm_base, &tm), &s.ctm);
            let (ox, oy) = apply(&trm, 0.0, 0.0);
            let (ex, ey) = apply(&trm, w0, 0.0);
            let (tx, ty) = apply(&trm, 0.0, 1.0);
            let origin = self.page.to_display(ox, oy);
            let end = self.page.to_display(ex, ey);
            let top = self.page.to_display(tx, ty);
            glyphs.push(Glyph {
                text: font.text(code),
                op,
                part,
                bytes: (from, to),
                origin,
                end,
                up: (top.0 - origin.0, top.1 - origin.1),
                adjustment: (s.size.abs() > 1e-6).then(|| -(w0 * 1000.0 + spacing * 1000.0 / s.size)),
            });
            tm = mul(&translate((w0 * s.size + spacing) * s.scale, 0.0), &tm);
        }
        self.tm = tm;
        let room = MAX_GLYPHS.saturating_sub(self.out.glyphs.len());
        self.out.glyphs.extend(glyphs.into_iter().take(room));
    }

    fn adjust(&mut self, amount: f64) {
        let tx = -amount / 1000.0 * self.state.size * self.state.scale;
        self.tm = mul(&translate(tx, 0.0), &self.tm);
    }

    fn next_line(&mut self, x: f64, y: f64) {
        self.tlm = mul(&translate(x, y), &self.tlm);
        self.tm = self.tlm;
    }

    fn point(&self, x: f64, y: f64) -> (f64, f64) {
        self.display(&self.state.ctm, x, y)
    }

    fn paint(&mut self, filled: bool, stroked: bool) {
        let path = std::mem::take(&mut self.path);
        if !(filled || stroked) || self.out.shapes.len() >= MAX_SEGMENTS {
            return;
        }
        for shape in path {
            let shape = match shape {
                Shape::Rect { x0, y0, x1, y1, .. } => Shape::Rect { x0, y0, x1, y1, filled },
                line if stroked => line,
                _ => continue,
            };
            self.out.shapes.push(shape);
        }
    }

    fn operation(&mut self, index: usize, op: &Operation) {
        let v = numbers(&op.operands);
        match op.operator.as_str() {
            "q" if self.stack.len() < MAX_STACK => self.stack.push(self.state.clone()),
            "Q" => {
                if let Some(s) = self.stack.pop() {
                    self.state = s;
                }
            }
            "cm" if v.len() == 6 => self.state.ctm = mul(&[v[0], v[1], v[2], v[3], v[4], v[5]], &self.state.ctm),
            "BT" => {
                self.tm = IDENTITY;
                self.tlm = IDENTITY;
            }
            "Tm" if v.len() == 6 => {
                self.tm = [v[0], v[1], v[2], v[3], v[4], v[5]];
                self.tlm = self.tm;
            }
            "Td" if v.len() == 2 => self.next_line(v[0], v[1]),
            "TD" if v.len() == 2 => {
                self.state.leading = -v[1];
                self.next_line(v[0], v[1]);
            }
            "T*" => self.next_line(0.0, -self.state.leading),
            "TL" if v.len() == 1 => self.state.leading = v[0],
            "Tc" if v.len() == 1 => self.state.char_spacing = v[0],
            "Tw" if v.len() == 1 => self.state.word_spacing = v[0],
            "Tz" if v.len() == 1 => self.state.scale = v[0] / 100.0,
            "Ts" if v.len() == 1 => self.state.rise = v[0],
            "Tf" if op.operands.len() == 2 => {
                if let Ok(name) = op.operands[0].as_name() {
                    self.state.font = name.to_vec();
                }
                self.state.size = number(&op.operands[1]).unwrap_or(self.state.size);
            }
            "Tj" => {
                if let Some(Object::String(bytes, _)) = op.operands.first() {
                    self.show(index, 0, bytes);
                }
            }
            "'" | "\"" => {
                if op.operator == "\"" && v.len() >= 2 {
                    self.state.word_spacing = v[0];
                    self.state.char_spacing = v[1];
                }
                self.next_line(0.0, -self.state.leading);
                if let Some(Object::String(bytes, _)) = op.operands.last() {
                    self.show(index, 0, bytes);
                }
            }
            "TJ" => {
                if let Some(Object::Array(items)) = op.operands.first() {
                    for (part, item) in items.iter().enumerate() {
                        match item {
                            Object::String(bytes, _) => self.show(index, part, bytes),
                            other => {
                                if let Some(n) = number(other) {
                                    self.adjust(n);
                                }
                            }
                        }
                    }
                }
            }
            "m" if v.len() == 2 => {
                self.start = self.point(v[0], v[1]);
                self.current = self.start;
            }
            "l" if v.len() == 2 => {
                let to = self.point(v[0], v[1]);
                if self.path.len() < MAX_SEGMENTS {
                    self.path.push(Shape::Line { from: self.current, to });
                }
                self.current = to;
            }
            "c" | "v" | "y" if v.len() >= 4 => self.current = self.point(v[v.len() - 2], v[v.len() - 1]),
            "h" => {
                if self.path.len() < MAX_SEGMENTS && self.current != self.start {
                    self.path.push(Shape::Line { from: self.current, to: self.start });
                }
                self.current = self.start;
            }
            "re" if v.len() == 4 => {
                let corners = [
                    self.point(v[0], v[1]),
                    self.point(v[0] + v[2], v[1]),
                    self.point(v[0], v[1] + v[3]),
                    self.point(v[0] + v[2], v[1] + v[3]),
                ];
                let xs = corners.iter().map(|c| c.0);
                let ys = corners.iter().map(|c| c.1);
                if self.path.len() < MAX_SEGMENTS {
                    self.path.push(Shape::Rect {
                        x0: xs.clone().fold(f64::INFINITY, f64::min),
                        y0: ys.clone().fold(f64::INFINITY, f64::min),
                        x1: xs.fold(f64::NEG_INFINITY, f64::max),
                        y1: ys.fold(f64::NEG_INFINITY, f64::max),
                        filled: false,
                    });
                }
                self.start = corners[0];
                self.current = corners[0];
            }
            "S" | "s" => self.paint(false, true),
            "f" | "F" | "f*" => self.paint(true, false),
            "B" | "B*" | "b" | "b*" => self.paint(true, true),
            "n" => self.path.clear(),
            _ => {}
        }
    }
}

/// Decodes a page's content; `None` when it cannot be read or is too large to walk.
pub(crate) fn page_content(doc: &Document, page: ObjectId) -> Option<Content> {
    let data = doc.get_page_content_with_limit(page, MAX_CONTENT_BYTES).ok()?;
    let content = Content::decode(&data).ok()?;
    (content.operations.len() <= MAX_OPERATIONS).then_some(content)
}

pub(crate) fn walk(doc: &Document, page: ObjectId, content: &Content) -> Result<PageLayout> {
    let page_box = page_box(doc, page)?;
    let (width, height) = page_box.displayed();
    let font_dicts = doc.get_page_fonts(page).unwrap_or_default();
    let mut walker = Walker {
        doc,
        page: page_box,
        fonts: HashMap::new(),
        font_dicts,
        state: State {
            ctm: IDENTITY,
            char_spacing: 0.0,
            word_spacing: 0.0,
            scale: 1.0,
            leading: 0.0,
            font: Vec::new(),
            size: 12.0,
            rise: 0.0,
        },
        stack: Vec::new(),
        tm: IDENTITY,
        tlm: IDENTITY,
        path: Vec::new(),
        start: (0.0, 0.0),
        current: (0.0, 0.0),
        out: PageLayout { width, height, ..Default::default() },
    };
    for (index, op) in content.operations.iter().enumerate() {
        walker.operation(index, op);
    }
    Ok(walker.out)
}

/// The layout of every page, in order. A page whose content cannot be read comes back empty.
pub(crate) fn layout(doc: &Document) -> Result<Vec<PageLayout>> {
    let mut pages = Vec::new();
    for (_, id) in doc.get_pages() {
        let layout = match page_content(doc, id) {
            Some(content) => walk(doc, id, &content)?,
            None => {
                let (width, height) = page_box(doc, id)?.displayed();
                PageLayout { width, height, ..Default::default() }
            }
        };
        pages.push(layout);
    }
    Ok(pages)
}

pub(crate) fn load(pdf: &[u8]) -> Result<Document> {
    let options = lopdf::LoadOptions { max_decompressed_size: Some(MAX_CONTENT_BYTES), ..Default::default() };
    Document::load_mem_with_options(pdf, options).map_err(|e| PdfError::Malformed(e.to_string()))
}

#[cfg(test)]
pub(crate) mod tests {
    #![allow(clippy::unwrap_used)]

    use lopdf::{Stream, dictionary};

    use super::*;

    /// A one-page A4 PDF with Helvetica as /F1 and the given content.
    pub fn page_with(content: &[u8]) -> Vec<u8> {
        let mut doc = Document::with_version("1.7");
        let pages_id = doc.new_object_id();
        let font = doc
            .add_object(dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica", "Encoding" => "WinAnsiEncoding" });
        let stream = doc.add_object(Stream::new(dictionary! {}, content.to_vec()));
        let page = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
            "Contents" => stream,
            "Resources" => dictionary! { "Font" => dictionary! { "F1" => font } },
        });
        doc.objects.insert(pages_id, Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => vec![page.into()], "Count" => 1 }));
        let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        doc.trailer.set("Root", catalog);
        let mut out = Vec::new();
        doc.save_to(&mut out).unwrap();
        out
    }

    fn text_of(layout: &PageLayout) -> String {
        layout.glyphs.iter().map(|g| g.text.as_str()).collect()
    }

    #[test]
    fn places_glyphs_on_the_displayed_page() {
        let pdf = page_with(b"BT /F1 10 Tf 100 742 Td (AB) Tj ET");
        let pages = layout(&load(&pdf).unwrap()).unwrap();
        let g = &pages[0].glyphs;
        assert_eq!(text_of(&pages[0]), "AB");
        assert!((g[0].origin.0 - 100.0).abs() < 1e-6 && (g[0].origin.1 - 100.0).abs() < 1e-6);
        assert!((g[1].origin.0 - 105.0).abs() < 1e-6, "default width 500 moves 5 points: {:?}", g[1].origin);
        assert!((g[0].size() - 10.0).abs() < 1e-6 && g[0].is_horizontal());
    }

    #[test]
    fn follows_tj_adjustments_matrices_and_state() {
        let pdf = page_with(
            b"q 2 0 0 2 0 0 cm BT /F1 10 Tf 1 0 0 1 10 10 Tm [(A) -1000 (B)] TJ 14 TL T* (C) Tj ET Q BT /F1 10 Tf 0 0 Td (D) Tj ET",
        );
        let pages = layout(&load(&pdf).unwrap()).unwrap();
        let g = &pages[0].glyphs;
        assert_eq!(text_of(&pages[0]), "ABCD");
        assert!((g[0].origin.0 - 20.0).abs() < 1e-6 && (g[0].origin.1 - (842.0 - 20.0)).abs() < 1e-6);
        assert!((g[1].origin.0 - 50.0).abs() < 1e-6, "A is 5 wide and the gap 10, doubled: {:?}", g[1].origin);
        assert!((g[2].origin.1 - (842.0 - 20.0 + 28.0)).abs() < 1e-6, "T* moves down one leading: {:?}", g[2].origin);
        assert!((g[3].origin.0).abs() < 1e-6, "Q restored the matrix");
    }

    #[test]
    fn reads_tounicode_maps() {
        let (map, one_byte) = parse_cmap(
            b"1 begincodespacerange <0000> <FFFF> endcodespacerange 2 beginbfchar <0001> <007B> <0002> <00E5> endbfchar \
              1 beginbfrange <0010> <0012> <0041> endbfrange 1 beginbfrange <0020> <0021> [<0078> <0079>] endbfrange",
        );
        assert_eq!(one_byte, Some(false));
        assert_eq!(map.get(&1).map(String::as_str), Some("{"));
        assert_eq!(map.get(&2).map(String::as_str), Some("å"));
        assert_eq!(map.get(&0x12).map(String::as_str), Some("C"));
        assert_eq!(map.get(&0x21).map(String::as_str), Some("y"));
    }

    #[test]
    fn hostile_cmaps_stay_bounded() {
        let (map, _) = parse_cmap(b"beginbfrange <00000000> <FFFFFFFF> <0041> endbfrange beginbfchar <0001");
        assert!(map.len() <= MAX_CMAP_ENTRIES);
        assert!(parse_cmap(b"<<<<[[[]]]>>>> beginbfrange <01> [").0.is_empty());
    }

    #[test]
    fn records_lines_and_boxes() {
        let pdf = page_with(b"1 w 100 100 m 300 100 l S 50 50 10 10 re S 0 0 595 1 re f 10 10 m 20 20 l n");
        let pages = layout(&load(&pdf).unwrap()).unwrap();
        let shapes = &pages[0].shapes;
        assert_eq!(shapes.len(), 3);
        assert_eq!(shapes[0], Shape::Line { from: (100.0, 742.0), to: (300.0, 742.0) });
        assert!(matches!(shapes[1], Shape::Rect { filled: false, .. }));
        assert!(matches!(shapes[2], Shape::Rect { filled: true, .. }));
    }
}
