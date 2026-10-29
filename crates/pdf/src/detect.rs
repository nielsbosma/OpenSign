use serde::Serialize;

use crate::Result;
use crate::text::{Glyph, PageLayout, Shape, layout, load};

const MAX_SUGGESTIONS: usize = 300;
const BOX_CHARS: &[char] = &['☐', '□', '❏', '❐', '❑', '❒', '▢', '◻', '⬜', '\u{f06f}', '\u{f0a8}', '\u{f071}'];

/// A place on a page that looks like it wants a field, with a guess at the kind. The box is in
/// fractions of the displayed page, origin top-left.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedField {
    /// Zero-based page within the file.
    pub page: usize,
    /// A field kind: signature, initials, date, date_input, name, email, company, phone, text or checkbox.
    pub kind: &'static str,
    /// The words next to it, such as "Name" or "I accept the terms".
    pub label: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// A line of text on a page, with its position as fractions of the displayed page.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextLine {
    pub x: f64,
    pub y: f64,
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PageText {
    pub width: f64,
    pub height: f64,
    pub lines: Vec<TextLine>,
}

#[derive(Debug, Clone)]
struct Word {
    text: String,
    x0: f64,
    x1: f64,
    top: f64,
    baseline: f64,
    size: f64,
    underscores: bool,
}

/// A horizontal line someone writes on, in displayed points.
#[derive(Debug, Clone, Copy)]
struct Rule {
    x0: f64,
    x1: f64,
    y: f64,
    typed: bool,
}

fn words(glyphs: &[Glyph]) -> Vec<Word> {
    let mut glyphs: Vec<&Glyph> = glyphs.iter().filter(|g| g.is_horizontal() && g.size() > 2.0 && !g.text.is_empty()).collect();
    glyphs.sort_by(|a, b| {
        let line = |g: &Glyph| (g.origin.1 / (g.size() * 0.5)).round();
        line(a).total_cmp(&line(b)).then(a.origin.0.total_cmp(&b.origin.0))
    });
    let mut out: Vec<Word> = Vec::new();
    for g in glyphs {
        let size = g.size();
        let blank = g.text.chars().all(char::is_whitespace);
        let underscore = g.text.chars().all(|c| c == '_');
        let joins = out.last().is_some_and(|w| {
            (w.baseline - g.origin.1).abs() < size * 0.3
                && g.origin.0 - w.x1 < size * 0.25
                && g.origin.0 - w.x1 > -size
                && w.underscores == underscore
                && !w.text.ends_with(' ')
        });
        if blank {
            if let Some(w) = out.last_mut().filter(|_| joins) {
                w.text.push(' ');
            }
            continue;
        }
        if joins && let Some(w) = out.last_mut() {
            w.text.push_str(&g.text);
            w.x1 = w.x1.max(g.end.0);
            continue;
        }
        out.push(Word {
            text: g.text.clone(),
            x0: g.origin.0,
            x1: g.end.0,
            top: g.origin.1 - size * 0.8,
            baseline: g.origin.1,
            size,
            underscores: underscore,
        });
    }
    for w in &mut out {
        w.text = w.text.trim().to_string();
    }
    out.retain(|w| !w.text.is_empty());
    out
}

fn rules(page: &PageLayout, words: &[Word]) -> Vec<Rule> {
    let mut out = Vec::new();
    for shape in &page.shapes {
        match *shape {
            Shape::Line { from, to } if (from.1 - to.1).abs() < 0.8 && (from.0 - to.0).abs() >= 36.0 => {
                out.push(Rule { x0: from.0.min(to.0), x1: from.0.max(to.0), y: (from.1 + to.1) / 2.0, typed: false });
            }
            Shape::Rect { x0, y0, x1, y1, .. } if y1 - y0 <= 2.5 && x1 - x0 >= 36.0 => {
                out.push(Rule { x0, x1, y: (y0 + y1) / 2.0, typed: false });
            }
            _ => {}
        }
    }
    for w in words.iter().filter(|w| w.underscores && w.text.chars().count() >= 3) {
        out.push(Rule { x0: w.x0, x1: w.x1, y: w.baseline, typed: true });
    }
    out.sort_by(|a, b| a.y.total_cmp(&b.y).then(a.x0.total_cmp(&b.x0)));
    let mut merged: Vec<Rule> = Vec::new();
    for r in out {
        if let Some(last) = merged.last_mut()
            && (last.y - r.y).abs() < 2.0
            && r.x0 <= last.x1 + 1.0
        {
            last.x1 = last.x1.max(r.x1);
            last.typed |= r.typed;
            continue;
        }
        merged.push(r);
    }
    merged
}

/// What a label asks for, by keyword in the languages OpenSign speaks most often.
fn kind_for(label: &str) -> &'static str {
    let l = label.to_lowercase();
    let tokens: Vec<&str> = l.split(|c: char| !c.is_alphanumeric() && c != '-').filter(|t| !t.is_empty()).collect();
    let has = |words: &[&str]| words.iter().any(|w| if w.contains(' ') { l.contains(w) } else { tokens.iter().any(|t| t.starts_with(w)) });
    if has(&["initial", "paraf", "signum"]) {
        "initials"
    } else if has(&["signature", "sign here", "signed", "underskrift", "signatur", "unterschrift", "firma", "firmado"]) {
        "signature"
    } else if has(&["birth", "födelse", "födelsedatum", "geburt", "naissance", "nacimiento"]) {
        "date_input"
    } else if has(&["date", "dated", "datum", "fecha"]) {
        "date"
    } else if has(&["e-mail", "email", "e-post", "epost", "correo", "courriel"]) {
        "email"
    } else if has(&["phone", "telefon", "telephone", "mobile", "mobil", "tel", "teléfono", "téléphone"]) {
        "phone"
    } else if has(&["company", "företag", "organisation", "organization", "bolag", "empresa", "société"]) {
        "company"
    } else if has(&["name", "namn", "nombre", "nom", "print"]) {
        "name"
    } else {
        "text"
    }
}

fn clean_label(words: &[&Word]) -> String {
    let text = words.iter().map(|w| w.text.as_str()).collect::<Vec<_>>().join(" ");
    let text = text.trim().trim_end_matches([':', '_', '.', ' ']).trim();
    text.chars().take(60).collect()
}

fn overlap(a: &DetectedField, b: &DetectedField) -> f64 {
    let ix = (a.x + a.width).min(b.x + b.width) - a.x.max(b.x);
    let iy = (a.y + a.height).min(b.y + b.height) - a.y.max(b.y);
    if ix <= 0.0 || iy <= 0.0 {
        return 0.0;
    }
    ix * iy / (a.width * a.height).min(b.width * b.height).max(1e-9)
}

fn detect_page(index: usize, page: &PageLayout) -> Vec<DetectedField> {
    let (pw, ph) = (page.width, page.height);
    let words = words(&page.glyphs);
    let text_words: Vec<&Word> = words.iter().filter(|w| !w.underscores).collect();
    let mut out: Vec<DetectedField> = Vec::new();
    let mut push = |kind: &'static str, label: String, x0: f64, y0: f64, x1: f64, y1: f64| {
        let (x0, x1) = (x0.clamp(0.0, pw), x1.clamp(0.0, pw));
        let (y0, y1) = (y0.clamp(0.0, ph), y1.clamp(0.0, ph));
        if x1 - x0 < 4.0 || y1 - y0 < 4.0 {
            return;
        }
        out.push(DetectedField { page: index, kind, label, x: x0 / pw, y: y0 / ph, width: (x1 - x0) / pw, height: (y1 - y0) / ph });
    };

    for rule in rules(page, &words) {
        let width = rule.x1 - rule.x0;
        if width > pw * 0.85 {
            continue;
        }
        let underlined = text_words.iter().any(|w| {
            let covered = w.x1.min(rule.x1) - w.x0.max(rule.x0);
            !rule.typed && rule.y - w.baseline >= -1.0 && rule.y - w.baseline < w.size * 0.5 && covered > width * 0.4
        });
        if underlined {
            continue;
        }
        let mut left: Vec<&Word> = text_words
            .iter()
            .copied()
            .filter(|w| (w.baseline - rule.y).abs() < w.size.max(8.0) * 1.1 && w.x1 <= rule.x0 + 2.0 && w.x1 > rule.x0 - 200.0)
            .collect();
        left.sort_by(|a, b| a.x0.total_cmp(&b.x0));
        let mut label: Vec<&Word> = Vec::new();
        let mut edge = rule.x0;
        for w in left.iter().rev() {
            if edge - w.x1 > w.size * 2.5 || label.len() >= 5 {
                break;
            }
            label.insert(0, w);
            edge = w.x0;
        }
        let beside = !label.is_empty();
        if !beside {
            let mut below: Vec<&Word> = text_words
                .iter()
                .copied()
                .filter(|w| w.top >= rule.y - 1.0 && w.top - rule.y < 14.0 && w.x1 > rule.x0 && w.x0 < rule.x1)
                .collect();
            below.sort_by(|a, b| a.x0.total_cmp(&b.x0));
            label = below.into_iter().take(6).collect();
        }
        let text = clean_label(&label);
        let kind = kind_for(&text);
        let size = label.first().map_or(11.0, |w| w.size);
        let height = match kind {
            "signature" => 30.0,
            "initials" => 22.0,
            _ => (size * 1.35).clamp(11.0, 22.0),
        };
        push(kind, text, rule.x0, rule.y - height - 0.5, rule.x1, rule.y - 0.5);
    }

    let boxes: Vec<[f64; 4]> = page
        .shapes
        .iter()
        .filter_map(|s| match *s {
            Shape::Rect { x0, y0, x1, y1, .. } => {
                let (w, h) = (x1 - x0, y1 - y0);
                ((6.0..=22.0).contains(&w) && (6.0..=22.0).contains(&h) && (w / h - 1.0).abs() < 0.25).then_some([x0, y0, x1, y1])
            }
            _ => None,
        })
        .chain(page.glyphs.iter().filter(|g| g.text.chars().any(|c| BOX_CHARS.contains(&c))).map(|g| {
            let b = g.bounds();
            let side = (g.end.0 - g.origin.0).max(g.size() * 0.6);
            let mid = (b[1] + b[3]) / 2.0;
            [g.origin.0, mid - side / 2.0, g.origin.0 + side, mid + side / 2.0]
        }))
        .collect();
    for [x0, y0, x1, y1] in boxes {
        let mut right: Vec<&Word> = text_words
            .iter()
            .copied()
            .filter(|w| w.baseline > y0 && w.baseline < y1 + w.size * 0.6 && w.x0 >= x1 - 1.0 && w.x0 - x1 < 200.0)
            .filter(|w| !w.text.chars().all(|c| BOX_CHARS.contains(&c)))
            .collect();
        right.sort_by(|a, b| a.x0.total_cmp(&b.x0));
        let mut label = Vec::new();
        let mut edge = x1;
        for w in right {
            if w.x0 - edge > w.size * 2.0 || label.len() >= 8 {
                break;
            }
            edge = w.x1;
            label.push(w);
        }
        push("checkbox", clean_label(&label), x0, y0, x1, y1);
    }

    let mut kept: Vec<DetectedField> = Vec::new();
    for f in out {
        if !kept.iter().any(|k| overlap(k, &f) > 0.5) {
            kept.push(f);
        }
    }
    kept
}

/// Places in the PDF that look like they want a field: lines and underscores to write on, with
/// the label beside or below them, and empty boxes to tick. A guess for the sender to confirm.
pub fn detect_fields(pdf: &[u8]) -> Result<Vec<DetectedField>> {
    let doc = load(pdf)?;
    let mut out = Vec::new();
    for (index, page) in layout(&doc)?.iter().enumerate() {
        out.extend(detect_page(index, page));
        if out.len() >= MAX_SUGGESTIONS {
            out.truncate(MAX_SUGGESTIONS);
            break;
        }
    }
    Ok(out)
}

/// The lines of text on each page with their positions, for a model to suggest fields from.
pub fn page_text(pdf: &[u8]) -> Result<Vec<PageText>> {
    let doc = load(pdf)?;
    Ok(layout(&doc)?
        .iter()
        .map(|page| {
            let mut lines: Vec<TextLine> = Vec::new();
            let mut last: Option<(f64, f64)> = None;
            for w in words(&page.glyphs) {
                match (last, lines.last_mut()) {
                    (Some((baseline, size)), Some(line)) if (w.baseline - baseline).abs() < size * 0.3 => {
                        line.text.push(' ');
                        line.text.push_str(&w.text);
                    }
                    _ => lines.push(TextLine { x: w.x0 / page.width, y: w.top / page.height, text: w.text.clone() }),
                }
                last = Some((w.baseline, w.size));
            }
            PageText { width: page.width, height: page.height, lines }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::text::tests::page_with;

    #[test]
    fn finds_lines_underscores_and_boxes() {
        let pdf = page_with(
            b"BT /F1 11 Tf 72 700 Td (Name:) Tj ET 110 698 m 300 698 l S \
              BT /F1 11 Tf 72 600 Td (Email: ____________) Tj ET \
              320 500 m 520 500 l S BT /F1 9 Tf 320 488 Td (Signature) Tj ET \
              72 400 10 10 re S BT /F1 10 Tf 88 401 Td (I accept the terms) Tj ET \
              BT /F1 11 Tf 72 300 Td (Underlined) Tj ET 72 298 m 130 298 l S \
              0 200 595 1 re f",
        );
        let found = detect_fields(&pdf).unwrap();
        let kinds: Vec<(&str, &str)> = found.iter().map(|f| (f.kind, f.label.as_str())).collect();
        assert_eq!(
            kinds,
            vec![("name", "Name"), ("email", "Email"), ("signature", "Signature"), ("checkbox", "I accept the terms")],
            "{found:#?}"
        );
        let name = &found[0];
        assert!((name.x - 110.0 / 595.0).abs() < 1e-6 && (name.width - 190.0 / 595.0).abs() < 1e-6);
        assert!(name.y + name.height <= (842.0 - 698.0) / 842.0);
    }

    #[test]
    fn finds_underscores_in_a_converted_word_file() {
        let found = detect_fields(include_bytes!("../fixtures/word-detect.pdf")).unwrap();
        assert_eq!(found.iter().map(|f| (f.kind, f.label.as_str())).collect::<Vec<_>>(), vec![("name", "Name")], "{found:#?}");
    }

    #[test]
    fn swedish_labels() {
        assert_eq!(kind_for("Underskrift"), "signature");
        assert_eq!(kind_for("Namnförtydligande"), "name");
        assert_eq!(kind_for("Ort och datum"), "date");
        assert_eq!(kind_for("Födelsedatum"), "date_input");
        assert_eq!(kind_for("Företag"), "company");
        assert_eq!(kind_for("Adress"), "text");
    }

    #[test]
    fn reads_lines_for_a_model() {
        let pdf = page_with(b"BT /F1 12 Tf 72 700 Td (Hello) Tj ( world) Tj ET BT /F1 12 Tf 72 680 Td (Second) Tj ET");
        let pages = page_text(&pdf).unwrap();
        let lines: Vec<&str> = pages[0].lines.iter().map(|l| l.text.as_str()).collect();
        assert_eq!(lines, vec!["Hello world", "Second"]);
    }
}

#[cfg(test)]
mod hostile {
    use crate::{Limits, sanitize, take_text_tags};

    /// Damaged copies of real files must give an error or a result, never a panic or a hang.
    #[test]
    fn damaged_files_never_panic() {
        let originals: [&[u8]; 3] = [
            include_bytes!("../fixtures/word-tags.pdf"),
            include_bytes!("../fixtures/acroform.pdf"),
            include_bytes!("../fixtures/word-detect.pdf"),
        ];
        let mut seed = 0x9E37_79B9_7F4A_7C15_u64;
        for original in originals {
            for _ in 0..60 {
                let mut pdf = original.to_vec();
                for _ in 0..8 {
                    seed ^= seed << 13;
                    seed ^= seed >> 7;
                    seed ^= seed << 17;
                    let at = (seed as usize) % pdf.len();
                    pdf[at] = (seed >> 32) as u8;
                }
                if let Ok(clean) = sanitize(&pdf, Limits::default()) {
                    let _ = take_text_tags(&clean.pdf);
                    let _ = super::detect_fields(&clean.pdf);
                    let _ = super::page_text(&clean.pdf);
                }
            }
        }
    }
}
