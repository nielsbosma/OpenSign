//! Short-lived links for files too big to pass as base64 through MCP or JSON. The link itself is
//! the credential, so it is signed, expires quickly, names one file or one draft, and an upload
//! link works once.

use std::collections::BTreeSet;

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sqlx::PgConnection;
use uuid::Uuid;

use super::documents::{DownloadInput, FileOptions, UploadOutput, Version, add_file, document_file, require_draft};
use super::settings;
use crate::auth::{actor_for, user_by_id};
use crate::context::{App, Credential, Ctx, Surface};
use crate::documents::{Need, load_document};
use crate::error::{AppError, ErrorKind, Result, bad_request, forbidden};
use crate::permissions::Permission;
use crate::service::{Access, Meta, Service};
use crate::settings::SecuritySettings;
use crate::storage::{FileRecord, file_record};

const UPLOAD: &str = "file-upload";
const DOWNLOAD: &str = "file-download";
const UPLOAD_MINUTES: i64 = 15;
const DOWNLOAD_MINUTES: i64 = 10;
pub const MAX_UPLOAD_BYTES: usize = 100 * 1024 * 1024;

fn link(ctx: &Ctx, token: &str) -> String {
    format!("{}/api/transfers/{token}", ctx.app.public_url())
}

fn expired() -> AppError {
    AppError::new(ErrorKind::NotFound, "This link has expired or is not valid. Ask for a new one.")
}

#[derive(Serialize, Deserialize)]
struct UploadClaim {
    id: Uuid,
    document: Uuid,
    name: String,
    user: Uuid,
    /// The caller's permissions when the link was made, so an API key's scopes still apply.
    permissions: BTreeSet<Permission>,
    credential: Credential,
    surface: Surface,
    text_tags: bool,
    form_fields: bool,
}

#[derive(Serialize, Deserialize)]
struct DownloadClaim {
    file: Uuid,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UploadUrlInput {
    pub document_id: Uuid,
    /// The file name, e.g. "Agreement.docx".
    pub name: String,
    /// As in documents.upload: turn DocuSeal-style text tags into fields. On unless false.
    pub text_tags: Option<bool>,
    /// As in documents.upload: turn a fillable PDF form's fields into fields. On unless false.
    pub form_fields: Option<bool>,
}

#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UploadUrl {
    /// Send the file's raw bytes here with PUT, e.g. `curl -T Agreement.pdf <url>`. No other header is needed.
    pub url: String,
    pub method: String,
    pub expires_at: DateTime<Utc>,
    pub max_bytes: usize,
}

pub struct CreateUploadUrl;

#[async_trait]
impl Service for CreateUploadUrl {
    type Input = UploadUrlInput;
    type Output = UploadUrl;
    const META: Meta = Meta::mutation(
        "documents.upload_url",
        "Get a one-time link to upload a big file (PDF or Word) to a draft with an HTTP PUT of the raw bytes, instead of base64 through documents.upload. The link works once, for 15 minutes, and answers like documents.upload.",
        Access::Permission(Permission::DocumentsCreate),
    )
    .tool();

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: UploadUrlInput) -> Result<UploadUrl> {
        let actor = ctx.actor()?;
        let name = input.name.trim();
        if name.is_empty() || name.len() > 200 {
            return Err(bad_request("Give the file a name of up to 200 characters"));
        }
        let document = load_document(ctx, conn, input.document_id, Need::Edit).await?;
        require_draft(&document)?;
        let claim = UploadClaim {
            id: Uuid::new_v4(),
            document: document.id,
            name: name.to_string(),
            user: actor.user_id,
            permissions: actor.permissions.clone(),
            credential: actor.credential,
            surface: ctx.surface,
            text_tags: input.text_tags.unwrap_or(FileOptions::default().text_tags),
            form_fields: input.form_fields.unwrap_or(FileOptions::default().form_fields),
        };
        let token = ctx.keys().sign_token(UPLOAD, &claim, UPLOAD_MINUTES * 60, ctx.now.timestamp());
        Ok(UploadUrl {
            url: link(ctx, &token),
            method: "PUT".into(),
            expires_at: ctx.now + Duration::minutes(UPLOAD_MINUTES),
            max_bytes: MAX_UPLOAD_BYTES,
        })
    }
}

/// Opens an upload link: spends it, rebuilds the caller who made it, and adds the file to the draft.
pub async fn upload(app: std::sync::Arc<App>, token: &str, ip: Option<String>, request_id: String, bytes: Vec<u8>) -> Result<UploadOutput> {
    let mut ctx = Ctx::new(app, Surface::Api);
    let claim: UploadClaim = ctx.keys().verify_token(UPLOAD, token, ctx.now.timestamp()).map_err(|_| expired())?;
    let mut conn = ctx.db().acquire().await?;
    let spent = sqlx::query("insert into used_tokens (id, expires_at) values ($1, $2) on conflict do nothing")
        .bind(claim.id)
        .bind(ctx.now + Duration::minutes(UPLOAD_MINUTES))
        .execute(&mut *conn)
        .await?;
    if spent.rows_affected() == 0 {
        return Err(AppError::new(ErrorKind::Conflict, "This upload link was already used. Ask for a new one."));
    }
    if let Credential::ApiKey(key) = claim.credential {
        let live: Option<Uuid> =
            sqlx::query_scalar("select id from api_keys where id = $1 and revoked_at is null and (expires_at is null or expires_at > $2)")
                .bind(key)
                .bind(ctx.now)
                .fetch_optional(&mut *conn)
                .await?;
        live.ok_or_else(expired)?;
    }
    let security: SecuritySettings = settings(&ctx, &mut conn).await?;
    let user = user_by_id(&mut conn, claim.user).await?.ok_or_else(expired)?;
    let mut actor = actor_for(&user, &security, claim.credential).ok_or_else(expired)?;
    drop(conn);
    actor.permissions = actor.permissions.intersection(&claim.permissions).copied().collect();
    if matches!(claim.credential, Credential::ApiKey(_)) {
        actor.must_enable_two_factor = false;
    } else if actor.must_enable_two_factor {
        return Err(forbidden());
    }
    if !actor.can(Permission::DocumentsCreate) {
        return Err(forbidden());
    }
    ctx.surface = claim.surface;
    ctx.ip = ip;
    ctx.request_id = Some(request_id);
    let ctx = ctx.with_actor(actor);
    add_file(&ctx, claim.document, &claim.name, bytes, FileOptions { text_tags: claim.text_tags, form_fields: claim.form_fields }).await
}

#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DownloadUrl {
    /// Fetch the file with a plain GET, e.g. `curl -o file.pdf <url>`. No other header is needed.
    pub url: String,
    pub name: String,
    pub sha256: String,
    pub size_bytes: i64,
    pub expires_at: DateTime<Utc>,
}

pub struct CreateDownloadUrl;

#[async_trait]
impl Service for CreateDownloadUrl {
    type Input = DownloadInput;
    type Output = DownloadUrl;
    const META: Meta = Meta::query(
        "documents.download_url",
        "Get a link to download a document's signed PDF (default) or the original as sent, for files too big for documents.download. Anyone holding the link can fetch the file for 10 minutes, so hand it only to the program that downloads it. Audited like a download.",
        Access::Authenticated,
    )
    .tool();

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: DownloadInput) -> Result<DownloadUrl> {
        let record = document_file(ctx, conn, input.id, input.version.unwrap_or(Version::Signed)).await?;
        let token = ctx.keys().sign_token(DOWNLOAD, &DownloadClaim { file: record.id }, DOWNLOAD_MINUTES * 60, ctx.now.timestamp());
        Ok(DownloadUrl {
            url: link(ctx, &token),
            name: record.name,
            sha256: record.sha256,
            size_bytes: record.size_bytes,
            expires_at: ctx.now + Duration::minutes(DOWNLOAD_MINUTES),
        })
    }
}

/// The file a download link names, if the link is still good and the document still exists.
pub async fn download(app: &App, token: &str) -> Result<FileRecord> {
    let claim: DownloadClaim = app.keys.verify_token(DOWNLOAD, token, Utc::now().timestamp()).map_err(|_| expired())?;
    let mut conn = app.db.acquire().await?;
    let live: Option<Uuid> = sqlx::query_scalar(
        "select d.id from documents d join files f on f.id = $1
         where f.deleted_at is null and (d.sealed_file_id = $1 or d.source_file_id = $1) limit 1",
    )
    .bind(claim.file)
    .fetch_optional(&mut *conn)
    .await?;
    live.ok_or_else(expired)?;
    file_record(&mut conn, claim.file).await.map_err(|_| expired())
}
