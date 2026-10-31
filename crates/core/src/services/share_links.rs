//! Public links to a template. Whoever opens one gives their name and email, confirms the email
//! with a code, and then signs a new document made from the template. Links are off until turned
//! on per template, and limited per hour, in total and by date.

use async_trait::async_trait;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use std::sync::LazyLock;

use chrono::{DateTime, Duration, Utc};
use opensign_crypto::{KeyPurpose, constant_time_eq, random_bytes};
use regex::Regex;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{Connection, PgConnection};
use uuid::Uuid;

use super::documents::{self as docs, CreateInput, Origin, PartyInput};
use super::settings;
use super::signing::clean_value;
use crate::audit::{Entry, audit};
use crate::auth::{actor_for, normalize_email, user_by_id};
use crate::context::{Credential, Ctx};
use crate::documents::{
    AuthMethod, DOCUMENT_COLUMNS, Delivery, DocumentRow, Need, PartyRow, current_turn, load_document, parties, signer_url, status,
};
use crate::email::{self, Message};
use crate::error::{AppError, ErrorKind, Result, bad_request, conflict, not_found, too_many};
use crate::i18n::Lang;
use crate::permissions::Permission;
use crate::service::{Access, Meta, Service};
use crate::settings::{EmailSettings, OrgSettings, SecuritySettings, SmsSettings};

const CODE_MINUTES: i64 = 15;
const CODE_ATTEMPTS: i32 = 5;
const CODES_PER_EMAIL_HOUR: i64 = 3;
const MAX_VISITOR_NAME: usize = 100;

fn inactive() -> AppError {
    AppError::new(ErrorKind::NotFound, "This link is not active. Ask whoever shared it for a new one.")
}

fn new_slug() -> String {
    B64.encode(random_bytes::<16>())
}

fn link_url(ctx: &Ctx, slug: &str) -> String {
    format!("{}/s/{slug}", ctx.app.public_url())
}

#[derive(sqlx::FromRow)]
struct LinkRow {
    id: Uuid,
    template_id: Uuid,
    slug: String,
    enabled: bool,
    party_id: Option<Uuid>,
    owner_id: Option<Uuid>,
    per_hour: i32,
    max_total: Option<i32>,
    expires_at: Option<DateTime<Utc>>,
    documents_created: i32,
    updated_at: DateTime<Utc>,
}

const LINK_COLUMNS: &str =
    "id, template_id, slug, enabled, party_id, owner_id, per_hour, max_total, expires_at, documents_created, updated_at";

#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ShareLinkView {
    pub template_id: Uuid,
    pub enabled: bool,
    /// The public address to share or print as a QR code.
    pub url: String,
    /// The template party that whoever opens the link becomes.
    pub party_id: Option<Uuid>,
    pub party_name: Option<String>,
    /// At most this many documents per hour.
    pub per_hour: i32,
    /// At most this many documents in total; unset for no limit.
    pub max_total: Option<i32>,
    /// The link stops working at this time.
    pub expires_at: Option<DateTime<Utc>>,
    pub documents_created: i32,
    /// Documents made through the link belong to this user.
    pub owner_name: Option<String>,
    pub updated_at: DateTime<Utc>,
}

async fn link_for_template(conn: &mut PgConnection, template_id: Uuid) -> Result<Option<LinkRow>> {
    Ok(sqlx::query_as(&format!("select {LINK_COLUMNS} from share_links where template_id = $1"))
        .bind(template_id)
        .fetch_optional(&mut *conn)
        .await?)
}

async fn view(ctx: &Ctx, conn: &mut PgConnection, link: LinkRow) -> Result<ShareLinkView> {
    let party_name: Option<String> = match link.party_id {
        Some(id) => sqlx::query_scalar("select name from parties where id = $1").bind(id).fetch_optional(&mut *conn).await?,
        None => None,
    };
    let owner_name: Option<String> = match link.owner_id {
        Some(id) => sqlx::query_scalar("select name from users where id = $1").bind(id).fetch_optional(&mut *conn).await?,
        None => None,
    };
    Ok(ShareLinkView {
        template_id: link.template_id,
        enabled: link.enabled,
        url: link_url(ctx, &link.slug),
        party_id: link.party_id,
        party_name,
        per_hour: link.per_hour,
        max_total: link.max_total,
        expires_at: link.expires_at,
        documents_created: link.documents_created,
        owner_name,
        updated_at: link.updated_at,
    })
}

async fn template(ctx: &Ctx, conn: &mut PgConnection, id: Uuid, need: Need) -> Result<DocumentRow> {
    let template = load_document(ctx, conn, id, need).await?;
    if !template.is_template {
        return Err(bad_request("Share links are made from templates"));
    }
    Ok(template)
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TemplateInput {
    pub template_id: Uuid,
}

pub struct Get;

#[async_trait]
impl Service for Get {
    type Input = TemplateInput;
    type Output = Option<ShareLinkView>;
    const META: Meta = Meta::query(
        "share_links.get",
        "Get a template's public share link: whether it is on, its address, which party the visitor becomes, its limits and how many documents it made. Empty when the template never had one.",
        Access::Authenticated,
    )
    .tool();

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: TemplateInput) -> Result<Option<ShareLinkView>> {
        let template = template(ctx, conn, input.template_id, Need::Read).await?;
        match link_for_template(conn, template.id).await? {
            Some(link) => Ok(Some(view(ctx, conn, link).await?)),
            None => Ok(None),
        }
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SetInput {
    pub template_id: Uuid,
    /// Turn the public link on or off. Off stops it at once; the address is kept for when it is turned on again.
    pub enabled: bool,
    /// The template's signer or approver that whoever opens the link becomes. Other parties stay as in the template.
    pub party_id: Option<Uuid>,
    /// At most this many documents per hour, 1 to 1000. Defaults to 10.
    pub per_hour: Option<i32>,
    /// At most this many documents in total. Leave out for no limit.
    pub max_total: Option<i32>,
    /// Stop the link at this time. Leave out for no end date.
    pub expires_at: Option<DateTime<Utc>>,
}

pub struct Set;

#[async_trait]
impl Service for Set {
    type Input = SetInput;
    type Output = ShareLinkView;
    const META: Meta = Meta::mutation(
        "share_links.set",
        "Turn a template's public share link on or off and set its limits. Anyone with the link can start a document from the template after confirming their email address; the documents belong to the user who turns it on.",
        Access::Permission(Permission::DocumentsCreate),
    )
    .tool()
    .destructive();

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: SetInput) -> Result<ShareLinkView> {
        let template = template(ctx, conn, input.template_id, Need::Edit).await?;
        let per_hour = input.per_hour.unwrap_or(10);
        if !(1..=1000).contains(&per_hour) {
            return Err(bad_request("The hourly limit is between 1 and 1000"));
        }
        if input.max_total.is_some_and(|m| m < 1) {
            return Err(bad_request("The total limit is at least 1"));
        }
        let all = parties(conn, template.id).await?;
        if input.enabled {
            if template.archived_at.is_some() {
                return Err(conflict("Restore the template before sharing it"));
            }
            if crate::documents::files(conn, template.id).await?.is_empty() {
                return Err(bad_request("Add a file to the template first"));
            }
            let party = input.party_id.and_then(|id| all.iter().find(|p| p.id == id));
            match party {
                None => return Err(bad_request("Choose the party that whoever opens the link becomes")),
                Some(p) if !p.must_act() => return Err(bad_request("The party who opens the link must sign or approve")),
                _ => {}
            }
            if input.expires_at.is_some_and(|at| at <= ctx.now) {
                return Err(bad_request("The end date must be in the future"));
            }
            let email: EmailSettings = settings(ctx, conn).await?;
            if !ctx.app.env.development && !email::is_configured(&email) {
                return Err(bad_request(
                    "Email is not set up. Share links confirm each visitor's email address, so an admin connects email first.",
                ));
            }
        } else if let Some(id) = input.party_id
            && !all.iter().any(|p| p.id == id)
        {
            return Err(not_found("Party"));
        }
        let before = link_for_template(conn, template.id).await?;
        let actor = ctx.actor()?.user_id;
        sqlx::query(
            "insert into share_links (id, template_id, slug, enabled, party_id, owner_id, per_hour, max_total, expires_at)
             values ($1, $2, $3, $4, $5, $6, $7, $8, $9)
             on conflict (template_id) do update set enabled = excluded.enabled, party_id = excluded.party_id,
               owner_id = case when excluded.enabled then excluded.owner_id else share_links.owner_id end,
               per_hour = excluded.per_hour, max_total = excluded.max_total, expires_at = excluded.expires_at, updated_at = now()",
        )
        .bind(Uuid::now_v7())
        .bind(template.id)
        .bind(new_slug())
        .bind(input.enabled)
        .bind(input.party_id)
        .bind(actor)
        .bind(per_hour)
        .bind(input.max_total)
        .bind(input.expires_at)
        .execute(&mut *conn)
        .await?;
        audit(
            ctx,
            conn,
            Entry::new("share_link.updated", "document", template.id)
                .before(before.as_ref().map_or(json!(null), |b| {
                    json!({ "enabled": b.enabled, "party": b.party_id, "perHour": b.per_hour, "maxTotal": b.max_total, "expiresAt": b.expires_at })
                }))
                .after(json!({
                    "enabled": input.enabled, "party": input.party_id, "perHour": per_hour, "maxTotal": input.max_total, "expiresAt": input.expires_at,
                })),
        )
        .await?;
        let link = link_for_template(conn, template.id).await?.ok_or_else(|| not_found("Share link"))?;
        view(ctx, conn, link).await
    }
}

pub struct Rotate;

#[async_trait]
impl Service for Rotate {
    type Input = TemplateInput;
    type Output = ShareLinkView;
    const META: Meta = Meta::mutation(
        "share_links.rotate",
        "Give a template's share link a new address. The old address and any printed QR codes stop working at once.",
        Access::Permission(Permission::DocumentsCreate),
    )
    .tool()
    .destructive();

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: TemplateInput) -> Result<ShareLinkView> {
        let template = template(ctx, conn, input.template_id, Need::Edit).await?;
        sqlx::query("update share_links set slug = $2, updated_at = now() where template_id = $1")
            .bind(template.id)
            .bind(new_slug())
            .execute(&mut *conn)
            .await?;
        let link = link_for_template(conn, template.id).await?.ok_or_else(|| not_found("Share link"))?;
        audit(ctx, conn, Entry::new("share_link.rotated", "document", template.id)).await?;
        view(ctx, conn, link).await
    }
}

struct Active {
    link: LinkRow,
    template: DocumentRow,
    party: PartyRow,
}

/// The link behind a public address, if it may be used now.
async fn active(ctx: &Ctx, conn: &mut PgConnection, slug: &str, lock: bool) -> Result<Active> {
    let slug = slug.trim();
    if slug.len() != 22 || !slug.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return Err(inactive());
    }
    let lock = if lock { " for update" } else { "" };
    let link: LinkRow = sqlx::query_as(&format!("select {LINK_COLUMNS} from share_links where slug = $1{lock}"))
        .bind(slug)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or_else(inactive)?;
    if !link.enabled || link.expires_at.is_some_and(|at| at <= ctx.now) || link.max_total.is_some_and(|m| link.documents_created >= m) {
        return Err(inactive());
    }
    let template: DocumentRow = sqlx::query_as(&format!("select {DOCUMENT_COLUMNS} from documents d where id = $1"))
        .bind(link.template_id)
        .fetch_one(&mut *conn)
        .await?;
    if template.archived_at.is_some() {
        return Err(inactive());
    }
    let party = parties(conn, template.id).await?.into_iter().find(|p| Some(p.id) == link.party_id).ok_or_else(inactive)?;
    Ok(Active { link, template, party })
}

/// The visitor's name lands in a document title and in email sent in the owner's name, so it has to
/// look like a name and cannot carry an address.
fn visitor_name(input: &str) -> Result<String> {
    static NAME: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[\p{L}\p{M}][\p{L}\p{M} .,'\u{2019}-]*$").expect("a valid pattern"));
    static ADDRESS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\.[\p{L}\p{M}]{2,}").expect("a valid pattern"));
    let name = clean_value(input)?.replace('\n', " ");
    let name = name.trim();
    if name.is_empty() {
        return Err(bad_request("Enter your name"));
    }
    if name.chars().count() > MAX_VISITOR_NAME || !NAME.is_match(name) || ADDRESS.is_match(name) {
        return Err(bad_request("Enter your name as it is written on your ID, with letters only."));
    }
    Ok(name.to_string())
}

fn needs_phone(party: &PartyRow) -> bool {
    party.delivery == Delivery::Sms.as_str() || party.auth_to_view() == AuthMethod::SmsPin || party.auth_to_sign() == AuthMethod::SmsPin
}

async fn created_last_hour(ctx: &Ctx, conn: &mut PgConnection, link: Uuid) -> Result<i64> {
    Ok(sqlx::query_scalar("select count(*) from documents where share_link_id = $1 and created_at > $2")
        .bind(link)
        .bind(ctx.now - Duration::hours(1))
        .fetch_one(&mut *conn)
        .await?)
}

fn busy() -> AppError {
    too_many("Many people are using this link right now. Try again in an hour.")
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SlugInput {
    /// The last part of the share link's address.
    pub slug: String,
}

#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PublicShareLink {
    pub title: String,
    pub message: Option<String>,
    pub organisation: String,
    pub brand_color: String,
    pub logo_url: Option<String>,
    /// The template's language: en, zh, hi, es, fr, ar, bn, pt, ru, ur or sv.
    pub language: String,
    /// The visitor signs (`signer`) or approves (`approver`).
    pub role: String,
    /// A phone number is asked for, because the party gets SMS codes or SMS links.
    pub needs_phone: bool,
}

pub struct Open;

#[async_trait]
impl Service for Open {
    type Input = SlugInput;
    type Output = PublicShareLink;
    const META: Meta =
        Meta::query("share_links.open", "Open a public share link: what the visitor is asked to sign, and by whom.", Access::Public);

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: SlugInput) -> Result<PublicShareLink> {
        let Active { template, party, .. } = active(ctx, conn, &input.slug, false).await?;
        let org: OrgSettings = settings(ctx, conn).await?;
        Ok(PublicShareLink {
            title: template.title.clone(),
            message: Some(template.message.clone()).filter(|m| !m.trim().is_empty()),
            organisation: if org.name.is_empty() { "OpenSign".into() } else { org.name.clone() },
            brand_color: org.brand_color.clone(),
            logo_url: super::branding::logo_path(&org),
            language: party.lang(&template).code().to_string(),
            role: party.role.clone(),
            needs_phone: needs_phone(&party),
        })
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RequestCodeInput {
    pub slug: String,
    pub name: String,
    pub email: String,
    /// With country code. Asked for when `needsPhone` is set.
    pub phone: Option<String>,
    /// The language the visitor reads the page in, for the email.
    pub language: Option<String>,
}

#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CodeRequested {
    /// Pass this with the code to `share_links.confirm`.
    pub request_id: Uuid,
    pub expires_at: DateTime<Utc>,
}

pub fn code_hash(ctx: &Ctx, request: Uuid, code: &str) -> String {
    hex::encode(ctx.keys().mac(KeyPurpose::TokenMac, format!("share-code:{request}:{code}").as_bytes()))
}

pub struct RequestCode;

#[async_trait]
impl Service for RequestCode {
    type Input = RequestCodeInput;
    type Output = CodeRequested;
    const META: Meta = Meta::mutation(
        "share_links.request_code",
        "Email a one-time code to confirm the visitor's address before a document is made from a share link.",
        Access::Public,
    )
    .without_transaction();

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: RequestCodeInput) -> Result<CodeRequested> {
        let name = visitor_name(&input.name)?;
        // Committed before the email goes out, so a slow mail server holds no lock on the link.
        let mut tx = conn.begin().await?;
        let conn_tx: &mut PgConnection = &mut tx;
        let Active { link, template, party } = active(ctx, conn_tx, &input.slug, true).await?;
        let email = normalize_email(&input.email)?;
        let phone = match input.phone.as_deref().map(str::trim).filter(|p| !p.is_empty()) {
            Some(p) => Some(crate::sms::normalize_phone(p)?),
            None if needs_phone(&party) => return Err(bad_request("Enter your phone number")),
            None => None,
        };
        if let Some(phone) = &phone {
            let sms: SmsSettings = settings(ctx, conn_tx).await?;
            crate::sms::check_country(&sms, phone)?;
        }
        sqlx::query("delete from share_link_requests where created_at < $1")
            .bind(ctx.now - Duration::days(1))
            .execute(&mut *conn_tx)
            .await?;
        if created_last_hour(ctx, conn_tx, link.id).await? >= link.per_hour as i64 {
            return Err(busy());
        }
        let (for_link, for_email): (i64, i64) = sqlx::query_as(
            "select count(*) filter (where share_link_id = $1), count(*) filter (where lower(email) = $2) from share_link_requests where created_at > $3",
        )
        .bind(link.id)
        .bind(&email)
        .bind(ctx.now - Duration::hours(1))
        .fetch_one(&mut *conn_tx)
        .await?;
        if for_link >= (link.per_hour as i64 * 3).max(20) {
            return Err(busy());
        }
        if for_email >= CODES_PER_EMAIL_HOUR {
            return Err(too_many("Too many codes were sent to this address. Try again in an hour."));
        }
        let id = Uuid::now_v7();
        let code = format!("{:06}", u32::from_be_bytes(random_bytes::<4>()) % 1_000_000);
        let expires_at = ctx.now + Duration::minutes(CODE_MINUTES);
        sqlx::query(
            "insert into share_link_requests (id, share_link_id, email, name, phone, code_hash, ip, expires_at) values ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(id)
        .bind(link.id)
        .bind(&email)
        .bind(&name)
        .bind(&phone)
        .bind(code_hash(ctx, id, &code))
        .bind(&ctx.ip)
        .bind(expires_at)
        .execute(&mut *conn_tx)
        .await?;
        tx.commit().await?;
        let org: OrgSettings = settings(ctx, conn).await?;
        let org_name = if org.name.is_empty() { "OpenSign".to_string() } else { org.name.clone() };
        let lang = input.language.as_deref().and_then(Lang::parse).unwrap_or_else(|| party.lang(&template));
        let email_settings: EmailSettings = settings(ctx, conn).await?;
        let sent = email::send(
            &ctx.app,
            &email_settings,
            &org,
            &Message {
                to: email.clone(),
                to_name: Some(name.clone()),
                subject: lang.share_code_subject(&code, &template.title),
                paragraphs: vec![lang.share_code(&template.title, &org_name), code.clone()],
                button: None,
                footer: Some(lang.share_code_ignore().into()),
                attachments: vec![],
                lang,
            },
        )
        .await;
        if let Err(e) = sent {
            sqlx::query("delete from share_link_requests where id = $1").bind(id).execute(&mut *conn).await?;
            return Err(e);
        }
        audit(ctx, conn, Entry::new("share_link.code_sent", "document", template.id).metadata(json!({ "email": email, "name": name })))
            .await?;
        Ok(CodeRequested { request_id: id, expires_at })
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmInput {
    pub slug: String,
    pub request_id: Uuid,
    pub code: String,
}

#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Started {
    /// The visitor's personal signing link for the new document. It was also emailed to them.
    pub url: String,
    /// False when other parties sign first; the visitor is emailed when it is their turn.
    pub your_turn: bool,
}

#[derive(sqlx::FromRow)]
struct RequestRow {
    id: Uuid,
    email: String,
    name: String,
    phone: Option<String>,
    code_hash: String,
}

pub struct Confirm;

#[async_trait]
impl Service for Confirm {
    type Input = ConfirmInput;
    type Output = Started;
    const META: Meta = Meta::mutation(
        "share_links.confirm",
        "Check the emailed code, then make and send a document from the share link's template with the visitor as its party.",
        Access::Public,
    )
    .without_transaction();

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: ConfirmInput) -> Result<Started> {
        let wrong = || bad_request("That code is not right, or it has expired. Ask for a new one.");
        let request: Option<RequestRow> = sqlx::query_as(
            "update share_link_requests r set attempts = attempts + 1 from share_links l
             where r.id = $1 and l.id = r.share_link_id and l.slug = $2 and r.expires_at > $3 and r.attempts < $4
             returning r.id, r.email, r.name, r.phone, r.code_hash",
        )
        .bind(input.request_id)
        .bind(input.slug.trim())
        .bind(ctx.now)
        .bind(CODE_ATTEMPTS)
        .fetch_optional(&mut *conn)
        .await?;
        let Some(request) = request else { return Err(wrong()) };
        let code: String = input.code.chars().filter(char::is_ascii_digit).collect();
        if !constant_time_eq(code_hash(ctx, request.id, &code).as_bytes(), request.code_hash.as_bytes()) {
            return Err(wrong());
        }
        let mut tx = conn.begin().await?;
        let started = start_document(ctx, &mut tx, &input.slug, &request).await?;
        tx.commit().await?;
        Ok(started)
    }
}

async fn start_document(ctx: &Ctx, conn: &mut PgConnection, slug: &str, request: &RequestRow) -> Result<Started> {
    let Active { link, template, party } = active(ctx, conn, slug, true).await?;
    let used = sqlx::query("delete from share_link_requests where id = $1").bind(request.id).execute(&mut *conn).await?;
    if used.rows_affected() == 0 {
        return Err(bad_request("That code was already used. Ask for a new one."));
    }
    if created_last_hour(ctx, conn, link.id).await? >= link.per_hour as i64 {
        return Err(busy());
    }
    let security: SecuritySettings = settings(ctx, conn).await?;
    let owner = match link.owner_id {
        Some(id) => user_by_id(conn, id).await?,
        None => None,
    };
    let actor = owner.as_ref().and_then(|u| actor_for(u, &security, Credential::Session)).filter(|a| a.can(Permission::DocumentsCreate));
    let Some(actor) = actor else {
        tracing::warn!(link = %link.id, "share link owner can no longer send documents");
        return Err(inactive());
    };
    let as_owner = ctx.clone().with_actor(actor);
    let title: String = format!("{} \u{2013} {}", template.title, request.name).chars().take(200).collect();
    let (created, ids) = docs::create(
        &as_owner,
        conn,
        CreateInput {
            title: title.trim().to_string(),
            message: None,
            language: None,
            template_id: Some(template.id),
            is_template: false,
            sequential: None,
            external_id: None,
            metadata: Some(json!({ "shareLink": link.id })),
            folder_id: None,
        },
        Origin::ShareLink(link.id),
    )
    .await?;
    let document_id = created.document.id;
    let visitor = ids.get(&party.id).copied().ok_or_else(inactive)?;
    let inputs = parties(conn, document_id)
        .await?
        .iter()
        .map(|p| {
            let me = p.id == visitor;
            PartyInput {
                id: Some(p.id),
                role: p.role(),
                name: if me { request.name.clone() } else { p.name.clone() },
                email: if me { Some(request.email.clone()) } else { p.email.clone() },
                phone: if me { request.phone.clone() } else { p.phone.clone() },
                personal_number: me.then(String::new),
                company: if me { None } else { p.company.clone() },
                sign_order: Some(p.sign_order),
                delivery: match Delivery::parse(&p.delivery).unwrap_or(Delivery::Email) {
                    Delivery::Sms if me => Delivery::Sms,
                    _ if me => Delivery::Email,
                    other => other,
                },
                auth_to_view: p.auth_to_view(),
                auth_to_sign: p.auth_to_sign(),
                attachments: None,
                language: p.language.clone(),
                webcam_photo: None,
            }
        })
        .collect();
    docs::SetParties::run(&as_owner, conn, docs::SetPartiesInput { document_id, parties: inputs }).await?;
    docs::Send::run(&as_owner, conn, docs::SendInput { id: document_id, expires_at: None }).await?;
    sqlx::query("update share_links set documents_created = documents_created + 1 where id = $1").bind(link.id).execute(&mut *conn).await?;
    audit(
        ctx,
        conn,
        Entry::new("share_link.used", "document", template.id)
            .metadata(json!({ "document": document_id, "email": request.email, "name": request.name })),
    )
    .await?;
    let document: DocumentRow = sqlx::query_as(&format!("select {DOCUMENT_COLUMNS} from documents d where id = $1"))
        .bind(document_id)
        .fetch_one(&mut *conn)
        .await?;
    if document.status != status::PENDING {
        return Err(conflict("The document could not be sent"));
    }
    let all = parties(conn, document_id).await?;
    let me = all.iter().find(|p| p.id == visitor).ok_or_else(inactive)?;
    Ok(Started { url: signer_url(ctx, ctx.keys(), me), your_turn: current_turn(&document, &all).contains(&me.id) })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::visitor_name;

    #[test]
    fn a_visitor_name_looks_like_a_name() {
        for fine in ["Anna Andersson", "José-María O'Neil", "J.R. Smith", "Björn Åkesson, Jr.", "स्वामी विवेकानन्द", "李明", "محمد علي"]
        {
            assert_eq!(visitor_name(fine).unwrap(), fine);
        }
        for bad in ["Open https://a.example", "a.example", "www.a.example now", "me@a.example", "Call 0701234567", "<b>Anna</b>", ""] {
            assert!(visitor_name(bad).is_err(), "{bad}");
        }
        assert!(visitor_name(&"a".repeat(101)).is_err());
    }
}
