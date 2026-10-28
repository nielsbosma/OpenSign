use lopdf::{Dictionary, Document, Object, ObjectId};
use serde::Serialize;

use crate::{PdfError, Result};

/// A page's visible area in PDF units (1/72 inch) and its rotation, as the viewer shows it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PageBox {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub rotate: i64,
}

impl PageBox {
    /// Width and height as displayed, after rotation.
    pub fn displayed(&self) -> (f64, f64) {
        if self.rotate % 180 == 0 { (self.width, self.height) } else { (self.height, self.width) }
    }

    /// Maps a box given as fractions of the displayed page (origin top-left) to unrotated page
    /// space, returning the transform for content drawn in a unit square and the box size.
    pub fn place(&self, fx: f64, fy: f64, fw: f64, fh: f64) -> ([f64; 6], f64, f64) {
        let (dw, dh) = self.displayed();
        let (w, h) = (fw * dw, fh * dh);
        let left = fx * dw;
        let bottom = dh - (fy + fh) * dh;
        match self.rotate.rem_euclid(360) {
            90 => ([0.0, 1.0, -1.0, 0.0, self.x + self.width - bottom, self.y + left], w, h),
            180 => ([-1.0, 0.0, 0.0, -1.0, self.x + self.width - left, self.y + self.height - bottom], w, h),
            270 => ([0.0, -1.0, 1.0, 0.0, self.x + bottom, self.y + self.height - left], w, h),
            _ => ([1.0, 0.0, 0.0, 1.0, self.x + left, self.y + bottom], w, h),
        }
    }
}

fn number(object: &Object) -> Option<f64> {
    match object {
        Object::Integer(i) => Some(*i as f64),
        Object::Real(r) => Some(*r as f64),
        _ => None,
    }
}

fn inherited<'a>(doc: &'a Document, mut dict: &'a Dictionary, key: &[u8]) -> Option<&'a Object> {
    for _ in 0..32 {
        if let Ok(value) = dict.get(key) {
            return doc.dereference(value).ok().map(|(_, o)| o);
        }
        let parent = dict.get(b"Parent").ok()?.as_reference().ok()?;
        dict = doc.get_dictionary(parent).ok()?;
    }
    None
}

fn rect(doc: &Document, object: Option<&Object>) -> Option<[f64; 4]> {
    let array = object?.as_array().ok()?;
    if array.len() != 4 {
        return None;
    }
    let mut out = [0.0; 4];
    for (i, item) in array.iter().enumerate() {
        out[i] = number(doc.dereference(item).ok()?.1)?;
    }
    let [a, b, c, d] = out;
    Some([a.min(c), b.min(d), a.max(c), b.max(d)])
}

pub fn page_box(doc: &Document, page: ObjectId) -> Result<PageBox> {
    let dict = doc.get_dictionary(page)?;
    let media = rect(doc, inherited(doc, dict, b"MediaBox")).unwrap_or([0.0, 0.0, 612.0, 792.0]);
    let visible = rect(doc, inherited(doc, dict, b"CropBox"))
        .map(|c| [c[0].max(media[0]), c[1].max(media[1]), c[2].min(media[2]), c[3].min(media[3])])
        .filter(|c| c[2] > c[0] && c[3] > c[1])
        .unwrap_or(media);
    let rotate = inherited(doc, dict, b"Rotate").and_then(number).unwrap_or(0.0) as i64;
    let width = visible[2] - visible[0];
    let height = visible[3] - visible[1];
    if !(1.0..=14_400.0).contains(&width) || !(1.0..=14_400.0).contains(&height) {
        return Err(PdfError::Rejected("A page has an impossible size".into()));
    }
    Ok(PageBox { x: visible[0], y: visible[1], width, height, rotate: rotate.rem_euclid(360) / 90 * 90 })
}

/// Displayed width and height of each page, in order. For files OpenSign produced or sanitized.
pub fn page_sizes(pdf: &[u8]) -> Result<Vec<(f64, f64)>> {
    let doc = Document::load_mem(pdf)?;
    doc.get_pages().values().map(|&page| page_box(&doc, page).map(|b| b.displayed())).collect()
}

/// The text of each page, in order, for review by a language model. Best effort: PDFs drawn as
/// images or with unusual font encodings give little or nothing.
pub fn extract_text(pdf: &[u8]) -> Result<Vec<String>> {
    let doc = Document::load_mem(pdf)?;
    let pages: Vec<u32> = doc.get_pages().keys().copied().collect();
    Ok(pages.iter().map(|&n| doc.extract_text(&[n]).unwrap_or_default()).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn places_boxes_on_unrotated_pages() {
        let page = PageBox { x: 0.0, y: 0.0, width: 600.0, height: 800.0, rotate: 0 };
        let (m, w, h) = page.place(0.5, 0.25, 0.25, 0.125);
        assert_eq!((w, h), (150.0, 100.0));
        assert_eq!(m, [1.0, 0.0, 0.0, 1.0, 300.0, 500.0]);
    }

    #[test]
    fn rotated_pages_swap_the_displayed_size() {
        let page = PageBox { x: 0.0, y: 0.0, width: 600.0, height: 800.0, rotate: 90 };
        assert_eq!(page.displayed(), (800.0, 600.0));
        let (_, w, h) = page.place(0.0, 0.0, 0.5, 0.5);
        assert_eq!((w, h), (400.0, 300.0));
    }
}
