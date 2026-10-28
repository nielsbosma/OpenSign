//! Files a party is asked to upload while signing, such as a copy of an ID or a certificate.

use chrono::{DateTime, Utc};
use opensign_pdf::Limits;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sqlx::PgConnection;
use std::collections::HashMap;
use uuid::Uuid;

use crate::error::{AppError, Result, bad_request};

pub const MAX_PER_PARTY: usize = 10;
pub const MAX_BYTES: usize = 10 * 1024 * 1024;
const MAX_PIXELS: u64 = 40_000_000;

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentRequestInput {
    /// What to upload, e.g. "Copy of your ID".
    pub name: String,
    /// Shown to the party under the name.
    #[serde(default)]
    pub description: String,
    /// The party cannot sign without it. Defaults to true.
    pub required: Option<bool>,
}

#[derive(Debug, Clone, Serialize, JsonSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentRequest {
    pub id: Uuid,
    pub party_id: Uuid,
    pub name: String,
    pub description: String,
    pub required: bool,
    pub file_id: Option<Uuid>,
    pub file_name: Option<String>,
    pub content_type: Option<String>,
    pub size_bytes: Option<i64>,
    pub sha256: Option<String>,
    pub uploaded_at: Option<DateTime<Utc>>,
}

const COLUMNS: &str = "r.id, r.party_id, r.name, r.description, r.required, r.file_id, f.name as file_name, f.content_type, f.size_bytes, f.sha256, r.uploaded_at";

pub async fn for_document(conn: &mut PgConnection, document_id: Uuid) -> Result<Vec<AttachmentRequest>> {
    Ok(sqlx::query_as(&format!(
        "select {COLUMNS} from attachment_requests r left join files f on f.id = r.file_id where r.document_id = $1 order by r.party_id, r.position"
    ))
    .bind(document_id)
    .fetch_all(&mut *conn)
    .await?)
}

pub async fn for_party(conn: &mut PgConnection, party_id: Uuid) -> Result<Vec<AttachmentRequest>> {
    Ok(sqlx::query_as(&format!(
        "select {COLUMNS} from attachment_requests r left join files f on f.id = r.file_id where r.party_id = $1 order by r.position"
    ))
    .bind(party_id)
    .fetch_all(&mut *conn)
    .await?)
}

/// Replaces a draft party's requests.
pub async fn set(conn: &mut PgConnection, document_id: Uuid, party_id: Uuid, inputs: &[AttachmentRequestInput]) -> Result<()> {
    if inputs.len() > MAX_PER_PARTY {
        return Err(bad_request(format!("Ask each party for at most {MAX_PER_PARTY} attachments")));
    }
    sqlx::query("delete from attachment_requests where party_id = $1").bind(party_id).execute(&mut *conn).await?;
    for (position, input) in inputs.iter().enumerate() {
        let name = input.name.trim().replace(['\n', '\r', '\t'], " ");
        if name.is_empty() || name.chars().count() > 120 {
            return Err(bad_request("Give each attachment a name of at most 120 characters"));
        }
        let description = input.description.trim();
        if description.chars().count() > 500 {
            return Err(bad_request("Keep attachment descriptions under 500 characters"));
        }
        sqlx::query(
            "insert into attachment_requests (id, document_id, party_id, position, name, description, required) values ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(Uuid::now_v7())
        .bind(document_id)
        .bind(party_id)
        .bind(position as i32)
        .bind(&name)
        .bind(description)
        .bind(input.required.unwrap_or(true))
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

/// Copies a template's requests to a new document, without any uploads.
pub async fn copy(conn: &mut PgConnection, to: Uuid, parties: &HashMap<Uuid, Uuid>, from: Uuid) -> Result<()> {
    for r in for_document(conn, from).await? {
        let Some(party) = parties.get(&r.party_id) else { continue };
        sqlx::query(
            "insert into attachment_requests (id, document_id, party_id, position, name, description, required)
             select $1, $2, $3, position, name, description, required from attachment_requests where id = $4",
        )
        .bind(Uuid::now_v7())
        .bind(to)
        .bind(party)
        .bind(r.id)
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

/// A clean copy of an upload: PDFs are rewritten without active content, images are decoded and
/// encoded again as PNG, which also drops metadata such as GPS positions. Anything else is refused.
pub async fn clean(bytes: Vec<u8>, name: &str) -> Result<(String, &'static str, Vec<u8>)> {
    if bytes.len() > MAX_BYTES {
        return Err(bad_request(format!("Attachments can be at most {} MB", MAX_BYTES / 1024 / 1024)));
    }
    let stem = name.rsplit(['/', '\\']).next().unwrap_or(name).rsplit_once('.').map_or(name, |(s, _)| s);
    let stem: String = stem.chars().filter(|c| !c.is_control()).take(80).collect();
    let stem = if stem.trim().is_empty() { "attachment".to_string() } else { stem.trim().to_string() };
    if bytes[..bytes.len().min(1024)].windows(5).any(|w| w == b"%PDF-") {
        let limits = Limits { max_bytes: MAX_BYTES, max_pages: 50, ..Limits::default() };
        let pdf = tokio::task::spawn_blocking(move || opensign_pdf::sanitize(&bytes, limits))
            .await
            .map_err(AppError::internal)?
            .map_err(super::upload::pdf_failure)?;
        return Ok((format!("{stem}.pdf"), "application/pdf", pdf.pdf));
    }
    let png = tokio::task::spawn_blocking(move || reencode_image(&bytes)).await.map_err(AppError::internal)??;
    Ok((format!("{stem}.png"), "image/png", png))
}

fn unreadable() -> AppError {
    bad_request("Upload a PDF, JPEG or PNG file")
}

fn reencode_image(bytes: &[u8]) -> Result<Vec<u8>> {
    let (width, height, rgb) = if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        let mut decoder = jpeg_decoder::Decoder::new(bytes);
        decoder.read_info().map_err(|_| unreadable())?;
        let info = decoder.info().ok_or_else(unreadable)?;
        if u64::from(info.width) * u64::from(info.height) > MAX_PIXELS {
            return Err(bad_request("The image is too large"));
        }
        let pixels = decoder.decode().map_err(|_| unreadable())?;
        let rgb = match info.pixel_format {
            jpeg_decoder::PixelFormat::RGB24 => pixels,
            jpeg_decoder::PixelFormat::L8 => pixels.iter().flat_map(|&l| [l, l, l]).collect(),
            jpeg_decoder::PixelFormat::L16 => pixels.as_chunks::<2>().0.iter().flat_map(|c| [c[0], c[0], c[0]]).collect(),
            jpeg_decoder::PixelFormat::CMYK32 => pixels
                .as_chunks::<4>()
                .0
                .iter()
                .flat_map(|c| {
                    let k = u16::from(c[3]);
                    [c[0], c[1], c[2]].map(|v| ((u16::from(v) * k) / 255) as u8)
                })
                .collect(),
        };
        (u32::from(info.width), u32::from(info.height), rgb)
    } else if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
        decoder.set_transformations(png::Transformations::normalize_to_color8());
        let header = decoder.read_header_info().map_err(|_| unreadable())?;
        if u64::from(header.width) * u64::from(header.height) > MAX_PIXELS {
            return Err(bad_request("The image is too large"));
        }
        let mut reader = decoder.read_info().map_err(|_| unreadable())?;
        let mut buf = vec![0; reader.output_buffer_size()];
        let frame = reader.next_frame(&mut buf).map_err(|_| unreadable())?;
        let data = &buf[..frame.buffer_size()];
        let rgb = match frame.color_type {
            png::ColorType::Rgb => data.to_vec(),
            png::ColorType::Rgba => data.as_chunks::<4>().0.iter().flat_map(|c| [c[0], c[1], c[2]]).collect(),
            png::ColorType::Grayscale => data.iter().flat_map(|&l| [l, l, l]).collect(),
            png::ColorType::GrayscaleAlpha => data.as_chunks::<2>().0.iter().flat_map(|c| [c[0], c[0], c[0]]).collect(),
            png::ColorType::Indexed => return Err(unreadable()),
        };
        (frame.width, frame.height, rgb)
    } else {
        return Err(unreadable());
    };
    if rgb.len() != width as usize * height as usize * 3 {
        return Err(unreadable());
    }
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, width, height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_compression(png::Compression::Fast);
    let mut writer = encoder.write_header().map_err(AppError::internal)?;
    writer.write_image_data(&rgb).map_err(AppError::internal)?;
    writer.finish().map_err(AppError::internal)?;
    Ok(out)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn png(width: u32, height: u32, color: png::ColorType, data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(color);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.add_text_chunk("Comment".into(), "secret metadata".into()).unwrap();
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(data).unwrap();
        writer.finish().unwrap();
        out
    }

    #[tokio::test]
    async fn images_come_back_as_plain_png() {
        let input = png(2, 1, png::ColorType::Rgba, &[255, 0, 0, 255, 0, 0, 255, 128]);
        let (name, content_type, out) = clean(input, "C:\\Users\\me\\passport scan.png").await.unwrap();
        assert_eq!((name.as_str(), content_type), ("passport scan.png", "image/png"));
        assert!(!out.windows(15).any(|w| w == b"secret metadata"));
        assert!(clean(b"<html><script>alert(1)</script>".to_vec(), "x.png").await.is_err());
        assert!(clean(b"\x89PNG\r\n\x1a\nbroken".to_vec(), "x.png").await.is_err());
    }
}
