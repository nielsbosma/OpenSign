use lopdf::{Dictionary, Document, Object, ObjectId, Stream, dictionary};

use crate::geometry::page_box;
use crate::{PdfError, Result};

pub const FONT: &[u8] = b"OSHelv";
pub const FONT_BOLD: &[u8] = b"OSHelvB";

#[derive(Debug, Clone)]
pub enum StampContent {
    Text(String),
    Check(bool),
    /// A drawn or typed signature as PNG, with the signer's name and a caption under it.
    Signature {
        png: Option<Vec<u8>>,
        name: String,
        caption: String,
    },
}

#[derive(Debug, Clone)]
pub struct Stamp {
    /// Zero-based page index.
    pub page: usize,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub content: StampContent,
}

/// WinAnsi covers Latin-1, which includes the Nordic letters; anything else prints as '?'.
pub fn winansi(text: &str) -> Vec<u8> {
    text.chars().map(winansi_byte).collect()
}

/// WinAnsi bytes escaped for a literal string in a content stream.
pub fn encode_text(text: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len() + 8);
    for byte in winansi(text) {
        if matches!(byte, b'(' | b')' | b'\\') {
            out.push(b'\\');
        }
        out.push(byte);
    }
    out
}

fn winansi_byte(c: char) -> u8 {
    match c {
        '\u{20}'..='\u{7e}' => c as u8,
        '\u{a0}'..='\u{ff}' => c as u32 as u8,
        '\u{20ac}' => 0x80,
        '\u{2013}' => 0x96,
        '\u{2014}' => 0x97,
        '\u{2018}' => 0x91,
        '\u{2019}' => 0x92,
        '\u{201c}' => 0x93,
        '\u{201d}' => 0x94,
        '\u{2022}' => 0x95,
        '\t' | '\n' | '\r' => b' ',
        _ => b'?',
    }
}

/// Rough Helvetica advance width per character in em, good enough to shrink text to fit a box.
pub fn text_width(text: &str, size: f64) -> f64 {
    text.chars()
        .map(|c| match c {
            'i' | 'j' | 'l' | '.' | ',' | ':' | ';' | '\'' | '|' | '!' | ' ' => 0.28,
            'm' | 'w' | 'M' | 'W' | '@' => 0.85,
            'A'..='Z' => 0.67,
            '0'..='9' => 0.556,
            _ => 0.52,
        })
        .sum::<f64>()
        * size
}

pub fn fit_size(text: &str, width: f64, height: f64, max: f64) -> f64 {
    let mut size = max.min(height * 0.8).max(4.0);
    while size > 4.0 && text_width(text, size) > width {
        size -= 0.5;
    }
    size
}

fn font_dict(name: &str) -> Dictionary {
    dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => Object::Name(name.as_bytes().to_vec()), "Encoding" => "WinAnsiEncoding" }
}

/// Gives the page its own Resources dictionary (copying inherited or shared ones) and adds our
/// fonts and images to it, so nothing else in the document changes.
pub fn page_resources(doc: &mut Document, page: ObjectId, images: &[(Vec<u8>, ObjectId)]) -> Result<()> {
    let mut resources = match doc.get_dictionary(page)?.get(b"Resources").ok().cloned() {
        Some(Object::Reference(id)) => doc.get_dictionary(id)?.clone(),
        Some(Object::Dictionary(d)) => d,
        _ => {
            let mut current = doc.get_dictionary(page)?.clone();
            let mut found = Dictionary::new();
            for _ in 0..32 {
                let Ok(parent) = current.get(b"Parent").and_then(|p| p.as_reference()) else { break };
                current = doc.get_dictionary(parent)?.clone();
                if let Ok(r) = current.get(b"Resources") {
                    found = match r {
                        Object::Reference(id) => doc.get_dictionary(*id)?.clone(),
                        Object::Dictionary(d) => d.clone(),
                        _ => Dictionary::new(),
                    };
                    break;
                }
            }
            found
        }
    };
    let sub = |key: &[u8], doc: &Document| -> Result<Dictionary> {
        Ok(match resources.get(key).ok() {
            Some(Object::Reference(id)) => doc.get_dictionary(*id)?.clone(),
            Some(Object::Dictionary(d)) => d.clone(),
            _ => Dictionary::new(),
        })
    };
    let mut fonts = sub(b"Font", doc)?;
    fonts.set(FONT, font_dict("Helvetica"));
    fonts.set(FONT_BOLD, font_dict("Helvetica-Bold"));
    let mut xobjects = sub(b"XObject", doc)?;
    for (name, id) in images {
        xobjects.set(name.clone(), Object::Reference(*id));
    }
    resources.set("Font", fonts);
    resources.set("XObject", xobjects);
    doc.get_dictionary_mut(page)?.set("Resources", resources);
    Ok(())
}

/// Appends drawing to a page, isolated from whatever graphics state the existing content leaves.
pub fn append_content(doc: &mut Document, page: ObjectId, content: Vec<u8>) -> Result<()> {
    let existing: Vec<Object> = match doc.get_dictionary(page)?.get(b"Contents").ok().cloned() {
        Some(Object::Array(items)) => items,
        Some(other @ Object::Reference(_)) => vec![other],
        _ => Vec::new(),
    };
    let open = doc.add_object(Stream::new(Dictionary::new(), b"q\n".to_vec()));
    let mut close = b"\nQ\n".to_vec();
    close.extend_from_slice(&content);
    let mut stream = Stream::new(Dictionary::new(), close);
    let _ = stream.compress();
    let close = doc.add_object(stream);
    let mut contents = vec![Object::Reference(open)];
    contents.extend(existing);
    contents.push(Object::Reference(close));
    doc.get_dictionary_mut(page)?.set("Contents", Object::Array(contents));
    Ok(())
}

/// Checks a signature image when the signer submits it, so a bad image cannot stall sealing later.
pub fn check_signature_image(png_bytes: &[u8]) -> Result<()> {
    png_image(&mut Document::with_version("1.7"), png_bytes).map(|_| ())
}

/// Decodes a PNG (limited size) into an RGB image XObject with an alpha mask.
pub fn png_image(doc: &mut Document, png_bytes: &[u8]) -> Result<ObjectId> {
    let mut decoder = png::Decoder::new(png_bytes);
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    decoder.set_limits(png::Limits { bytes: 16 * 1024 * 1024 });
    let mut reader = decoder.read_info().map_err(|e| PdfError::Rejected(format!("The signature image is not a valid PNG: {e}")))?;
    let (width, height) = (reader.info().width, reader.info().height);
    if width == 0 || height == 0 || width > 4000 || height > 2000 {
        return Err(PdfError::Rejected("The signature image is too large".into()));
    }
    let mut buffer = vec![0; reader.output_buffer_size()];
    let frame = reader.next_frame(&mut buffer).map_err(|e| PdfError::Rejected(format!("The signature image is not a valid PNG: {e}")))?;
    let data = &buffer[..frame.buffer_size()];
    let (mut rgb, mut alpha) = (Vec::with_capacity((width * height * 3) as usize), Vec::with_capacity((width * height) as usize));
    match frame.color_type {
        png::ColorType::Rgba => data.as_chunks::<4>().0.iter().for_each(|p| {
            rgb.extend_from_slice(&p[..3]);
            alpha.push(p[3]);
        }),
        png::ColorType::Rgb => data.as_chunks::<3>().0.iter().for_each(|p| {
            rgb.extend_from_slice(p);
            alpha.push(255);
        }),
        png::ColorType::GrayscaleAlpha => data.as_chunks::<2>().0.iter().for_each(|p| {
            rgb.extend_from_slice(&[p[0], p[0], p[0]]);
            alpha.push(p[1]);
        }),
        png::ColorType::Grayscale => data.iter().for_each(|g| {
            rgb.extend_from_slice(&[*g, *g, *g]);
            alpha.push(255);
        }),
        png::ColorType::Indexed => return Err(PdfError::Rejected("Unsupported PNG colour type".into())),
    }
    let mut mask = Stream::new(
        dictionary! { "Type" => "XObject", "Subtype" => "Image", "Width" => width as i64, "Height" => height as i64, "ColorSpace" => "DeviceGray", "BitsPerComponent" => 8 },
        alpha,
    );
    let _ = mask.compress();
    let mask = doc.add_object(mask);
    let mut image = Stream::new(
        dictionary! {
            "Type" => "XObject", "Subtype" => "Image", "Width" => width as i64, "Height" => height as i64,
            "ColorSpace" => "DeviceRGB", "BitsPerComponent" => 8, "SMask" => Object::Reference(mask),
        },
        rgb,
    );
    let _ = image.compress();
    Ok(doc.add_object(image))
}

fn fmt(n: f64) -> String {
    let s = format!("{n:.3}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

pub fn text_op(font: &[u8], size: f64, x: f64, y: f64, text: &str) -> Vec<u8> {
    let mut out = format!("BT /{} {} Tf {} {} Td (", String::from_utf8_lossy(font), fmt(size), fmt(x), fmt(y)).into_bytes();
    out.extend(encode_text(text));
    out.extend_from_slice(b") Tj ET\n");
    out
}

/// Draws field values and signatures onto the pages. Fields are given as fractions of the
/// displayed page with the origin top-left, the way the web editor places them.
pub fn stamp(pdf: &[u8], stamps: &[Stamp]) -> Result<Vec<u8>> {
    let mut doc = Document::load_mem(pdf)?;
    let pages: Vec<ObjectId> = doc.get_pages().into_values().collect();
    let mut by_page: std::collections::BTreeMap<usize, Vec<&Stamp>> = Default::default();
    for s in stamps {
        if s.page >= pages.len() {
            return Err(PdfError::Rejected(format!("A field is on page {} but the document has {} pages", s.page + 1, pages.len())));
        }
        if !(0.0..=1.0).contains(&s.x)
            || !(0.0..=1.0).contains(&s.y)
            || s.width <= 0.0
            || s.height <= 0.0
            || s.x + s.width > 1.0001
            || s.y + s.height > 1.0001
        {
            return Err(PdfError::Rejected("A field lies outside its page".into()));
        }
        by_page.entry(s.page).or_default().push(s);
    }
    for (index, stamps) in by_page {
        let page = pages[index];
        let geometry = page_box(&doc, page)?;
        let mut content = Vec::new();
        let mut images = Vec::new();
        for s in stamps {
            let (m, w, h) = geometry.place(s.x, s.y, s.width, s.height);
            content.extend(
                format!("q {} {} {} {} {} {} cm 0 0 0.2 rg\n", fmt(m[0]), fmt(m[1]), fmt(m[2]), fmt(m[3]), fmt(m[4]), fmt(m[5]))
                    .into_bytes(),
            );
            match &s.content {
                StampContent::Text(text) => {
                    let size = fit_size(text, w - 2.0, h, 11.0);
                    content.extend(text_op(FONT, size, 1.0, (h - size) / 2.0 + size * 0.22, text));
                }
                StampContent::Check(checked) => {
                    let side = w.min(h);
                    content.extend(format!("0.8 w 0 0 0.2 RG 0 0 {} {} re S\n", fmt(side), fmt(side)).into_bytes());
                    if *checked {
                        content.extend(
                            format!(
                                "1.5 w {} {} m {} {} l {} {} l S\n",
                                fmt(side * 0.2),
                                fmt(side * 0.5),
                                fmt(side * 0.42),
                                fmt(side * 0.22),
                                fmt(side * 0.82),
                                fmt(side * 0.8)
                            )
                            .into_bytes(),
                        );
                    }
                }
                StampContent::Signature { png, name, caption } => {
                    let caption_size = (h * 0.13).clamp(5.0, 8.0);
                    let name_size = (h * 0.16).clamp(6.0, 10.0);
                    let art_bottom = caption_size * 2.0 + name_size + 4.0;
                    let art_height = (h - art_bottom).max(h * 0.4);
                    if let Some(png) = png {
                        let id = png_image(&mut doc, png)?;
                        let key = format!("OSImg{}", images.len()).into_bytes();
                        let (iw, ih) = image_size(&doc, id);
                        let scale = (w / iw).min(art_height / ih);
                        let (dw, dh) = (iw * scale, ih * scale);
                        content.extend(
                            format!("q {} 0 0 {} 0 {} cm /{} Do Q\n", fmt(dw), fmt(dh), fmt(art_bottom), String::from_utf8_lossy(&key))
                                .into_bytes(),
                        );
                        images.push((key, id));
                    } else {
                        let size = fit_size(name, w, art_height, 22.0);
                        content.extend(text_op(FONT, size, 0.0, art_bottom + 2.0, name));
                    }
                    content.extend(format!("0.5 w 0 {} m {} {} l S\n", fmt(art_bottom - 1.0), fmt(w), fmt(art_bottom - 1.0)).into_bytes());
                    content.extend(text_op(
                        FONT_BOLD,
                        name_size.min(fit_size(name, w, name_size * 1.25, name_size)),
                        0.0,
                        caption_size * 2.0 + 1.0,
                        name,
                    ));
                    content.extend(text_op(FONT, fit_size(caption, w, caption_size * 1.25, caption_size), 0.0, 1.0, caption));
                }
            }
            content.extend_from_slice(b"Q\n");
        }
        page_resources(&mut doc, page, &images)?;
        append_content(&mut doc, page, content)?;
    }
    let mut out = Vec::with_capacity(pdf.len() + 4096);
    doc.compress();
    doc.save_to(&mut out).map_err(|e| PdfError::Malformed(e.to_string()))?;
    Ok(out)
}

fn image_size(doc: &Document, id: ObjectId) -> (f64, f64) {
    let dict = doc.get_object(id).ok().and_then(|o| o.as_stream().ok()).map(|s| s.dict.clone()).unwrap_or_default();
    let get = |k: &[u8]| dict.get(k).ok().and_then(|v| v.as_i64().ok()).unwrap_or(1) as f64;
    (get(b"Width"), get(b"Height"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_and_escapes_text() {
        assert_eq!(encode_text("(å)"), b"\\(\xe5\\)".to_vec());
        assert_eq!(encode_text("€ ✓"), b"\x80 ?".to_vec());
    }

    #[test]
    fn shrinks_long_text() {
        assert!(fit_size("A rather long company name AB", 80.0, 20.0, 11.0) < 11.0);
        assert_eq!(fit_size("Hi", 80.0, 20.0, 11.0), 11.0);
    }
}
