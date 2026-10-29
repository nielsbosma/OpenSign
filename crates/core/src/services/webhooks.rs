use async_trait::async_trait;
use chrono::{DateTime, Utc};
use opensign_crypto::{KeyPurpose, random_token};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::PgConnection;
use uuid::Uuid;

use super::{Empty, Ok_};
use crate::audit::{Entry, audit};
use crate::context::Ctx;
use crate::documents::notify::{emit_to, queue_delivery, webhook_secret_context};
use crate::error::{Result, bad_request, not_found};
use crate::net::check_public_url;
use crate::permissions::Permission;
use crate::service::{Access, Meta, Service};

pub const EVENTS: &[&str] = &[
    "document.created",
    "document.sent",
    "document.viewed",
    "document.signed",
    "document.completed",
    "document.declined",
    "document.canceled",
    "document.expired",
];

#[derive(Serialize, JsonSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct WebhookView {
    pub id: Uuid,
    pub url: String,
    pub events: Vec<String>,
    pub active: bool,
    pub created_at: DateTime<Utc>,
    pub last_status: Option<i32>,
    pub last_error: Option<String>,
    /// Deliveries in the log that were given up and not resent.
    pub failed: i64,
}

pub struct List;

#[async_trait]
impl Service for List {
    type Input = Empty;
    type Output = Vec<WebhookView>;
    const META: Meta = Meta::query(
        "webhooks.list",
        "List webhooks, their events, the result of the latest delivery and how many deliveries failed.",
        Access::Permission(Permission::WebhooksManage),
    );

    async fn run(_ctx: &Ctx, conn: &mut PgConnection, _: Empty) -> Result<Vec<WebhookView>> {
        Ok(sqlx::query_as(&format!(
            "select w.id, w.url, w.events, w.active, w.created_at, d.status_code as last_status, d.error as last_error,
               (select count(*) from webhook_deliveries f where f.webhook_id = w.id and {FAILED}) as failed
             from webhooks w left join lateral (select status_code, error from webhook_deliveries where webhook_id = w.id order by created_at desc limit 1) d on true
             order by w.created_at"
        ))
        .fetch_all(&mut *conn)
        .await?)
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateInput {
    /// A public https URL.
    pub url: String,
    /// Event names, or "*" for all: document.created, document.sent, document.viewed, document.signed, document.completed, document.declined, document.canceled, document.expired.
    pub events: Vec<String>,
}

#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreatedWebhook {
    pub webhook: WebhookView,
    /// Shown once. Verify the `OpenSign-Signature` header with it.
    pub secret: String,
}

pub struct Create;

#[async_trait]
impl Service for Create {
    type Input = CreateInput;
    type Output = CreatedWebhook;
    const META: Meta = Meta::mutation(
        "webhooks.create",
        "Add a webhook that receives signed JSON events when documents change. Returns its signing secret once.",
        Access::Permission(Permission::WebhooksManage),
    );

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: CreateInput) -> Result<CreatedWebhook> {
        let url = check_public_url(input.url.trim()).map_err(bad_request)?.to_string();
        let mut events: Vec<String> = input.events.iter().map(|e| e.trim().to_string()).collect();
        events.sort();
        events.dedup();
        if events.is_empty() || events.iter().any(|e| e != "*" && !EVENTS.contains(&e.as_str())) {
            return Err(bad_request(format!("Choose events from: *, {}", EVENTS.join(", "))));
        }
        let id = Uuid::now_v7();
        let secret = format!("whsec_{}", random_token());
        sqlx::query("insert into webhooks (id, url, secret, events, created_by) values ($1, $2, $3, $4, $5)")
            .bind(id)
            .bind(&url)
            .bind(ctx.keys().encrypt_str(KeyPurpose::Settings, &webhook_secret_context(id), &secret))
            .bind(&events)
            .bind(ctx.actor()?.user_id)
            .execute(&mut *conn)
            .await?;
        audit(ctx, conn, Entry::new("webhooks.create", "webhook", id).after(json!({ "url": url, "events": events }))).await?;
        let webhook = WebhookView { id, url, events, active: true, created_at: ctx.now, last_status: None, last_error: None, failed: 0 };
        Ok(CreatedWebhook { webhook, secret })
    }
}

#[derive(Deserialize, JsonSchema)]
pub struct IdInput {
    pub id: Uuid,
}

pub struct Delete;

#[async_trait]
impl Service for Delete {
    type Input = IdInput;
    type Output = Ok_;
    const META: Meta = Meta::mutation(
        "webhooks.delete",
        "Remove a webhook. Queued deliveries are dropped.",
        Access::Permission(Permission::WebhooksManage),
    );

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: IdInput) -> Result<Ok_> {
        let url: Option<String> =
            sqlx::query_scalar("delete from webhooks where id = $1 returning url").bind(input.id).fetch_optional(&mut *conn).await?;
        let url = url.ok_or_else(|| not_found("Webhook"))?;
        audit(ctx, conn, Entry::new("webhooks.delete", "webhook", input.id).before(json!({ "url": url }))).await?;
        Ok(Ok_::default())
    }
}

pub struct Test;

#[async_trait]
impl Service for Test {
    type Input = IdInput;
    type Output = Ok_;
    const META: Meta = Meta::mutation(
        "webhooks.test",
        "Send a webhook.test event to one webhook, to check the receiving end.",
        Access::Permission(Permission::WebhooksManage),
    );

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: IdInput) -> Result<Ok_> {
        let exists: Option<Uuid> =
            sqlx::query_scalar("select id from webhooks where id = $1").bind(input.id).fetch_optional(&mut *conn).await?;
        exists.ok_or_else(|| not_found("Webhook"))?;
        emit_to(conn, input.id, "webhook.test", &json!({ "webhookId": input.id, "message": "Test event from OpenSign" })).await?;
        audit(ctx, conn, Entry::new("webhooks.test", "webhook", input.id)).await?;
        Ok(Ok_::default())
    }
}

/// Given up after the last retry, and not resent since.
const FAILED: &str = "f.delivered_at is null and f.next_attempt_at is null and f.attempts > 0
    and not exists (select 1 from webhook_deliveries r where r.resent_from = f.id)";

const DELIVERY_COLUMNS: &str = "f.id, f.webhook_id, f.event, f.created_at, f.attempts, f.status_code, f.error, f.delivered_at,
    f.last_attempt_at, f.next_attempt_at, f.resent_from,
    case when f.delivered_at is not null then 'delivered' when f.next_attempt_at is not null then 'pending' else 'failed' end as status";

#[derive(Deserialize, JsonSchema, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryStatus {
    /// Queued or waiting for a retry.
    Pending,
    Delivered,
    /// Given up after the last retry.
    Failed,
}

#[derive(Serialize, JsonSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct DeliveryView {
    pub id: Uuid,
    pub webhook_id: Uuid,
    pub event: String,
    pub created_at: DateTime<Utc>,
    /// pending (queued or waiting for a retry), delivered, or failed (given up after the last retry).
    pub status: String,
    pub attempts: i32,
    /// The status code of the latest attempt, if the endpoint answered.
    pub status_code: Option<i32>,
    /// Why the latest attempt failed.
    pub error: Option<String>,
    pub delivered_at: Option<DateTime<Utc>>,
    pub last_attempt_at: Option<DateTime<Utc>>,
    pub next_attempt_at: Option<DateTime<Utc>>,
    /// The delivery this one resends.
    pub resent_from: Option<Uuid>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeliveriesInput {
    pub webhook_id: Uuid,
    /// Only deliveries with this status. "failed" also includes deliveries waiting for a retry after a failed attempt.
    pub status: Option<DeliveryStatus>,
    /// Only this event.
    pub event: Option<String>,
    /// For the next page: the id of the last delivery of the previous page.
    pub before: Option<Uuid>,
    /// At most 200; 50 when left out.
    pub limit: Option<i64>,
}

pub struct Deliveries;

#[async_trait]
impl Service for Deliveries {
    type Input = DeliveriesInput;
    type Output = Vec<DeliveryView>;
    const META: Meta = Meta::query(
        "webhooks.deliveries",
        "List a webhook's deliveries from the last 30 days, newest first: event, status, attempts, the latest status code or error, and the next retry. Use it to find out why an endpoint missed events.",
        Access::Permission(Permission::WebhooksManage),
    );

    async fn run(_ctx: &Ctx, conn: &mut PgConnection, input: DeliveriesInput) -> Result<Vec<DeliveryView>> {
        let status = match input.status {
            None => "true",
            Some(DeliveryStatus::Delivered) => "f.delivered_at is not null",
            Some(DeliveryStatus::Pending) => "f.delivered_at is null and f.next_attempt_at is not null",
            Some(DeliveryStatus::Failed) => "f.delivered_at is null and f.attempts > 0",
        };
        Ok(sqlx::query_as(&format!(
            "select {DELIVERY_COLUMNS} from webhook_deliveries f
             where f.webhook_id = $1 and {status} and ($2::text is null or f.event = $2) and ($3::uuid is null or f.id < $3)
             order by f.id desc limit $4"
        ))
        .bind(input.webhook_id)
        .bind(input.event.as_deref().map(str::trim).filter(|e| !e.is_empty()))
        .bind(input.before)
        .bind(input.limit.unwrap_or(50).clamp(1, 200))
        .fetch_all(&mut *conn)
        .await?)
    }
}

#[derive(Serialize, JsonSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct AttemptView {
    pub created_at: DateTime<Utc>,
    pub status_code: Option<i32>,
    pub error: Option<String>,
    /// The first 2 KB of the endpoint's answer.
    pub response_excerpt: Option<String>,
    pub duration_ms: i32,
}

#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeliveryDetail {
    #[serde(flatten)]
    pub delivery: DeliveryView,
    /// The JSON body that was posted. The signature is made fresh on every attempt and is not kept.
    pub payload: Value,
    pub attempts_log: Vec<AttemptView>,
    /// Later deliveries that resent this one.
    pub resent_as: Vec<Uuid>,
}

pub struct GetDelivery;

#[async_trait]
impl Service for GetDelivery {
    type Input = IdInput;
    type Output = DeliveryDetail;
    const META: Meta = Meta::query(
        "webhooks.delivery",
        "One webhook delivery with its JSON payload and every attempt: time, status code, error, duration and the start of the endpoint's answer.",
        Access::Permission(Permission::WebhooksManage),
    );

    async fn run(_ctx: &Ctx, conn: &mut PgConnection, input: IdInput) -> Result<DeliveryDetail> {
        let delivery: DeliveryView = sqlx::query_as(&format!("select {DELIVERY_COLUMNS} from webhook_deliveries f where f.id = $1"))
            .bind(input.id)
            .fetch_optional(&mut *conn)
            .await?
            .ok_or_else(|| not_found("Delivery"))?;
        let payload: Value =
            sqlx::query_scalar("select payload from webhook_deliveries where id = $1").bind(input.id).fetch_one(&mut *conn).await?;
        let attempts_log = sqlx::query_as(
            "select created_at, status_code, error, response_excerpt, duration_ms from webhook_attempts where delivery_id = $1 order by created_at, id",
        )
        .bind(input.id)
        .fetch_all(&mut *conn)
        .await?;
        let resent_as = sqlx::query_scalar("select id from webhook_deliveries where resent_from = $1 order by id")
            .bind(input.id)
            .fetch_all(&mut *conn)
            .await?;
        Ok(DeliveryDetail { delivery, payload, attempts_log, resent_as })
    }
}

async fn resend(conn: &mut PgConnection, id: Uuid) -> Result<Option<Uuid>> {
    let row: Option<(Uuid, String, Value)> = sqlx::query_as("select webhook_id, event, payload from webhook_deliveries where id = $1")
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
    let Some((hook, event, payload)) = row else { return Ok(None) };
    let created_at = payload["createdAt"].as_str().and_then(|t| t.parse::<DateTime<Utc>>().ok()).unwrap_or_else(crate::context::now);
    Ok(Some(queue_delivery(conn, hook, &event, &payload["data"], created_at, Some(id)).await?))
}

#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ResendOutput {
    /// The new delivery's id.
    pub id: Uuid,
}

pub struct Resend;

#[async_trait]
impl Service for Resend {
    type Input = IdInput;
    type Output = ResendOutput;
    const META: Meta = Meta::mutation(
        "webhooks.resend",
        "Send a webhook delivery again, as a new delivery with a new id and a fresh signature. The event's data and createdAt stay the same.",
        Access::Permission(Permission::WebhooksManage),
    );

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: IdInput) -> Result<ResendOutput> {
        let id = resend(conn, input.id).await?.ok_or_else(|| not_found("Delivery"))?;
        audit(ctx, conn, Entry::new("webhooks.resend", "webhook_delivery", input.id).metadata(json!({ "resentAs": id }))).await?;
        Ok(ResendOutput { id })
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ResendFailedInput {
    pub webhook_id: Uuid,
}

#[derive(Serialize, JsonSchema)]
pub struct ResendFailedOutput {
    /// How many deliveries were queued again.
    pub count: usize,
}

pub struct ResendFailed;

#[async_trait]
impl Service for ResendFailed {
    type Input = ResendFailedInput;
    type Output = ResendFailedOutput;
    const META: Meta = Meta::mutation(
        "webhooks.resend_failed",
        "Resend every delivery of a webhook that was given up after its last retry and not resent yet, up to 500 at a time. Use it after fixing the receiving end.",
        Access::Permission(Permission::WebhooksManage),
    );

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: ResendFailedInput) -> Result<ResendFailedOutput> {
        let exists: Option<Uuid> =
            sqlx::query_scalar("select id from webhooks where id = $1").bind(input.webhook_id).fetch_optional(&mut *conn).await?;
        exists.ok_or_else(|| not_found("Webhook"))?;
        let failed: Vec<Uuid> = sqlx::query_scalar(&format!(
            "select f.id from webhook_deliveries f where f.webhook_id = $1 and {FAILED} order by f.id limit 500"
        ))
        .bind(input.webhook_id)
        .fetch_all(&mut *conn)
        .await?;
        if failed.is_empty() {
            return Ok(ResendFailedOutput { count: 0 });
        }
        for id in &failed {
            resend(conn, *id).await?;
        }
        audit(ctx, conn, Entry::new("webhooks.resend_failed", "webhook", input.webhook_id).metadata(json!({ "count": failed.len() })))
            .await?;
        Ok(ResendFailedOutput { count: failed.len() })
    }
}
