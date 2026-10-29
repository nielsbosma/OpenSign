use std::collections::{BTreeMap, HashMap};

use lopdf::content::{Content, Operation};
use lopdf::{Object, Stream};
use serde::Serialize;

use crate::Result;
use crate::text::{Glyph, load, page_content, walk};

const MAX_TAG_CHARS: usize = 1000;
const MAX_TAGS: usize = 1000;

/// A `{{…}}` text tag found on a page. The box is in fractions of the displayed page, origin
/// top-left, covering the tag's text.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextTag {
    /// Zero-based page within the file.
    pub page: usize,
    /// What was between the braces.
    pub text: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    /// The displayed page's size in points, so a tag can ask for a size in points.
    pub page_width: f64,
    pub page_height: f64,
}

#[derive(Debug, Clone, Default)]
pub struct TagScan {
    /// The PDF with the tags' text taken out of the page content, or the input when there were none.
    pub pdf: Vec<u8>,
    pub tags: Vec<TextTag>,
    /// Tags that could not be used, as sentences for the person who uploaded the file.
    pub problems: Vec<String>,
}

/// Finds DocuSeal-style `{{Name;type=…}}` tags and removes their text from the page content
/// stream (not painted over, so nothing hidden is left behind). The text after a tag keeps its
/// place because each removed glyph becomes a TJ move of the same width.
pub fn take_text_tags(pdf: &[u8]) -> Result<TagScan> {
    let mut doc = load(pdf)?;
    let mut tags = Vec::new();
    let mut problems = Vec::new();
    let mut rewrites = Vec::new();
    for (index, (_, page)) in doc.get_pages().into_iter().enumerate() {
        let Some(content) = page_content(&doc, page) else { continue };
        let layout = walk(&doc, page, &content)?;
        let found = find(&layout.glyphs);
        if found.is_empty() {
            continue;
        }
        let mut removals: Vec<&Glyph> = Vec::new();
        for (first, last) in found {
            if tags.len() >= MAX_TAGS {
                problems.push(format!("Only the first {MAX_TAGS} text tags were used"));
                break;
            }
            let glyphs = &layout.glyphs[first..=last];
            let text: String = glyphs.iter().map(|g| g.text.as_str()).collect();
            let inner = text.trim_start_matches('{').trim_end_matches('}').to_string();
            let line = glyphs[0].origin.1;
            let size = glyphs[0].size().max(1.0);
            if glyphs.iter().any(|g| (g.origin.1 - line).abs() > size * 0.6) {
                problems.push(format!(
                    "The tag {{{{{}}}}} on page {} runs over two lines, so it was left as text",
                    short(&inner),
                    index + 1
                ));
                continue;
            }
            if glyphs.iter().any(|g| g.adjustment.is_none()) {
                problems.push(format!(
                    "The tag {{{{{}}}}} on page {} could not be removed, so it was left as text",
                    short(&inner),
                    index + 1
                ));
                continue;
            }
            let mut b = [f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY];
            for g in glyphs {
                let gb = g.bounds();
                b = [b[0].min(gb[0]), b[1].min(gb[1]), b[2].max(gb[2]), b[3].max(gb[3])];
            }
            let x0 = b[0].clamp(0.0, layout.width);
            let y0 = b[1].clamp(0.0, layout.height);
            let x1 = b[2].clamp(0.0, layout.width);
            let y1 = b[3].clamp(0.0, layout.height);
            if x1 - x0 < 0.5 || y1 - y0 < 0.5 {
                problems.push(format!("The tag {{{{{}}}}} on page {} is outside the page", short(&inner), index + 1));
                continue;
            }
            tags.push(TextTag {
                page: index,
                text: inner,
                x: x0 / layout.width,
                y: y0 / layout.height,
                width: (x1 - x0) / layout.width,
                height: (y1 - y0) / layout.height,
                page_width: layout.width,
                page_height: layout.height,
            });
            removals.extend(glyphs);
        }
        if !removals.is_empty() {
            rewrites.push((page, remove(content, &removals)));
        }
    }
    if rewrites.is_empty() {
        return Ok(TagScan { pdf: pdf.to_vec(), tags, problems });
    }
    for (page, content) in rewrites {
        let bytes = content.encode()?;
        let mut stream = Stream::new(lopdf::dictionary! {}, bytes);
        let _ = stream.compress();
        let id = doc.add_object(stream);
        doc.get_dictionary_mut(page)?.set("Contents", Object::Reference(id));
    }
    doc.prune_objects();
    let mut out = Vec::with_capacity(pdf.len());
    doc.save_to(&mut out).map_err(|e| crate::PdfError::Malformed(e.to_string()))?;
    Ok(TagScan { pdf: out, tags, problems })
}

fn short(text: &str) -> String {
    let mut s: String = text.chars().take(40).collect();
    if text.chars().count() > 40 {
        s.push('…');
    }
    s
}

/// Glyph index ranges of each `{{…}}`, innermost when braces nest.
fn find(glyphs: &[Glyph]) -> Vec<(usize, usize)> {
    let chars: Vec<(char, usize)> =
        glyphs.iter().enumerate().flat_map(|(i, g)| g.text.chars().map(move |c| (c, i)).collect::<Vec<_>>()).collect();
    let mut found = Vec::new();
    let mut i = 0;
    while i + 1 < chars.len() {
        if chars[i].0 == '{' && chars[i + 1].0 == '{' {
            let mut start = i;
            let mut j = i + 2;
            let mut closed = None;
            while j < chars.len() && j - start <= MAX_TAG_CHARS {
                if chars[j].0 == '{' && chars.get(j + 1).is_some_and(|c| c.0 == '{') {
                    start = j;
                    j += 2;
                    continue;
                }
                if chars[j].0 == '}' && chars.get(j + 1).is_some_and(|c| c.0 == '}') {
                    closed = Some(j + 1);
                    break;
                }
                j += 1;
            }
            if let Some(end) = closed {
                let inner: String = chars[start + 2..end - 1].iter().map(|c| c.0).collect();
                if !inner.trim().is_empty() {
                    found.push((chars[start].1, chars[end].1));
                }
                i = end + 1;
                continue;
            }
            i = j.max(i + 2);
            continue;
        }
        i += 1;
    }
    found
}

/// Removed glyphs of one operation: (TJ element, first byte) to (end byte, TJ move).
type Removals = BTreeMap<(usize, usize), (usize, f64)>;

/// Rewrites the text operations that drew removed glyphs as TJ arrays with the glyphs replaced
/// by moves of the same width.
fn remove(content: Content, glyphs: &[&Glyph]) -> Content {
    let mut by_op: HashMap<usize, Removals> = HashMap::new();
    for g in glyphs {
        by_op.entry(g.op).or_default().insert((g.part, g.bytes.0), (g.bytes.1, g.adjustment.unwrap_or(0.0)));
    }
    let mut operations = Vec::with_capacity(content.operations.len() + by_op.len());
    for (index, op) in content.operations.into_iter().enumerate() {
        let Some(removed) = by_op.get(&index) else {
            operations.push(op);
            continue;
        };
        let parts: Vec<Object> = match op.operator.as_str() {
            "TJ" => match op.operands.into_iter().next() {
                Some(Object::Array(items)) => items,
                _ => continue,
            },
            "Tj" | "'" | "\"" => {
                if op.operator == "\"" && op.operands.len() == 3 {
                    operations.push(Operation::new("Tw", vec![op.operands[0].clone()]));
                    operations.push(Operation::new("Tc", vec![op.operands[1].clone()]));
                }
                if op.operator != "Tj" {
                    operations.push(Operation::new("T*", vec![]));
                }
                op.operands.into_iter().last().into_iter().collect()
            }
            _ => {
                operations.push(op);
                continue;
            }
        };
        let mut out: Vec<Object> = Vec::new();
        for (part, item) in parts.into_iter().enumerate() {
            let Object::String(bytes, format) = item else {
                out.push(item);
                continue;
            };
            let mut kept = Vec::new();
            let mut i = 0;
            while i < bytes.len() {
                match removed.get(&(part, i)) {
                    Some((end, width)) => {
                        if !kept.is_empty() {
                            out.push(Object::String(std::mem::take(&mut kept), format));
                        }
                        match out.last_mut() {
                            Some(Object::Real(n)) => *n += *width as f32,
                            _ => out.push(Object::Real(*width as f32)),
                        }
                        i = (*end).max(i + 1);
                    }
                    None => {
                        kept.push(bytes[i]);
                        i += 1;
                    }
                }
            }
            if !kept.is_empty() {
                out.push(Object::String(kept, format));
            }
        }
        operations.push(Operation::new("TJ", vec![Object::Array(out)]));
    }
    Content { operations }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use lopdf::{Document, Stream, dictionary};

    use super::*;
    use crate::text::{layout, tests::page_with};

    const WORD: &[u8] = include_bytes!("../fixtures/word-tags.pdf");

    fn glyph_x(pdf: &[u8], needle: &str) -> f64 {
        let pages = layout(&load(pdf).unwrap()).unwrap();
        let glyphs = &pages[0].glyphs;
        let text: Vec<(char, usize)> =
            glyphs.iter().enumerate().flat_map(|(i, g)| g.text.chars().map(move |c| (c, i)).collect::<Vec<_>>()).collect();
        let s: String = text.iter().map(|c| c.0).collect();
        let at = s.find(needle).unwrap_or_else(|| panic!("{needle} not in {s}"));
        let index = s[..at].chars().count();
        glyphs[text[index].1].origin.0
    }

    #[test]
    fn takes_tags_out_of_a_converted_word_file() {
        let scan = take_text_tags(WORD).unwrap();
        let texts: Vec<&str> = scan.tags.iter().map(|t| t.text.as_str()).collect();
        assert_eq!(
            texts,
            vec![
                "Tenant Name;role=Tenant;type=text",
                "Rent;role=Landlord;type=number;format=usd;required=false",
                "Move In;role=Tenant;type=date;format=DD/MM/YYYY",
                "Plan;role=Tenant;type=radio;option=Basic",
                "Plan;role=Tenant;type=radio;option=Plus",
                "Parking;role=Tenant;type=select;options=None,Garage,Street",
                "type=checkbox;role=Tenant",
                "Tenant Sign;type=signature;role=Tenant",
                "Date;type=datenow;role=Tenant",
            ]
        );
        assert!(scan.problems.iter().any(|p| p.contains("Landlord") && p.contains("two lines")), "{:?}", scan.problems);
        let text = Document::load_mem(&scan.pdf).unwrap().extract_text(&[1]).unwrap();
        assert!(!text.contains("type=text") && !text.contains("role=Tenant;type=date"), "{text}");
        assert!(text.contains("house rules") && text.contains("Broken tag"), "{text}");
        assert!(take_text_tags(&scan.pdf).unwrap().tags.is_empty());
        assert!(crate::hidden_text(&scan.pdf).unwrap().is_empty());
        for after in ["per month", "I accept", "stays", "Broken"] {
            assert!((glyph_x(WORD, after) - glyph_x(&scan.pdf, after)).abs() < 0.01, "{after} moved");
        }
        let sign = &scan.tags[7];
        assert!(sign.x > 0.1 && sign.x < 0.9 && sign.width > 0.05 && sign.height > 0.005 && sign.height < 0.03, "{sign:?}");
        assert_eq!((sign.page_width.round(), sign.page_height.round()), (595.0, 842.0));
    }

    #[test]
    fn keeps_text_after_a_tag_in_place() {
        let pdf = page_with(b"BT /F1 12 Tf 72 700 Td (Name: {{Full Name}} here) Tj 0 -20 Td 2 Tw (a {{x}}) ' ET");
        let scan = take_text_tags(&pdf).unwrap();
        assert_eq!(scan.tags.iter().map(|t| t.text.as_str()).collect::<Vec<_>>(), vec!["Full Name", "x"]);
        assert!((glyph_x(&pdf, "here") - glyph_x(&scan.pdf, "here")).abs() < 0.01);
        let text = Document::load_mem(&scan.pdf).unwrap().extract_text(&[1]).unwrap();
        assert!(text.contains("Name:") && text.contains("here") && !text.contains("Full"), "{text}");
        let tag = &scan.tags[0];
        assert!((tag.x * 595.0 - (72.0 + 6.0 * 6.0)).abs() < 1.0, "{tag:?}");
        assert!((tag.height * 842.0 - 12.0).abs() < 0.5);
    }

    #[test]
    fn reads_two_byte_fonts() {
        let mut doc = Document::with_version("1.7");
        let pages_id = doc.new_object_id();
        let cmap = doc.add_object(Stream::new(
            dictionary! {},
            b"begincmap 1 begincodespacerange <0000> <FFFF> endcodespacerange 1 beginbfrange <0001> <0003> [<007B> <007D> <0041>] endbfrange endcmap".to_vec(),
        ));
        let cid = doc.add_object(dictionary! { "Type" => "Font", "Subtype" => "CIDFontType2", "BaseFont" => "X", "DW" => 600, "W" => vec![3.into(), vec![Object::Integer(700)].into()] });
        let font = doc.add_object(dictionary! { "Type" => "Font", "Subtype" => "Type0", "BaseFont" => "X", "Encoding" => "Identity-H", "DescendantFonts" => vec![cid.into()], "ToUnicode" => cmap });
        let content = b"BT /F1 10 Tf 50 50 Td <00010001000300020002> Tj <0003> Tj ET".to_vec();
        let stream = doc.add_object(Stream::new(dictionary! {}, content));
        let page = doc.add_object(dictionary! {
            "Type" => "Page", "Parent" => pages_id, "MediaBox" => vec![0.into(), 0.into(), 200.into(), 100.into()], "Contents" => stream,
            "Resources" => dictionary! { "Font" => dictionary! { "F1" => font } },
        });
        doc.objects.insert(pages_id, Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => vec![page.into()], "Count" => 1 }));
        let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        doc.trailer.set("Root", catalog);
        let mut pdf = Vec::new();
        doc.save_to(&mut pdf).unwrap();
        let scan = take_text_tags(&pdf).unwrap();
        assert_eq!(scan.tags.len(), 1);
        assert_eq!(scan.tags[0].text, "A");
        assert!((scan.tags[0].width * 200.0 - (6.0 * 4.0 + 7.0)).abs() < 0.5, "{:?}", scan.tags[0]);
        assert!((glyph_x(&scan.pdf, "A") - 81.0).abs() < 0.01, "the A after the tag stays where it was");
    }

    #[test]
    fn hostile_content_does_not_hurt() {
        assert!(take_text_tags(b"%PDF-1.7 garbage").is_err());
        let mut content = b"q ".repeat(10_000);
        content.extend_from_slice(b"BT /Missing 12 Tf ({{a}}) Tj /F1 0 Tf ({{b}}) Tj /F1 12 Tf ({{");
        content.extend_from_slice(&b"x".repeat(5000));
        content.extend_from_slice(b"}}) Tj 1e30 0 Td ({{far}}) Tj ET");
        let scan = take_text_tags(&page_with(&content)).unwrap();
        assert!(scan.tags.is_empty(), "{:?}", scan.tags);
        assert!(scan.problems.iter().any(|p| p.contains("could not be removed")), "{:?}", scan.problems);
    }
}
