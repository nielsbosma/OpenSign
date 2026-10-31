use chrono::{DateTime, Duration, Utc};
use hmac::{Hmac, Mac};
use opensign_crypto::KeyPurpose;
use serde_json::{Value, json};
use sha2::Sha256;
use sqlx::PgConnection;
use uuid::Uuid;

use super::{Delivery, DocumentRow, PartyRole, PartyRow, current_turn, parties, party, signer_url, status};
use crate::audit::{Entry, audit};
use crate::context::Ctx;
use crate::email::{self, Attachment, Message};
use crate::error::{AppError, Result};
use crate::i18n::{Lang, Stopped};
use crate::jobs::{Job, enqueue, retry_delay};
use crate::services::settings;
use crate::settings::{EmailSettings, OrgSettings, SecuritySettings, SmsSettings};
use crate::storage::{file_record, read_file};

const ATTACHMENT_LIMIT: i64 = 10 * 1024 * 1024;

async fn document(conn: &mut PgConnection, id: Uuid) -> Result<Option<DocumentRow>> {
    Ok(sqlx::query_as(&format!("select {} from documents d where id = $1", super::DOCUMENT_COLUMNS))
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?)
}

async fn owner(conn: &mut PgConnection, document: &DocumentRow) -> Result<Option<(String, String)>> {
    let Some(id) = document.owner_id else { return Ok(None) };
    Ok(sqlx::query_as("select name, email from users where id = $1 and active").bind(id).fetch_optional(&mut *conn).await?)
}

fn org_name(org: &OrgSettings) -> &str {
    if org.name.is_empty() { "OpenSign" } else { &org.name }
}

/// Sends a party their link, by email or SMS, if it is their turn.
pub async fn notify_party(ctx: &Ctx, conn: &mut PgConnection, party_id: Uuid, reminder: bool) -> Result<()> {
    let Some(mut party) = party(conn, party_id).await? else { return Ok(()) };
    let Some(document) = document(conn, party.document_id).await? else { return Ok(()) };
    if document.status != status::PENDING || party.signed_at.is_some() || party.declined_at.is_some() {
        return Ok(());
    }
    let all = parties(conn, document.id).await?;
    if !current_turn(&document, &all).contains(&party.id) {
        return Ok(());
    }
    let delivery = Delivery::parse(&party.delivery).unwrap_or(Delivery::None);
    if !matches!(delivery, Delivery::Email | Delivery::Sms) {
        return Ok(());
    }
    if party.link_expires_at.is_none() {
        let security: SecuritySettings = settings(ctx, conn).await?;
        let expires = document.expires_at.unwrap_or(ctx.now + Duration::days(security.signer_link_days.max(1) as i64));
        sqlx::query("update parties set link_expires_at = $2 where id = $1").bind(party.id).bind(expires).execute(&mut *conn).await?;
        party.link_expires_at = Some(expires);
    }
    let org: OrgSettings = settings(ctx, conn).await?;
    let sender = owner(conn, &document).await?.map(|(name, _)| name).unwrap_or_else(|| org_name(&org).to_string());
    let url = signer_url(ctx, ctx.keys(), &party);
    let approve = party.role() == PartyRole::Approver;
    let lang = party.lang(&document);
    match delivery {
        Delivery::Email => {
            let to = party
                .email
                .clone()
                .ok_or_else(|| AppError::internal(anyhow::anyhow!("party {} has email delivery without an email", party.id)))?;
            let mut paragraphs = vec![lang.invitation(&sender, org_name(&org), &document.title, approve)];
            if !document.message.trim().is_empty() {
                paragraphs.push(document.message.trim().to_string());
            }
            if let Some(expires) = party.link_expires_at {
                paragraphs.push(lang.link_until(expires));
            }
            let email_settings: EmailSettings = settings(ctx, conn).await?;
            email::send(
                &ctx.app,
                &email_settings,
                &org,
                &Message {
                    to,
                    to_name: Some(party.name.clone()),
                    subject: lang.invitation_subject(reminder, &sender, &document.title, approve),
                    paragraphs,
                    button: Some((lang.open_button(approve).into(), url)),
                    footer: Some(lang.personal_link().into()),
                    attachments: vec![],
                    lang,
                },
            )
            .await?;
        }
        Delivery::Sms => {
            let phone = party
                .phone
                .clone()
                .ok_or_else(|| AppError::internal(anyhow::anyhow!("party {} has SMS delivery without a phone", party.id)))?;
            let sms: SmsSettings = settings(ctx, conn).await?;
            let text = lang.invitation_sms(org_name(&org), &sender, &document.title, approve, &url);
            crate::sms::send(&ctx.app, &sms, &phone, &text).await?;
        }
        _ => {}
    }
    sqlx::query("update parties set notified_at = $2, status = case when status = 'waiting' then 'notified' else status end, updated_at = now() where id = $1")
        .bind(party.id)
        .bind(ctx.now)
        .execute(&mut *conn)
        .await?;
    audit(
        ctx,
        conn,
        Entry::new(if reminder { "party.reminded" } else { "party.notified" }, "document", document.id)
            .metadata(json!({ "party": party.id, "name": party.name, "channel": party.delivery })),
    )
    .await?;
    Ok(())
}

/// Queues notifications for whoever's turn it is now.
pub async fn notify_turn(conn: &mut PgConnection, document: &DocumentRow) -> Result<()> {
    let all = parties(conn, document.id).await?;
    for id in current_turn(document, &all) {
        let party = all.iter().find(|p| p.id == id);
        if party.is_some_and(|p| p.notified_at.is_none()) {
            enqueue(conn, "notify_party", json!({ "party": id }), None, Some(&format!("notify:{id}"))).await?;
        }
    }
    Ok(())
}

/// Who a per-recipient email job is for: a party, or the document's owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Recipient {
    Party(Uuid),
    Owner,
}

impl Recipient {
    pub fn parse(value: &str) -> Option<Recipient> {
        if value == "owner" { Some(Recipient::Owner) } else { Uuid::try_parse(value).ok().map(Recipient::Party) }
    }

    fn as_string(self) -> String {
        match self {
            Recipient::Party(id) => id.to_string(),
            Recipient::Owner => "owner".into(),
        }
    }
}

/// Everyone who gets the sealed document: parties with an email address, and the owner unless
/// they are also a party.
async fn completed_recipients(
    conn: &mut PgConnection,
    document: &DocumentRow,
) -> Result<Vec<(Recipient, String, String, Option<PartyRow>)>> {
    let mut recipients = Vec::new();
    for p in parties(conn, document.id).await? {
        if let Some(email) = p.email.clone() {
            recipients.push((Recipient::Party(p.id), email, p.name.clone(), Some(p)));
        }
    }
    if let Some((name, email)) = owner(conn, document).await?
        && !recipients.iter().any(|(_, e, _, _)| e.eq_ignore_ascii_case(&email))
    {
        recipients.push((Recipient::Owner, email, name, None));
    }
    Ok(recipients)
}

/// Emails the sealed document to every party with an email address and to the owner. Without a
/// recipient it queues one job per recipient, so a failed send is retried without repeating the
/// emails that already went out.
pub async fn notify_completed(ctx: &Ctx, conn: &mut PgConnection, document_id: Uuid, to: Option<Recipient>) -> Result<()> {
    let Some(document) = document(conn, document_id).await? else { return Ok(()) };
    let Some(sealed_id) = document.sealed_file_id.filter(|_| document.status == status::COMPLETED) else { return Ok(()) };
    let recipients = completed_recipients(conn, &document).await?;
    let Some(to) = to else {
        for (recipient, ..) in recipients {
            let key = format!("completed:{}:{}", document.id, recipient.as_string());
            enqueue(conn, "notify_completed", json!({ "document": document.id, "to": recipient.as_string() }), None, Some(&key)).await?;
        }
        return Ok(());
    };
    let Some((_, email, name, party)) = recipients.into_iter().find(|(r, ..)| *r == to) else { return Ok(()) };
    let record = file_record(conn, sealed_id).await?;
    let attachment =
        if record.size_bytes <= ATTACHMENT_LIMIT { Some(read_file(&ctx.app.storage, ctx.keys(), &record).await?) } else { None };
    let org: OrgSettings = settings(ctx, conn).await?;
    let email_settings: EmailSettings = settings(ctx, conn).await?;
    let lang = party.as_ref().map_or_else(|| Lang::of(&document.language), |p| p.lang(&document));
    let mut paragraphs = vec![lang.completed(&document.title)];
    let button = match (&attachment, &party) {
        (Some(_), _) => {
            paragraphs.push(lang.completed_attached().into());
            None
        }
        (None, Some(p)) => Some((lang.download_button().to_string(), signer_url(ctx, ctx.keys(), p))),
        (None, None) => Some(("Open Document".to_string(), format!("{}/documents/{}", ctx.app.public_url(), document.id))),
    };
    let filename = format!("{}.pdf", sanitize_filename(&document.title));
    email::send(
        &ctx.app,
        &email_settings,
        &org,
        &Message {
            to: email,
            to_name: Some(name),
            subject: lang.completed_subject(&document.title),
            paragraphs,
            button,
            footer: None,
            attachments: attachment.into_iter().map(|content| Attachment { filename: filename.clone(), content }).collect(),
            lang,
        },
    )
    .await
}

pub fn sanitize_filename(title: &str) -> String {
    let name: String = title.chars().map(|c| if c.is_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.') { c } else { '_' }).collect();
    let name = name.trim().trim_matches('.').to_string();
    if name.is_empty() { "document".into() } else { name.chars().take(100).collect() }
}

/// Tells the owner a party declined, then queues the note to every other notified party.
pub async fn notify_declined(ctx: &Ctx, conn: &mut PgConnection, document_id: Uuid, party_id: Uuid) -> Result<()> {
    let Some(document) = document(conn, document_id).await? else { return Ok(()) };
    let Some(decliner) = party(conn, party_id).await? else { return Ok(()) };
    let org: OrgSettings = settings(ctx, conn).await?;
    let email_settings: EmailSettings = settings(ctx, conn).await?;
    if let Some((name, email)) = owner(conn, &document).await? {
        let mut paragraphs = vec![format!("{} declined to sign \u{201c}{}\u{201d}. Signing has stopped.", decliner.name, document.title)];
        if let Some(reason) = decliner.decline_reason.as_deref().filter(|r| !r.trim().is_empty()) {
            paragraphs.push(format!("Their reason: {reason}"));
        }
        let message = Message {
            to: email,
            to_name: Some(name),
            subject: format!("Declined: {}", document.title),
            paragraphs,
            button: Some(("Open Document".into(), format!("{}/documents/{}", ctx.app.public_url(), document.id))),
            footer: None,
            attachments: vec![],
            lang: Lang::En,
        };
        email::send(&ctx.app, &email_settings, &org, &message).await?;
    }
    queue_stopped(conn, &document, Some(party_id)).await
}

fn told_stopped(document: &DocumentRow, party: &PartyRow) -> bool {
    party.notified_at.is_some() && party.email.is_some() && !(party.signed_at.is_some() && document.status == status::COMPLETED)
}

/// Queues one `notify_stopped` job per party who already got a link, except the one who declined.
pub async fn queue_stopped(conn: &mut PgConnection, document: &DocumentRow, except: Option<Uuid>) -> Result<()> {
    for p in parties(conn, document.id).await? {
        if Some(p.id) != except && told_stopped(document, &p) {
            let key = format!("stopped:{}:{}", document.id, p.id);
            enqueue(conn, "notify_stopped", json!({ "document": document.id, "party": p.id }), None, Some(&key)).await?;
        }
    }
    Ok(())
}

/// Tells a party who already got a link that the document will not be signed.
pub async fn notify_stopped(ctx: &Ctx, conn: &mut PgConnection, document_id: Uuid, party_id: Uuid) -> Result<()> {
    let Some(document) = document(conn, document_id).await? else { return Ok(()) };
    let Some(p) = party(conn, party_id).await?.filter(|p| p.document_id == document.id && told_stopped(&document, p)) else {
        return Ok(());
    };
    let decliner = parties(conn, document.id).await?.into_iter().find(|p| p.declined_at.is_some()).map(|p| p.name);
    let why = match (document.status.as_str(), &decliner) {
        (status::DECLINED, Some(name)) => Stopped::Declined(name),
        (status::CANCELED, _) => Stopped::Canceled,
        _ => return Ok(()),
    };
    let lang = p.lang(&document);
    let org: OrgSettings = settings(ctx, conn).await?;
    let email_settings: EmailSettings = settings(ctx, conn).await?;
    let message = Message {
        to: p.email.clone().unwrap_or_default(),
        to_name: Some(p.name.clone()),
        subject: lang.stopped_subject(&document.title),
        paragraphs: vec![lang.stopped(&why, &document.title)],
        button: None,
        footer: None,
        attachments: vec![],
        lang,
    };
    email::send(&ctx.app, &email_settings, &org, &message).await
}

/// Where a delivery goes: a webhook endpoint, or a Composio automation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Webhook(Uuid),
    Automation(Uuid),
}

/// Queues a delivery for every active webhook subscribed to `event` and every active automation
/// whose rule matches the document, in the caller's transaction.
pub async fn emit(conn: &mut PgConnection, event: &str, data: Value) -> Result<()> {
    let hooks: Vec<Uuid> = sqlx::query_scalar("select id from webhooks where active and ($1 = any(events) or '*' = any(events))")
        .bind(event)
        .fetch_all(&mut *conn)
        .await?;
    for hook in hooks {
        emit_to(conn, hook, event, &data).await?;
    }
    let document = data["documentId"].as_str().and_then(|id| Uuid::try_parse(id).ok());
    let automations: Vec<Uuid> = sqlx::query_scalar(
        "select a.id from automations a left join documents d on d.id = $2
         where a.active and a.event = $1
           and (cardinality(a.template_ids) = 0 or d.template_id = any(a.template_ids))
           and (cardinality(a.folder_ids) = 0 or d.folder_id = any(a.folder_ids))
         order by a.created_at",
    )
    .bind(event)
    .bind(document)
    .fetch_all(&mut *conn)
    .await?;
    for automation in automations {
        queue_delivery(conn, Target::Automation(automation), event, &data, crate::context::now(), None).await?;
    }
    Ok(())
}

pub async fn emit_to(conn: &mut PgConnection, hook: Uuid, event: &str, data: &Value) -> Result<Uuid> {
    queue_delivery(conn, Target::Webhook(hook), event, data, crate::context::now(), None).await
}

/// Queues one delivery with a new id. A resend keeps the event's original `createdAt`.
pub async fn queue_delivery(
    conn: &mut PgConnection,
    target: Target,
    event: &str,
    data: &Value,
    created_at: DateTime<Utc>,
    resent_from: Option<Uuid>,
) -> Result<Uuid> {
    let id = Uuid::now_v7();
    let payload = json!({ "id": id, "event": event, "createdAt": created_at, "data": data });
    let (hook, automation, job) = match target {
        Target::Webhook(hook) => (Some(hook), None, "webhook"),
        Target::Automation(automation) => (None, Some(automation), "automation"),
    };
    sqlx::query(
        "insert into webhook_deliveries (id, webhook_id, automation_id, event, payload, resent_from, next_attempt_at) values ($1, $2, $3, $4, $5, $6, now())",
    )
    .bind(id)
    .bind(hook)
    .bind(automation)
    .bind(event)
    .bind(&payload)
    .bind(resent_from)
    .execute(&mut *conn)
    .await?;
    enqueue(conn, job, json!({ "delivery": id }), None, None).await?;
    Ok(id)
}

pub fn webhook_secret_context(id: Uuid) -> String {
    format!("webhook:{id}:secret")
}

/// `t=<unix seconds>,v1=<hex HMAC-SHA256 of "<t>.<body>">`, the scheme Stripe popularised, so
/// receivers can reject replays by checking `t`.
pub fn webhook_signature(secret: &[u8], timestamp: i64, body: &[u8]) -> String {
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(secret).expect("HMAC takes any key length");
    mac.update(timestamp.to_string().as_bytes());
    mac.update(b".");
    mac.update(body);
    format!("t={timestamp},v1={}", hex::encode(mac.finalize().into_bytes()))
}

#[derive(sqlx::FromRow)]
struct PendingDelivery {
    webhook_id: Uuid,
    payload: Value,
    url: String,
    secret: String,
    active: bool,
}

pub const EXCERPT_BYTES: usize = 2048;

pub async fn deliver_webhook(ctx: &Ctx, conn: &mut PgConnection, delivery_id: Uuid, job: &Job) -> Result<()> {
    let Some(d): Option<PendingDelivery> = sqlx::query_as(
        "select d.webhook_id, d.payload, w.url, w.secret, w.active from webhook_deliveries d join webhooks w on w.id = d.webhook_id where d.id = $1 and d.delivered_at is null",
    )
    .bind(delivery_id)
    .fetch_optional(&mut *conn)
    .await?
    else {
        return Ok(());
    };
    if !d.active {
        return Ok(());
    }
    let secret = ctx.keys().decrypt(KeyPurpose::Settings, &webhook_secret_context(d.webhook_id), &d.secret).map_err(AppError::internal)?;
    let body = serde_json::to_vec(&d.payload).map_err(AppError::internal)?;
    let signature = webhook_signature(&secret, ctx.now.timestamp(), &body);
    let started = std::time::Instant::now();
    let result = async {
        let development = ctx.app.env.development;
        crate::net::check_webhook_url(&d.url, development).map_err(|e| anyhow::anyhow!(e))?;
        let http = if development { &ctx.app.http.internal } else { &ctx.app.http.public };
        let mut response = http
            .post(&d.url)
            .header("content-type", "application/json")
            .header("opensign-signature", signature)
            .header("opensign-delivery", delivery_id.to_string())
            .timeout(std::time::Duration::from_secs(10))
            .body(body)
            .send()
            .await
            .map_err(describe)?;
        let code = response.status().as_u16();
        anyhow::Ok((code, excerpt(&mut response).await))
    }
    .await;
    let duration_ms = i32::try_from(started.elapsed().as_millis()).unwrap_or(i32::MAX);
    let (code, error, response) = match result {
        Ok((code, body)) if (200..300).contains(&code) => (Some(code as i32), None, body),
        Ok((code, body)) => (Some(code as i32), Some(format!("The endpoint answered {code}")), body),
        Err(e) => (None, Some(e.to_string()), None),
    };
    let attempt = Attempt { status_code: code, error: error.clone(), response, duration_ms, retry: true };
    record_attempt(ctx, conn, delivery_id, job, attempt).await?;
    match error {
        Some(e) => Err(AppError::internal(anyhow::anyhow!("webhook delivery failed: {e}"))),
        None => Ok(()),
    }
}

/// One try at a delivery, for the log.
pub struct Attempt {
    pub status_code: Option<i32>,
    pub error: Option<String>,
    pub response: Option<String>,
    pub duration_ms: i32,
    /// Off for failures that another try cannot fix.
    pub retry: bool,
}

/// Logs an attempt and moves the delivery on: delivered, waiting for the job's next retry, or given up.
pub async fn record_attempt(ctx: &Ctx, conn: &mut PgConnection, delivery_id: Uuid, job: &Job, attempt: Attempt) -> Result<()> {
    let next_attempt =
        (attempt.error.is_some() && attempt.retry && job.attempts < job.max_attempts).then(|| ctx.now + retry_delay(job.attempts));
    sqlx::query(
        "insert into webhook_attempts (id, delivery_id, status_code, error, response_excerpt, duration_ms, created_at) values ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(Uuid::now_v7())
    .bind(delivery_id)
    .bind(attempt.status_code)
    .bind(&attempt.error)
    .bind(&attempt.response)
    .bind(attempt.duration_ms)
    .bind(ctx.now)
    .execute(&mut *conn)
    .await?;
    sqlx::query(
        "update webhook_deliveries set attempts = attempts + 1, status_code = $2, error = $3, last_attempt_at = $4, next_attempt_at = $5,
           delivered_at = case when $3 is null then $4 else null end where id = $1",
    )
    .bind(delivery_id)
    .bind(attempt.status_code)
    .bind(&attempt.error)
    .bind(ctx.now)
    .bind(next_attempt)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// The error and its causes, without the URL, which can carry a token in its path.
fn describe(e: reqwest::Error) -> anyhow::Error {
    if e.is_timeout() {
        return anyhow::anyhow!("The endpoint did not answer within 10 seconds");
    }
    let e = e.without_url();
    let mut text = e.to_string();
    let mut source = std::error::Error::source(&e);
    while let Some(cause) = source {
        text.push_str(": ");
        text.push_str(&cause.to_string());
        source = cause.source();
    }
    anyhow::anyhow!(text)
}

async fn excerpt(response: &mut reqwest::Response) -> Option<String> {
    let mut bytes = Vec::new();
    while bytes.len() < EXCERPT_BYTES {
        match response.chunk().await {
            Ok(Some(chunk)) => bytes.extend_from_slice(&chunk),
            _ => break,
        }
    }
    excerpt_text(&bytes)
}

pub fn excerpt_text(bytes: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(&bytes[..bytes.len().min(EXCERPT_BYTES)]).replace('\0', "");
    (!text.trim().is_empty()).then_some(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signs_webhooks_like_stripe() {
        let sig = webhook_signature(b"secret", 1_700_000_000, b"{}");
        assert!(sig.starts_with("t=1700000000,v1="));
        assert_eq!(sig.len(), "t=1700000000,v1=".len() + 64);
    }

    #[test]
    fn response_excerpts_are_short_text() {
        assert_eq!(excerpt_text(b"  \n"), None);
        assert_eq!(excerpt_text(b"ok\0"), Some("ok".into()));
        assert_eq!(excerpt_text(&[b'a'; 5000]).map(|t| t.len()), Some(EXCERPT_BYTES));
        assert!(excerpt_text(&[0xff, b'x']).is_some_and(|t| t.ends_with('x')));
    }

    #[test]
    fn filenames_are_safe() {
        assert_eq!(sanitize_filename("../../etc/passwd"), "_.._etc_passwd");
        assert_eq!(sanitize_filename("Avtal: Acme/Beta"), "Avtal_ Acme_Beta");
        assert_eq!(sanitize_filename("..."), "document");
    }
}
