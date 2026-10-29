use async_trait::async_trait;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sqlx::PgConnection;
use uuid::Uuid;

use super::{Empty, settings};
use crate::audit::{Entry, audit};
use crate::context::Ctx;
use crate::documents::attachments::reencode_image_with_alpha;
use crate::error::{AppError, Result, bad_request, not_found};
use crate::permissions::Permission;
use crate::service::{Access, Meta, Service};
use crate::settings::OrgSettings;
use crate::storage::{FileRecord, NewFile, file_record, store_file};

pub const LOGO_KIND: &str = "logo";
pub const LOGO_MAX_BYTES: usize = 2 * 1024 * 1024;
const LOGO_MIN_SIDE: u32 = 16;
const LOGO_MAX_SIDE: u32 = 2000;

/// Where the logo is served, relative to the app. The file id is in the path, so a new logo gets
/// a new address and caches never show an old one.
pub fn logo_path(org: &OrgSettings) -> Option<String> {
    org.logo_file_id.map(|id| format!("/api/logo/{}.png", id.simple()))
}

/// A safe copy of an uploaded logo: a PNG or JPEG of sensible size, decoded and encoded again as
/// PNG with its transparency, which drops metadata and anything hidden in the file. SVG is refused
/// because it can carry script.
pub async fn clean_logo(bytes: Vec<u8>) -> Result<Vec<u8>> {
    if bytes.len() > LOGO_MAX_BYTES {
        return Err(bad_request(format!("The logo can be at most {} MB", LOGO_MAX_BYTES / 1024 / 1024)));
    }
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(512)]).to_ascii_lowercase();
    if head.contains("<svg") || head.trim_start().starts_with("<?xml") {
        return Err(bad_request("SVG logos are not accepted because they can contain script. Upload the logo as a PNG or JPEG"));
    }
    if !bytes.starts_with(&[0xFF, 0xD8, 0xFF]) && !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err(bad_request("The logo must be a PNG or JPEG image"));
    }
    let (width, height, png) =
        tokio::task::spawn_blocking(move || reencode_image_with_alpha(&bytes)).await.map_err(AppError::internal)??;
    if width.min(height) < LOGO_MIN_SIDE || width.max(height) > LOGO_MAX_SIDE {
        return Err(bad_request(format!("The logo must be between {LOGO_MIN_SIDE} and {LOGO_MAX_SIDE} pixels on each side")));
    }
    Ok(png)
}

/// The current logo's file, if `id` is it. Earlier logos and every other file are not served.
pub async fn logo_file(ctx: &Ctx, conn: &mut PgConnection, id: Uuid) -> Result<FileRecord> {
    let org: OrgSettings = settings(ctx, conn).await?;
    if org.logo_file_id != Some(id) {
        return Err(not_found("Logo"));
    }
    let record = file_record(conn, id).await?;
    if record.kind != LOGO_KIND || record.content_type != "image/png" {
        return Err(not_found("Logo"));
    }
    Ok(record)
}

/// The logo as PNG bytes, for the evidence page. A missing or unreadable logo is left out.
pub async fn logo_png(ctx: &Ctx, conn: &mut PgConnection) -> Option<Vec<u8>> {
    let org: OrgSettings = settings(ctx, conn).await.ok()?;
    let record = logo_file(ctx, conn, org.logo_file_id?).await.ok()?;
    crate::storage::read_file(&ctx.app.storage, ctx.keys(), &record).await.ok()
}

#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Branding {
    pub organisation: String,
    pub brand_color: String,
    /// Relative to the app, such as `/api/logo/<id>.png`. Unset when no logo is uploaded.
    pub logo_url: Option<String>,
}

fn branding(org: &OrgSettings) -> Branding {
    Branding {
        organisation: if org.name.is_empty() { "OpenSign".into() } else { org.name.clone() },
        brand_color: org.brand_color.clone(),
        logo_url: logo_path(org),
    }
}

pub struct Get;

#[async_trait]
impl Service for Get {
    type Input = Empty;
    type Output = Branding;
    const META: Meta = Meta::query(
        "branding.get",
        "The organisation's name, brand color and logo, as shown to signers and on public pages.",
        Access::Public,
    );

    async fn run(ctx: &Ctx, conn: &mut PgConnection, _: Empty) -> Result<Branding> {
        Ok(branding(&settings(ctx, conn).await?))
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SetLogoInput {
    /// The image's bytes, base64. PNG or JPEG, at most 2 MB and 2000 pixels on a side. SVG is refused.
    pub content_base64: String,
}

async fn replace_logo(ctx: &Ctx, conn: &mut PgConnection, new: Option<Uuid>) -> Result<(OrgSettings, Option<Uuid>)> {
    let mut org: OrgSettings = settings(ctx, conn).await?;
    let old = org.logo_file_id;
    org.logo_file_id = new;
    ctx.app.settings.save(conn, ctx.keys(), &org, Some(ctx.actor()?.user_id)).await?;
    if let Some(old) = old {
        let key: Option<String> = sqlx::query_scalar(
            "update files set deleted_at = now() where id = $1 and kind = $2 and deleted_at is null returning storage_key",
        )
        .bind(old)
        .bind(LOGO_KIND)
        .fetch_optional(&mut *conn)
        .await?;
        if let Some(key) = key {
            crate::jobs::enqueue(conn, "delete_objects", serde_json::json!({ "keys": [key] }), None, None).await?;
        }
    }
    Ok((org, old))
}

pub struct SetLogo;

#[async_trait]
impl Service for SetLogo {
    type Input = SetLogoInput;
    type Output = Branding;
    const META: Meta = Meta::mutation(
        "branding.set_logo",
        "Upload the organisation's logo, shown in the app, on signing pages, in emails and on the evidence page. PNG or JPEG; it replaces any earlier logo.",
        Access::Permission(Permission::SettingsManage),
    ).tool();

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: SetLogoInput) -> Result<Branding> {
        if input.content_base64.len() > LOGO_MAX_BYTES / 3 * 4 + 8 {
            return Err(bad_request(format!("The logo can be at most {} MB", LOGO_MAX_BYTES / 1024 / 1024)));
        }
        let bytes = B64.decode(input.content_base64.trim()).map_err(|_| bad_request("contentBase64 is not valid base64"))?;
        let png = clean_logo(bytes).await?;
        let record = store_file(
            conn,
            &ctx.app.storage,
            ctx.keys(),
            NewFile { kind: LOGO_KIND, name: "logo.png", content_type: "image/png", body: &png, uploaded_by: Some(ctx.actor()?.user_id) },
        )
        .await?;
        let (org, old) = replace_logo(ctx, conn, Some(record.id)).await?;
        audit(
            ctx,
            conn,
            Entry::new("branding.logo_set", "settings", "org")
                .before(serde_json::json!({ "logoFileId": old }))
                .after(serde_json::json!({ "logoFileId": record.id, "sha256": record.sha256, "sizeBytes": record.size_bytes })),
        )
        .await?;
        Ok(branding(&org))
    }
}

pub struct RemoveLogo;

#[async_trait]
impl Service for RemoveLogo {
    type Input = Empty;
    type Output = Branding;
    const META: Meta = Meta::mutation(
        "branding.remove_logo",
        "Remove the organisation's logo. The OpenSign mark in the brand color is shown instead.",
        Access::Permission(Permission::SettingsManage),
    )
    .tool()
    .destructive();

    async fn run(ctx: &Ctx, conn: &mut PgConnection, _: Empty) -> Result<Branding> {
        let (org, old) = replace_logo(ctx, conn, None).await?;
        if old.is_some() {
            audit(ctx, conn, Entry::new("branding.logo_removed", "settings", "org").before(serde_json::json!({ "logoFileId": old })))
                .await?;
        }
        Ok(branding(&org))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut out = Vec::new();
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.add_text_chunk("Comment".into(), "secret metadata".into()).unwrap();
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&vec![120; (width * height * 4) as usize]).unwrap();
        writer.finish().unwrap();
        out
    }

    #[tokio::test]
    async fn logos_are_reencoded_with_transparency() {
        let out = clean_logo(png(64, 32)).await.unwrap();
        assert!(!out.windows(15).any(|w| w == b"secret metadata"));
        let decoder = png::Decoder::new(std::io::Cursor::new(&out));
        let reader = decoder.read_info().unwrap();
        assert_eq!((reader.info().width, reader.info().height, reader.info().color_type), (64, 32, png::ColorType::Rgba));
    }

    #[tokio::test]
    async fn refuses_svg_other_formats_and_odd_sizes() {
        let svg = br#"<?xml version="1.0"?><svg xmlns="http://www.w3.org/2000/svg"><script>alert(1)</script></svg>"#;
        assert!(clean_logo(svg.to_vec()).await.unwrap_err().message.contains("SVG"));
        assert!(clean_logo(b"  <svg onload=alert(1)>".to_vec()).await.unwrap_err().message.contains("SVG"));
        assert!(clean_logo(b"GIF89a....".to_vec()).await.is_err());
        assert!(clean_logo(b"\x89PNG\r\n\x1a\nbroken".to_vec()).await.is_err());
        assert!(clean_logo(png(8, 8)).await.is_err());
        assert!(clean_logo(png(2001, 20)).await.is_err());
        assert!(clean_logo(vec![0xFF; LOGO_MAX_BYTES + 1]).await.is_err());
    }

    #[test]
    fn logo_path_changes_with_the_file() {
        let id = Uuid::now_v7();
        let org = OrgSettings { logo_file_id: Some(id), ..Default::default() };
        assert_eq!(logo_path(&org), Some(format!("/api/logo/{}.png", id.simple())));
        assert_eq!(logo_path(&OrgSettings::default()), None);
    }
}
