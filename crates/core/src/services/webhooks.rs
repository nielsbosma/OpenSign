use async_trait::async_trait;
use chrono::{DateTime, Utc};
use opensign_crypto::{KeyPurpose, random_token};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::PgConnection;
use uuid::Uuid;

use super::{Empty, Ok_};
use crate::audit::{Entry, audit};
use crate::context::Ctx;
use crate::documents::notify::{emit_to, webhook_secret_context};
use crate::error::{Result, bad_request, not_found};
use crate::net::check_public_url;
use crate::permissions::Permission;
use crate::service::{Access, Meta, Service};

pub const EVENTS: &[&str] =
    &["document.sent", "document.signed", "document.completed", "document.declined", "document.canceled", "document.expired"];

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
}

pub struct List;

#[async_trait]
impl Service for List {
    type Input = Empty;
    type Output = Vec<WebhookView>;
    const META: Meta = Meta::query(
        "webhooks.list",
        "List webhooks, their events and the result of the latest delivery.",
        Access::Permission(Permission::WebhooksManage),
    );

    async fn run(_ctx: &Ctx, conn: &mut PgConnection, _: Empty) -> Result<Vec<WebhookView>> {
        Ok(sqlx::query_as(
            "select w.id, w.url, w.events, w.active, w.created_at, d.status_code as last_status, d.error as last_error
             from webhooks w left join lateral (select status_code, error from webhook_deliveries where webhook_id = w.id order by created_at desc limit 1) d on true
             order by w.created_at",
        )
        .fetch_all(&mut *conn)
        .await?)
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateInput {
    /// A public https URL.
    pub url: String,
    /// Event names, or "*" for all: document.sent, document.signed, document.completed, document.declined, document.canceled, document.expired.
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
        let webhook = WebhookView { id, url, events, active: true, created_at: ctx.now, last_status: None, last_error: None };
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
