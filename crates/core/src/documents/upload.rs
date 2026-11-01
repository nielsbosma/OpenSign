use opensign_pdf::{Limits, PdfError, Sanitized};

use crate::context::App;
use crate::error::{AppError, ErrorKind, Result, bad_request};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceType {
    Pdf,
    Docx,
    Doc,
    Odt,
    Rtf,
}

impl SourceType {
    fn extension(self) -> &'static str {
        match self {
            SourceType::Pdf => "pdf",
            SourceType::Docx => "docx",
            SourceType::Doc => "doc",
            SourceType::Odt => "odt",
            SourceType::Rtf => "rtf",
        }
    }
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

/// Decides by content, never by the file name or the declared content type.
pub fn detect(bytes: &[u8]) -> Result<SourceType> {
    let head = &bytes[..bytes.len().min(1024)];
    if contains(head, b"%PDF-") {
        return Ok(SourceType::Pdf);
    }
    if bytes.starts_with(b"PK\x03\x04") {
        if contains(bytes, b"word/document.xml") {
            return Ok(SourceType::Docx);
        }
        if contains(&bytes[..bytes.len().min(256)], b"application/vnd.oasis.opendocument.text") {
            return Ok(SourceType::Odt);
        }
    }
    if bytes.starts_with(&[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1]) {
        return Ok(SourceType::Doc);
    }
    if bytes.starts_with(b"{\\rtf") {
        return Ok(SourceType::Rtf);
    }
    Err(bad_request("Upload a PDF or a Word document (.docx, .doc, .odt or .rtf)"))
}

/// Converts a word-processor file to PDF in the Gotenberg sidecar. LibreOffice there runs without
/// macros, and the result goes through the same sanitizer as an uploaded PDF.
async fn convert(app: &App, bytes: &[u8], source: SourceType) -> Result<Vec<u8>> {
    let Some(base) = &app.env.converter_url else {
        return Err(AppError::new(
            ErrorKind::Unavailable,
            "Word documents cannot be converted on this server. Upload a PDF, or ask an admin to set CONVERTER_URL.",
        ));
    };
    let part = reqwest::multipart::Part::bytes(bytes.to_vec()).file_name(format!("document.{}", source.extension()));
    let form = reqwest::multipart::Form::new().part("files", part);
    let response = app
        .http
        .internal
        .post(format!("{}/forms/libreoffice/convert", base.trim_end_matches('/')))
        .multipart(form)
        .send()
        .await
        .map_err(|e| AppError::new(ErrorKind::Unavailable, "The document converter is not reachable").with_cause(e))?;
    if !response.status().is_success() {
        return Err(bad_request("This document could not be converted to PDF. Save it as PDF and upload that instead."));
    }
    let pdf = response.bytes().await.map_err(AppError::internal)?;
    Ok(pdf.to_vec())
}

fn pdf_error(error: PdfError) -> AppError {
    match error {
        PdfError::Rejected(message) => bad_request(message),
        PdfError::Malformed(_) => bad_request("This PDF is damaged or uses features OpenSign cannot read. Try printing it to a new PDF."),
        other => AppError::internal(other),
    }
}

/// Turns an upload into a clean PDF: detect, convert when needed, then rewrite without active
/// content. Parsing runs on a blocking thread so a slow file cannot stall the runtime.
pub async fn prepare(app: &App, bytes: Vec<u8>, max_mb: u32) -> Result<Sanitized> {
    let limits = Limits { max_bytes: max_mb.max(1) as usize * 1024 * 1024, ..Limits::default() };
    if bytes.len() > limits.max_bytes {
        return Err(bad_request("The file is larger than {max} MB").params([("max", &max_mb)]));
    }
    let source = detect(&bytes)?;
    let pdf = if source == SourceType::Pdf { bytes } else { convert(app, &bytes, source).await? };
    tokio::task::spawn_blocking(move || opensign_pdf::sanitize(&pdf, limits)).await.map_err(AppError::internal)?.map_err(pdf_error)
}

pub fn pdf_failure(error: PdfError) -> AppError {
    pdf_error(error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_by_content() {
        assert_eq!(detect(b"%PDF-1.7\n...").ok(), Some(SourceType::Pdf));
        assert_eq!(detect(b"PK\x03\x04....word/document.xml....").ok(), Some(SourceType::Docx));
        assert_eq!(detect(&[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1, 0]).ok(), Some(SourceType::Doc));
        assert!(detect(b"MZ\x90\x00 an executable named contract.pdf").is_err());
        assert!(detect(b"PK\x03\x04 a zip of something else").is_err());
    }
}
