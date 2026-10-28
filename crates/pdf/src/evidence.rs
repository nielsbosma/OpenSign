use lopdf::{Document, Object, ObjectId, Stream, StringFormat, dictionary};
use qrcode::QrCode;
use serde::Serialize;

use crate::stamp::{FONT, FONT_BOLD, append_content, page_resources, text_op, text_width};
use crate::{PdfError, Result};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceParty {
    pub name: String,
    pub email: Option<String>,
    pub role: String,
    pub authentication: String,
    pub signed_at: Option<String>,
    pub ip: Option<String>,
    /// e.g. "Swedish BankID, personal number 19xxxxxx-1234".
    pub identity: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceEvent {
    pub at: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceReport {
    pub document_id: String,
    pub title: String,
    pub organisation: String,
    pub original_sha256: String,
    pub signed_sha256: String,
    pub completed_at: String,
    pub verify_url: String,
    pub parties: Vec<EvidenceParty>,
    pub events: Vec<EvidenceEvent>,
    pub audit_head: Option<String>,
}

const A4: (f64, f64) = (595.28, 841.89);
const MARGIN: f64 = 56.0;

fn wrap(text: &str, size: f64, width: f64) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        let candidate = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
        if text_width(&candidate, size) > width && !line.is_empty() {
            lines.push(std::mem::take(&mut line));
            line = word.to_string();
        } else {
            line = candidate;
        }
        while text_width(&line, size) > width && line.chars().count() > 8 {
            let cut: String = line.chars().take(((width / (size * 0.55)) as usize).max(8)).collect();
            line = line[cut.len()..].to_string();
            lines.push(cut);
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

struct Writer {
    pages: Vec<Vec<u8>>,
    y: f64,
}

impl Writer {
    fn new() -> Self {
        Self { pages: vec![Vec::new()], y: A4.1 - MARGIN }
    }

    fn need(&mut self, height: f64) {
        if self.y - height < MARGIN + 24.0 {
            self.pages.push(Vec::new());
            self.y = A4.1 - MARGIN;
        }
    }

    fn page(&mut self) -> &mut Vec<u8> {
        self.pages.last_mut().expect("at least one page")
    }

    fn text(&mut self, font: &[u8], size: f64, indent: f64, text: &str, gray: f64) {
        for line in wrap(text, size, A4.0 - 2.0 * MARGIN - indent) {
            self.need(size * 1.45);
            self.y -= size * 1.45;
            let y = self.y;
            let mut op = format!("{gray} g ").into_bytes();
            op.extend(text_op(font, size, MARGIN + indent, y, &line));
            self.page().extend(op);
        }
    }

    fn gap(&mut self, h: f64) {
        self.y -= h;
    }

    fn rule(&mut self) {
        self.need(10.0);
        self.y -= 6.0;
        let y = self.y;
        self.page().extend(format!("0.85 G 0.5 w {MARGIN} {y} m {} {y} l S\n", A4.0 - MARGIN).into_bytes());
        self.y -= 6.0;
    }
}

fn qr(url: &str, x: f64, y: f64, size: f64) -> Vec<u8> {
    let Ok(code) = QrCode::new(url.as_bytes()) else { return Vec::new() };
    let width = code.width();
    let cell = size / width as f64;
    let mut out = b"0 g\n".to_vec();
    for (i, dark) in code.to_colors().iter().enumerate() {
        if *dark == qrcode::Color::Dark {
            let (cx, cy) = ((i % width) as f64, (i / width) as f64);
            out.extend(format!("{:.2} {:.2} {:.2} {:.2} re\n", x + cx * cell, y + size - (cy + 1.0) * cell, cell, cell).into_bytes());
        }
    }
    out.extend_from_slice(b"f\n");
    out
}

/// Adds the verification appendix (parties, how each one was identified, the event log, the
/// document hashes, a QR code to the verification page) and attaches the same data as
/// `evidence.json`, so the signed PDF carries its own evidence.
pub fn append_evidence(pdf: &[u8], report: &EvidenceReport) -> Result<Vec<u8>> {
    let mut doc = Document::load_mem(pdf)?;
    let mut w = Writer::new();
    w.text(FONT_BOLD, 16.0, 0.0, "Verification", 0.1);
    w.gap(4.0);
    w.text(FONT, 9.5, 0.0, &format!("{} · {}", report.title, report.organisation), 0.35);
    w.text(FONT, 9.5, 0.0, &format!("Document {} · completed {}", report.document_id, report.completed_at), 0.35);
    w.gap(6.0);
    let qr_size = 84.0;
    let qr_ops = qr(&report.verify_url, A4.0 - MARGIN - qr_size, A4.1 - MARGIN - qr_size, qr_size);
    w.page().extend(qr_ops);
    w.text(FONT, 8.5, 0.0, &format!("Verify this document at {}", report.verify_url), 0.35);
    w.rule();

    w.text(FONT_BOLD, 11.0, 0.0, "Parties", 0.1);
    for party in &report.parties {
        w.gap(4.0);
        w.text(FONT_BOLD, 10.0, 0.0, &format!("{} ({})", party.name, party.role), 0.1);
        if let Some(email) = &party.email {
            w.text(FONT, 9.0, 12.0, email, 0.3);
        }
        w.text(FONT, 9.0, 12.0, &format!("Identified by: {}", party.authentication), 0.3);
        if let Some(identity) = &party.identity {
            w.text(FONT, 9.0, 12.0, identity, 0.3);
        }
        if let Some(at) = &party.signed_at {
            w.text(
                FONT,
                9.0,
                12.0,
                &format!("Signed {at}{}", party.ip.as_deref().map(|ip| format!(" from {ip}")).unwrap_or_default()),
                0.3,
            );
        }
    }
    w.rule();
    w.text(FONT_BOLD, 11.0, 0.0, "Events", 0.1);
    for event in &report.events {
        w.text(FONT, 8.5, 0.0, &format!("{}  {}", event.at, event.text), 0.25);
    }
    w.rule();
    w.text(FONT_BOLD, 11.0, 0.0, "Integrity", 0.1);
    w.text(FONT, 8.5, 0.0, &format!("SHA-256 of the original document: {}", report.original_sha256), 0.25);
    w.text(FONT, 8.5, 0.0, &format!("SHA-256 of the signed content before this page: {}", report.signed_sha256), 0.25);
    if let Some(head) = &report.audit_head {
        w.text(FONT, 8.5, 0.0, &format!("Audit log chain head: {head}"), 0.25);
    }
    w.text(
        FONT,
        8.5,
        0.0,
        "This file is sealed with an electronic seal (PAdES) and a qualified time stamp. Any change to it after sealing invalidates the seal. The full evidence is attached as evidence.json.",
        0.25,
    );

    let pages_root = doc.catalog()?.get(b"Pages")?.as_reference()?;
    let total = w.pages.len();
    for (i, mut content) in w.pages.into_iter().enumerate() {
        content.extend(text_op(FONT, 7.5, MARGIN, MARGIN - 20.0, &format!("OpenSign verification · page {} of {total}", i + 1)));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => Object::Reference(pages_root),
            "MediaBox" => vec![0.into(), 0.into(), Object::Real(A4.0 as f32), Object::Real(A4.1 as f32)],
            "Contents" => Object::Array(vec![]),
        });
        page_resources(&mut doc, page_id, &[])?;
        append_content(&mut doc, page_id, content)?;
        let root = doc.get_dictionary_mut(pages_root)?;
        let mut kids = root.get(b"Kids")?.as_array()?.clone();
        kids.push(Object::Reference(page_id));
        let count = root.get(b"Count")?.as_i64()? + 1;
        root.set("Kids", kids);
        root.set("Count", count);
    }
    attach_json(&mut doc, report)?;
    let mut out = Vec::new();
    doc.compress();
    doc.save_to(&mut out).map_err(|e| PdfError::Malformed(e.to_string()))?;
    Ok(out)
}

fn attach_json(doc: &mut Document, report: &EvidenceReport) -> Result<ObjectId> {
    let json = serde_json::to_vec_pretty(report).map_err(|e| PdfError::Malformed(e.to_string()))?;
    let mut stream = Stream::new(
        dictionary! { "Type" => "EmbeddedFile", "Subtype" => Object::Name(b"application/json".to_vec()), "Params" => dictionary! { "Size" => json.len() as i64 } },
        json,
    );
    let _ = stream.compress();
    let file = doc.add_object(stream);
    let name = Object::String(b"evidence.json".to_vec(), StringFormat::Literal);
    let spec = doc.add_object(dictionary! {
        "Type" => "Filespec",
        "F" => name.clone(),
        "UF" => name.clone(),
        "AFRelationship" => "Data",
        "Desc" => Object::String(b"OpenSign signing evidence".to_vec(), StringFormat::Literal),
        "EF" => dictionary! { "F" => Object::Reference(file), "UF" => Object::Reference(file) },
    });
    let catalog = doc.catalog_mut()?;
    catalog.set("Names", dictionary! { "EmbeddedFiles" => dictionary! { "Names" => vec![name, Object::Reference(spec)] } });
    catalog.set("AF", vec![Object::Reference(spec)]);
    Ok(spec)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_long_lines_and_words() {
        let lines = wrap(&format!("short words then {}", "x".repeat(300)), 9.0, 200.0);
        assert!(lines.len() > 2);
        assert!(lines.iter().all(|l| text_width(l, 9.0) <= 230.0));
    }
}
