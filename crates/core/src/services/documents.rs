use async_trait::async_trait;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use chrono::{DateTime, Duration, Utc};
use opensign_crypto::{KeyPurpose, sha256_hex};
use opensign_pdf::SignatureCheck;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::PgConnection;
use uuid::Uuid;

use super::signing::{SignerView, signer_view};
use super::{Ok_, settings};
use crate::audit::{Entry, audit};
use crate::auth::normalize_email;
use crate::context::Ctx;
use crate::documents::attachments::{self, AttachmentRequestInput};
use crate::documents::logic::{self, Condition, Owner};
use crate::documents::values::{self, Spec, Validation};
use crate::documents::{
    AuthMethod, Delivery, DocumentRow, DocumentView, FieldKind, Need, PartyRole, PartyRow, PartyView, VISIBLE, current_turn,
    decrypt_personal_number, files, load_document, notify, parties, personal_number_context, signer_url, status, upload, view, visibility,
};
use crate::error::{AppError, ErrorKind, Result, bad_request, conflict, forbidden, not_found};
use crate::i18n::{LANGUAGES, Lang};
use crate::permissions::Permission;
use crate::service::{Access, Meta, Service};
use crate::settings::{EidSettings, EmailSettings, SecuritySettings, SmsSettings};
use crate::storage::{FileRecord, NewFile, file_record, read_file, store_file};

const MAX_PARTIES: usize = 50;
const MAX_FIELDS: usize = 500;
const MAX_DOWNLOAD: i64 = 20 * 1024 * 1024;

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct IdInput {
    pub id: Uuid,
}

fn require_draft(document: &DocumentRow) -> Result<()> {
    if document.status != status::DRAFT {
        return Err(conflict("Only drafts can be changed. Cancel the document and make a copy to change it."));
    }
    Ok(())
}

fn clean_text(value: &str, what: &str, max: usize) -> Result<String> {
    let value = value.trim();
    if value.chars().count() > max {
        return Err(bad_request(format!("{what} is too long (at most {max} characters)")));
    }
    if value.chars().any(|c| c.is_control() && c != '\n' && c != '\t') {
        return Err(bad_request(format!("{what} contains characters that are not allowed")));
    }
    Ok(value.to_string())
}

fn title(value: &str) -> Result<String> {
    let title = clean_text(value, "The title", 200)?.replace(['\n', '\t'], " ");
    if title.is_empty() {
        return Err(bad_request("Give the document a title"));
    }
    Ok(title)
}

async fn reload(ctx: &Ctx, conn: &mut PgConnection, id: Uuid) -> Result<DocumentView> {
    let document = load_document(ctx, conn, id, Need::Read).await?;
    view(ctx, conn, document).await
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ListInput {
    /// draft, pending, completed, declined, canceled or expired.
    pub status: Option<String>,
    /// Matches the title, or an exact external id.
    pub search: Option<String>,
    /// List templates instead of documents.
    #[serde(default)]
    pub templates: bool,
    /// Only what is directly in this folder. Leave out to list everything regardless of folder.
    pub folder_id: Option<Uuid>,
    /// List the archive instead: archived documents or templates only. Archived ones are left out otherwise.
    #[serde(default)]
    pub archived: bool,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Serialize, JsonSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct DocumentSummary {
    pub id: Uuid,
    pub title: String,
    pub status: String,
    pub is_template: bool,
    pub owner_name: Option<String>,
    pub external_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub sent_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
    pub parties_total: i64,
    pub parties_done: i64,
    pub folder_id: Option<Uuid>,
    pub archived_at: Option<DateTime<Utc>>,
}

pub(crate) fn like_pattern(search: &str) -> String {
    let escaped = search.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
    format!("%{escaped}%")
}

pub struct List;

#[async_trait]
impl Service for List {
    type Input = ListInput;
    type Output = Vec<DocumentSummary>;
    const META: Meta = Meta::query(
        "documents.list",
        "List documents (or templates) you can see, newest first, optionally filtered by status, folder or a search on the title. Archived ones are only listed with archived true.",
        Access::Authenticated,
    );

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: ListInput) -> Result<Vec<DocumentSummary>> {
        let (read_all, user_id, can_create) = visibility(ctx)?;
        let search = input.search.as_deref().map(str::trim).filter(|s| !s.is_empty());
        Ok(sqlx::query_as(&format!(
            "select d.id, d.title, d.status, d.is_template, u.name as owner_name, d.external_id, d.created_at, d.sent_at, d.completed_at, d.expires_at,
                    (select count(*) from parties p where p.document_id = d.id and p.role <> 'viewer') as parties_total,
                    (select count(*) from parties p where p.document_id = d.id and p.role <> 'viewer' and p.signed_at is not null) as parties_done,
                    d.folder_id, d.archived_at
             from documents d left join users u on u.id = d.owner_id
             where {VISIBLE} and d.is_template = $4 and ($5::text is null or d.status = $5)
               and ($6::text is null or d.title ilike $7 escape '\\' or d.external_id = $6)
               and ($10::uuid is null or d.folder_id = $10) and (d.archived_at is not null) = $11
             order by d.created_at desc limit $8 offset $9"
        ))
        .bind(read_all)
        .bind(user_id)
        .bind(can_create)
        .bind(input.templates)
        .bind(input.status)
        .bind(search)
        .bind(search.map(like_pattern))
        .bind(input.limit.unwrap_or(50).clamp(1, 200))
        .bind(input.offset.unwrap_or(0).max(0))
        .bind(input.folder_id)
        .bind(input.archived)
        .fetch_all(&mut *conn)
        .await?)
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CountsInput {
    /// Matches the title, or an exact external id, as in documents.list.
    pub search: Option<String>,
    /// Count templates instead of documents.
    #[serde(default)]
    pub templates: bool,
    /// Only what is directly in this folder, as in documents.list.
    pub folder_id: Option<Uuid>,
}

#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StatusCounts {
    pub all: i64,
    /// Documents per status: draft, pending, completed, declined, canceled and expired. Missing means none.
    pub by_status: std::collections::BTreeMap<String, i64>,
    /// Archived ones, which `all` and `byStatus` leave out.
    pub archived: i64,
}

pub struct Counts;

#[async_trait]
impl Service for Counts {
    type Input = CountsInput;
    type Output = StatusCounts;
    const META: Meta = Meta::query(
        "documents.counts",
        "Count the documents (or templates) you can see, in total, per status and archived, optionally in one folder or matching a search on the title. Use it for an overview before listing.",
        Access::Authenticated,
    );

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: CountsInput) -> Result<StatusCounts> {
        let (read_all, user_id, can_create) = visibility(ctx)?;
        let search = input.search.as_deref().map(str::trim).filter(|s| !s.is_empty());
        let rows: Vec<(String, bool, i64)> = sqlx::query_as(&format!(
            "select d.status, d.archived_at is not null, count(*) from documents d
             where {VISIBLE} and d.is_template = $6
               and ($4::text is null or d.title ilike $5 escape '\\' or d.external_id = $4)
               and ($7::uuid is null or d.folder_id = $7)
             group by 1, 2"
        ))
        .bind(read_all)
        .bind(user_id)
        .bind(can_create)
        .bind(search)
        .bind(search.map(like_pattern))
        .bind(input.templates)
        .bind(input.folder_id)
        .fetch_all(&mut *conn)
        .await?;
        let archived = rows.iter().filter(|(_, a, _)| *a).map(|(_, _, n)| n).sum();
        let active: Vec<_> = rows.into_iter().filter(|(_, a, _)| !a).map(|(s, _, n)| (s, n)).collect();
        Ok(StatusCounts { all: active.iter().map(|(_, n)| n).sum(), by_status: active.into_iter().collect(), archived })
    }
}

pub struct Get;

#[async_trait]
impl Service for Get {
    type Input = IdInput;
    type Output = DocumentView;
    const META: Meta =
        Meta::query("documents.get", "Get a document or template with its files, parties, fields and status.", Access::Authenticated);

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: IdInput) -> Result<DocumentView> {
        reload(ctx, conn, input.id).await
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateInput {
    pub title: String,
    pub message: Option<String>,
    /// What signers see and receive unless a party has its own: en, zh, hi, es, fr, ar, bn, pt, ru, ur or sv.
    /// Defaults to the template's, then the organisation's.
    pub language: Option<String>,
    /// Copy files, parties and fields from this template.
    pub template_id: Option<Uuid>,
    /// Create a reusable template instead of a document.
    #[serde(default)]
    pub is_template: bool,
    pub sequential: Option<bool>,
    /// Your own reference, e.g. a CRM id. Searchable.
    pub external_id: Option<String>,
    pub metadata: Option<Value>,
    /// Put it in this folder: a document folder for documents, a template folder for templates.
    pub folder_id: Option<Uuid>,
}

pub fn language(value: Option<&str>) -> Result<String> {
    match Lang::parse(value.unwrap_or("en")) {
        Some(lang) => Ok(lang.code().to_string()),
        None => Err(bad_request(format!(
            "Use one of these language codes: {}",
            LANGUAGES.iter().map(|(code, _)| *code).collect::<Vec<_>>().join(", ")
        ))),
    }
}

fn metadata(value: Option<Value>) -> Result<Value> {
    let value = value.unwrap_or_else(|| json!({}));
    if !value.is_object() || value.to_string().len() > 16 * 1024 {
        return Err(bad_request("Metadata must be a JSON object of at most 16 KB"));
    }
    Ok(value)
}

pub struct Create;

#[async_trait]
impl Service for Create {
    type Input = CreateInput;
    type Output = DocumentView;
    const META: Meta = Meta::mutation(
        "documents.create",
        "Create a draft document, optionally from a template, or a new template. Add files, parties and fields next, then send it.",
        Access::Permission(Permission::DocumentsCreate),
    );

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: CreateInput) -> Result<DocumentView> {
        if input.is_template && !ctx.can(Permission::TemplatesManage) {
            return Err(forbidden());
        }
        let template = match input.template_id {
            Some(id) => {
                let t = load_document(ctx, conn, id, Need::Read).await?;
                if !t.is_template {
                    return Err(bad_request("That document is not a template"));
                }
                if t.archived_at.is_some() {
                    return Err(conflict("That template is archived. Restore it before using it."));
                }
                Some(t)
            }
            None => None,
        };
        if let Some(folder_id) = input.folder_id {
            super::folders::check_folder(conn, folder_id, input.is_template).await?;
        }
        let org: crate::settings::OrgSettings = settings(ctx, conn).await?;
        let id = Uuid::now_v7();
        let message = match (&input.message, &template) {
            (Some(m), _) => clean_text(m, "The message", 4000)?,
            (None, Some(t)) => t.message.clone(),
            (None, None) => String::new(),
        };
        sqlx::query(
            "insert into documents (id, title, is_template, template_id, owner_id, message, language, sequential, reminder_days, external_id, metadata, review_links, folder_id)
             values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)",
        )
        .bind(id)
        .bind(title(&input.title)?)
        .bind(input.is_template)
        .bind(template.as_ref().map(|t| t.id))
        .bind(ctx.actor()?.user_id)
        .bind(message)
        .bind(match input.language.as_deref() {
            Some(l) => language(Some(l))?,
            None => Lang::of(template.as_ref().map_or(org.locale.as_str(), |t| t.language.as_str())).code().to_string(),
        })
        .bind(input.sequential.or(template.as_ref().map(|t| t.sequential)).unwrap_or(false))
        .bind(template.as_ref().and_then(|t| t.reminder_days))
        .bind(input.external_id.as_deref().map(|e| clean_text(e, "The external id", 200)).transpose()?.filter(|e| !e.is_empty()))
        .bind(metadata(input.metadata)?)
        .bind(template.as_ref().is_none_or(|t| t.review_links))
        .bind(input.folder_id)
        .execute(&mut *conn)
        .await?;
        if let Some(t) = &template {
            copy_template(ctx, conn, t.id, id).await?;
        }
        audit(
            ctx,
            conn,
            Entry::new("document.created", "document", id)
                .metadata(json!({ "template": input.template_id, "isTemplate": input.is_template, "folder": input.folder_id })),
        )
        .await?;
        if !input.is_template {
            let document = reload(ctx, conn, id).await?;
            notify::emit(
                conn,
                "document.created",
                json!({ "documentId": id, "title": document.document.title, "externalId": document.document.external_id, "templateId": input.template_id }),
            )
            .await?;
            return Ok(document);
        }
        reload(ctx, conn, id).await
    }
}

async fn copy_template(ctx: &Ctx, conn: &mut PgConnection, from: Uuid, to: Uuid) -> Result<()> {
    sqlx::query("insert into document_files (document_id, file_id, position) select $2, file_id, position from document_files where document_id = $1")
        .bind(from)
        .bind(to)
        .execute(&mut *conn)
        .await?;
    let mut ids = std::collections::HashMap::new();
    for p in parties(conn, from).await? {
        let new_id = Uuid::now_v7();
        ids.insert(p.id, new_id);
        let personal_number = decrypt_personal_number(ctx.keys(), &p)
            .map(|n| ctx.keys().encrypt_str(KeyPurpose::Settings, &personal_number_context(new_id), &n));
        sqlx::query(
            "insert into parties (id, document_id, role, name, email, phone, personal_number, company, sign_order, delivery, auth_to_view, auth_to_sign, language)
             values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)",
        )
        .bind(new_id)
        .bind(to)
        .bind(&p.role)
        .bind(&p.name)
        .bind(&p.email)
        .bind(&p.phone)
        .bind(personal_number)
        .bind(&p.company)
        .bind(p.sign_order)
        .bind(&p.delivery)
        .bind(&p.auth_to_view)
        .bind(&p.auth_to_sign)
        .bind(&p.language)
        .execute(&mut *conn)
        .await?;
    }
    attachments::copy(conn, to, &ids, from).await?;
    for f in crate::documents::fields(conn, from).await? {
        sqlx::query(
            "insert into fields (id, document_id, party_id, kind, page, x, y, width, height, required, label, value, options, validation, format, group_key, read_only, conditions, formula)
             values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19)",
        )
        .bind(Uuid::now_v7())
        .bind(to)
        .bind(f.party_id.and_then(|p| ids.get(&p).copied()))
        .bind(&f.kind)
        .bind(f.page)
        .bind(f.x)
        .bind(f.y)
        .bind(f.width)
        .bind(f.height)
        .bind(f.required)
        .bind(&f.label)
        .bind(f.file_id.is_none().then_some(f.value.as_ref()).flatten())
        .bind(&f.options)
        .bind(&f.validation)
        .bind(&f.format)
        .bind(&f.group_key)
        .bind(f.read_only)
        .bind(&f.conditions)
        .bind(&f.formula)
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UploadInput {
    pub document_id: Uuid,
    /// The file name, e.g. "Agreement.docx".
    pub name: String,
    /// The file's bytes, base64. PDF, .docx, .doc, .odt or .rtf.
    pub content_base64: String,
}

#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UploadOutput {
    pub file: FileRecord,
    pub pages: usize,
    /// Active content taken out of the file, e.g. "JavaScript".
    pub removed: Vec<String>,
}

fn pdf_name(name: &str) -> String {
    let stem = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let stem = stem.rsplit_once('.').map(|(s, _)| s).unwrap_or(stem);
    format!("{}.pdf", notify::sanitize_filename(stem))
}

/// Converts and sanitizes an upload, then attaches it to a draft. Conversion happens before the
/// transaction so a slow converter holds no locks.
pub async fn add_file(ctx: &Ctx, document_id: Uuid, name: &str, bytes: Vec<u8>) -> Result<UploadOutput> {
    let mut conn = ctx.db().acquire().await?;
    let document = load_document(ctx, &mut conn, document_id, Need::Edit).await?;
    require_draft(&document)?;
    let security: SecuritySettings = settings(ctx, &mut conn).await?;
    drop(conn);
    let original_sha256 = sha256_hex(&bytes);
    let clean = upload::prepare(&ctx.app, bytes, security.max_upload_mb).await?;

    let mut tx = ctx.db().begin().await?;
    let state: Option<String> =
        sqlx::query_scalar("select status from documents where id = $1 for update").bind(document_id).fetch_optional(&mut *tx).await?;
    if state.as_deref() != Some(status::DRAFT) {
        return Err(conflict("Only drafts can be changed"));
    }
    let name = pdf_name(name);
    let file = store_file(
        &mut tx,
        &ctx.app.storage,
        ctx.keys(),
        NewFile {
            kind: "document",
            name: &name,
            content_type: "application/pdf",
            body: &clean.pdf,
            uploaded_by: ctx.actor.as_ref().map(|a| a.user_id),
        },
    )
    .await?;
    sqlx::query("insert into document_files (document_id, file_id, position) select $1, $2, coalesce(max(position), 0) + 1 from document_files where document_id = $1")
        .bind(document_id)
        .bind(file.id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("update documents set updated_at = now() where id = $1").bind(document_id).execute(&mut *tx).await?;
    audit(
        ctx,
        &mut tx,
        Entry::new("document.file_added", "document", document_id).metadata(
            json!({ "file": file.id, "name": name, "uploadSha256": original_sha256, "sha256": file.sha256, "removed": clean.removed }),
        ),
    )
    .await?;
    tx.commit().await?;
    Ok(UploadOutput { file, pages: clean.pages.len(), removed: clean.removed })
}

pub struct Upload;

#[async_trait]
impl Service for Upload {
    type Input = UploadInput;
    type Output = UploadOutput;
    const META: Meta = Meta::mutation(
        "documents.upload",
        "Add a file (PDF or Word, base64) to a draft. Word files are converted to PDF; scripts, attachments and forms are removed.",
        Access::Permission(Permission::DocumentsCreate),
    )
    .without_transaction();

    async fn run(ctx: &Ctx, _conn: &mut PgConnection, input: UploadInput) -> Result<UploadOutput> {
        if input.content_base64.len() > 140 * 1024 * 1024 {
            return Err(bad_request("The file is too large"));
        }
        let bytes = B64.decode(input.content_base64.trim()).map_err(|_| bad_request("contentBase64 is not valid base64"))?;
        add_file(ctx, input.document_id, &input.name, bytes).await
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RemoveFileInput {
    pub document_id: Uuid,
    pub file_id: Uuid,
}

pub struct RemoveFile;

#[async_trait]
impl Service for RemoveFile {
    type Input = RemoveFileInput;
    type Output = DocumentView;
    const META: Meta =
        Meta::mutation("documents.remove_file", "Remove a file from a draft.", Access::Permission(Permission::DocumentsCreate));

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: RemoveFileInput) -> Result<DocumentView> {
        let document = load_document(ctx, conn, input.document_id, Need::Edit).await?;
        require_draft(&document)?;
        let done = sqlx::query("delete from document_files where document_id = $1 and file_id = $2")
            .bind(document.id)
            .bind(input.file_id)
            .execute(&mut *conn)
            .await?;
        if done.rows_affected() == 0 {
            return Err(not_found("File"));
        }
        audit(ctx, conn, Entry::new("document.file_removed", "document", document.id).metadata(json!({ "file": input.file_id }))).await?;
        reload(ctx, conn, document.id).await
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInput {
    pub id: Uuid,
    pub title: Option<String>,
    pub message: Option<String>,
    /// What signers see and receive: `en` or `sv`.
    pub language: Option<String>,
    /// Parties sign one sign-order group at a time.
    pub sequential: Option<bool>,
    /// When the signing links stop working and the document expires.
    pub expires_at: Option<DateTime<Utc>>,
    /// Remind parties who have not signed every this many days; 0 turns reminders off.
    pub reminder_days: Option<i32>,
    /// Your own reference; "" removes it.
    pub external_id: Option<String>,
    pub metadata: Option<Value>,
}

pub struct Update;

#[async_trait]
impl Service for Update {
    type Input = UpdateInput;
    type Output = DocumentView;
    const META: Meta = Meta::mutation(
        "documents.update",
        "Change a draft's title, message, language, signing order, expiry, reminders, external id or metadata.",
        Access::Permission(Permission::DocumentsCreate),
    );

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: UpdateInput) -> Result<DocumentView> {
        let document = load_document(ctx, conn, input.id, Need::Edit).await?;
        require_draft(&document)?;
        if let Some(at) = input.expires_at
            && at <= ctx.now + Duration::hours(1)
        {
            return Err(bad_request("The expiry must be at least an hour from now"));
        }
        if input.reminder_days.is_some_and(|d| !(0..=60).contains(&d)) {
            return Err(bad_request("Reminders can be every 1 to 60 days, or 0 for none"));
        }
        sqlx::query(
            "update documents set title = coalesce($2, title), message = coalesce($3, message), language = coalesce($4, language),
                sequential = coalesce($5, sequential), expires_at = coalesce($6, expires_at), reminder_days = coalesce($7, reminder_days),
                external_id = case when $8::text is null then external_id else nullif($8, '') end, metadata = coalesce($9, metadata), updated_at = now() where id = $1",
        )
        .bind(document.id)
        .bind(input.title.as_deref().map(title).transpose()?)
        .bind(input.message.as_deref().map(|m| clean_text(m, "The message", 4000)).transpose()?)
        .bind(input.language.as_deref().map(|l| language(Some(l))).transpose()?)
        .bind(input.sequential)
        .bind(input.expires_at)
        .bind(input.reminder_days)
        .bind(input.external_id.as_deref().map(|e| clean_text(e, "The external id", 200)).transpose()?)
        .bind(input.metadata.map(|m| metadata(Some(m))).transpose()?)
        .execute(&mut *conn)
        .await?;
        audit(ctx, conn, Entry::new("document.updated", "document", document.id)).await?;
        reload(ctx, conn, document.id).await
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PartyInput {
    /// Keep an existing party (and its fields) by passing its id.
    pub id: Option<Uuid>,
    #[serde(default = "default_role")]
    pub role: PartyRole,
    pub name: String,
    pub email: Option<String>,
    /// With country code, e.g. +46701234567. Needed for SMS delivery and SMS codes.
    pub phone: Option<String>,
    /// A national id number to match against the eID result, e.g. a Swedish personnummer. Stored encrypted. Leave it out (or
    /// send the masked value) to keep the stored one; send "" to remove it.
    pub personal_number: Option<String>,
    pub company: Option<String>,
    /// In sequential documents, lower numbers sign first. Equal numbers sign in parallel.
    pub sign_order: Option<i32>,
    #[serde(default = "default_delivery")]
    pub delivery: Delivery,
    #[serde(default = "default_view_auth")]
    pub auth_to_view: AuthMethod,
    #[serde(default = "default_sign_auth")]
    pub auth_to_sign: AuthMethod,
    /// Files the party must or may upload while signing, such as a copy of an ID. Leave out to keep the current ones.
    pub attachments: Option<Vec<AttachmentRequestInput>>,
    /// The language of this party's emails, SMS and signing page, if not the document's: en, zh, hi, es, fr, ar, bn, pt, ru,
    /// ur or sv. Leave out to use the document's.
    pub language: Option<String>,
    /// Ask the party to take a live photo of themselves with their webcam or phone camera before they can sign. The photo is
    /// stored encrypted, shown to the sender and sealed into the evidence. Leave out to keep the current setting.
    pub webcam_photo: Option<bool>,
}

fn default_role() -> PartyRole {
    PartyRole::Signer
}
fn default_delivery() -> Delivery {
    Delivery::Email
}
fn default_view_auth() -> AuthMethod {
    AuthMethod::None
}
fn default_sign_auth() -> AuthMethod {
    AuthMethod::Standard
}

fn personal_number(value: &str) -> Result<String> {
    let cleaned: String = value.chars().filter(|c| !c.is_whitespace()).collect();
    if cleaned.is_empty() || cleaned.len() > 20 || !cleaned.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '+') {
        return Err(bad_request("The personal number is not valid"));
    }
    Ok(cleaned)
}

struct CleanParty {
    id: Uuid,
    role: PartyRole,
    name: String,
    email: Option<String>,
    phone: Option<String>,
    personal_number: Option<String>,
    keep_personal_number: bool,
    company: Option<String>,
    sign_order: i32,
    delivery: Delivery,
    auth_to_view: AuthMethod,
    auth_to_sign: AuthMethod,
    attachments: Option<Vec<AttachmentRequestInput>>,
    language: Option<String>,
    webcam_photo: Option<bool>,
}

fn clean_party(input: PartyInput, existing: &[PartyRow]) -> Result<CleanParty> {
    let id = match input.id {
        Some(id) if existing.iter().any(|p| p.id == id) => id,
        Some(_) => return Err(not_found("Party")),
        None => Uuid::now_v7(),
    };
    let name = clean_text(&input.name, "The name", 200)?.replace(['\n', '\t'], " ");
    if name.is_empty() {
        return Err(bad_request("Every party needs a name"));
    }
    if input.auth_to_view == AuthMethod::Standard {
        return Err(bad_request("authToView is none, sms_pin, se_bankid, freja or broker"));
    }
    if input.auth_to_sign == AuthMethod::None {
        return Err(bad_request("authToSign is standard, sms_pin, se_bankid, freja or broker"));
    }
    let keep_personal_number = input.personal_number.as_deref().is_none_or(|n| n.contains('\u{2022}'));
    Ok(CleanParty {
        id,
        role: input.role,
        name,
        email: input.email.as_deref().map(str::trim).filter(|e| !e.is_empty()).map(normalize_email).transpose()?,
        phone: input.phone.as_deref().map(str::trim).filter(|p| !p.is_empty()).map(crate::sms::normalize_phone).transpose()?,
        personal_number: match keep_personal_number {
            true => None,
            false => input.personal_number.as_deref().map(str::trim).filter(|p| !p.is_empty()).map(personal_number).transpose()?,
        },
        keep_personal_number,
        company: input.company.as_deref().map(|c| clean_text(c, "The company", 200)).transpose()?.filter(|c| !c.is_empty()),
        sign_order: input.sign_order.unwrap_or(1).clamp(1, 100),
        delivery: input.delivery,
        auth_to_view: input.auth_to_view,
        auth_to_sign: input.auth_to_sign,
        attachments: input.attachments,
        language: input.language.as_deref().map(str::trim).filter(|l| !l.is_empty()).map(|l| language(Some(l))).transpose()?,
        webcam_photo: if input.role == PartyRole::Viewer { Some(false) } else { input.webcam_photo },
    })
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SetPartiesInput {
    pub document_id: Uuid,
    /// The complete list. Parties left out are removed with their fields.
    pub parties: Vec<PartyInput>,
}

pub struct SetParties;

#[async_trait]
impl Service for SetParties {
    type Input = SetPartiesInput;
    type Output = Vec<PartyView>;
    const META: Meta = Meta::mutation(
        "documents.set_parties",
        "Set who signs, approves or receives a copy of a draft, how each gets their link, and how each must identify themselves.",
        Access::Permission(Permission::DocumentsCreate),
    );

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: SetPartiesInput) -> Result<Vec<PartyView>> {
        let document = load_document(ctx, conn, input.document_id, Need::Edit).await?;
        require_draft(&document)?;
        if input.parties.len() > MAX_PARTIES {
            return Err(bad_request(format!("A document can have at most {MAX_PARTIES} parties")));
        }
        let existing = parties(conn, document.id).await?;
        let cleaned: Vec<CleanParty> = input.parties.into_iter().map(|p| clean_party(p, &existing)).collect::<Result<_>>()?;
        let keep: Vec<Uuid> = cleaned.iter().map(|p| p.id).collect();
        sqlx::query("delete from parties where document_id = $1 and not (id = any($2))")
            .bind(document.id)
            .bind(&keep)
            .execute(&mut *conn)
            .await?;
        for p in &cleaned {
            let personal =
                p.personal_number.as_deref().map(|n| ctx.keys().encrypt_str(KeyPurpose::Settings, &personal_number_context(p.id), n));
            sqlx::query(
                "insert into parties (id, document_id, role, name, email, phone, personal_number, company, sign_order, delivery, auth_to_view, auth_to_sign, language)
                 values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $14)
                 on conflict (id) do update set role = excluded.role, name = excluded.name, email = excluded.email, phone = excluded.phone,
                   personal_number = case when $13 then parties.personal_number else excluded.personal_number end, company = excluded.company, sign_order = excluded.sign_order,
                   delivery = excluded.delivery, auth_to_view = excluded.auth_to_view, auth_to_sign = excluded.auth_to_sign, language = excluded.language, updated_at = now()",
            )
            .bind(p.id)
            .bind(document.id)
            .bind(p.role.as_str())
            .bind(&p.name)
            .bind(&p.email)
            .bind(&p.phone)
            .bind(personal)
            .bind(&p.company)
            .bind(p.sign_order)
            .bind(p.delivery.as_str())
            .bind(p.auth_to_view.as_str())
            .bind(p.auth_to_sign.as_str())
            .bind(p.keep_personal_number)
            .bind(&p.language)
            .execute(&mut *conn)
            .await?;
            if let Some(requests) = &p.attachments {
                attachments::set(conn, document.id, p.id, requests).await?;
            }
            if let Some(wanted) = p.webcam_photo {
                attachments::set_photo(conn, document.id, p.id, wanted).await?;
            }
        }
        sqlx::query("update fields set party_id = null where document_id = $1 and party_id is not null and not (party_id = any($2))")
            .bind(document.id)
            .bind(&keep)
            .execute(&mut *conn)
            .await?;
        audit(
            ctx,
            conn,
            Entry::new("document.parties_set", "document", document.id).after(
                cleaned
                    .iter()
                    .map(|p| {
                        json!({
                            "id": p.id, "name": p.name, "email": p.email, "role": p.role, "authToSign": p.auth_to_sign, "language": p.language,
                            "attachments": p.attachments.as_ref().map(|a| a.iter().map(|r| r.name.clone()).collect::<Vec<_>>()),
                            "webcamPhoto": p.webcam_photo,
                        })
                    })
                    .collect::<Vec<_>>(),
            ),
        )
        .await?;
        crate::documents::party_views(ctx, conn, document.id).await
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FieldInput {
    /// The party who fills it in. Leave out for text the sender fills in now.
    pub party_id: Option<Uuid>,
    pub kind: FieldKind,
    /// Zero-based page index in the combined document.
    pub page: i32,
    /// Position and size as fractions of the page (0 to 1), origin top-left.
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub required: Option<bool>,
    pub label: Option<String>,
    /// A prefilled value: for a party's field, what they start from. Numbers as `1234.5`, dates as
    /// YYYY-MM-DD, a multiselect as a JSON array of options, a radio button as its option when chosen.
    pub value: Option<String>,
    /// The choices of a `select` or `multiselect` field, or the one option of a `radio` button.
    #[serde(default)]
    pub options: Vec<String>,
    /// Checks the signer's value must pass, with an optional error text of your own.
    #[serde(default)]
    pub validation: Option<Validation>,
    /// How a date or number is written on the page. Dates: YYYY-MM-DD, DD/MM/YYYY, MM/DD/YYYY,
    /// DD.MM.YYYY or D MMM YYYY. Numbers: how 1234 would look, such as `1,234.00`, `1 234,00 kr` or `$1234`.
    pub format: Option<String>,
    /// Ties `radio` buttons into one group: give every button of the group the same key.
    pub group_key: Option<String>,
    /// The party sees the prefilled value but cannot change it.
    pub read_only: Option<bool>,
    /// Show the field only when all of these hold. Each looks at another field by its label. A
    /// party's field may look at their own fields, the sender's, and fields of parties who sign
    /// earlier in order; a hidden field is never required and is left empty.
    #[serde(default)]
    pub conditions: Vec<Condition>,
    /// For a `formula` field: the number it computes from other fields, which are named by label in
    /// braces. `+ - * /`, brackets, comparisons (`= != < <= > >=`) and the functions round(x, decimals),
    /// floor, ceil, abs, min, max, sum, if(condition, then, else), and, or, not and contains. Example:
    /// `round({Hours} * {Rate} * 1.25, 2)`. The signer cannot change it; the server computes it when they sign.
    pub formula: Option<String>,
}

impl FieldInput {
    /// The input that would save a stored field as it is.
    pub fn from_row(f: &crate::documents::FieldRow) -> Result<FieldInput> {
        let kind = FieldKind::parse(&f.kind).ok_or_else(|| AppError::internal(anyhow::anyhow!("field kind {}", f.kind)))?;
        let spec = Spec::of(f);
        Ok(FieldInput {
            party_id: f.party_id,
            kind,
            page: f.page,
            x: f.x,
            y: f.y,
            width: f.width,
            height: f.height,
            required: Some(f.required),
            label: Some(f.label.clone()),
            value: f.value.clone(),
            options: spec.options,
            validation: (!spec.validation.is_empty()).then_some(spec.validation),
            format: spec.format,
            group_key: f.group_key.clone(),
            read_only: Some(f.read_only),
            conditions: f.conditions(),
            formula: f.formula.clone(),
        })
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SetFieldsInput {
    pub document_id: Uuid,
    /// The complete list; it replaces the draft's fields.
    pub fields: Vec<FieldInput>,
}

fn check_box(f: &FieldInput) -> Result<()> {
    let values = [f.x, f.y, f.width, f.height];
    if values.iter().any(|v| !v.is_finite())
        || f.x < 0.0
        || f.y < 0.0
        || f.width <= 0.0
        || f.height <= 0.0
        || f.x + f.width > 1.0001
        || f.y + f.height > 1.0001
    {
        return Err(bad_request("Every field must lie inside its page (fractions from 0 to 1)"));
    }
    if f.width < 0.005 || f.height < 0.005 {
        return Err(bad_request("A field is too small"));
    }
    if !(0..10_000).contains(&f.page) {
        return Err(bad_request("A field's page is out of range"));
    }
    Ok(())
}

pub struct SetFields;

#[async_trait]
impl Service for SetFields {
    type Input = SetFieldsInput;
    type Output = DocumentView;
    const META: Meta = Meta::mutation(
        "documents.set_fields",
        "Place the fields of a draft, each assigned to a party: signatures, initials, text, numbers, dropdowns, multi-select lists, radio groups, checkboxes, phone numbers, date pickers, image and file uploads, formulas, and the automatic name, email, company and date of signing. Text fields take a pattern and length limits, numbers and dates a smallest and largest value and a format, each with an optional error text of your own. Any field can be shown only when other fields hold given values, and a formula field computes a number from other fields.",
        Access::Permission(Permission::DocumentsCreate),
    );

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: SetFieldsInput) -> Result<DocumentView> {
        let document = load_document(ctx, conn, input.document_id, Need::Edit).await?;
        require_draft(&document)?;
        if input.fields.len() > MAX_FIELDS {
            return Err(bad_request(format!("A document can have at most {MAX_FIELDS} fields")));
        }
        let all = parties(conn, document.id).await?;
        let mut groups: std::collections::HashMap<String, (Option<Uuid>, Vec<String>, bool)> = Default::default();
        let mut prepared = Vec::with_capacity(input.fields.len());
        for f in &input.fields {
            check_box(f)?;
            let party = match f.party_id {
                Some(id) => Some(all.iter().find(|p| p.id == id).ok_or_else(|| not_found("Party"))?),
                None => None,
            };
            if matches!(f.kind, FieldKind::Signature | FieldKind::Initials) && !party.is_some_and(|p| p.role() == PartyRole::Signer) {
                return Err(bad_request("Signature and initials fields belong to a signer"));
            }
            if (f.kind.is_upload() || f.kind == FieldKind::Radio) && !party.is_some_and(|p| p.must_act()) {
                return Err(bad_request("Image, file and radio fields belong to a party who signs or approves"));
            }
            if f.kind == FieldKind::Formula && party.is_some_and(|p| !p.must_act()) {
                return Err(bad_request("A formula belongs to you or to a party who signs or approves"));
            }
            let label = f.label.as_deref().map(|l| clean_text(l, "A field label", 200)).transpose()?.unwrap_or_default();
            let spec = Spec {
                options: f.options.clone(),
                validation: f.validation.clone().unwrap_or_default(),
                format: f.format.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string),
            };
            let group_key = f.group_key.as_deref().map(str::trim).filter(|g| !g.is_empty());
            let options = values::check_spec(f.kind, &spec, group_key)?;
            let spec = Spec { options, ..spec };
            let value = f.value.as_deref().map(|v| clean_text(v, "A field value", 2000)).transpose()?.filter(|v| !v.is_empty());
            let value = match f.kind {
                FieldKind::Checkbox if value.as_deref().is_some_and(|v| v != "true" && v != "false") => {
                    return Err(bad_request("A checkbox value is true or false"));
                }
                _ if f.kind.is_upload() && value.is_some() => return Err(bad_request("Image and file fields are filled by the party")),
                FieldKind::Formula => None,
                FieldKind::Checkbox | FieldKind::Signature => value,
                kind => match value {
                    Some(v) => values::normalize(kind, &spec, &v).map_err(|e| {
                        bad_request(values::error_message(&spec, if label.is_empty() { kind.as_str() } else { &label }, &e))
                    })?,
                    None => None,
                },
            };
            let read_only = f.read_only.unwrap_or(false);
            if read_only
                && (f.party_id.is_none()
                    || f.kind.is_upload()
                    || f.kind.is_automatic()
                    || matches!(f.kind, FieldKind::Signature | FieldKind::Initials | FieldKind::Formula))
            {
                return Err(bad_request("Only a party's fields that take a value can be read-only"));
            }
            if let Some(key) = group_key {
                let group = groups.entry(key.to_string()).or_insert((f.party_id, Vec::new(), false));
                if group.0 != f.party_id {
                    return Err(bad_request("All radio buttons of a group belong to the same party"));
                }
                if group.1.contains(&spec.options[0]) {
                    return Err(bad_request(format!("The option \u{201c}{}\u{201d} is in its radio group twice", spec.options[0])));
                }
                group.1.push(spec.options[0].clone());
                if value.is_some() {
                    if group.2 {
                        return Err(bad_request("Only one radio button of a group can be chosen"));
                    }
                    group.2 = true;
                }
            }
            let conditions: Vec<Condition> = f
                .conditions
                .iter()
                .map(|c| Condition {
                    field: c.field.trim().to_string(),
                    op: c.op,
                    value: c.value.as_deref().map(str::trim).filter(|v| c.op.takes_value() && !v.is_empty()).map(str::to_string),
                })
                .collect();
            let formula = f.formula.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
            let owner = match party {
                Some(p) => Owner::Party(p.id, document.sequential.then_some(p.sign_order)),
                None => Owner::Sender,
            };
            prepared.push(Prepared {
                input: f,
                owner,
                label,
                value,
                spec,
                group_key: group_key.map(str::to_string),
                read_only,
                required: f.kind != FieldKind::Formula && f.required.unwrap_or(f.kind != FieldKind::Checkbox),
                conditions,
                formula,
            });
        }
        let items: Vec<logic::Item> = prepared
            .iter()
            .map(|p| logic::Item {
                label: &p.label,
                kind: p.input.kind,
                value: p.value.as_deref(),
                conditions: &p.conditions,
                formula: p.formula.as_deref(),
                format: p.spec.format.as_deref(),
            })
            .collect();
        let owners: Vec<Owner> = prepared.iter().map(|p| p.owner).collect();
        logic::check(&items, &owners)?;
        // Formulas the sender fills in only use the sender's own fields, so they are known now.
        let outcomes = logic::evaluate(&items);

        sqlx::query("delete from fields where document_id = $1").bind(document.id).execute(&mut *conn).await?;
        for (p, outcome) in prepared.iter().zip(outcomes) {
            let f = p.input;
            let value = match (f.kind, p.owner) {
                (FieldKind::Formula, Owner::Sender) => outcome.value,
                _ => p.value.clone(),
            };
            sqlx::query(
                "insert into fields (id, document_id, party_id, kind, page, x, y, width, height, required, label, value, options, validation, format, group_key, read_only, conditions, formula)
                 values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19)",
            )
            .bind(Uuid::now_v7())
            .bind(document.id)
            .bind(f.party_id)
            .bind(f.kind.as_str())
            .bind(f.page)
            .bind(f.x)
            .bind(f.y)
            .bind(f.width)
            .bind(f.height)
            .bind(p.required)
            .bind(&p.label)
            .bind(value)
            .bind(json!(p.spec.options))
            .bind(serde_json::to_value(&p.spec.validation).map_err(AppError::internal)?)
            .bind(&p.spec.format)
            .bind(&p.group_key)
            .bind(p.read_only)
            .bind(serde_json::to_value(&p.conditions).map_err(AppError::internal)?)
            .bind(&p.formula)
            .execute(&mut *conn)
            .await?;
        }
        audit(ctx, conn, Entry::new("document.fields_set", "document", document.id).metadata(json!({ "count": input.fields.len() })))
            .await?;
        reload(ctx, conn, document.id).await
    }
}

struct Prepared<'a> {
    input: &'a FieldInput,
    owner: Owner,
    label: String,
    value: Option<String>,
    spec: Spec,
    group_key: Option<String>,
    read_only: bool,
    required: bool,
    conditions: Vec<Condition>,
    formula: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SendInput {
    pub id: Uuid,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SignerLink {
    pub party_id: Uuid,
    pub name: String,
    pub url: String,
}

#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SendOutput {
    pub document: DocumentView,
    /// Links for parties with `link` delivery, to hand over yourself.
    pub links: Vec<SignerLink>,
}

fn email_ready(ctx: &Ctx, settings: &EmailSettings) -> bool {
    ctx.app.env.development || crate::email::is_configured(settings)
}

async fn check_auth_available(ctx: &Ctx, conn: &mut PgConnection, method: AuthMethod, party: &PartyRow) -> Result<()> {
    match method {
        AuthMethod::None | AuthMethod::Standard => Ok(()),
        AuthMethod::SmsPin => {
            let sms: SmsSettings = settings(ctx, conn).await?;
            if party.phone.is_none() {
                return Err(bad_request(format!("{} needs a phone number for SMS codes", party.name)));
            }
            if !crate::sms::is_configured(&sms) && !ctx.app.env.development {
                return Err(bad_request("SMS is not set up. An admin chooses an SMS provider in Admin settings."));
            }
            Ok(())
        }
        AuthMethod::SeBankid | AuthMethod::Freja | AuthMethod::Broker => {
            let eid: EidSettings = settings(ctx, conn).await?;
            crate::eid::ready(&eid, method).map_err(|e| bad_request(format!("{} cannot be used yet: {}", method.label(), e.message)))
        }
    }
}

#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AuthMethods {
    /// The ways a party can be asked to identify on this server.
    pub available: Vec<AuthMethod>,
}

pub struct ListAuthMethods;

#[async_trait]
impl Service for ListAuthMethods {
    type Input = super::Empty;
    type Output = AuthMethods;
    const META: Meta = Meta::query(
        "documents.auth_methods",
        "The identification methods parties can be asked for here: email link always, SMS codes and eIDs when an admin has set them up.",
        Access::Authenticated,
    );

    async fn run(ctx: &Ctx, conn: &mut PgConnection, _: super::Empty) -> Result<AuthMethods> {
        let sms: SmsSettings = settings(ctx, conn).await?;
        let eid: EidSettings = settings(ctx, conn).await?;
        let mut available = vec![AuthMethod::None, AuthMethod::Standard];
        if crate::sms::is_configured(&sms) || ctx.app.env.development {
            available.push(AuthMethod::SmsPin);
        }
        available.extend(
            [AuthMethod::SeBankid, AuthMethod::Freja, AuthMethod::Broker].into_iter().filter(|m| crate::eid::ready(&eid, *m).is_ok()),
        );
        Ok(AuthMethods { available })
    }
}

pub struct Send;

#[async_trait]
impl Service for Send {
    type Input = SendInput;
    type Output = SendOutput;
    const META: Meta = Meta::mutation(
        "documents.send",
        "Send a draft for signing. Its files are combined into one PDF that can no longer change, and parties are notified in signing order.",
        Access::Permission(Permission::DocumentsCreate),
    );

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: SendInput) -> Result<SendOutput> {
        let document = load_document(ctx, conn, input.id, Need::Edit).await?;
        if document.is_template {
            return Err(bad_request("Templates cannot be sent. Create a document from it first."));
        }
        let state: String =
            sqlx::query_scalar("select status from documents where id = $1 for update").bind(document.id).fetch_one(&mut *conn).await?;
        if state != status::DRAFT {
            return Err(conflict("Only drafts can be sent"));
        }
        let records = files(conn, document.id).await?;
        if records.is_empty() {
            return Err(bad_request("Add a file before sending"));
        }
        let all = parties(conn, document.id).await?;
        if !all.iter().any(PartyRow::must_act) {
            return Err(bad_request("Add at least one party who signs or approves"));
        }
        let email_settings: EmailSettings = settings(ctx, conn).await?;
        let sms_ready = crate::sms::is_configured(&settings::<SmsSettings>(ctx, conn).await?) || ctx.app.env.development;
        for p in &all {
            match Delivery::parse(&p.delivery) {
                Some(Delivery::Email) if p.email.is_none() => return Err(bad_request(format!("{} needs an email address", p.name))),
                Some(Delivery::Email) if !email_ready(ctx, &email_settings) => {
                    return Err(bad_request("Email is not set up. An admin connects Resend or an SMTP server in Admin settings."));
                }
                Some(Delivery::Sms) if p.phone.is_none() => return Err(bad_request(format!("{} needs a phone number", p.name))),
                Some(Delivery::Sms) if !sms_ready => {
                    return Err(bad_request("SMS is not set up. An admin chooses an SMS provider in Admin settings."));
                }
                _ => {}
            }
            check_auth_available(ctx, conn, p.auth_to_view(), p).await?;
            check_auth_available(ctx, conn, p.auth_to_sign(), p).await?;
        }
        let expires_at = input.expires_at.or(document.expires_at);
        if expires_at.is_some_and(|at| at <= ctx.now + Duration::hours(1)) {
            return Err(bad_request("The expiry must be at least an hour from now"));
        }

        let mut bodies = Vec::with_capacity(records.len());
        for record in &records {
            bodies.push(read_file(&ctx.app.storage, ctx.keys(), record).await?);
        }
        let combined = tokio::task::spawn_blocking(move || {
            opensign_pdf::merge(&bodies).and_then(|m| opensign_pdf::sanitize(&m, opensign_pdf::Limits::default()))
        })
        .await
        .map_err(AppError::internal)?
        .map_err(upload::pdf_failure)?;
        let page_count = combined.pages.len() as i32;
        for f in crate::documents::fields(conn, document.id).await? {
            if f.page >= page_count {
                return Err(bad_request(format!("A field is on page {} but the document has {page_count} pages", f.page + 1)));
            }
            if f.party_id.is_none() && f.required && f.value.is_none() {
                return Err(bad_request("Fill in the sender's fields before sending"));
            }
        }
        let source = store_file(
            conn,
            &ctx.app.storage,
            ctx.keys(),
            NewFile {
                kind: "source",
                name: &format!("{}.pdf", notify::sanitize_filename(&document.title)),
                content_type: "application/pdf",
                body: &combined.pdf,
                uploaded_by: ctx.actor.as_ref().map(|a| a.user_id),
            },
        )
        .await?;
        let security: SecuritySettings = settings(ctx, conn).await?;
        let link_expires = expires_at.unwrap_or(ctx.now + Duration::days(security.signer_link_days.max(1) as i64));
        sqlx::query(
            "update documents set status = 'pending', source_file_id = $2, original_sha256 = $3, page_count = $4, sent_at = $5, expires_at = $6, updated_at = now() where id = $1",
        )
        .bind(document.id)
        .bind(source.id)
        .bind(&source.sha256)
        .bind(page_count)
        .bind(ctx.now)
        .bind(expires_at)
        .execute(&mut *conn)
        .await?;
        sqlx::query("update parties set link_expires_at = $2, updated_at = now() where document_id = $1")
            .bind(document.id)
            .bind(link_expires)
            .execute(&mut *conn)
            .await?;
        audit(
            ctx,
            conn,
            Entry::new("document.sent", "document", document.id)
                .metadata(json!({ "sha256": source.sha256, "pages": page_count, "parties": all.len() })),
        )
        .await?;
        let document = load_document(ctx, conn, document.id, Need::Read).await?;
        notify::notify_turn(conn, &document).await?;
        notify::emit(
            conn,
            "document.sent",
            json!({ "documentId": document.id, "title": document.title, "externalId": document.external_id }),
        )
        .await?;
        let links = parties(conn, document.id)
            .await?
            .iter()
            .filter(|p| p.delivery == Delivery::Link.as_str())
            .map(|p| SignerLink { party_id: p.id, name: p.name.clone(), url: signer_url(ctx, ctx.keys(), p) })
            .collect();
        Ok(SendOutput { document: view(ctx, conn, document).await?, links })
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CancelInput {
    pub id: Uuid,
    pub reason: Option<String>,
}

pub struct Cancel;

#[async_trait]
impl Service for Cancel {
    type Input = CancelInput;
    type Output = DocumentView;
    const META: Meta = Meta::mutation(
        "documents.cancel",
        "Stop signing a pending document. Every signing link stops working and notified parties are told.",
        Access::Permission(Permission::DocumentsCreate),
    );

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: CancelInput) -> Result<DocumentView> {
        let document = load_document(ctx, conn, input.id, Need::Edit).await?;
        let done = sqlx::query(
            "update documents set status = 'canceled', canceled_at = $2, updated_at = now() where id = $1 and status = 'pending'",
        )
        .bind(document.id)
        .bind(ctx.now)
        .execute(&mut *conn)
        .await?;
        if done.rows_affected() == 0 {
            return Err(conflict("Only pending documents can be canceled"));
        }
        sqlx::query("update parties set link_version = link_version + 1, status = case when signed_at is null then 'canceled' else status end, updated_at = now() where document_id = $1")
            .bind(document.id)
            .execute(&mut *conn)
            .await?;
        let reason = input.reason.as_deref().map(|r| clean_text(r, "The reason", 1000)).transpose()?;
        audit(ctx, conn, Entry::new("document.canceled", "document", document.id).metadata(json!({ "reason": reason }))).await?;
        crate::jobs::enqueue(conn, "notify_canceled", json!({ "document": document.id }), None, None).await?;
        notify::emit(conn, "document.canceled", json!({ "documentId": document.id, "externalId": document.external_id })).await?;
        reload(ctx, conn, document.id).await
    }
}

pub struct Remind;

#[async_trait]
impl Service for Remind {
    type Input = IdInput;
    type Output = Ok_;
    const META: Meta = Meta::mutation(
        "documents.remind",
        "Send a reminder now to every party whose turn it is and who has not signed.",
        Access::Permission(Permission::DocumentsCreate),
    );

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: IdInput) -> Result<Ok_> {
        let document = load_document(ctx, conn, input.id, Need::Edit).await?;
        if document.status != status::PENDING {
            return Err(conflict("Only pending documents have parties to remind"));
        }
        let all = parties(conn, document.id).await?;
        for id in current_turn(&document, &all) {
            crate::jobs::enqueue(conn, "notify_party", json!({ "party": id, "reminder": true }), None, Some(&format!("notify:{id}")))
                .await?;
        }
        audit(ctx, conn, Entry::new("document.reminded", "document", document.id)).await?;
        Ok(Ok_::default())
    }
}

pub struct Delete;

#[async_trait]
impl Service for Delete {
    type Input = IdInput;
    type Output = Ok_;
    const META: Meta = Meta::mutation(
        "documents.delete",
        "Delete a document or template that is not pending, with its files. Signed documents can only be deleted by someone who manages all documents, and documents on legal hold by no one.",
        Access::Permission(Permission::DocumentsCreate),
    );

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: IdInput) -> Result<Ok_> {
        let document = load_document(ctx, conn, input.id, Need::Edit).await?;
        if document.status == status::PENDING {
            return Err(conflict("Cancel the document before deleting it"));
        }
        if document.status == status::COMPLETED && !ctx.can(Permission::DocumentsManageAll) {
            return Err(forbidden());
        }
        crate::documents::purge(ctx, conn, &document, "manual").await?;
        Ok(Ok_::default())
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ShareInput {
    pub id: Uuid,
    pub user_id: Uuid,
    #[serde(default)]
    pub can_edit: bool,
}

pub struct Share;

#[async_trait]
impl Service for Share {
    type Input = ShareInput;
    type Output = Ok_;
    const META: Meta = Meta::mutation(
        "documents.share",
        "Let a colleague see a document, and optionally change it.",
        Access::Permission(Permission::DocumentsCreate),
    );

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: ShareInput) -> Result<Ok_> {
        let document = load_document(ctx, conn, input.id, Need::Edit).await?;
        let exists: Option<bool> =
            sqlx::query_scalar("select active from users where id = $1").bind(input.user_id).fetch_optional(&mut *conn).await?;
        if exists != Some(true) {
            return Err(not_found("User"));
        }
        sqlx::query("insert into document_access (document_id, user_id, can_edit) values ($1, $2, $3) on conflict (document_id, user_id) do update set can_edit = excluded.can_edit")
            .bind(document.id)
            .bind(input.user_id)
            .bind(input.can_edit)
            .execute(&mut *conn)
            .await?;
        audit(
            ctx,
            conn,
            Entry::new("document.shared", "document", document.id).metadata(json!({ "user": input.user_id, "canEdit": input.can_edit })),
        )
        .await?;
        Ok(Ok_::default())
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UnshareInput {
    pub id: Uuid,
    pub user_id: Uuid,
}

pub struct Unshare;

#[async_trait]
impl Service for Unshare {
    type Input = UnshareInput;
    type Output = Ok_;
    const META: Meta =
        Meta::mutation("documents.unshare", "Stop sharing a document with a colleague.", Access::Permission(Permission::DocumentsCreate));

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: UnshareInput) -> Result<Ok_> {
        let document = load_document(ctx, conn, input.id, Need::Edit).await?;
        sqlx::query("delete from document_access where document_id = $1 and user_id = $2")
            .bind(document.id)
            .bind(input.user_id)
            .execute(&mut *conn)
            .await?;
        audit(ctx, conn, Entry::new("document.unshared", "document", document.id).metadata(json!({ "user": input.user_id }))).await?;
        Ok(Ok_::default())
    }
}

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Version {
    /// The sealed PDF with signatures and evidence. Only for completed documents.
    Signed,
    /// The PDF exactly as it was sent for signing.
    Original,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DownloadInput {
    pub id: Uuid,
    pub version: Option<Version>,
}

#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DownloadOutput {
    pub name: String,
    pub content_type: String,
    pub sha256: String,
    pub content_base64: String,
}

/// The file record for a document's signed or original PDF, after the read check.
pub async fn document_file(ctx: &Ctx, conn: &mut PgConnection, id: Uuid, version: Version) -> Result<FileRecord> {
    let document = load_document(ctx, conn, id, Need::Read).await?;
    let file_id = match version {
        Version::Signed => document.sealed_file_id.ok_or_else(|| conflict("The document is not signed yet"))?,
        Version::Original => document.source_file_id.ok_or_else(|| conflict("The document has not been sent yet"))?,
    };
    let record = file_record(conn, file_id).await?;
    audit(
        ctx,
        conn,
        Entry::new("document.downloaded", "document", document.id).metadata(json!({ "version": format!("{version:?}").to_lowercase() })),
    )
    .await?;
    Ok(record)
}

/// A file a party uploaded, for someone who may read the document. Every download is audited.
pub async fn attachment_file(ctx: &Ctx, conn: &mut PgConnection, document_id: Uuid, request_id: Uuid) -> Result<FileRecord> {
    let document = load_document(ctx, conn, document_id, Need::Read).await?;
    let request = attachments::for_document(conn, document.id)
        .await?
        .into_iter()
        .find(|r| r.id == request_id)
        .ok_or_else(|| not_found("Attachment"))?;
    let file_id = request.file_id.ok_or_else(|| conflict("Nothing has been uploaded yet"))?;
    let record = file_record(conn, file_id).await?;
    audit(
        ctx,
        conn,
        Entry::new("document.attachment_downloaded", "document", document.id)
            .metadata(json!({ "attachment": request.name, "file": record.name, "party": request.party_id })),
    )
    .await?;
    Ok(record)
}

pub struct Download;

#[async_trait]
impl Service for Download {
    type Input = DownloadInput;
    type Output = DownloadOutput;
    const META: Meta = Meta::query(
        "documents.download",
        "Download a document's signed PDF (default) or the original as sent, base64. Every download is audited.",
        Access::Authenticated,
    );

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: DownloadInput) -> Result<DownloadOutput> {
        let record = document_file(ctx, conn, input.id, input.version.unwrap_or(Version::Signed)).await?;
        if record.size_bytes > MAX_DOWNLOAD {
            return Err(bad_request("The file is too large to return here. Download it from the web app or the REST API."));
        }
        let body = read_file(&ctx.app.storage, ctx.keys(), &record).await?;
        Ok(DownloadOutput { name: record.name, content_type: record.content_type, sha256: record.sha256, content_base64: B64.encode(body) })
    }
}

/// The PDF to show in the editor: the sent document once there is one, otherwise the draft's files
/// combined the way sending will combine them.
pub async fn preview_pdf(ctx: &Ctx, id: Uuid) -> Result<(String, Vec<u8>)> {
    let mut conn = ctx.db().acquire().await?;
    let document = load_document(ctx, &mut conn, id, Need::Read).await?;
    let name = format!("{}.pdf", notify::sanitize_filename(&document.title));
    if let Some(file_id) = document.sealed_file_id.or(document.source_file_id) {
        let record = file_record(&mut conn, file_id).await?;
        return Ok((name, read_file(&ctx.app.storage, ctx.keys(), &record).await?));
    }
    let records = files(&mut conn, document.id).await?;
    if records.is_empty() {
        return Err(not_found("File"));
    }
    let mut bodies = Vec::with_capacity(records.len());
    for record in &records {
        bodies.push(read_file(&ctx.app.storage, ctx.keys(), record).await?);
    }
    let merged = tokio::task::spawn_blocking(move || opensign_pdf::merge(&bodies))
        .await
        .map_err(AppError::internal)?
        .map_err(upload::pdf_failure)?;
    Ok((name, merged))
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SigningLinkInput {
    pub document_id: Uuid,
    pub party_id: Uuid,
    /// Revoke the party's earlier links first, e.g. when one was forwarded by mistake.
    #[serde(default)]
    pub rotate: bool,
    /// For embedded signing: where the signing page sends the party after they sign or decline,
    /// with `event=signing_complete` or `event=decline` added. Must be on a site allowed to embed.
    pub redirect_url: Option<String>,
}

pub struct IssueSigningLink;

#[async_trait]
impl Service for IssueSigningLink {
    type Input = SigningLinkInput;
    type Output = SignerLink;
    const META: Meta = Meta::mutation(
        "documents.signing_link",
        "Get a party's personal signing link to hand over yourself, optionally revoking the old one.",
        Access::Permission(Permission::DocumentsCreate),
    );

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: SigningLinkInput) -> Result<SignerLink> {
        let document = load_document(ctx, conn, input.document_id, Need::Edit).await?;
        if document.status != status::PENDING {
            return Err(conflict("Signing links exist only while a document is pending"));
        }
        if input.rotate {
            sqlx::query("update parties set link_version = link_version + 1 where id = $1 and document_id = $2")
                .bind(input.party_id)
                .bind(document.id)
                .execute(&mut *conn)
                .await?;
        }
        let party = parties(conn, document.id).await?.into_iter().find(|p| p.id == input.party_id).ok_or_else(|| not_found("Party"))?;
        let mut url = signer_url(ctx, ctx.keys(), &party);
        if let Some(redirect) = input.redirect_url.as_deref().filter(|r| !r.is_empty()) {
            let security: SecuritySettings = settings(ctx, conn).await?;
            if !security.allows_redirect(redirect) {
                return Err(bad_request(
                    "The redirect URL must be on a site allowed to embed, under Security in Admin settings, with embedded signing turned on",
                ));
            }
            url = with_redirect(&url, redirect);
        }
        audit(
            ctx,
            conn,
            Entry::new("party.link_issued", "document", document.id)
                .metadata(json!({ "party": party.id, "name": party.name, "rotated": input.rotate, "redirectUrl": input.redirect_url })),
        )
        .await?;
        Ok(SignerLink { party_id: party.id, name: party.name.clone(), url })
    }
}

/// `…/sign?redirect=<url>#<token>`: the token stays in the fragment.
fn with_redirect(url: &str, redirect: &str) -> String {
    let (base, token) = url.split_once('#').unwrap_or((url, ""));
    let Ok(mut parsed) = reqwest::Url::parse(base) else { return url.to_string() };
    parsed.query_pairs_mut().append_pair("redirect", redirect);
    format!("{parsed}#{token}")
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct InPersonInput {
    pub document_id: Uuid,
    pub party_id: Uuid,
}

pub struct SignInPerson;

#[async_trait]
impl Service for SignInPerson {
    type Input = InPersonInput;
    type Output = SignerLink;
    const META: Meta = Meta::mutation(
        "documents.sign_in_person",
        "Start in-person signing: a fresh signing link for a party who signs on the sender's own device. Earlier links for the party stop working. Only for parties whose delivery is in_person and whose turn it is.",
        Access::Permission(Permission::DocumentsCreate),
    )
    .not_a_tool();

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: InPersonInput) -> Result<SignerLink> {
        let document = load_document(ctx, conn, input.document_id, Need::Edit).await?;
        if document.status != status::PENDING {
            return Err(conflict("Only a pending document can be signed"));
        }
        let all = parties(conn, document.id).await?;
        let party = all.iter().find(|p| p.id == input.party_id).ok_or_else(|| not_found("Party"))?;
        if party.delivery != Delivery::InPerson.as_str() || !party.must_act() {
            return Err(bad_request(format!("{} does not sign in person", party.name)));
        }
        if party.signed_at.is_some() || party.declined_at.is_some() {
            return Err(conflict(format!("{} has already answered", party.name)));
        }
        if !current_turn(&document, &all).contains(&party.id) {
            return Err(conflict(format!("It is not {}'s turn yet", party.name)));
        }
        sqlx::query("update parties set link_version = link_version + 1 where id = $1").bind(party.id).execute(&mut *conn).await?;
        let party = parties(conn, document.id).await?.into_iter().find(|p| p.id == input.party_id).ok_or_else(|| not_found("Party"))?;
        audit(
            ctx,
            conn,
            Entry::new("party.in_person_started", "document", document.id).metadata(json!({ "party": party.id, "name": party.name })),
        )
        .await?;
        Ok(SignerLink { party_id: party.id, name: party.name.clone(), url: signer_url(ctx, ctx.keys(), &party) })
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SigningPreviewInput {
    pub document_id: Uuid,
    pub party_id: Uuid,
}

pub struct SigningPreview;

#[async_trait]
impl Service for SigningPreview {
    type Input = SigningPreviewInput;
    type Output = SignerView;
    const META: Meta = Meta::query(
        "documents.signing_preview",
        "Show what a party will see on their signing page: branding, message, language and their fields, with the identification they will be asked for. Read-only: nothing is sent, signed or recorded as the party's.",
        Access::Authenticated,
    );

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: SigningPreviewInput) -> Result<SignerView> {
        let document = load_document(ctx, conn, input.document_id, Need::Read).await?;
        let party = parties(conn, document.id).await?.into_iter().find(|p| p.id == input.party_id).ok_or_else(|| not_found("Party"))?;
        let mut view = signer_view(ctx, conn, &document, &party, false).await?;
        let required = |method: AuthMethod| (!matches!(method, AuthMethod::None | AuthMethod::Standard)).then_some(method);
        view.needs_authentication = required(party.auth_to_view());
        view.sign_authentication = required(party.auth_to_sign());
        view.your_turn = true;
        Ok(view)
    }
}

/// Current signing links of a pending document's parties that sign through a link the sender hands
/// over, for compatibility APIs that show them on every read. Editors only; issuing a link stays audited.
pub async fn handed_over_links(ctx: &Ctx, conn: &mut PgConnection, id: Uuid) -> Result<Vec<(Uuid, String)>> {
    if !ctx.can(Permission::DocumentsCreate) {
        return Ok(Vec::new());
    }
    let document = match load_document(ctx, conn, id, Need::Edit).await {
        Ok(d) if d.status == status::PENDING => d,
        _ => return Ok(Vec::new()),
    };
    Ok(parties(conn, document.id)
        .await?
        .iter()
        .filter(|p| p.delivery == "link" && p.must_act() && p.signed_at.is_none() && p.declined_at.is_none())
        .map(|p| (p.id, signer_url(ctx, ctx.keys(), p)))
        .collect())
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct VerifyInput {
    /// A PDF, base64.
    pub content_base64: String,
}

#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct VerifiedDocument {
    pub id: Uuid,
    pub title: String,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct VerifyOutput {
    pub sha256: String,
    #[schemars(with = "Vec<Value>")]
    pub signatures: Vec<SignatureCheck>,
    /// The document this exact file was sealed for, when OpenSign sealed it.
    pub document: Option<VerifiedDocument>,
}

/// Checks a PDF's signatures and whether this server sealed exactly these bytes.
pub async fn verify_pdf(conn: &mut PgConnection, bytes: Vec<u8>) -> Result<VerifyOutput> {
    if !bytes.windows(5).take(1024).any(|w| w == b"%PDF-") {
        return Err(bad_request("This is not a PDF file"));
    }
    let sha256 = sha256_hex(&bytes);
    let signatures = tokio::task::spawn_blocking(move || opensign_pdf::verify_signatures(&bytes))
        .await
        .map_err(AppError::internal)?
        .map_err(|_| bad_request("This PDF could not be read"))?;
    let document = sqlx::query_as::<_, (Uuid, String, Option<DateTime<Utc>>)>(
        "select id, title, completed_at from documents where sealed_sha256 = $1 and status = 'completed'",
    )
    .bind(&sha256)
    .fetch_optional(&mut *conn)
    .await?
    .map(|(id, title, completed_at)| VerifiedDocument { id, title, completed_at });
    Ok(VerifyOutput { sha256, signatures, document })
}

pub struct Verify;

#[async_trait]
impl Service for Verify {
    type Input = VerifyInput;
    type Output = VerifyOutput;
    const META: Meta = Meta::query(
        "documents.verify",
        "Check a signed PDF (base64): whether its signatures are intact and whether this server sealed exactly this file.",
        Access::Authenticated,
    );

    async fn run(_ctx: &Ctx, conn: &mut PgConnection, input: VerifyInput) -> Result<VerifyOutput> {
        if input.content_base64.len() > 70 * 1024 * 1024 {
            return Err(bad_request("The file is too large"));
        }
        let bytes = B64.decode(input.content_base64.trim()).map_err(|_| bad_request("contentBase64 is not valid base64"))?;
        verify_pdf(conn, bytes).await
    }
}

#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SealInfo {
    pub subject: String,
    pub not_after: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

pub struct GetSeal;

#[async_trait]
impl Service for GetSeal {
    type Input = super::Empty;
    type Output = Option<SealInfo>;
    const META: Meta = Meta::query(
        "seal.get",
        "Show the certificate that seals completed documents. None until the first document is sealed.",
        Access::Permission(Permission::SettingsManage),
    );

    async fn run(_ctx: &Ctx, conn: &mut PgConnection, _: super::Empty) -> Result<Option<SealInfo>> {
        Ok(sqlx::query_as::<_, (String, DateTime<Utc>, DateTime<Utc>)>(
            "select subject, not_after, created_at from seal_identities where active",
        )
        .fetch_optional(&mut *conn)
        .await?
        .map(|(subject, not_after, created_at)| SealInfo { subject, not_after, created_at }))
    }
}

pub struct RegenerateSeal;

#[async_trait]
impl Service for RegenerateSeal {
    type Input = super::Empty;
    type Output = SealInfo;
    const META: Meta = Meta::mutation(
        "seal.regenerate",
        "Replace the document seal with a new self-signed certificate in the organisation's current name. Documents already sealed are unaffected.",
        Access::Permission(Permission::SettingsManage),
    )
    .not_a_tool();

    async fn run(ctx: &Ctx, conn: &mut PgConnection, _: super::Empty) -> Result<SealInfo> {
        let org: crate::settings::OrgSettings = settings(ctx, conn).await?;
        let signing: crate::settings::SigningSettings = settings(ctx, conn).await?;
        let name = if signing.seal_name.trim().is_empty() { org.name.clone() } else { signing.seal_name.clone() };
        let identity = opensign_pdf::generate_seal_identity(&name, &org.country, 10).map_err(AppError::internal)?;
        crate::documents::seal::store_identity(ctx, conn, &identity).await?;
        GetSeal::run(ctx, conn, super::Empty {}).await?.ok_or_else(|| AppError::new(ErrorKind::Internal, "The seal was not stored"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pdf_names_and_like_patterns() {
        assert_eq!(pdf_name("C:\\Users\\x\\Avtal 2026.docx"), "Avtal 2026.pdf");
        assert_eq!(pdf_name("../../evil"), "evil.pdf");
        assert_eq!(like_pattern("50%_off\\"), "%50\\%\\_off\\\\%");
    }
}
