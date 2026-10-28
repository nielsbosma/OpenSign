//! Documents, their parties and fields: shared types, who may see what, and signer links.

pub mod attachments;
pub mod notify;
pub mod seal;
pub mod upload;

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
    /// How the party gets their link. `link` hands it to the sender (or the calling system) instead.
    Delivery { Email = "email", Sms = "sms", Link = "link", None = "none", }
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
    FieldKind {
        Signature = "signature",
        Initials = "initials",
        Text = "text",
        Date = "date",
        Checkbox = "checkbox",
        Name = "name",
        Email = "email",
        Company = "company",
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
}

pub mod status {
    pub const DRAFT: &str = "draft";
    pub const PENDING: &str = "pending";
    pub const COMPLETED: &str = "completed";
    pub const DECLINED: &str = "declined";
    pub const CANCELED: &str = "canceled";
    pub const EXPIRED: &str = "expired";
}

pub const DOCUMENT_COLUMNS: &str = "id, title, status, is_template, template_id, owner_id, message, language, sequential, expires_at, reminder_days, \
     source_file_id, sealed_file_id, original_sha256, sealed_sha256, page_count, external_id, metadata, sent_at, completed_at, canceled_at, created_at, updated_at";

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
}

pub const PARTY_COLUMNS: &str = "id, document_id, role, name, email, phone, personal_number, company, sign_order, delivery, auth_to_view, auth_to_sign, \
     status, link_version, link_expires_at, notified_at, viewed_at, signed_at, declined_at, decline_reason, signature, evidence";

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
}

pub fn personal_number_context(party_id: Uuid) -> String {
    format!("party:{party_id}:personal_number")
}

pub fn decrypt_personal_number(keys: &Keys, party: &PartyRow) -> Option<String> {
    let sealed = party.personal_number.as_deref()?;
    keys.decrypt_str(KeyPurpose::Settings, &personal_number_context(party.id), sealed).ok().map(|s| s.to_string())
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
        }
    }
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
    pub value: Option<String>,
    pub options: Value,
}

pub const FIELD_COLUMNS: &str = "id, party_id, kind, page, x, y, width, height, required, label, value, options";

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
    pub can_edit: bool,
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
    let parties = parties(conn, document.id).await?.iter().map(|p| PartyView::from_row(ctx.keys(), p)).collect();
    let fields = fields(conn, document.id).await?;
    let files = files(conn, document.id).await?;
    let attachments = attachments::for_document(conn, document.id).await?;
    Ok(DocumentView { document, owner_name, files, parties, fields, attachments, can_edit })
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
