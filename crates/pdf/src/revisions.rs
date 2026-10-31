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

/// Reads the updates after `end`. Err when they are not of the plain form above, or when one frees
/// or rolls back an object: those changes leave no trace in what lopdf loads.
pub(crate) fn appended_objects(pdf: &[u8], end: usize) -> Result<Appended, String> {
    let (signed_xref, _) = last_startxref(&pdf[..end]).ok_or("the signed revision has no cross-reference table")?;
    let (mut offset, keyword) = last_startxref(pdf).ok_or("the file has no cross-reference table")?;
    let mut appended = Appended::default();
    if keyword < end {
        return if only_comments(&pdf[end..]) { Ok(appended) } else { Err("data was added without a cross-reference table".into()) };
    }
    for _ in 0..MAX_REVISIONS {
        if offset == signed_xref {
            return Ok(appended);
        }
        if offset < end {
            return Err("an update does not continue the signed revision".into());
        }
        let section = pdf.get(offset..).ok_or_else(malformed)?;
        let previous = if section.starts_with(b"xref") {
            read_table(pdf, section, end, &mut appended)?
        } else {
            read_stream(pdf, section, end, &mut appended)?
        };
        if previous >= offset {
            return Err("the updates are out of order".into());
        }
        offset = previous;
    }
    Err("the file has too many updates".into())
}

fn malformed() -> String {
    "an update's cross-reference table is not in the expected form".into()
}

/// Refusing `#` means a key cannot be spelled so that this search misses it. Returns the previous
/// table's offset.
fn read_trailer(trailer: &[u8]) -> Result<usize, String> {
    let hidden = [b"/Encrypt".as_slice(), b"/XRefStm".as_slice()];
    if trailer.contains(&b'#') || hidden.iter().any(|key| find(trailer, key).is_some()) {
        return Err("an update's trailer changes how the file is read".into());
    }
    number_after(trailer, b"/Prev")
}

/// A key must appear once, or a string holding its name could make this read another value than a viewer does.
fn number_after(dict: &[u8], key: &[u8]) -> Result<usize, String> {
    if count(dict, key) != 1 {
        return Err(malformed());
    }
    let at = find(dict, key).ok_or_else(malformed)?;
    number(dict, skip_space(dict, at + key.len())).map(|(value, _)| value).ok_or_else(malformed)
}

fn read_table(pdf: &[u8], section: &[u8], end: usize, appended: &mut Appended) -> Result<usize, String> {
    let mut at = skip_space(section, b"xref".len());
    while !section[at..].starts_with(b"trailer") {
        let (first, next) = number(section, at).ok_or_else(malformed)?;
        let (count, next) = number(section, skip_space(section, next)).ok_or_else(malformed)?;
        at = skip_space(section, next);
        for n in 0..count {
            let entry = section.get(at..at + ENTRY_BYTES).ok_or_else(malformed)?;
            let (offset, _) = number(entry, 0).filter(|(_, next)| *next == 10).ok_or_else(malformed)?;
            let (generation, _) = number(entry, 11).filter(|(_, next)| *next == 16).ok_or_else(malformed)?;
            match entry.get(17) {
                Some(b'f') if first + n == 0 && offset == 0 && generation == 65535 => {}
                Some(b'f') => return Err("an update deletes an object".into()),
                Some(b'n') if generation == 0 => add_object(pdf, first + n, offset, end, appended)?,
                _ => return Err(malformed()),
            }
            at += ENTRY_BYTES;
        }
        at = skip_space(section, at);
    }
    let trailer = &section[at..];
    read_trailer(&trailer[..find(trailer, b"startxref").ok_or_else(malformed)?])
}

/// The cross-reference stream lopdf writes: uncompressed, with fixed field widths.
fn read_stream(pdf: &[u8], section: &[u8], end: usize, appended: &mut Appended) -> Result<usize, String> {
    let (own, next) = number(section, 0).ok_or_else(malformed)?;
    let start = next + find(&section[next..], b"obj").ok_or_else(malformed)?;
    let body = find(&section[start..], b">>stream\n").map(|at| start + at).ok_or_else(malformed)?;
    let dict = &section[start..body];
    if !section[next..start].eq(b" 0 ")
        || find(dict, b"/Type/XRef").is_none()
        || count(dict, b"/W") != 1
        || find(dict, b"/W[1 4 2]").is_none()
        || find(dict, b"/Filter").is_some()
    {
        return Err(malformed());
    }
    let previous = read_trailer(dict)?;
    if count(dict, b"/Index[") != 1 {
        return Err(malformed());
    }
    let index = find(dict, b"/Index[").ok_or_else(malformed)? + b"/Index[".len();
    let index = &dict[index..index + find(&dict[index..], b"]").ok_or_else(malformed)?];
    let index: Vec<usize> =
        index.split(u8::is_ascii_whitespace).map(|n| number(n, 0).map(|(value, _)| value)).collect::<Option<_>>().ok_or_else(malformed)?;
    let (ranges, []) = index.as_chunks::<2>() else { return Err(malformed()) };
    let content = body + b">>stream\n".len();
    let length = number_after(dict, b"/Length")?;
    let (entries, []) = section.get(content..content.saturating_add(length)).ok_or_else(malformed)?.as_chunks::<STREAM_ENTRY_BYTES>()
    else {
        return Err(malformed());
    };
    let mut entries = entries.iter();
    for [first, count] in ranges {
        for n in 0..*count {
            let entry = entries.next().ok_or_else(malformed)?;
            let offset = entry[1..5].iter().fold(0usize, |value, byte| value << 8 | *byte as usize);
            match (entry[0], &entry[5..7]) {
                (0, _) if first + n == 0 => {}
                (0, _) => return Err("an update deletes an object".into()),
                (1, [0, 0]) => add_object(pdf, first + n, offset, end, appended)?,
                _ => return Err(malformed()),
            }
        }
    }
    if entries.next().is_some() {
        return Err(malformed());
    }
    appended.xref_streams.insert(u32::try_from(own).map_err(|_| malformed())?);
    Ok(previous)
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
