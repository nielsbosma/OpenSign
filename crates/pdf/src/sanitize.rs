use std::collections::BTreeSet;

use lopdf::{Dictionary, Document, LoadOptions, Object};
use serde::Serialize;

use crate::geometry::{PageBox, page_box};
use crate::{PdfError, Result};

#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub max_bytes: usize,
    pub max_pages: usize,
    pub max_objects: usize,
    pub max_stream_bytes: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self { max_bytes: 25 * 1024 * 1024, max_pages: 500, max_objects: 200_000, max_stream_bytes: 64 * 1024 * 1024 }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Sanitized {
    #[serde(skip)]
    pub pdf: Vec<u8>,
    pub pages: Vec<PageBox>,
    /// What was taken out, for the uploader and the audit log.
    pub removed: Vec<String>,
}

/// Catalog entries that run code, carry files or replace the page content.
const CATALOG_KEYS: &[(&[u8], &str)] = &[
    (b"OpenAction", "an action that runs when the file opens"),
    (b"AA", "automatic actions"),
    (b"Names", "embedded files and scripts"),
    (b"AcroForm", "form fields"),
    (b"Perms", "existing signature permissions"),
    (b"Collection", "a PDF portfolio"),
    (b"Threads", "article threads"),
    (b"URI", "a base URI"),
    (b"Requirements", "viewer requirements"),
    (b"NeedsRendering", "XFA rendering"),
    (b"SpiderInfo", "web capture data"),
];

/// Keys removed wherever they appear.
const ANYWHERE_KEYS: &[(&[u8], &str)] = &[
    (b"JS", "JavaScript"),
    (b"JavaScript", "JavaScript"),
    (b"AA", "automatic actions"),
    (b"OpenAction", "an action that runs when the file opens"),
    (b"EmbeddedFiles", "embedded files"),
    (b"XFA", "XFA forms"),
];

/// Action types other than an in-document GoTo. `/S` also names transparency groups and masks,
/// so only these values mark a dictionary as an action.
const ACTION_TYPES: &[&[u8]] = &[
    b"JavaScript",
    b"Launch",
    b"SubmitForm",
    b"ImportData",
    b"ResetForm",
    b"GoToR",
    b"GoToE",
    b"GoTo3DView",
    b"URI",
    b"Rendition",
    b"Sound",
    b"Movie",
    b"RichMediaExecute",
    b"Hide",
    b"Named",
    b"SetOCGState",
    b"Thread",
    b"Trans",
];

const ALLOWED_PAGE_KEYS: &[&[u8]] = &[
    b"Type",
    b"Parent",
    b"Resources",
    b"MediaBox",
    b"CropBox",
    b"BleedBox",
    b"TrimBox",
    b"ArtBox",
    b"Rotate",
    b"Contents",
    b"UserUnit",
    b"Group",
];

fn scrub(object: &mut Object, removed: &mut BTreeSet<&'static str>, depth: usize) {
    if depth > 64 {
        *object = Object::Null;
        return;
    }
    match object {
        Object::Dictionary(dict) => scrub_dict(dict, removed, depth),
        Object::Stream(stream) => {
            let kind = stream.dict.get(b"Type").ok().and_then(|t| t.as_name().ok()).map(<[u8]>::to_vec);
            let subtype = stream.dict.get(b"Subtype").ok().and_then(|t| t.as_name().ok()).map(<[u8]>::to_vec);
            if kind.as_deref() == Some(b"EmbeddedFile") || matches!(subtype.as_deref(), Some(b"U3D") | Some(b"PRC")) {
                removed.insert("embedded files");
                *object = Object::Null;
                return;
            }
            scrub_dict(&mut stream.dict, removed, depth);
        }
        Object::Array(items) => items.iter_mut().for_each(|o| scrub(o, removed, depth + 1)),
        _ => {}
    }
}

fn scrub_dict(dict: &mut Dictionary, removed: &mut BTreeSet<&'static str>, depth: usize) {
    for (key, what) in ANYWHERE_KEYS {
        if dict.remove(key).is_some() {
            removed.insert(what);
        }
    }
    let kind = dict.get(b"Type").ok().and_then(|t| t.as_name().ok()).map(<[u8]>::to_vec);
    let action = dict.get(b"S").ok().and_then(|s| s.as_name().ok()).is_some_and(|s| ACTION_TYPES.contains(&s));
    if action {
        removed.insert("actions");
        *dict = Dictionary::new();
        return;
    }
    if matches!(kind.as_deref(), Some(b"Filespec") | Some(b"Action") | Some(b"Annot") | Some(b"Sig")) {
        removed.insert(match kind.as_deref() {
            Some(b"Filespec") => "file attachments",
            Some(b"Annot") => "annotations",
            Some(b"Sig") => "existing signatures",
            _ => "actions",
        });
        *dict = Dictionary::new();
        return;
    }
    for (_, value) in dict.iter_mut() {
        scrub(value, removed, depth + 1);
    }
}

fn reject(message: &str) -> PdfError {
    PdfError::Rejected(message.into())
}

/// Parses an untrusted PDF with resource limits and writes a fresh one without scripts, actions,
/// embedded files, forms, annotations, earlier signatures, encryption or incremental history.
/// The pages' content and resources are kept as they were.
pub fn sanitize(input: &[u8], limits: Limits) -> Result<Sanitized> {
    if input.len() > limits.max_bytes {
        return Err(reject("The PDF is too large"));
    }
    let head = &input[..input.len().min(1024)];
    if !head.windows(5).any(|w| w == b"%PDF-") {
        return Err(reject("This is not a PDF file"));
    }
    let options = LoadOptions { max_decompressed_size: Some(limits.max_stream_bytes), ..Default::default() };
    let mut doc = Document::load_mem_with_options(input, options).map_err(|e| PdfError::Malformed(e.to_string()))?;
    if doc.is_encrypted() || doc.was_encrypted() || doc.trailer.has(b"Encrypt") {
        return Err(reject("The PDF is password protected. Remove the password and upload it again."));
    }
    if doc.objects.len() > limits.max_objects {
        return Err(reject("The PDF is too complex"));
    }
    let pages: Vec<_> = doc.get_pages().into_values().collect();
    if pages.is_empty() {
        return Err(reject("The PDF has no pages"));
    }
    if pages.len() > limits.max_pages {
        return Err(reject("The PDF has too many pages"));
    }

    let mut removed = BTreeSet::new();
    {
        let catalog = doc.catalog_mut()?;
        for (key, what) in CATALOG_KEYS {
            if catalog.remove(key).is_some() {
                removed.insert(*what);
            }
        }
    }
    for page in &pages {
        let dict = doc.get_dictionary_mut(*page)?;
        if dict.remove(b"Annots").is_some() {
            removed.insert("annotations and links");
        }
        let extra: Vec<Vec<u8>> = dict.iter().map(|(k, _)| k.clone()).filter(|k| !ALLOWED_PAGE_KEYS.contains(&k.as_slice())).collect();
        for key in extra {
            if key == b"AA" || key == b"PieceInfo" || key == b"Thumb" {
                removed.insert("page actions and extras");
            }
            dict.remove(&key);
        }
    }
    let ids: Vec<_> = doc.objects.keys().copied().collect();
    for id in ids {
        if let Some(object) = doc.objects.get_mut(&id) {
            scrub(object, &mut removed, 0);
        }
    }
    doc.trailer.remove(b"Info");
    doc.prune_objects();
    doc.renumber_objects();
    doc.compress();

    let mut pdf = Vec::with_capacity(input.len());
    doc.save_to(&mut pdf).map_err(|e| PdfError::Malformed(e.to_string()))?;

    let check =
        Document::load_mem_with_options(&pdf, LoadOptions { max_decompressed_size: Some(limits.max_stream_bytes), ..Default::default() })?;
    let boxes = check.get_pages().into_values().map(|p| page_box(&check, p)).collect::<Result<Vec<_>>>()?;
    if boxes.len() != pages.len() {
        return Err(PdfError::Malformed("pages were lost while cleaning the file".into()));
    }
    Ok(Sanitized { pdf, pages: boxes, removed: removed.into_iter().map(String::from).collect() })
}
