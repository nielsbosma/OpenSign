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
    /// `role` and `authentication` as the page shows them, in the document's language. The
    /// attached evidence.json keeps the English values, which tools read.
    #[serde(skip)]
    pub shown_role: String,
    #[serde(skip)]
    pub shown_authentication: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceEvent {
    pub at: String,
    pub text: String,
    /// `text` in the document's language, for the page.
    #[serde(skip)]
    pub shown: String,
}

/// The page's own wording. `{name}` placeholders are filled in.
#[derive(Debug, Clone, Copy)]
pub struct EvidenceLabels {
    pub heading: &'static str,
    /// `{id}`, `{at}`
    pub document: &'static str,
    /// `{url}`
    pub verify_at: &'static str,
    pub parties: &'static str,
    /// `{method}`
    pub identified_by: &'static str,
    /// `{at}`
    pub signed: &'static str,
    /// `{at}`, `{ip}`
    pub signed_from: &'static str,
    pub attachments: &'static str,
    /// `{name}`, `{party}`, `{file}`
    pub attachment: &'static str,
    pub attachments_note: &'static str,
    pub events: &'static str,
    pub integrity: &'static str,
    /// `{hash}`
    pub original_sha256: &'static str,
    /// `{hash}`
    pub signed_sha256: &'static str,
    /// `{hash}`
    pub audit_head: &'static str,
    pub seal_note: &'static str,
    /// `{page}`, `{total}`
    pub footer: &'static str,
}

impl EvidenceLabels {
    pub const EN: EvidenceLabels = EvidenceLabels {
        heading: "Verification",
        document: "Document {id} · completed {at}",
        verify_at: "Verify this document at {url}",
        parties: "Parties",
        identified_by: "Identified by: {method}",
        signed: "Signed {at}",
        signed_from: "Signed {at} from {ip}",
        attachments: "Attachments",
        attachment: "{name} from {party}: {file}",
        attachments_note: "The files are embedded in this PDF. Open the attachments pane of your PDF reader to see them.",
        events: "Events",
        integrity: "Integrity",
        original_sha256: "SHA-256 of the original document: {hash}",
        signed_sha256: "SHA-256 of the signed content before this page: {hash}",
        audit_head: "Audit log chain head: {hash}",
        seal_note: "This file is sealed with an electronic seal (PAdES), with a time stamp when the time stamp authority answered. Any change to it after sealing invalidates the seal. The full evidence is attached as evidence.json.",
        footer: "OpenSign verification · page {page} of {total}",
    };
}

fn fill(template: &str, values: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('}').map(|end| (&after[..end], end)) {
            Some((key, end)) if values.iter().any(|(k, _)| *k == key) => {
                out.push_str(values.iter().find(|(k, _)| *k == key).map(|(_, v)| *v).unwrap_or_default());
                rest = &after[end + 1..];
            }
            _ => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// A file a party uploaded while signing. It is listed in the evidence and embedded in the PDF.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceAttachment {
    pub party: String,
    pub name: String,
    pub file_name: String,
    pub sha256: String,
    #[serde(skip)]
    pub content_type: String,
    #[serde(skip)]
    pub content: Vec<u8>,
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
    pub attachments: Vec<EvidenceAttachment>,
    pub audit_head: Option<String>,
    /// The page's wording, in the document's language.
    #[serde(skip)]
    pub labels: EvidenceLabels,
    /// The organisation's logo as PNG, drawn at the top of the first page.
    #[serde(skip)]
    pub logo_png: Option<Vec<u8>>,
}

const A4: (f64, f64) = (595.28, 841.89);
const MARGIN: f64 = 56.0;
const LOGO_HEIGHT: f64 = 32.0;
const LOGO_MAX_WIDTH: f64 = 180.0;

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
    let mut doc = crate::load::load(pdf)?;
    let mut w = Writer::new();
    let mut images = Vec::new();
    if let Some(logo) = report.logo_png.as_deref()
        && let Ok(id) = crate::stamp::png_image(&mut doc, logo)
    {
        let (width, height) = crate::stamp::image_size(&doc, id);
        let scale = (LOGO_HEIGHT / height).min(LOGO_MAX_WIDTH / width);
        let (w_pt, h_pt) = (width * scale, height * scale);
        w.y -= h_pt;
        let y = w.y;
        w.page().extend(format!("q {w_pt:.2} 0 0 {h_pt:.2} {MARGIN} {y:.2} cm /Logo Do Q\n").into_bytes());
        w.gap(10.0);
        images.push((b"Logo".to_vec(), id));
    }
    let l = &report.labels;
    w.text(FONT_BOLD, 16.0, 0.0, l.heading, 0.1);
    w.gap(4.0);
    w.text(FONT, 9.5, 0.0, &format!("{} · {}", report.title, report.organisation), 0.35);
    w.text(FONT, 9.5, 0.0, &fill(l.document, &[("id", &report.document_id), ("at", &report.completed_at)]), 0.35);
    w.gap(6.0);
    let qr_size = 84.0;
    let qr_ops = qr(&report.verify_url, A4.0 - MARGIN - qr_size, A4.1 - MARGIN - qr_size, qr_size);
    w.page().extend(qr_ops);
    w.text(FONT, 8.5, 0.0, &fill(l.verify_at, &[("url", &report.verify_url)]), 0.35);
    w.rule();

    w.text(FONT_BOLD, 11.0, 0.0, l.parties, 0.1);
    for party in &report.parties {
        w.gap(4.0);
        w.text(FONT_BOLD, 10.0, 0.0, &format!("{} ({})", party.name, party.shown_role), 0.1);
        if let Some(email) = &party.email {
            w.text(FONT, 9.0, 12.0, email, 0.3);
        }
        w.text(FONT, 9.0, 12.0, &fill(l.identified_by, &[("method", &party.shown_authentication)]), 0.3);
        if let Some(identity) = &party.identity {
            w.text(FONT, 9.0, 12.0, identity, 0.3);
        }
        if let Some(at) = &party.signed_at {
            w.text(
                FONT,
                9.0,
                12.0,
                &match party.ip.as_deref() {
                    Some(ip) => fill(l.signed_from, &[("at", at), ("ip", ip)]),
                    None => fill(l.signed, &[("at", at)]),
                },
                0.3,
            );
        }
    }
    if !report.attachments.is_empty() {
        w.rule();
        w.text(FONT_BOLD, 11.0, 0.0, l.attachments, 0.1);
        for a in &report.attachments {
            w.gap(2.0);
            w.text(FONT, 9.0, 0.0, &fill(l.attachment, &[("name", &a.name), ("party", &a.party), ("file", &a.file_name)]), 0.1);
            w.text(FONT, 8.0, 12.0, &format!("SHA-256 {}", a.sha256), 0.35);
        }
        w.text(FONT, 8.5, 0.0, l.attachments_note, 0.35);
    }
    w.rule();
    w.text(FONT_BOLD, 11.0, 0.0, l.events, 0.1);
    for event in &report.events {
        w.text(FONT, 8.5, 0.0, &format!("{}  {}", event.at, event.shown), 0.25);
    }
    w.rule();
    w.text(FONT_BOLD, 11.0, 0.0, l.integrity, 0.1);
    w.text(FONT, 8.5, 0.0, &fill(l.original_sha256, &[("hash", &report.original_sha256)]), 0.25);
    w.text(FONT, 8.5, 0.0, &fill(l.signed_sha256, &[("hash", &report.signed_sha256)]), 0.25);
    if let Some(head) = &report.audit_head {
        w.text(FONT, 8.5, 0.0, &fill(l.audit_head, &[("hash", head)]), 0.25);
    }
    w.text(FONT, 8.5, 0.0, l.seal_note, 0.25);

    let pages_root = doc.catalog()?.get(b"Pages")?.as_reference()?;
    let total = w.pages.len();
    for (i, mut content) in w.pages.into_iter().enumerate() {
        content.extend(text_op(
            FONT,
            7.5,
            MARGIN,
            MARGIN - 20.0,
            &fill(report.labels.footer, &[("page", &(i + 1).to_string()), ("total", &total.to_string())]),
        ));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => Object::Reference(pages_root),
            "MediaBox" => vec![0.into(), 0.into(), Object::Real(A4.0 as f32), Object::Real(A4.1 as f32)],
            "Contents" => Object::Array(vec![]),
        });
        page_resources(&mut doc, page_id, if i == 0 { &images } else { &[] })?;
        append_content(&mut doc, page_id, content)?;
        let root = doc.get_dictionary_mut(pages_root)?;
        let mut kids = root.get(b"Kids")?.as_array()?.clone();
        kids.push(Object::Reference(page_id));
        let count = root.get(b"Count")?.as_i64()? + 1;
        root.set("Kids", kids);
        root.set("Count", count);
    }
    attach_files(&mut doc, report)?;
    let mut out = Vec::new();
    doc.compress();
    doc.save_to(&mut out).map_err(|e| PdfError::Malformed(e.to_string()))?;
    Ok(out)
}

fn embed(
    doc: &mut Document,
    name: &str,
    content_type: &str,
    description: &str,
    relationship: &str,
    content: Vec<u8>,
) -> (Vec<u8>, ObjectId) {
    let size = content.len() as i64;
    let mut stream = Stream::new(
        dictionary! { "Type" => "EmbeddedFile", "Subtype" => Object::Name(content_type.as_bytes().to_vec()), "Params" => dictionary! { "Size" => size } },
        content,
    );
    let _ = stream.compress();
    let file = doc.add_object(stream);
    let name = crate::stamp::winansi(name);
    let spec = doc.add_object(dictionary! {
        "Type" => "Filespec",
        "F" => Object::String(name.clone(), StringFormat::Literal),
        "UF" => Object::String(name.clone(), StringFormat::Literal),
        "AFRelationship" => Object::Name(relationship.as_bytes().to_vec()),
        "Desc" => Object::String(crate::stamp::winansi(description), StringFormat::Literal),
        "EF" => dictionary! { "F" => Object::Reference(file), "UF" => Object::Reference(file) },
    });
    (name, spec)
}

/// Embeds evidence.json and every attachment, in a name tree sorted as the PDF specification requires.
fn attach_files(doc: &mut Document, report: &EvidenceReport) -> Result<()> {
    let json = serde_json::to_vec_pretty(report).map_err(|e| PdfError::Malformed(e.to_string()))?;
    let mut specs = vec![embed(doc, "evidence.json", "application/json", "OpenSign signing evidence", "Data", json)];
    for (i, a) in report.attachments.iter().enumerate() {
        let name = format!("attachment-{:02}-{}", i + 1, a.file_name);
        let description = format!("{} from {}", a.name, a.party);
        specs.push(embed(doc, &name, &a.content_type, &description, "Supplement", a.content.clone()));
    }
    specs.sort_by(|a, b| a.0.cmp(&b.0));
    let names: Vec<Object> =
        specs.iter().flat_map(|(name, spec)| [Object::String(name.clone(), StringFormat::Literal), Object::Reference(*spec)]).collect();
    let catalog = doc.catalog_mut()?;
    catalog.set("Names", dictionary! { "EmbeddedFiles" => dictionary! { "Names" => names } });
    catalog.set("AF", specs.iter().map(|(_, spec)| Object::Reference(*spec)).collect::<Vec<_>>());
    Ok(())
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

#[cfg(test)]
mod fill_tests {
    #[test]
    fn fills_known_placeholders_only() {
        assert_eq!(super::fill("Signed {at} from {ip}", &[("at", "noon"), ("ip", "{at}")]), "Signed noon from {at}");
        assert_eq!(super::fill("{unknown} and {", &[("at", "x")]), "{unknown} and {");
    }
}
