//! What a party does through their personal link. These are public calls: the link token is the
//! credential, and parties who must identify themselves also carry a short-lived session token.

use async_trait::async_trait;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use chrono::{DateTime, Duration, Utc};
use opensign_crypto::{KeyPurpose, constant_time_eq, random_bytes};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::PgConnection;
use uuid::Uuid;

use super::settings;
use crate::audit::{Entry, audit};
use crate::context::Ctx;
use crate::documents::attachments::{self, AttachmentRequest};
use crate::documents::{
    AuthMethod, DOCUMENT_COLUMNS, Delivery, DocumentRow, FieldKind, FieldRow, PartyRole, PartyRow, current_turn, fields, notify, parties,
    party_for_token, status,
};
use crate::error::{AppError, ErrorKind, Result, bad_request, conflict, too_many};
use crate::jobs::enqueue;
use crate::service::{Access, Meta, Service};
use crate::settings::{OrgSettings, SmsSettings};
use crate::storage::{FileRecord, NewFile, file_record, store_file};

pub(crate) const SESSION_MINUTES: i64 = 30;
const CODE_MINUTES: i64 = 10;
const CODE_ATTEMPTS: i32 = 5;
const CODES_PER_HOUR: i64 = 5;
const MAX_SIGNATURE_IMAGE: usize = 300 * 1024;

pub(crate) fn invalid_link() -> AppError {
    AppError::new(ErrorKind::NotFound, "This signing link is not valid or has expired. Ask the sender for a new one.")
}

#[derive(Serialize, Deserialize)]
pub(crate) struct SignerSession {
    p: Uuid,
    v: i32,
    a: AuthMethod,
    /// What the identification proved, e.g. the name from an eID, for the evidence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    i: Option<String>,
}

pub fn issue_session(ctx: &Ctx, party: &PartyRow, method: AuthMethod, identity: Option<String>) -> String {
    ctx.keys().sign_token(
        "signer-session",
        &SignerSession { p: party.id, v: party.link_version, a: method, i: identity },
        SESSION_MINUTES * 60,
        ctx.now.timestamp(),
    )
}

fn session(ctx: &Ctx, party: &PartyRow, token: Option<&str>) -> Option<SignerSession> {
    let s: SignerSession = ctx.keys().verify_token("signer-session", token?, ctx.now.timestamp()).ok()?;
    (s.p == party.id && s.v == party.link_version).then_some(s)
}

/// An eID also satisfies an SMS code requirement; nothing weaker satisfies an eID.
pub(crate) fn satisfied(required: AuthMethod, session: Option<&SignerSession>) -> bool {
    match required {
        AuthMethod::None | AuthMethod::Standard => true,
        AuthMethod::SmsPin => session.is_some_and(|s| s.a != AuthMethod::None && s.a != AuthMethod::Standard),
        eid => session.is_some_and(|s| s.a == eid),
    }
}

pub(crate) struct Resolved {
    pub party: PartyRow,
    pub document: DocumentRow,
    pub session: Option<SignerSession>,
}

pub(crate) async fn resolve(ctx: &Ctx, conn: &mut PgConnection, token: &str, session_token: Option<&str>) -> Result<Resolved> {
    let party = party_for_token(ctx.keys(), conn, token).await?.ok_or_else(invalid_link)?;
    if party.link_expires_at.is_some_and(|at| at < ctx.now) {
        return Err(invalid_link());
    }
    let document: DocumentRow = sqlx::query_as(&format!("select {DOCUMENT_COLUMNS} from documents d where id = $1"))
        .bind(party.document_id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or_else(invalid_link)?;
    match document.status.as_str() {
        status::PENDING | status::COMPLETED => {}
        status::DECLINED => return Err(conflict("This document was declined, so it will not be signed.")),
        status::CANCELED => return Err(conflict("The sender canceled this document.")),
        status::EXPIRED => return Err(conflict("This document has expired. Ask the sender to send it again.")),
        _ => return Err(invalid_link()),
    }
    let session = session(ctx, &party, session_token);
    Ok(Resolved { party, document, session })
}

/// `resolve` with the document row locked and everything read again after the lock, so two
/// responses from the same link (a double click, sign and decline) cannot both pass the checks.
async fn resolve_locked(ctx: &Ctx, conn: &mut PgConnection, token: &str, session_token: Option<&str>) -> Result<Resolved> {
    let first = resolve(ctx, conn, token, session_token).await?;
    sqlx::query("select 1 from documents where id = $1 for update").bind(first.document.id).execute(&mut *conn).await?;
    resolve(ctx, conn, token, session_token).await
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TokenInput {
    pub token: String,
    pub session: Option<String>,
}

#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SignerDocument {
    pub id: Uuid,
    pub title: String,
    pub message: Option<String>,
    pub status: String,
    pub page_count: Option<i32>,
    pub organisation: String,
    pub brand_color: String,
    pub sender: Option<String>,
    pub language: String,
}

#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SignerParty {
    pub id: Uuid,
    pub name: String,
    pub email: Option<String>,
    pub company: Option<String>,
    pub role: String,
    pub status: String,
    pub signed_at: Option<DateTime<Utc>>,
    /// The party signs on the sender's device, which is handed over to them.
    pub in_person: bool,
}

#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SignerView {
    pub document: SignerDocument,
    pub party: SignerParty,
    /// Set when the party must identify before seeing the document; nothing else is filled in.
    pub needs_authentication: Option<AuthMethod>,
    /// Set when the party must identify before signing.
    pub sign_authentication: Option<AuthMethod>,
    /// The party's own fields, and the sender's prefilled text for context.
    pub fields: Vec<FieldRow>,
    pub your_turn: bool,
    pub can_download: bool,
    /// Files the party is asked to upload before signing.
    pub attachments: Vec<AttachmentRequest>,
}

pub struct Open;

#[async_trait]
impl Service for Open {
    type Input = TokenInput;
    type Output = SignerView;
    const META: Meta = Meta::mutation("signing.open", "Open a signing link: the document, the party and their fields.", Access::Public);

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: TokenInput) -> Result<SignerView> {
        let Resolved { party, document, session } = resolve(ctx, conn, &input.token, input.session.as_deref()).await?;
        let org: OrgSettings = settings(ctx, conn).await?;
        let sender: Option<String> = match document.owner_id {
            Some(id) => sqlx::query_scalar("select name from users where id = $1").bind(id).fetch_optional(&mut *conn).await?,
            None => None,
        };
        let locked = !satisfied(party.auth_to_view(), session.as_ref());
        let all = parties(conn, document.id).await?;
        let your_turn = document.status == status::PENDING && current_turn(&document, &all).contains(&party.id);
        let view = SignerView {
            document: SignerDocument {
                id: document.id,
                title: document.title.clone(),
                message: (!locked).then(|| document.message.clone()),
                status: document.status.clone(),
                page_count: (!locked).then_some(document.page_count).flatten(),
                organisation: if org.name.is_empty() { "OpenSign".into() } else { org.name.clone() },
                brand_color: org.brand_color.clone(),
                sender,
                language: document.language.clone(),
            },
            party: SignerParty {
                id: party.id,
                name: party.name.clone(),
                email: party.email.clone(),
                company: party.company.clone(),
                role: party.role.clone(),
                status: party.status.clone(),
                signed_at: party.signed_at,
                in_person: party.delivery == Delivery::InPerson.as_str(),
            },
            needs_authentication: locked.then(|| party.auth_to_view()),
            sign_authentication: (!satisfied(party.auth_to_sign(), session.as_ref())).then(|| party.auth_to_sign()),
            fields: if locked {
                Vec::new()
            } else {
                fields(conn, document.id).await?.into_iter().filter(|f| f.party_id == Some(party.id) || f.party_id.is_none()).collect()
            },
            your_turn,
            can_download: !locked && document.status == status::COMPLETED,
            attachments: if locked { Vec::new() } else { attachments::for_party(conn, party.id).await? },
        };
        if !locked && party.viewed_at.is_none() && document.status == status::PENDING {
            sqlx::query("update parties set viewed_at = $2, status = case when status in ('waiting', 'notified') then 'viewed' else status end, updated_at = now() where id = $1")
                .bind(party.id)
                .bind(ctx.now)
                .execute(&mut *conn)
                .await?;
            audit(
                ctx,
                conn,
                Entry::new("party.viewed", "document", document.id).metadata(json!({ "party": party.id, "name": party.name })),
            )
            .await?;
        }
        Ok(view)
    }
}

pub fn sms_code_hash(ctx: &Ctx, challenge: Uuid, code: &str) -> String {
    hex::encode(ctx.keys().mac(KeyPurpose::TokenMac, format!("sms-code:{challenge}:{code}").as_bytes()))
}

#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CodeSent {
    /// The last digits of the number the code went to.
    pub phone_hint: String,
    pub expires_at: DateTime<Utc>,
}

pub struct RequestCode;

#[async_trait]
impl Service for RequestCode {
    type Input = TokenInput;
    type Output = CodeSent;
    const META: Meta = Meta::mutation("signing.request_code", "Send a one-time code by SMS to the party's phone.", Access::Public);

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: TokenInput) -> Result<CodeSent> {
        let Resolved { party, document, .. } = resolve(ctx, conn, &input.token, None).await?;
        if party.auth_to_view() != AuthMethod::SmsPin && party.auth_to_sign() != AuthMethod::SmsPin {
            return Err(bad_request("This party does not use SMS codes"));
        }
        let phone = party.phone.clone().ok_or_else(|| bad_request("There is no phone number for you. Ask the sender to add one."))?;
        let recent: i64 = sqlx::query_scalar("select count(*) from sms_challenges where party_id = $1 and created_at > $2")
            .bind(party.id)
            .bind(ctx.now - Duration::hours(1))
            .fetch_one(&mut *conn)
            .await?;
        if recent >= CODES_PER_HOUR {
            return Err(too_many("Too many codes requested. Try again in an hour."));
        }
        let id = Uuid::now_v7();
        let code = format!("{:06}", u32::from_be_bytes(random_bytes::<4>()) % 1_000_000);
        let expires_at = ctx.now + Duration::minutes(CODE_MINUTES);
        sqlx::query("update sms_challenges set expires_at = least(expires_at, $2) where party_id = $1 and verified_at is null")
            .bind(party.id)
            .bind(ctx.now)
            .execute(&mut *conn)
            .await?;
        sqlx::query("insert into sms_challenges (id, party_id, purpose, code_hash, expires_at) values ($1, $2, 'auth', $3, $4)")
            .bind(id)
            .bind(party.id)
            .bind(sms_code_hash(ctx, id, &code))
            .bind(expires_at)
            .execute(&mut *conn)
            .await?;
        let org: OrgSettings = settings(ctx, conn).await?;
        let sms: SmsSettings = settings(ctx, conn).await?;
        let org_name = if org.name.is_empty() { "OpenSign" } else { org.name.as_str() };
        crate::sms::send(&ctx.app, &sms, &phone, &crate::i18n::Lang::of(&document.language).code_sms(&code, &document.title, org_name))
            .await?;
        audit(ctx, conn, Entry::new("party.code_sent", "document", document.id).metadata(json!({ "party": party.id, "name": party.name })))
            .await?;
        let hint: String = phone.chars().rev().take(3).collect::<Vec<_>>().into_iter().rev().collect();
        Ok(CodeSent { phone_hint: format!("\u{2022}\u{2022}\u{2022}{hint}"), expires_at })
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct VerifyCodeInput {
    pub token: String,
    pub code: String,
}

#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SignerSessionOutput {
    /// Pass this as `session` with the next calls.
    pub session: String,
    pub expires_at: DateTime<Utc>,
}

pub struct VerifyCode;

#[async_trait]
impl Service for VerifyCode {
    type Input = VerifyCodeInput;
    type Output = SignerSessionOutput;
    const META: Meta =
        Meta::mutation("signing.verify_code", "Check the SMS code and start a short signer session.", Access::Public).without_transaction();

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: VerifyCodeInput) -> Result<SignerSessionOutput> {
        let Resolved { party, document, .. } = resolve(ctx, conn, &input.token, None).await?;
        let wrong = || bad_request("That code is not right, or it has expired. Request a new one.");
        let challenge: Option<(Uuid, String)> = sqlx::query_as(
            "update sms_challenges set attempts = attempts + 1 where id = (
               select id from sms_challenges where party_id = $1 and verified_at is null and expires_at > $2 and attempts < $3 order by created_at desc limit 1)
             returning id, code_hash",
        )
        .bind(party.id)
        .bind(ctx.now)
        .bind(CODE_ATTEMPTS)
        .fetch_optional(&mut *conn)
        .await?;
        let Some((id, hash)) = challenge else { return Err(wrong()) };
        let code: String = input.code.chars().filter(char::is_ascii_digit).collect();
        if !constant_time_eq(sms_code_hash(ctx, id, &code).as_bytes(), hash.as_bytes()) {
            audit(
                ctx,
                conn,
                Entry::new("party.code_failed", "document", document.id).metadata(json!({ "party": party.id, "name": party.name })),
            )
            .await?;
            return Err(wrong());
        }
        sqlx::query("update sms_challenges set verified_at = $2 where id = $1").bind(id).bind(ctx.now).execute(&mut *conn).await?;
        audit(
            ctx,
            conn,
            Entry::new("party.authenticated", "document", document.id)
                .metadata(json!({ "party": party.id, "name": party.name, "method": "sms_pin" })),
        )
        .await?;
        let identity = party.phone.as_deref().map(|p| format!("SMS code to {p}"));
        Ok(SignerSessionOutput {
            session: issue_session(ctx, &party, AuthMethod::SmsPin, identity),
            expires_at: ctx.now + Duration::minutes(SESSION_MINUTES),
        })
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FieldValue {
    pub field_id: Uuid,
    pub value: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SignatureInput {
    /// The name as typed, drawn in a script style when there is no image.
    pub typed_name: Option<String>,
    /// A drawn signature as PNG, base64.
    pub image_png_base64: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SignInput {
    pub token: String,
    pub session: Option<String>,
    #[serde(default)]
    pub values: Vec<FieldValue>,
    pub signature: Option<SignatureInput>,
    /// The party confirmed they want to sign electronically.
    pub consent: bool,
}

#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SignOutput {
    /// Everyone has signed; the sealed document is on its way by email.
    pub completed: bool,
}

fn clean_value(value: &str) -> Result<String> {
    let value = value.trim();
    if value.chars().count() > 1000 || value.chars().any(|c| c.is_control() && c != '\n') {
        return Err(bad_request("A field value is too long or contains characters that are not allowed"));
    }
    Ok(value.to_string())
}

async fn store_signature_image(ctx: &Ctx, conn: &mut PgConnection, encoded: &str) -> Result<FileRecord> {
    if encoded.len() > MAX_SIGNATURE_IMAGE * 4 / 3 + 8 {
        return Err(bad_request("The signature image is too large"));
    }
    let encoded = encoded.trim().trim_start_matches("data:image/png;base64,");
    let png = B64.decode(encoded).map_err(|_| bad_request("The signature image is not valid base64"))?;
    opensign_pdf::check_signature_image(&png).map_err(|e| bad_request(e.to_string()))?;
    store_file(
        conn,
        &ctx.app.storage,
        ctx.keys(),
        NewFile { kind: "signature", name: "signature.png", content_type: "image/png", body: &png, uploaded_by: None },
    )
    .await
}

pub struct Sign;

#[async_trait]
impl Service for Sign {
    type Input = SignInput;
    type Output = SignOutput;
    const META: Meta = Meta::mutation("signing.sign", "Fill in the party's fields and sign or approve.", Access::Public);

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: SignInput) -> Result<SignOutput> {
        let Resolved { party, document, session } = resolve_locked(ctx, conn, &input.token, input.session.as_deref()).await?;
        if document.status != status::PENDING {
            return Err(conflict("This document is no longer waiting for signatures"));
        }
        if !party.must_act() {
            return Err(bad_request("You receive a copy of this document; there is nothing to sign"));
        }
        if party.signed_at.is_some() || party.declined_at.is_some() {
            return Err(conflict("You have already responded to this document"));
        }
        let all = parties(conn, document.id).await?;
        if !current_turn(&document, &all).contains(&party.id) {
            return Err(conflict("It is not your turn yet. You will get an email when it is."));
        }
        if !satisfied(party.auth_to_view(), session.as_ref()) || !satisfied(party.auth_to_sign(), session.as_ref()) {
            return Err(AppError::new(ErrorKind::Unauthorized, format!("Identify yourself with {} first", party.auth_to_sign().label())));
        }
        if !input.consent {
            return Err(bad_request("Confirm that you want to sign electronically"));
        }
        if let Some(missing) = attachments::for_party(conn, party.id).await?.into_iter().find(|r| r.required && r.file_id.is_none()) {
            return Err(bad_request(format!("Upload \u{201c}{}\u{201d} first", missing.name)));
        }

        let own: Vec<FieldRow> = fields(conn, document.id).await?.into_iter().filter(|f| f.party_id == Some(party.id)).collect();
        for v in &input.values {
            if !own.iter().any(|f| f.id == v.field_id) {
                return Err(bad_request("A value was given for a field that is not yours"));
            }
        }
        let is_signer = party.role() == PartyRole::Signer;
        let typed_name =
            input.signature.as_ref().and_then(|s| s.typed_name.as_deref()).map(clean_value).transpose()?.filter(|n| !n.is_empty());
        let image = match input.signature.as_ref().and_then(|s| s.image_png_base64.as_deref()).filter(|s| !s.is_empty()) {
            Some(encoded) if is_signer => Some(store_signature_image(ctx, conn, encoded).await?),
            _ => None,
        };
        if is_signer && typed_name.is_none() && image.is_none() {
            return Err(bad_request("Draw or type your signature"));
        }
        let today = ctx.now.format("%Y-%m-%d").to_string();
        for field in &own {
            let given = input.values.iter().find(|v| v.field_id == field.id).map(|v| clean_value(&v.value)).transpose()?;
            let kind = FieldKind::parse(&field.kind).unwrap_or(FieldKind::Text);
            let value = match kind {
                FieldKind::Name => Some(party.name.clone()),
                FieldKind::Email => party.email.clone(),
                FieldKind::Date => Some(today.clone()),
                FieldKind::Signature => Some("signed".into()),
                FieldKind::Initials => given.filter(|v| !v.is_empty()),
                FieldKind::Company => given.filter(|v| !v.is_empty()).or_else(|| party.company.clone()),
                FieldKind::Checkbox => match given.as_deref() {
                    None | Some("") | Some("false") => Some("false".into()),
                    Some("true") => Some("true".into()),
                    Some(_) => return Err(bad_request("A checkbox value is true or false")),
                },
                FieldKind::Text => given.filter(|v| !v.is_empty()).or_else(|| field.value.clone()),
            };
            let missing = match kind {
                FieldKind::Checkbox => field.required && value.as_deref() != Some("true"),
                _ => field.required && value.as_deref().is_none_or(str::is_empty),
            };
            if missing {
                let label = if field.label.is_empty() { kind.as_str() } else { field.label.as_str() };
                return Err(bad_request(format!("Fill in \u{201c}{label}\u{201d}")));
            }
            sqlx::query("update fields set value = $2 where id = $1").bind(field.id).bind(&value).execute(&mut *conn).await?;
        }

        let method = session.as_ref().map_or(party.auth_to_sign(), |s| s.a);
        let evidence = json!({
            "auth": method,
            "identity": session.as_ref().and_then(|s| s.i.clone()),
            "ip": ctx.ip,
            "userAgent": ctx.user_agent,
            "signedAt": ctx.now,
            "documentSha256": document.original_sha256,
        });
        let signature = is_signer.then(|| json!({ "typedName": typed_name, "imageFileId": image.as_ref().map(|i| i.id) }));
        sqlx::query(
            "update parties set status = 'signed', signed_at = $2, signature = $3, evidence = $4, updated_at = now() where id = $1",
        )
        .bind(party.id)
        .bind(ctx.now)
        .bind(signature)
        .bind(&evidence)
        .execute(&mut *conn)
        .await?;
        audit(
            ctx,
            conn,
            Entry::new(if is_signer { "party.signed" } else { "party.approved" }, "document", document.id)
                .metadata(json!({ "party": party.id, "name": party.name, "method": method, "documentSha256": document.original_sha256 })),
        )
        .await?;
        notify::emit(
            conn,
            "document.signed",
            json!({ "documentId": document.id, "externalId": document.external_id, "partyId": party.id, "name": party.name }),
        )
        .await?;

        let all = parties(conn, document.id).await?;
        let completed = all.iter().all(|p| !p.must_act() || p.signed_at.is_some());
        if completed {
            enqueue(conn, "seal_document", json!({ "document": document.id }), None, Some(&format!("seal:{}", document.id))).await?;
        } else {
            notify::notify_turn(conn, &document).await?;
        }
        Ok(SignOutput { completed })
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeclineInput {
    pub token: String,
    pub session: Option<String>,
    pub reason: Option<String>,
}

pub struct Decline;

#[async_trait]
impl Service for Decline {
    type Input = DeclineInput;
    type Output = super::Ok_;
    const META: Meta =
        Meta::mutation("signing.decline", "Decline to sign. Signing stops for everyone and the sender is told.", Access::Public);

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: DeclineInput) -> Result<super::Ok_> {
        let Resolved { party, document, session } = resolve_locked(ctx, conn, &input.token, input.session.as_deref()).await?;
        if document.status != status::PENDING || !party.must_act() || party.signed_at.is_some() || party.declined_at.is_some() {
            return Err(conflict("You cannot decline this document now"));
        }
        if !satisfied(party.auth_to_view(), session.as_ref()) {
            return Err(AppError::new(ErrorKind::Unauthorized, format!("Identify yourself with {} first", party.auth_to_view().label())));
        }
        let reason = input.reason.as_deref().map(clean_value).transpose()?.filter(|r| !r.is_empty());
        sqlx::query("update parties set status = 'declined', declined_at = $2, decline_reason = $3, updated_at = now() where id = $1")
            .bind(party.id)
            .bind(ctx.now)
            .bind(&reason)
            .execute(&mut *conn)
            .await?;
        sqlx::query("update documents set status = 'declined', updated_at = now() where id = $1 and status = 'pending'")
            .bind(document.id)
            .execute(&mut *conn)
            .await?;
        audit(
            ctx,
            conn,
            Entry::new("party.declined", "document", document.id)
                .metadata(json!({ "party": party.id, "name": party.name, "reason": reason })),
        )
        .await?;
        enqueue(conn, "notify_declined", json!({ "document": document.id, "party": party.id }), None, None).await?;
        notify::emit(
            conn,
            "document.declined",
            json!({ "documentId": document.id, "externalId": document.external_id, "partyId": party.id, "reason": reason }),
        )
        .await?;
        Ok(super::Ok_::default())
    }
}

/// The party may change their attachments: it is their turn, they have not responded, and they
/// identified as needed to see the document.
async fn attachment_party(ctx: &Ctx, conn: &mut PgConnection, token: &str, session_token: Option<&str>) -> Result<Resolved> {
    let resolved = resolve_locked(ctx, conn, token, session_token).await?;
    let Resolved { party, document, session } = &resolved;
    if document.status != status::PENDING || !party.must_act() || party.signed_at.is_some() || party.declined_at.is_some() {
        return Err(conflict("Attachments can no longer be changed"));
    }
    if !current_turn(document, &parties(conn, document.id).await?).contains(&party.id) {
        return Err(conflict("It is not your turn yet. You will get an email when it is."));
    }
    if !satisfied(party.auth_to_view(), session.as_ref()) {
        return Err(AppError::new(ErrorKind::Unauthorized, format!("Identify yourself with {} first", party.auth_to_view().label())));
    }
    Ok(resolved)
}

async fn request_of(conn: &mut PgConnection, party_id: Uuid, id: Uuid) -> Result<AttachmentRequest> {
    sqlx::query("select 1 from attachment_requests where id = $1 and party_id = $2 for update")
        .bind(id)
        .bind(party_id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or_else(|| crate::error::not_found("Attachment"))?;
    attachments::for_party(conn, party_id).await?.into_iter().find(|r| r.id == id).ok_or_else(|| crate::error::not_found("Attachment"))
}

async fn retire_file(conn: &mut PgConnection, file_id: Option<Uuid>) -> Result<()> {
    if let Some(id) = file_id {
        sqlx::query("update files set deleted_at = now() where id = $1 and deleted_at is null").bind(id).execute(&mut *conn).await?;
    }
    Ok(())
}

/// Stores a file the party was asked for, cleaned first (see `attachments::clean`), replacing any
/// earlier upload for the same request.
pub async fn upload_attachment(
    ctx: &Ctx,
    conn: &mut PgConnection,
    token: &str,
    session_token: Option<&str>,
    request_id: Uuid,
    name: &str,
    bytes: Vec<u8>,
) -> Result<AttachmentRequest> {
    let Resolved { party, document, .. } = attachment_party(ctx, conn, token, session_token).await?;
    let request = request_of(conn, party.id, request_id).await?;
    let (file_name, content_type, body) = attachments::clean(bytes, name).await?;
    let record = store_file(
        conn,
        &ctx.app.storage,
        ctx.keys(),
        NewFile { kind: "attachment", name: &file_name, content_type, body: &body, uploaded_by: None },
    )
    .await?;
    retire_file(conn, request.file_id).await?;
    sqlx::query("update attachment_requests set file_id = $2, uploaded_at = $3 where id = $1")
        .bind(request.id)
        .bind(record.id)
        .bind(ctx.now)
        .execute(&mut *conn)
        .await?;
    audit(
        ctx,
        conn,
        Entry::new("party.attachment_uploaded", "document", document.id).metadata(json!({
            "party": party.id, "name": party.name, "attachment": request.name, "file": record.name, "sha256": record.sha256,
        })),
    )
    .await?;
    request_of(conn, party.id, request_id).await
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RemoveAttachmentInput {
    pub token: String,
    pub session: Option<String>,
    /// The attachment request whose upload to remove.
    pub id: Uuid,
}

pub struct RemoveAttachment;

#[async_trait]
impl Service for RemoveAttachment {
    type Input = RemoveAttachmentInput;
    type Output = super::Ok_;
    const META: Meta = Meta::mutation("signing.remove_attachment", "Remove a file the party uploaded, before they sign.", Access::Public);

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: RemoveAttachmentInput) -> Result<super::Ok_> {
        let Resolved { party, document, .. } = attachment_party(ctx, conn, &input.token, input.session.as_deref()).await?;
        let request = request_of(conn, party.id, input.id).await?;
        if request.file_id.is_none() {
            return Ok(super::Ok_::default());
        }
        sqlx::query("update attachment_requests set file_id = null, uploaded_at = null where id = $1")
            .bind(request.id)
            .execute(&mut *conn)
            .await?;
        retire_file(conn, request.file_id).await?;
        audit(
            ctx,
            conn,
            Entry::new("party.attachment_removed", "document", document.id)
                .metadata(json!({ "party": party.id, "name": party.name, "attachment": request.name })),
        )
        .await?;
        Ok(super::Ok_::default())
    }
}

/// The PDF a party may see through their link: the document as sent while pending, the sealed
/// one once complete.
pub async fn signer_file(ctx: &Ctx, conn: &mut PgConnection, token: &str, session_token: Option<&str>) -> Result<FileRecord> {
    let Resolved { party, document, session } = resolve(ctx, conn, token, session_token).await?;
    if !satisfied(party.auth_to_view(), session.as_ref()) {
        return Err(AppError::new(ErrorKind::Unauthorized, format!("Identify yourself with {} first", party.auth_to_view().label())));
    }
    let id = match document.status.as_str() {
        status::COMPLETED => document.sealed_file_id,
        _ => document.source_file_id,
    }
    .ok_or_else(invalid_link)?;
    if document.status == status::COMPLETED {
        audit(
            ctx,
            conn,
            Entry::new("party.downloaded", "document", document.id).metadata(json!({ "party": party.id, "name": party.name })),
        )
        .await?;
    }
    file_record(conn, id).await
}
