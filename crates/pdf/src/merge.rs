use lopdf::{Dictionary, Document, Object, ObjectId, dictionary};

use crate::{PdfError, Result};

const INHERITED: &[&[u8]] = &[b"Resources", b"MediaBox", b"CropBox", b"Rotate"];

fn inherited(doc: &Document, page: ObjectId) -> Dictionary {
    let mut found = Dictionary::new();
    let mut current = doc.get_dictionary(page).ok().and_then(|d| d.get(b"Parent").ok()).and_then(|p| p.as_reference().ok());
    let mut depth = 0;
    while let Some(id) = current {
        let Ok(node) = doc.get_dictionary(id) else { break };
        for key in INHERITED {
            if !found.has(key)
                && let Ok(value) = node.get(key)
            {
                found.set(*key, value.clone());
            }
        }
        current = node.get(b"Parent").ok().and_then(|p| p.as_reference().ok());
        depth += 1;
        if depth > 64 {
            break;
        }
    }
    found
}

/// Concatenates sanitized PDFs into one, in order. Inherited page attributes are copied onto each
/// page so pages keep their size and resources under the new page tree.
pub fn merge(files: &[Vec<u8>]) -> Result<Vec<u8>> {
    match files {
        [] => return Err(PdfError::Rejected("There is nothing to merge".into())),
        [only] => return Ok(only.clone()),
        _ => {}
    }
    let mut out = Document::with_version("1.7");
    let pages_root = out.new_object_id();
    let mut kids = Vec::new();
    for bytes in files {
        let mut doc = Document::load_mem(bytes)?;
        doc.renumber_objects_with(out.max_id + 1);
        let pages: Vec<ObjectId> = doc.get_pages().into_values().collect();
        for page in &pages {
            let extra = inherited(&doc, *page);
            let dict = doc.get_dictionary_mut(*page)?;
            for (key, value) in extra.iter() {
                if !dict.has(key) {
                    dict.set(key.clone(), value.clone());
                }
            }
            dict.set("Parent", pages_root);
        }
        out.max_id = doc.max_id;
        out.objects.extend(doc.objects);
        kids.extend(pages.into_iter().map(Object::Reference));
    }
    let count = kids.len() as i64;
    out.objects.insert(pages_root, Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => kids, "Count" => count }));
    let catalog = out.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_root });
    out.trailer.set("Root", catalog);
    out.prune_objects();
    out.compress();
    let mut bytes = Vec::new();
    out.save_to(&mut bytes).map_err(|e| PdfError::Malformed(e.to_string()))?;
    Ok(bytes)
}
