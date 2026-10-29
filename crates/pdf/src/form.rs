use std::collections::{HashMap, HashSet};

use lopdf::{Dictionary, Document, Object, ObjectId};
use serde::Serialize;

use crate::geometry::{PageBox, page_box};

const MAX_FORM_FIELDS: usize = 1000;
const MAX_DEPTH: usize = 16;
const MAX_NODES: usize = 20_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FormKind {
    Text,
    Checkbox,
    Radio,
    Select,
    Multiselect,
    Signature,
}

/// One widget of a fillable PDF form, read before the form is removed. The box is in fractions of
/// the displayed page, origin top-left.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FormField {
    /// The field's full name, parts joined with dots.
    pub name: String,
    pub kind: FormKind,
    /// Zero-based page within the file.
    pub page: usize,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    /// The choices of a list, or the one option this radio button stands for.
    pub options: Vec<String>,
    /// The value it was filled in with: text, the chosen option, or "true" for a ticked box.
    pub value: Option<String>,
    pub required: bool,
    pub read_only: bool,
}

/// PDF text strings are UTF-16BE with a byte order mark, or PDFDocEncoding (close to Latin-1).
pub(crate) fn text_string(bytes: &[u8]) -> String {
    let text = if bytes.starts_with(&[0xFE, 0xFF]) {
        let units: Vec<u16> = bytes[2..].chunks(2).map(|c| u16::from_be_bytes([c[0], *c.get(1).unwrap_or(&0)])).collect();
        String::from_utf16_lossy(&units)
    } else if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        String::from_utf8_lossy(rest).into_owned()
    } else {
        bytes.iter().map(|b| *b as char).collect()
    };
    text.chars().filter(|c| !c.is_control() || *c == ' ').collect::<String>().trim().to_string()
}

fn resolve<'a>(doc: &'a Document, object: &'a Object) -> Option<&'a Object> {
    doc.dereference(object).ok().map(|(_, o)| o)
}

fn get<'a>(doc: &'a Document, dict: &'a Dictionary, key: &[u8]) -> Option<&'a Object> {
    resolve(doc, dict.get(key).ok()?)
}

fn string(doc: &Document, dict: &Dictionary, key: &[u8]) -> Option<String> {
    match get(doc, dict, key)? {
        Object::String(bytes, _) => Some(text_string(bytes)),
        Object::Name(name) => Some(text_string(name)),
        _ => None,
    }
}

#[derive(Clone, Default)]
struct Inherited {
    name: Vec<String>,
    kind: Option<Vec<u8>>,
    flags: i64,
    value: Option<Object>,
    options: Option<Vec<String>>,
}

struct Reader<'a> {
    doc: &'a Document,
    pages: Vec<(ObjectId, PageBox)>,
    annot_page: HashMap<ObjectId, usize>,
    seen: HashSet<ObjectId>,
    nodes: usize,
    out: Vec<FormField>,
}

impl Reader<'_> {
    fn node(&mut self, object: &Object, parent: &Inherited, depth: usize) {
        if depth > MAX_DEPTH || self.nodes >= MAX_NODES || self.out.len() >= MAX_FORM_FIELDS {
            return;
        }
        self.nodes += 1;
        let id = object.as_reference().ok();
        if let Some(id) = id
            && !self.seen.insert(id)
        {
            return;
        }
        let doc = self.doc;
        let Some(Object::Dictionary(dict)) = resolve(doc, object) else { return };
        let mut here = parent.clone();
        if let Some(part) = string(doc, dict, b"T").filter(|t| !t.is_empty()) {
            here.name.push(part);
        }
        if let Some(Object::Name(kind)) = get(doc, dict, b"FT") {
            here.kind = Some(kind.clone());
        }
        if let Some(Object::Integer(flags)) = get(doc, dict, b"Ff") {
            here.flags = *flags;
        }
        if let Some(value) = get(doc, dict, b"V") {
            here.value = Some(value.clone());
        }
        if let Some(Object::Array(items)) = get(doc, dict, b"Opt") {
            here.options = Some(
                items
                    .iter()
                    .take(500)
                    .filter_map(|item| match resolve(doc, item)? {
                        Object::String(bytes, _) => Some(text_string(bytes)),
                        Object::Array(pair) => match pair.get(1).and_then(|o| resolve(doc, o)) {
                            Some(Object::String(bytes, _)) => Some(text_string(bytes)),
                            _ => None,
                        },
                        _ => None,
                    })
                    .collect(),
            );
        }
        let kids: Vec<Object> = match get(doc, dict, b"Kids") {
            Some(Object::Array(kids)) => kids.clone(),
            _ => Vec::new(),
        };
        if kids.is_empty() {
            self.widget(id, dict, &here, 0);
            return;
        }
        let mut widget_index = 0;
        for kid in kids.iter().take(MAX_FORM_FIELDS) {
            let is_field = matches!(resolve(doc, kid), Some(Object::Dictionary(d)) if d.has(b"T") || d.has(b"Kids"));
            if is_field {
                self.node(kid, &here, depth + 1);
            } else if let Some(Object::Dictionary(widget)) = resolve(doc, kid) {
                self.nodes += 1;
                self.widget(kid.as_reference().ok(), widget, &here, widget_index);
                widget_index += 1;
            }
        }
    }

    fn widget(&mut self, id: Option<ObjectId>, widget: &Dictionary, field: &Inherited, index: usize) {
        if self.out.len() >= MAX_FORM_FIELDS {
            return;
        }
        let doc = self.doc;
        let hidden = matches!(get(doc, widget, b"F"), Some(Object::Integer(f)) if f & 0b10 != 0);
        let flags = field.flags;
        let kind = match field.kind.as_deref() {
            Some(b"Tx") => FormKind::Text,
            Some(b"Sig") => FormKind::Signature,
            Some(b"Btn") if flags & (1 << 16) != 0 => return,
            Some(b"Btn") if flags & (1 << 15) != 0 => FormKind::Radio,
            Some(b"Btn") => FormKind::Checkbox,
            Some(b"Ch") if flags & (1 << 17) == 0 && flags & (1 << 21) != 0 => FormKind::Multiselect,
            Some(b"Ch") => FormKind::Select,
            _ => return,
        };
        if hidden {
            return;
        }
        let page = match widget.get(b"P").ok().and_then(|p| p.as_reference().ok()) {
            Some(p) => self.pages.iter().position(|(id, _)| *id == p),
            None => None,
        }
        .or_else(|| id.and_then(|id| self.annot_page.get(&id).copied()));
        let Some(page) = page else { return };
        let Some(rect) = get(doc, widget, b"Rect").and_then(|r| r.as_array().ok()).filter(|r| r.len() == 4) else { return };
        let n: Vec<f64> = rect.iter().filter_map(|o| resolve(doc, o)).filter_map(|o| o.as_float().ok()).map(f64::from).collect();
        if n.len() != 4 || n.iter().any(|v| !v.is_finite()) {
            return;
        }
        let page_box = self.pages[page].1;
        let (w, h) = page_box.displayed();
        let corners = [page_box.to_display(n[0], n[1]), page_box.to_display(n[2], n[3])];
        let x0 = corners[0].0.min(corners[1].0).clamp(0.0, w);
        let x1 = corners[0].0.max(corners[1].0).clamp(0.0, w);
        let y0 = corners[0].1.min(corners[1].1).clamp(0.0, h);
        let y1 = corners[0].1.max(corners[1].1).clamp(0.0, h);
        if x1 - x0 < 2.0 || y1 - y0 < 2.0 {
            return;
        }
        let on_state = get(doc, widget, b"AP")
            .and_then(|ap| ap.as_dict().ok())
            .and_then(|ap| get(doc, ap, b"N"))
            .and_then(|n| n.as_dict().ok())
            .and_then(|n| n.iter().map(|(k, _)| k).find(|k| k.as_slice() != b"Off").cloned());
        let value_name = match &field.value {
            Some(Object::Name(n)) => Some(n.clone()),
            Some(Object::String(s, _)) => Some(s.clone()),
            _ => None,
        };
        let (options, value) = match kind {
            FormKind::Radio => {
                let option = field
                    .options
                    .as_ref()
                    .and_then(|o| o.get(index).cloned())
                    .or_else(|| on_state.as_deref().map(text_string))
                    .filter(|o| !o.is_empty())
                    .unwrap_or_else(|| format!("{}", index + 1));
                let chosen = on_state.is_some() && on_state == value_name;
                (vec![option.clone()], chosen.then_some(option))
            }
            FormKind::Checkbox => {
                let ticked = value_name.as_deref().is_some_and(|v| v != b"Off" && !v.is_empty());
                (Vec::new(), ticked.then(|| "true".to_string()))
            }
            FormKind::Select | FormKind::Multiselect => {
                let options = field.options.clone().unwrap_or_default();
                let value = match &field.value {
                    Some(Object::Array(items)) if kind == FormKind::Multiselect => {
                        let chosen: Vec<String> = items
                            .iter()
                            .filter_map(|o| match o {
                                Object::String(b, _) => Some(text_string(b)),
                                _ => None,
                            })
                            .collect();
                        (!chosen.is_empty()).then(|| serde_json::to_string(&chosen).unwrap_or_default())
                    }
                    Some(Object::String(b, _)) => {
                        let v = text_string(b);
                        if kind == FormKind::Multiselect { Some(serde_json::to_string(&[v]).unwrap_or_default()) } else { Some(v) }
                    }
                    _ => None,
                };
                (options, value.filter(|v| !v.is_empty()))
            }
            FormKind::Text => {
                let value = match &field.value {
                    Some(Object::String(b, _)) => Some(text_string(b)).filter(|v| !v.is_empty()),
                    _ => None,
                };
                (Vec::new(), value)
            }
            FormKind::Signature => (Vec::new(), None),
        };
        self.out.push(FormField {
            name: field.name.join("."),
            kind,
            page,
            x: x0 / w,
            y: y0 / h,
            width: (x1 - x0) / w,
            height: (y1 - y0) / h,
            options,
            value,
            required: flags & 0b10 != 0,
            read_only: flags & 0b1 != 0,
        });
    }
}

/// The widgets of the document's AcroForm. Anything malformed is skipped; nothing here fails.
pub(crate) fn read_form(doc: &Document, pages: &[ObjectId]) -> Vec<FormField> {
    let Some(form) = doc.catalog().ok().and_then(|c| get(doc, c, b"AcroForm")).and_then(|f| f.as_dict().ok()) else {
        return Vec::new();
    };
    let Some(Object::Array(fields)) = get(doc, form, b"Fields") else { return Vec::new() };
    let mut annot_page = HashMap::new();
    let mut boxes = Vec::new();
    for (index, page) in pages.iter().enumerate() {
        let Ok(page_box) = page_box(doc, *page) else { return Vec::new() };
        boxes.push((*page, page_box));
        if let Some(Object::Array(annots)) = doc.get_dictionary(*page).ok().and_then(|d| get(doc, d, b"Annots")) {
            for annot in annots.iter().take(MAX_NODES) {
                if let Ok(id) = annot.as_reference() {
                    annot_page.entry(id).or_insert(index);
                }
            }
        }
    }
    let mut reader = Reader { doc, pages: boxes, annot_page, seen: HashSet::new(), nodes: 0, out: Vec::new() };
    for field in fields.clone().iter() {
        reader.node(field, &Inherited::default(), 0);
    }
    reader.out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::{Limits, sanitize};

    #[test]
    fn reads_a_filled_form_before_it_is_removed() {
        let clean = sanitize(include_bytes!("../fixtures/acroform.pdf"), Limits::default()).unwrap();
        assert!(clean.removed.iter().any(|r| r == "form fields"));
        let fields: Vec<(&str, FormKind, Option<&str>, Vec<&str>)> = clean
            .form
            .iter()
            .map(|f| (f.name.as_str(), f.kind, f.value.as_deref(), f.options.iter().map(String::as_str).collect()))
            .collect();
        assert_eq!(
            fields,
            vec![
                ("Tenant.Name", FormKind::Text, Some("Anna Svensson"), vec![]),
                ("Notes", FormKind::Text, None, vec![]),
                ("Pets", FormKind::Checkbox, Some("true"), vec![]),
                ("Plan", FormKind::Radio, None, vec!["Basic"]),
                ("Plan", FormKind::Radio, Some("Plus"), vec!["Plus"]),
                ("Parking", FormKind::Select, Some("Garage"), vec!["None", "Garage", "Street"]),
                ("Extras", FormKind::Multiselect, Some("[\"Sauna\"]"), vec!["Storage", "Bike room", "Sauna"]),
            ]
        );
        assert!(clean.form[1].required);
        let name = &clean.form[0];
        assert!((name.x - 170.0 / 595.2756).abs() < 0.001 && (name.y - (841.8898 - 772.0) / 841.8898).abs() < 0.001, "{name:?}");
    }

    #[test]
    fn decodes_text_strings() {
        assert_eq!(text_string(&[0xFE, 0xFF, 0, b'h', 0, 0xE5]), "hå");
        assert_eq!(text_string(b"Name\x07 "), "Name");
        assert_eq!(text_string(&[b'G', 0xF6, b'r']), "Gör");
    }
}
