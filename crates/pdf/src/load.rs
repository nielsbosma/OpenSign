use lopdf::{Document, LoadOptions, Object, ObjectId};

use crate::{PdfError, Result};

/// Per stream, and per page's content when text is read.
pub(crate) const MAX_STREAM_BYTES: usize = 64 * 1024 * 1024;
const MAX_OBJECTS: usize = 500_000;
const MAX_OBJECTS_IN_STREAM: usize = 20_000;
/// Stands in for an object stream that must not be expanded, since the filter cannot return an error.
const REFUSED: &[u8] = b"OpenSign.RefusedObjectStream";

/// Every parse of a PDF goes through here. These limits keep ordinary use cheap; what bounds a
/// hostile file for certain is the worker process in `isolated`.
pub(crate) fn load(pdf: &[u8]) -> Result<Document> {
    load_with(pdf, MAX_STREAM_BYTES)
}

pub(crate) fn load_with(pdf: &[u8], max_stream_bytes: usize) -> Result<Document> {
    let options = LoadOptions { max_decompressed_size: Some(max_stream_bytes), filter: Some(refuse_overlapping), ..Default::default() };
    let doc = Document::load_mem_with_options(pdf, options).map_err(|e| PdfError::Malformed(e.to_string()))?;
    if doc.objects.len() > MAX_OBJECTS || doc.objects.values().any(|o| o.as_name().is_ok_and(|n| n == REFUSED)) {
        return Err(PdfError::Rejected("The PDF is too complex".into()));
    }
    Ok(doc)
}

/// lopdf parses an object stream's entries wherever their offsets point, so entries sharing bytes
/// could turn one megabyte into gigabytes of objects. Each entry must start after the one before.
///
/// The loader ignores what this returns for a stream and looks at the object in place, and never
/// finds a stream inside an object stream, so streams need no costly copy for the return value.
fn refuse_overlapping(id: ObjectId, object: &mut Object) -> Option<(ObjectId, Object)> {
    let Object::Stream(stream) = object else { return Some((id, object.clone())) };
    if stream.dict.has_type(b"ObjStm") && !entries_are_ordered(stream) {
        *object = Object::Name(REFUSED.to_vec());
    }
    Some((id, Object::Null))
}

fn entries_are_ordered(stream: &lopdf::Stream) -> bool {
    let Ok(first) = stream.dict.get(b"First").and_then(Object::as_i64) else { return true };
    let Ok(content) = stream.get_plain_content_with_limit(MAX_STREAM_BYTES) else { return true };
    let Some(index) = usize::try_from(first).ok().and_then(|first| content.get(..first)) else { return true };
    let Ok(index) = std::str::from_utf8(index) else { return true };
    let numbers: Vec<&str> = index.split_whitespace().collect();
    if numbers.len() / 2 > MAX_OBJECTS_IN_STREAM {
        return false;
    }
    let mut previous = None;
    for [_, offset] in numbers.as_chunks::<2>().0 {
        let Ok(offset) = offset.parse::<u64>() else { return false };
        if previous.is_some_and(|p| offset <= p) {
            return false;
        }
        previous = Some(offset);
    }
    true
}
