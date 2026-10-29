//! Documents, their parties and fields: shared types, who may see what, and signer links.

pub mod attachments;
pub mod logic;
pub mod ltv;
pub mod notify;
pub mod seal;
pub mod tags;
pub mod upload;
pub mod values;
pub mod versions;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use chrono::{DateTime, Utc};
use opensign_crypto::{KeyPurpose, Keys, constant_time_eq};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::context::Ctx;
use crate::error::{Result, not_found};
use crate::permissions::Permission;
use crate::storage::FileRecord;

macro_rules! string_enum {
    ($(#[$meta:meta])* $name:ident { $($variant:ident = $value:literal,)* }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
        pub enum $name {
            $(#[serde(rename = $value)] $variant,)*
        }

        impl $name {
            pub const ALL: &'static [$name] = &[$($name::$variant,)*];

            pub fn as_str(self) -> &'static str {
                match self { $($name::$variant => $value,)* }
            }

            pub fn parse(value: &str) -> Option<$name> {
                match value { $($value => Some($name::$variant),)* _ => None }
            }
        }
    };
}

string_enum!(
    /// Signers sign, approvers approve without a signature image, viewers receive the signed copy.
    PartyRole { Signer = "signer", Approver = "approver", Viewer = "viewer", }
);

string_enum!(
    /// How the party gets their link. `link` hands it to the sender (or the calling system) instead;
    /// `in_person` means the party signs on the sender's own device, started from the document page.
    Delivery { Email = "email", Sms = "sms", Link = "link", InPerson = "in_person", None = "none", }
);

string_enum!(
    /// Proof of identity a party must give. `standard` is possession of the link.
    AuthMethod {
        None = "none",
        Standard = "standard",
        SmsPin = "sms_pin",
        SeBankid = "se_bankid",
        Freja = "freja",
        Broker = "broker",
    }
);

string_enum!(
    /// `date` is the date of signing, filled in automatically; `date_input` is a date the signer picks.
    /// `select` is a dropdown, `multiselect` a list to tick several from, and each `radio` is one
    /// button of a group (`groupKey`) holding one option. `image` and `file` take an upload. `formula`
    /// is a number computed from other fields.
    FieldKind {
        Signature = "signature",
        Initials = "initials",
        Text = "text",
        Date = "date",
        Checkbox = "checkbox",
        Name = "name",
        Email = "email",
        Company = "company",
        Number = "number",
        Select = "select",
        Multiselect = "multiselect",
        Radio = "radio",
        Phone = "phone",
        DateInput = "date_input",
        Image = "image",
        File = "file",
        Formula = "formula",
    }
);

impl AuthMethod {
    pub fn label(self) -> &'static str {
        match self {
            AuthMethod::None | AuthMethod::Standard => "Email link",
            AuthMethod::SmsPin => "SMS code",
            AuthMethod::SeBankid => "Swedish BankID",
            AuthMethod::Freja => "Freja eID",
            AuthMethod::Broker => "eID",
        }
    }
}

impl FieldKind {
    /// Filled from the party's details when they sign, not typed by them.
    pub fn is_automatic(self) -> bool {
        matches!(self, FieldKind::Date | FieldKind::Name | FieldKind::Email)
    }

    /// Filled by uploading an image or a file while signing.
    pub fn is_upload(self) -> bool {
        matches!(self, FieldKind::Image | FieldKind::File)
    }
}

pub mod status {
    pub const DRAFT: &str = "draft";
    pub const PENDING: &str = "pending";
    pub const COMPLETED: &str = "completed";
    pub const DECLINED: &str = "declined";
    pub const CANCELED: &str = "canceled";
    pub const EXPIRED: &str = "expired";
    /// Statuses a document can be archived in.
    pub const FINISHED: &[&str] = &[COMPLETED, DECLINED, CANCELED, EXPIRED];
}

pub const DOCUMENT_COLUMNS: &str = "id, title, status, is_template, template_id, owner_id, message, language, sequential, expires_at, reminder_days, \
     source_file_id, sealed_file_id, original_sha256, sealed_sha256, page_count, external_id, metadata, sent_at, completed_at, canceled_at, created_at, updated_at, \
     legal_hold, legal_hold_reason, legal_hold_at, review_links, folder_id, archived_at, template_version";

#[derive(Debug, Clone, Serialize, JsonSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct DocumentRow {
    pub id: Uuid,
    pub title: String,
    pub status: String,
    pub is_template: bool,
    pub template_id: Option<Uuid>,
    pub owner_id: Option<Uuid>,
    pub message: String,
    pub language: String,
    pub sequential: bool,
    pub expires_at: Option<DateTime<Utc>>,
    pub reminder_days: Option<i32>,
    #[serde(skip)]
    #[schemars(skip)]
    pub source_file_id: Option<Uuid>,
    #[serde(skip)]
    #[schemars(skip)]
    pub sealed_file_id: Option<Uuid>,
    pub original_sha256: Option<String>,
    pub sealed_sha256: Option<String>,
    pub page_count: Option<i32>,
    pub external_id: Option<String>,
    pub metadata: Value,
    pub sent_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub canceled_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// On legal hold: no retention rule or person can delete it until the hold is released.
    pub legal_hold: bool,
    pub legal_hold_reason: Option<String>,
    pub legal_hold_at: Option<DateTime<Utc>>,
    /// Signers may share a read-only copy with their own AI agent.
    pub review_links: bool,
    pub folder_id: Option<Uuid>,
    /// Archived documents and templates are left out of lists unless asked for; archived templates cannot be used.
    pub archived_at: Option<DateTime<Utc>>,
    /// The version of the template this document was made from.
    pub template_version: Option<i32>,
}

pub const PARTY_COLUMNS: &str = "id, document_id, role, name, email, phone, personal_number, company, sign_order, delivery, auth_to_view, auth_to_sign, \
     status, link_version, link_expires_at, notified_at, viewed_at, signed_at, declined_at, decline_reason, signature, evidence, language";

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PartyRow {
    pub id: Uuid,
    pub document_id: Uuid,
    pub role: String,
    pub name: String,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub personal_number: Option<String>,
    pub company: Option<String>,
    pub sign_order: i32,
    pub delivery: String,
    pub auth_to_view: String,
    pub auth_to_sign: String,
    pub status: String,
    pub link_version: i32,
    pub link_expires_at: Option<DateTime<Utc>>,
    pub notified_at: Option<DateTime<Utc>>,
    pub viewed_at: Option<DateTime<Utc>>,
    pub signed_at: Option<DateTime<Utc>>,
    pub declined_at: Option<DateTime<Utc>>,
    pub decline_reason: Option<String>,
    pub signature: Option<Value>,
    pub evidence: Option<Value>,
    pub language: Option<String>,
}

impl PartyRow {
    pub fn role(&self) -> PartyRole {
        PartyRole::parse(&self.role).unwrap_or(PartyRole::Viewer)
    }

    pub fn auth_to_view(&self) -> AuthMethod {
        AuthMethod::parse(&self.auth_to_view).unwrap_or(AuthMethod::None)
    }

    pub fn auth_to_sign(&self) -> AuthMethod {
        AuthMethod::parse(&self.auth_to_sign).unwrap_or(AuthMethod::Standard)
    }

    /// Signers and approvers; viewers only receive the result.
    pub fn must_act(&self) -> bool {
        self.role() != PartyRole::Viewer
    }

    pub fn lang(&self, document: &DocumentRow) -> crate::i18n::Lang {
        crate::i18n::Lang::for_party(self.language.as_deref(), &document.language)
    }
}

/// A party as senders see it. The personal number is shown masked.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PartyView {
    pub id: Uuid,
    pub role: String,
    pub name: String,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub personal_number: Option<String>,
    pub company: Option<String>,
    pub sign_order: i32,
    pub delivery: String,
    pub auth_to_view: String,
    pub auth_to_sign: String,
    pub status: String,
    pub notified_at: Option<DateTime<Utc>>,
    pub viewed_at: Option<DateTime<Utc>>,
    pub signed_at: Option<DateTime<Utc>>,
    pub declined_at: Option<DateTime<Utc>>,
    pub decline_reason: Option<String>,
    /// What this party reads, if not the document's language.
    pub language: Option<String>,
    /// The party must take a live webcam photo before signing.
    pub webcam_photo: bool,
}

pub fn personal_number_context(party_id: Uuid) -> String {
    format!("party:{party_id}:personal_number")
}

pub fn decrypt_personal_number(keys: &Keys, party: &PartyRow) -> Option<String> {
    let sealed = party.personal_number.as_deref()?;
    keys.decrypt_str(KeyPurpose::Settings, &personal_number_context(party.id), sealed).ok().map(|s| s.to_string())
}

fn identity_context(party_id: Uuid) -> String {
    format!("party:{party_id}:identity")
}

/// What a party's identification proved can include their personal number, so it is stored
/// encrypted in the party's evidence.
pub fn seal_identity(keys: &Keys, party_id: Uuid, identity: &str) -> String {
    keys.encrypt_str(KeyPurpose::Settings, &identity_context(party_id), identity)
}

pub fn open_identity(keys: &Keys, party: &PartyRow) -> Option<String> {
    let stored = party.evidence.as_ref()?["identity"].as_str()?;
    match keys.decrypt_str(KeyPurpose::Settings, &identity_context(party.id), stored) {
        Ok(plain) => Some(plain.to_string()),
        Err(_) if !stored.starts_with("v1.") => Some(stored.to_string()),
        Err(_) => None,
    }
}

/// For identity checks: a personal number that is set but does not decrypt is an error, never
/// "no number", which would let any eID through.
pub fn required_personal_number(keys: &Keys, party: &PartyRow) -> Result<Option<String>> {
    let Some(sealed) = party.personal_number.as_deref() else { return Ok(None) };
    keys.decrypt_str(KeyPurpose::Settings, &personal_number_context(party.id), sealed)
        .map(|n| Some(n.to_string()))
        .map_err(crate::error::AppError::internal)
}

pub fn mask(value: &str) -> String {
    let chars: Vec<char> = value.chars().filter(|c| !c.is_whitespace()).collect();
    if chars.len() <= 4 {
        return "•".repeat(chars.len());
    }
    let tail: String = chars[chars.len() - 4..].iter().collect();
    format!("{}{tail}", "•".repeat(chars.len() - 4))
}

impl PartyView {
    pub fn from_row(keys: &Keys, row: &PartyRow) -> Self {
        Self {
            id: row.id,
            role: row.role.clone(),
            name: row.name.clone(),
            email: row.email.clone(),
            phone: row.phone.clone(),
            personal_number: decrypt_personal_number(keys, row).map(|n| mask(&n)),
            company: row.company.clone(),
            sign_order: row.sign_order,
            delivery: row.delivery.clone(),
            auth_to_view: row.auth_to_view.clone(),
            auth_to_sign: row.auth_to_sign.clone(),
            status: row.status.clone(),
            notified_at: row.notified_at,
            viewed_at: row.viewed_at,
            signed_at: row.signed_at,
            declined_at: row.declined_at,
            decline_reason: row.decline_reason.clone(),
            language: row.language.clone(),
            webcam_photo: false,
        }
    }
}

pub async fn party_views(ctx: &Ctx, conn: &mut PgConnection, document_id: Uuid) -> Result<Vec<PartyView>> {
    let photo = attachments::photo_parties(conn, document_id).await?;
    Ok(parties(conn, document_id)
        .await?
        .iter()
        .map(|p| PartyView { webcam_photo: photo.contains(&p.id), ..PartyView::from_row(ctx.keys(), p) })
        .collect())
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct FieldRow {
    pub id: Uuid,
    pub party_id: Option<Uuid>,
    pub kind: String,
    pub page: i32,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub required: bool,
    pub label: String,
    /// The value; for image and file fields, the name of the upload.
    pub value: Option<String>,
    /// The choices of a list field, or the one option of a radio button.
    #[schemars(with = "Vec<String>")]
    pub options: Value,
    #[schemars(with = "values::Validation")]
    pub validation: Value,
    /// How a date or number is written on the page.
    pub format: Option<String>,
    /// Radio buttons with the same key form one group.
    pub group_key: Option<String>,
    /// The signer sees the value but cannot change it.
    pub read_only: bool,
    /// The field is shown only when all of these hold.
    #[schemars(with = "Vec<logic::Condition>")]
    pub conditions: Value,
    /// For a formula field, the expression it computes.
    pub formula: Option<String>,
    #[serde(skip)]
    #[schemars(skip)]
    pub file_id: Option<Uuid>,
}

impl FieldRow {
    pub fn conditions(&self) -> Vec<logic::Condition> {
        serde_json::from_value(self.conditions.clone()).unwrap_or_default()
    }
}

pub const FIELD_COLUMNS: &str = "id, party_id, kind, page, x, y, width, height, required, label, value, options, validation, format, group_key, read_only, conditions, formula, file_id";

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DocumentView {
    #[serde(flatten)]
    pub document: DocumentRow,
    pub owner_name: Option<String>,
    pub files: Vec<FileRecord>,
    pub parties: Vec<PartyView>,
    pub fields: Vec<FieldRow>,
    /// Files parties are asked to upload while signing, and what they uploaded.
    pub attachments: Vec<attachments::AttachmentRequest>,
    /// Whether the seal carries a time stamp from the time stamp authority; unset until sealed.
    pub sealed_with_timestamp: Option<bool>,
    pub can_edit: bool,
    /// When the retention rules will delete this document, if any rule applies to it.
    pub deletes_at: Option<DateTime<Utc>>,
    /// The title of the template this document was made from, if you can see it.
    pub template_title: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Need {
    Read,
    Edit,
}

/// SQL condition for documents the actor may read, with `$1` = may read all, `$2` = user id.
pub const VISIBLE: &str = "($1 or d.owner_id = $2 or (d.is_template and $3) or exists (select 1 from document_access a where a.document_id = d.id and a.user_id = $2))";

/// What the actor may see: everything, their own and shared documents, and shared templates. An
/// API key without a document scope sees none of its owner's documents.
pub fn visibility(ctx: &Ctx) -> Result<(bool, Uuid, bool)> {
    let actor = ctx.actor()?;
    let can_create = actor.can(Permission::DocumentsCreate);
    let own = if can_create || actor.can(Permission::DocumentsRead) { actor.user_id } else { Uuid::nil() };
    Ok((actor.can(Permission::DocumentsReadAll), own, can_create))
}

/// Loads a document the actor may read or edit. A document they may not see is "not found", so
/// ids cannot be probed.
pub async fn load_document(ctx: &Ctx, conn: &mut PgConnection, id: Uuid, need: Need) -> Result<DocumentRow> {
    let (read_all, user_id, can_create) = visibility(ctx)?;
    let document: DocumentRow = sqlx::query_as(&format!("select {DOCUMENT_COLUMNS} from documents d where d.id = $4 and {VISIBLE}"))
        .bind(read_all)
        .bind(user_id)
        .bind(can_create)
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or_else(|| not_found("Document"))?;
    if need == Need::Edit && !can_edit(ctx, conn, &document).await? {
        return Err(crate::error::forbidden());
    }
    Ok(document)
}

pub async fn can_edit(ctx: &Ctx, conn: &mut PgConnection, document: &DocumentRow) -> Result<bool> {
    let actor = ctx.actor()?;
    if document.is_template {
        return Ok(
            actor.can(Permission::TemplatesManage) || (document.owner_id == Some(actor.user_id) && actor.can(Permission::DocumentsCreate))
        );
    }
    if actor.can(Permission::DocumentsManageAll) {
        return Ok(true);
    }
    if !actor.can(Permission::DocumentsCreate) {
        return Ok(false);
    }
    if document.owner_id == Some(actor.user_id) {
        return Ok(true);
    }
    let shared: Option<bool> = sqlx::query_scalar("select can_edit from document_access where document_id = $1 and user_id = $2")
        .bind(document.id)
        .bind(actor.user_id)
        .fetch_optional(&mut *conn)
        .await?;
    Ok(shared == Some(true))
}

pub async fn parties(conn: &mut PgConnection, document_id: Uuid) -> Result<Vec<PartyRow>> {
    Ok(sqlx::query_as(&format!("select {PARTY_COLUMNS} from parties where document_id = $1 order by sign_order, created_at, id"))
        .bind(document_id)
        .fetch_all(&mut *conn)
        .await?)
}

pub async fn party(conn: &mut PgConnection, id: Uuid) -> Result<Option<PartyRow>> {
    Ok(sqlx::query_as(&format!("select {PARTY_COLUMNS} from parties where id = $1")).bind(id).fetch_optional(&mut *conn).await?)
}

pub async fn fields(conn: &mut PgConnection, document_id: Uuid) -> Result<Vec<FieldRow>> {
    Ok(sqlx::query_as(&format!("select {FIELD_COLUMNS} from fields where document_id = $1 order by page, y, x, id"))
        .bind(document_id)
        .fetch_all(&mut *conn)
        .await?)
}

pub async fn files(conn: &mut PgConnection, document_id: Uuid) -> Result<Vec<FileRecord>> {
    Ok(sqlx::query_as(
        "select f.id, f.kind, f.name, f.content_type, f.size_bytes, f.sha256, f.storage_key, f.wrapped_key, f.created_at
         from document_files df join files f on f.id = df.file_id where df.document_id = $1 order by df.position",
    )
    .bind(document_id)
    .fetch_all(&mut *conn)
    .await?)
}

pub async fn view(ctx: &Ctx, conn: &mut PgConnection, document: DocumentRow) -> Result<DocumentView> {
    let owner_name: Option<String> = match document.owner_id {
        Some(id) => sqlx::query_scalar("select name from users where id = $1").bind(id).fetch_optional(&mut *conn).await?,
        None => None,
    };
    let can_edit = can_edit(ctx, conn, &document).await?;
    let parties = party_views(ctx, conn, document.id).await?;
    let fields = fields(conn, document.id).await?;
    let files = files(conn, document.id).await?;
    let attachments = attachments::for_document(conn, document.id).await?;
    let sealed_with_timestamp: Option<bool> = match document.sealed_file_id {
        Some(_) => sqlx::query_scalar(
            "select (metadata->>'timestamped')::boolean from audit_log
             where entity_type = 'document' and entity_id = $1 and action = 'document.completed' order by id desc limit 1",
        )
        .bind(document.id.to_string())
        .fetch_optional(&mut *conn)
        .await?
        .flatten(),
        None => None,
    };
    let rules: crate::settings::RetentionSettings = crate::services::settings(ctx, conn).await?;
    let deletes_at = crate::services::retention::deletes_at(&rules, &document);
    let template_title = match document.template_id {
        Some(id) => {
            let (read_all, user_id, can_create) = visibility(ctx)?;
            sqlx::query_scalar(&format!("select d.title from documents d where d.id = $4 and {VISIBLE}"))
                .bind(read_all)
                .bind(user_id)
                .bind(can_create)
                .bind(id)
                .fetch_optional(&mut *conn)
                .await?
        }
        None => None,
    };
    Ok(DocumentView {
        document,
        owner_name,
        files,
        parties,
        fields,
        attachments,
        sealed_with_timestamp,
        can_edit,
        deletes_at,
        template_title,
    })
}

/// Deletes a document with everything that belongs to it. File rows go in the same transaction,
/// which destroys their wrapped keys, so the encrypted bodies are unreadable at once; the bucket
/// objects are removed by a job after commit. Files another document still uses are kept.
pub async fn purge(ctx: &Ctx, conn: &mut PgConnection, document: &DocumentRow, reason: &str) -> Result<()> {
    if document.legal_hold {
        return Err(crate::error::conflict("This document is on legal hold and cannot be deleted"));
    }
    let file_ids: Vec<Uuid> = sqlx::query_scalar(
        "select f from (
           select source_file_id as f from documents where id = $1
           union select sealed_file_id from documents where id = $1
           union select file_id from document_files where document_id = $1
           union select file_id from attachment_requests where document_id = $1
           union select file_id from fields where document_id = $1
           union select vf.file_id from template_version_files vf join template_versions v on v.id = vf.version_id where v.template_id = $1
           union select (signature->>'imageFileId')::uuid from parties
             where document_id = $1 and signature->>'imageFileId' ~ '^[0-9a-f-]{36}$'
         ) ids where f is not null",
    )
    .bind(document.id)
    .fetch_all(&mut *conn)
    .await?;
    sqlx::query("delete from documents where id = $1").bind(document.id).execute(&mut *conn).await?;
    let keys: Vec<String> = sqlx::query_scalar(
        "delete from files f where f.id = any($1)
           and not exists (select 1 from documents d where d.source_file_id = f.id or d.sealed_file_id = f.id)
           and not exists (select 1 from document_files x where x.file_id = f.id)
           and not exists (select 1 from attachment_requests a where a.file_id = f.id)
           and not exists (select 1 from fields x where x.file_id = f.id)
           and not exists (select 1 from template_version_files x where x.file_id = f.id)
           and not exists (select 1 from parties p where p.signature->>'imageFileId' = f.id::text)
         returning storage_key",
    )
    .bind(&file_ids)
    .fetch_all(&mut *conn)
    .await?;
    if !keys.is_empty() {
        crate::jobs::enqueue(conn, "delete_objects", serde_json::json!({ "keys": keys }), None, None).await?;
    }
    crate::audit::audit(
        ctx,
        conn,
        crate::audit::Entry::new("document.deleted", "document", document.id)
            .before(serde_json::json!({ "title": document.title, "status": document.status, "sealedSha256": document.sealed_sha256 }))
            .metadata(serde_json::json!({ "reason": reason, "filesDeleted": keys.len() })),
    )
    .await?;
    Ok(())
}

fn link_mac(keys: &Keys, party_id: Uuid, version: i32) -> String {
    B64.encode(keys.mac(KeyPurpose::TokenMac, format!("signer-link:{party_id}:{version}").as_bytes()))
}

/// A signer link token: the party id and a MAC over it and the party's link version. Nothing
/// secret is stored, and bumping the version revokes every earlier link.
pub fn signer_token(keys: &Keys, party: &PartyRow) -> String {
    format!("{}.{}", party.id.simple(), link_mac(keys, party.id, party.link_version))
}

pub fn signer_url(ctx: &Ctx, keys: &Keys, party: &PartyRow) -> String {
    // A fragment never reaches server logs or Referer headers.
    format!("{}/sign#{}", ctx.app.public_url(), signer_token(keys, party))
}

/// The party a token names, if its MAC matches the party's current link version.
pub async fn party_for_token(keys: &Keys, conn: &mut PgConnection, token: &str) -> Result<Option<PartyRow>> {
    let Some((id, mac)) = token.trim().split_once('.') else { return Ok(None) };
    let Ok(id) = Uuid::try_parse(id) else { return Ok(None) };
    let Some(party) = party(conn, id).await? else { return Ok(None) };
    let expected = link_mac(keys, party.id, party.link_version);
    Ok(constant_time_eq(mac.as_bytes(), expected.as_bytes()).then_some(party))
}

/// The parties whose turn it is: everyone who must act and has not, or in a sequential document
/// only those with the lowest pending sign order.
pub fn current_turn(document: &DocumentRow, parties: &[PartyRow]) -> Vec<Uuid> {
    let pending: Vec<&PartyRow> = parties.iter().filter(|p| p.must_act() && p.signed_at.is_none() && p.declined_at.is_none()).collect();
    if !document.sequential {
        return pending.iter().map(|p| p.id).collect();
    }
    let Some(first) = pending.iter().map(|p| p.sign_order).min() else { return Vec::new() };
    pending.iter().filter(|p| p.sign_order == first).map(|p| p.id).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_all_but_the_last_four() {
        assert_eq!(mask("19800101-1234"), "•••••••••1234");
        assert_eq!(mask("123"), "•••");
    }

    #[test]
    fn enums_round_trip() {
        for m in AuthMethod::ALL {
            assert_eq!(AuthMethod::parse(m.as_str()), Some(*m));
        }
        for k in FieldKind::ALL {
            assert_eq!(serde_json::to_value(k).ok(), Some(Value::String(k.as_str().into())));
        }
    }
}
