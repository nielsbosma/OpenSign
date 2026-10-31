use std::collections::{BTreeSet, HashSet};

use lopdf::{Dictionary, Document, Object, ObjectId};
use serde::{Deserialize, Serialize};

use crate::form::{FormField, read_form};
use crate::geometry::{PageBox, page_box};
use crate::load::load_with;
use crate::{PdfError, Result};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Sanitized {
    #[serde(skip)]
    pub pdf: Vec<u8>,
    pub pages: Vec<PageBox>,
    /// What was taken out, for the uploader and the audit log.
    pub removed: Vec<String>,
    /// The fillable form's fields, read before the form was taken out.
    pub form: Vec<FormField>,
}

/// Catalog entries that are named to the uploader when they go.
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
    (b"AF", "embedded files"),
];

/// A catalog can name anything a future viewer acts on, so only what is known to be passive stays.
const ALLOWED_CATALOG_KEYS: &[&[u8]] = &[
    b"Type",
    b"Version",
    b"Pages",
    b"PageLabels",
    b"PageLayout",
    b"PageMode",
    b"Outlines",
    b"Dests",
    b"ViewerPreferences",
    b"Metadata",
    b"MarkInfo",
    b"StructTreeRoot",
    b"Lang",
    b"OCProperties",
    b"OutputIntents",
];

/// Keys removed wherever they appear.
const ANYWHERE_KEYS: &[(&[u8], &str)] = &[
    (b"JS", "JavaScript"),
    (b"JavaScript", "JavaScript"),
    (b"AA", "automatic actions"),
    (b"OpenAction", "an action that runs when the file opens"),
    (b"EmbeddedFiles", "embedded files"),
    (b"EF", "embedded files"),
    (b"RF", "embedded files"),
    (b"AF", "embedded files"),
    (b"FS", "file attachments"),
    (b"PA", "actions"),
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

const MAX_INDIRECTION: usize = 16;
const MAX_TREE_DEPTH: usize = 64;

/// A viewer follows a reference to a name, so `/S 15 0 R` is an action although it does not look
/// like one. Putting the names in place means every check below sees what a viewer sees.
fn inline_names(doc: &mut Document) {
    let names: Vec<(ObjectId, Object)> = doc
        .objects
        .keys()
        .filter_map(|id| {
            let mut object = doc.objects.get(id)?;
            for _ in 0..MAX_INDIRECTION {
                match object {
                    Object::Reference(next) => object = doc.objects.get(next)?,
                    Object::Name(_) => return Some((*id, object.clone())),
                    _ => return None,
                }
            }
            None
        })
        .collect();
    if names.is_empty() {
        return;
    }
    let names: std::collections::HashMap<ObjectId, Object> = names.into_iter().collect();
    for object in doc.objects.values_mut() {
        replace_references(object, &names, 0);
    }
}

fn replace_references(object: &mut Object, names: &std::collections::HashMap<ObjectId, Object>, depth: usize) {
    if depth > MAX_TREE_DEPTH {
        return;
    }
    match object {
        Object::Reference(id) => {
            if let Some(name) = names.get(id) {
                *object = name.clone();
            }
        }
        Object::Array(items) => items.iter_mut().for_each(|o| replace_references(o, names, depth + 1)),
        Object::Dictionary(dict) => dict.iter_mut().for_each(|(_, o)| replace_references(o, names, depth + 1)),
        Object::Stream(stream) => stream.dict.iter_mut().for_each(|(_, o)| replace_references(o, names, depth + 1)),
        _ => {}
    }
}

fn scrub(object: &mut Object, removed: &mut BTreeSet<&'static str>, depth: usize) {
    if depth > MAX_TREE_DEPTH {
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

/// `/A` is an action on outlines and links, and also what a font or image in a resource dictionary
/// may be called, so it goes only when it holds an action other than an in-document GoTo. One given
/// by reference is emptied where it stands.
fn is_action(object: &Object) -> bool {
    object
        .as_dict()
        .is_ok_and(|d| d.has(b"S") && d.get(b"S").and_then(Object::as_name).is_ok_and(|s| s != b"GoTo") || d.has_type(b"Action"))
}

fn scrub_dict(dict: &mut Dictionary, removed: &mut BTreeSet<&'static str>, depth: usize) {
    for (key, what) in ANYWHERE_KEYS {
        if dict.remove(key).is_some() {
            removed.insert(what);
        }
    }
    if dict.get(b"A").is_ok_and(is_action) {
        dict.remove(b"A");
        removed.insert("actions");
    }
    let kind = dict.get(b"Type").ok().and_then(|t| t.as_name().ok()).map(<[u8]>::to_vec);
    let action = dict.get(b"S").ok().and_then(|s| s.as_name().ok()).is_some_and(|s| ACTION_TYPES.contains(&s));
    if action {
        removed.insert("actions");
        *dict = Dictionary::new();
        return;
    }
    if matches!(kind.as_deref(), Some(b"Filespec") | Some(b"Action") | Some(b"Annot") | Some(b"Sig") | Some(b"DocTimeStamp")) {
        removed.insert(match kind.as_deref() {
            Some(b"Filespec") => "file attachments",
            Some(b"Annot") => "annotations",
            Some(b"Sig") | Some(b"DocTimeStamp") => "existing signatures",
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

/// Counts the leaves of the page tree. A node lopdf does not take for a page is skipped by
/// `get_pages`, so it would escape the page clean-up below and still be shown by some viewers.
fn page_tree_leaves(doc: &Document) -> Result<usize> {
    let root = doc.catalog()?.get(b"Pages").and_then(Object::as_reference)?;
    let mut seen = HashSet::new();
    let mut stack = vec![(root, 0)];
    let mut leaves = 0;
    while let Some((id, depth)) = stack.pop() {
        if depth > MAX_TREE_DEPTH || !seen.insert(id) {
            return Err(reject("The PDF's page tree is damaged"));
        }
        let node = doc.get_dictionary(id).map_err(|_| reject("The PDF's page tree is damaged"))?;
        match node.get(b"Kids").ok().map(|kids| doc.dereference(kids).map(|(_, o)| o)) {
            Some(Ok(Object::Array(kids))) => {
                for kid in kids {
                    stack.push((kid.as_reference().map_err(|_| reject("The PDF's page tree is damaged"))?, depth + 1));
                }
            }
            Some(_) => return Err(reject("The PDF's page tree is damaged")),
            None => leaves += 1,
        }
    }
    Ok(leaves)
}

/// A reference to nothing reads as null today, but an update appended later could define it and so
/// change a page without changing a single signed object. The clean file refers only to what it has.
fn null_dangling(object: &mut Object, known: &HashSet<ObjectId>, depth: usize) {
    if depth > MAX_TREE_DEPTH {
        return;
    }
    match object {
        Object::Reference(id) if !known.contains(id) => *object = Object::Null,
        Object::Array(items) => items.iter_mut().for_each(|o| null_dangling(o, known, depth + 1)),
        Object::Dictionary(dict) => dict.iter_mut().for_each(|(_, o)| null_dangling(o, known, depth + 1)),
        Object::Stream(stream) => stream.dict.iter_mut().for_each(|(_, o)| null_dangling(o, known, depth + 1)),
        _ => {}
    }
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
    let mut doc = load_with(input, limits.max_stream_bytes)?;
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
    if page_tree_leaves(&doc)? != pages.len() {
        return Err(reject("The PDF's page tree is damaged"));
    }
    // Later steps read this content, and readers will too; better to refuse it here than to fail
    // when the document is about to be sent.
    for page in &pages {
        if doc.get_page_content_with_limit(*page, limits.max_stream_bytes).is_err() {
            return Err(reject("The PDF is too complex"));
        }
    }

    let form = read_form(&doc, &pages);
    let mut removed = BTreeSet::new();
    inline_names(&mut doc);
    {
        let catalog = doc.catalog_mut()?;
        for (key, what) in CATALOG_KEYS {
            if catalog.remove(key).is_some() {
                removed.insert(*what);
            }
        }
        let extra: Vec<Vec<u8>> =
            catalog.iter().map(|(k, _)| k.clone()).filter(|k| !ALLOWED_CATALOG_KEYS.contains(&k.as_slice())).collect();
        for key in extra {
            catalog.remove(&key);
            removed.insert("other document-level entries");
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
    let known: HashSet<ObjectId> = doc.objects.keys().copied().collect();
    doc.objects.values_mut().for_each(|o| null_dangling(o, &known, 0));
    doc.renumber_objects();
    doc.compress();

    let mut pdf = Vec::with_capacity(input.len());
    doc.save_to(&mut pdf).map_err(|e| PdfError::Malformed(e.to_string()))?;

    let check = load_with(&pdf, limits.max_stream_bytes)?;
    let boxes = check.get_pages().into_values().map(|p| page_box(&check, p)).collect::<Result<Vec<_>>>()?;
    if boxes.len() != pages.len() {
        return Err(PdfError::Malformed("pages were lost while cleaning the file".into()));
    }
    Ok(Sanitized { pdf, pages: boxes, removed: removed.into_iter().map(String::from).collect(), form })
}
