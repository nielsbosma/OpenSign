//! Reads the incremental updates after a signature byte by byte. lopdf merges revisions as it
//! loads and forgets what it cannot represent (free entries, for one), and a viewer may not read
//! them the way lopdf does, so whether a signature still holds must not rest on lopdf's merge.
//!
//! Only the plain forms OpenSign itself writes pass: one cross-reference table or uncompressed
//! cross-reference stream per update, chained back to the signed revision. Anything else counts as
//! a change.

use std::collections::BTreeSet;

const MAX_REVISIONS: usize = 64;
const ENTRY_BYTES: usize = 20;
const STREAM_ENTRY_BYTES: usize = 7;

fn rfind(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).rposition(|w| w == needle)
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn count(haystack: &[u8], needle: &[u8]) -> usize {
    haystack.windows(needle.len()).filter(|w| *w == needle).count()
}

fn skip_space(bytes: &[u8], mut at: usize) -> usize {
    while bytes.get(at).is_some_and(|b| b.is_ascii_whitespace()) {
        at += 1;
    }
    at
}

fn number(bytes: &[u8], at: usize) -> Option<(usize, usize)> {
    let digits = bytes.get(at..)?.iter().take_while(|b| b.is_ascii_digit()).count();
    if digits == 0 || digits > 18 {
        return None;
    }
    let value = std::str::from_utf8(&bytes[at..at + digits]).ok()?.parse().ok()?;
    Some((value, at + digits))
}

/// Where the last `startxref` in `bytes` points, and where the keyword itself is.
fn last_startxref(bytes: &[u8]) -> Option<(usize, usize)> {
    let keyword = rfind(bytes, b"startxref")?;
    let (offset, _) = number(bytes, skip_space(bytes, keyword + b"startxref".len()))?;
    Some((offset, keyword))
}

/// The end of an update's `startxref`, which must name that update's own table, and `%%EOF`.
fn tail_end(pdf: &[u8], at: usize, own_offset: usize) -> Result<usize, String> {
    let rest = pdf.get(at..).ok_or_else(malformed)?;
    if !rest.starts_with(b"startxref") {
        return Err(malformed());
    }
    let (offset, next) = number(pdf, skip_space(pdf, at + b"startxref".len())).ok_or_else(malformed)?;
    let eof = skip_space(pdf, next);
    if offset != own_offset || !pdf[eof..].starts_with(b"%%EOF") {
        return Err("an update does not end the way its table says".into());
    }
    Ok(eof + b"%%EOF".len())
}

fn only_comments(bytes: &[u8]) -> bool {
    bytes.split(|b| *b == b'\n' || *b == b'\r').all(|line| {
        let line = line.trim_ascii();
        line.is_empty() || line.starts_with(b"%")
    })
}

/// What the updates after a signature consist of, by object number.
#[derive(Default)]
pub(crate) struct Appended {
    pub objects: BTreeSet<u32>,
    /// The cross-reference streams among them, which nothing in the document refers to.
    pub xref_streams: BTreeSet<u32>,
}

/// One update's table: where it starts and ends, and the objects it lists, by offset.
struct Section {
    start: usize,
    end: usize,
    objects: Vec<usize>,
    previous: usize,
}

/// Reads the updates after `end`. Err when they are not of the plain form above, when one frees or
/// rolls back an object (those changes leave no trace in what lopdf loads), or when any byte after
/// `end` is neither a listed object nor a table: a viewer that rebuilds a damaged table reads those.
pub(crate) fn appended_objects(pdf: &[u8], end: usize) -> Result<Appended, String> {
    let (signed_xref, _) = last_startxref(&pdf[..end]).ok_or("the signed revision has no cross-reference table")?;
    let (mut offset, keyword) = last_startxref(pdf).ok_or("the file has no cross-reference table")?;
    let mut appended = Appended::default();
    if keyword < end {
        return if only_comments(&pdf[end..]) { Ok(appended) } else { Err("data was added without a cross-reference table".into()) };
    }
    let mut sections = Vec::new();
    for _ in 0..MAX_REVISIONS {
        if offset == signed_xref {
            account_for_bytes(pdf, end, &mut sections)?;
            return Ok(appended);
        }
        if offset < end {
            return Err("an update does not continue the signed revision".into());
        }
        let bytes = pdf.get(offset..).ok_or_else(malformed)?;
        let section = if bytes.starts_with(b"xref") {
            read_table(pdf, bytes, offset, end, &mut appended)?
        } else {
            read_stream(pdf, bytes, offset, end, &mut appended)?
        };
        if section.previous >= offset {
            return Err("the updates are out of order".into());
        }
        offset = section.previous;
        sections.push(section);
    }
    Err("the file has too many updates".into())
}

/// Walks from the signed revision to the end of the file. `sections` holds the updates newest first.
fn account_for_bytes(pdf: &[u8], end: usize, sections: &mut [Section]) -> Result<(), String> {
    let mut at = end;
    for section in sections.iter_mut().rev() {
        section.objects.sort_unstable();
        for offset in &section.objects {
            if *offset < at || *offset >= section.start || !only_comments(&pdf[at..*offset]) {
                return Err("an update holds bytes its table does not list".into());
            }
            at = object_end(pdf, *offset, section.start)?;
        }
        if section.start < at || !only_comments(&pdf[at..section.start]) {
            return Err("an update holds bytes its table does not list".into());
        }
        at = section.end;
    }
    if only_comments(&pdf[at..]) { Ok(()) } else { Err("data was added after the last update".into()) }
}

/// Where the object at `offset` ends. A string holding one of these words only makes this fail.
fn object_end(pdf: &[u8], offset: usize, limit: usize) -> Result<usize, String> {
    let object = &pdf[offset..limit];
    let endobj = find(object, b"endobj").ok_or_else(malformed)?;
    let Some(stream) = find(object, b"stream").filter(|stream| *stream < endobj) else {
        return Ok(offset + endobj + b"endobj".len());
    };
    let header = object[..stream].trim_ascii_end().strip_suffix(b">>").ok_or_else(malformed)?;
    let open = find(header, b"<<").ok_or_else(malformed)?;
    let length = number_after(&without_strings(&header[open + 2..])?, b"/Length")?;
    let mut data = stream + b"stream".len();
    data += match object.get(data..) {
        Some([b'\r', b'\n', ..]) => 2,
        Some([b'\n', ..]) => 1,
        _ => return Err(malformed()),
    };
    let close = skip_space(object, data.checked_add(length).ok_or_else(malformed)?);
    if !object.get(close..).is_some_and(|rest| rest.starts_with(b"endstream")) {
        return Err(malformed());
    }
    let after = skip_space(object, close + b"endstream".len());
    if !object[after..].starts_with(b"endobj") {
        return Err(malformed());
    }
    Ok(offset + after + b"endobj".len())
}

fn malformed() -> String {
    "an update's cross-reference table is not in the expected form".into()
}

/// A copy of a dictionary's insides with strings blanked, so that searching it for a key finds only
/// keys. Comments, nested dictionaries and `#` escapes could also make a search read something other
/// than a viewer does, and OpenSign writes none of them here.
fn without_strings(dict: &[u8]) -> Result<Vec<u8>, String> {
    let mut plain = dict.to_vec();
    let mut at = 0;
    while let Some(byte) = dict.get(at) {
        match byte {
            b'%' | b'#' => return Err(malformed()),
            b'<' if dict.get(at + 1) == Some(&b'<') => return Err(malformed()),
            b'<' => {
                let close = at + find(&dict[at..], b">").ok_or_else(malformed)?;
                if !dict[at + 1..close].iter().all(|b| b.is_ascii_hexdigit() || b.is_ascii_whitespace()) {
                    return Err(malformed());
                }
                plain[at..=close].fill(b' ');
                at = close;
            }
            b'(' => {
                let close = string_end(dict, at).ok_or_else(malformed)?;
                plain[at..=close].fill(b' ');
                at = close;
            }
            _ => {}
        }
        at += 1;
    }
    Ok(plain)
}

fn string_end(bytes: &[u8], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut at = open;
    while let Some(byte) = bytes.get(at) {
        match byte {
            b'\\' => at += 1,
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(at);
                }
            }
            _ => {}
        }
        at += 1;
    }
    None
}

/// Returns the previous table's offset.
fn read_trailer(trailer: &[u8]) -> Result<usize, String> {
    let hidden = [b"/Encrypt".as_slice(), b"/XRefStm".as_slice()];
    if hidden.iter().any(|key| find(trailer, key).is_some()) {
        return Err("an update's trailer changes how the file is read".into());
    }
    number_after(trailer, b"/Prev")
}

/// A key must appear once and its value must be a plain number: `5 0 R` would read as 5 here and as
/// whatever object 5 holds in a viewer.
fn number_after(dict: &[u8], key: &[u8]) -> Result<usize, String> {
    if count(dict, key) != 1 {
        return Err(malformed());
    }
    let at = find(dict, key).ok_or_else(malformed)?;
    let (value, next) = number(dict, skip_space(dict, at + key.len())).ok_or_else(malformed)?;
    match dict.get(skip_space(dict, next)) {
        None | Some(b'/') => Ok(value),
        _ => Err(malformed()),
    }
}

fn read_table(pdf: &[u8], section: &[u8], offset: usize, end: usize, appended: &mut Appended) -> Result<Section, String> {
    let mut objects = Vec::new();
    let mut listed = BTreeSet::new();
    let mut at = skip_space(section, b"xref".len());
    while !section[at..].starts_with(b"trailer") {
        let (first, next) = number(section, at).ok_or_else(malformed)?;
        let (count, next) = number(section, skip_space(section, next)).ok_or_else(malformed)?;
        at = skip_space(section, next);
        for n in 0..count {
            let entry = section.get(at..at + ENTRY_BYTES).ok_or_else(malformed)?;
            let (entry_offset, _) = number(entry, 0).filter(|(_, next)| *next == 10).ok_or_else(malformed)?;
            let (generation, _) = number(entry, 11).filter(|(_, next)| *next == 16).ok_or_else(malformed)?;
            match entry.get(17) {
                Some(b'f') if first + n == 0 && entry_offset == 0 && generation == 65535 => {}
                Some(b'f') => return Err("an update deletes an object".into()),
                Some(b'n') if generation == 0 => {
                    if !listed.insert(first + n) {
                        return Err(malformed());
                    }
                    add_object(pdf, first + n, entry_offset, end, appended)?;
                    objects.push(entry_offset);
                }
                _ => return Err(malformed()),
            }
            at += ENTRY_BYTES;
        }
        at = skip_space(section, at);
    }
    let open = skip_space(section, at + b"trailer".len());
    if !section[open..].starts_with(b"<<") {
        return Err(malformed());
    }
    let keyword = open + find(&section[open..], b"startxref").ok_or_else(malformed)?;
    let dict = section[open + 2..keyword].trim_ascii_end().strip_suffix(b">>").ok_or_else(malformed)?;
    let previous = read_trailer(&without_strings(dict)?)?;
    Ok(Section { start: offset, end: tail_end(pdf, offset + keyword, offset)?, objects, previous })
}

/// The cross-reference stream lopdf writes: uncompressed, with fixed field widths.
fn read_stream(pdf: &[u8], section: &[u8], offset: usize, end: usize, appended: &mut Appended) -> Result<Section, String> {
    let (own, next) = number(section, 0).ok_or_else(malformed)?;
    let start = next + find(&section[next..], b"obj").ok_or_else(malformed)?;
    let body = find(&section[start..], b">>stream\n").map(|at| start + at).ok_or_else(malformed)?;
    let open = skip_space(section, start + b"obj".len());
    if !section[open..].starts_with(b"<<") || open + 2 > body {
        return Err(malformed());
    }
    let dict = without_strings(&section[open + 2..body])?;
    if !section[next..start].eq(b" 0 ")
        || find(&dict, b"/Type/XRef").is_none()
        || count(&dict, b"/W") != 1
        || find(&dict, b"/W[1 4 2]").is_none()
        || find(&dict, b"/Filter").is_some()
    {
        return Err(malformed());
    }
    let previous = read_trailer(&dict)?;
    if count(&dict, b"/Index") != 1 {
        return Err(malformed());
    }
    let index = skip_space(&dict, find(&dict, b"/Index").ok_or_else(malformed)? + b"/Index".len());
    if dict.get(index) != Some(&b'[') {
        return Err(malformed());
    }
    let index = &dict[index + 1..index + find(&dict[index..], b"]").ok_or_else(malformed)?];
    let index: Vec<usize> =
        index.split(u8::is_ascii_whitespace).map(|n| number(n, 0).map(|(value, _)| value)).collect::<Option<_>>().ok_or_else(malformed)?;
    let (ranges, []) = index.as_chunks::<2>() else { return Err(malformed()) };
    let content = body + b">>stream\n".len();
    let length = number_after(&dict, b"/Length")?;
    let close = content.checked_add(length).ok_or_else(malformed)?;
    let (entries, []) = section.get(content..close).ok_or_else(malformed)?.as_chunks::<STREAM_ENTRY_BYTES>() else {
        return Err(malformed());
    };
    let mut objects = Vec::new();
    let mut listed = BTreeSet::new();
    let mut entries = entries.iter();
    for [first, count] in ranges {
        for n in 0..*count {
            let entry = entries.next().ok_or_else(malformed)?;
            let entry_offset = entry[1..5].iter().fold(0usize, |value, byte| value << 8 | *byte as usize);
            match (entry[0], &entry[5..7]) {
                (0, _) if first + n == 0 => {}
                (0, _) => return Err("an update deletes an object".into()),
                (1, [0, 0]) => {
                    if !listed.insert(first + n) {
                        return Err(malformed());
                    }
                    add_object(pdf, first + n, entry_offset, end, appended)?;
                    if entry_offset != offset {
                        objects.push(entry_offset);
                    }
                }
                _ => return Err(malformed()),
            }
        }
    }
    if entries.next().is_some() {
        return Err(malformed());
    }
    appended.xref_streams.insert(u32::try_from(own).map_err(|_| malformed())?);
    let closing = skip_space(section, close);
    if !section[closing..].starts_with(b"endstream") {
        return Err(malformed());
    }
    let closing = skip_space(section, closing + b"endstream".len());
    if !section[closing..].starts_with(b"endobj") {
        return Err(malformed());
    }
    let keyword = skip_space(section, closing + b"endobj".len());
    Ok(Section { start: offset, end: tail_end(pdf, offset + keyword, offset)?, objects, previous })
}

fn add_object(pdf: &[u8], object: usize, offset: usize, end: usize, appended: &mut Appended) -> Result<(), String> {
    if object == 0 {
        return Err(malformed());
    }
    if offset < end {
        return Err("an update points back at bytes from before the signature".into());
    }
    let header = format!("{object} 0 obj");
    if !pdf.get(offset..).is_some_and(|rest| rest.starts_with(header.as_bytes())) {
        return Err(malformed());
    }
    appended.objects.insert(u32::try_from(object).map_err(|_| malformed())?);
    Ok(())
}
