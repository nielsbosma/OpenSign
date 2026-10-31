#![allow(clippy::unwrap_used)]

use std::sync::Arc;

use opensign_core::auth::load_session;
use opensign_core::call;
use opensign_core::context::{App, Ctx, Surface};
use opensign_core::services::auth::{self, LoginResult};
use opensign_core::services::webhooks::{self, DeliveryStatus};
use opensign_core::services::{Empty, documents, setup};
use opensign_core::settings::SecuritySettings;
use opensign_core::testing::production_app;
use opensign_core::worker;
use serde_json::json;
use uuid::Uuid;

const PASSWORD: &str = "correct horse battery staple 42";

async fn admin(app: &Arc<App>) -> Ctx {
    let anon = Ctx::new(app.clone(), Surface::Web);
    let input = setup::CompleteInput {
        setup_code: app.keys.setup_code(),
        org_name: "Acme AB".into(),
        name: "Ada Admin".into(),
        email: "ada@example.com".into(),
        password: PASSWORD.into(),
    };
    call::<setup::Complete>(&anon, input).await.unwrap();
    let result = call::<auth::Login>(&anon, auth::LoginInput { email: "ada@example.com".into(), password: PASSWORD.into() }).await.unwrap();
    let LoginResult::SignedIn { token, .. } = result else { panic!() };
    let mut conn = app.db.acquire().await.unwrap();
    let (actor, session) =
        load_session(&mut conn, &token, &SecuritySettings::default(), opensign_core::context::now()).await.unwrap().unwrap();
    let mut ctx = Ctx::new(app.clone(), Surface::Web).with_actor(actor);
    ctx.session_id = Some(session);
    ctx
}

fn deliveries(webhook_id: Uuid, status: Option<DeliveryStatus>) -> webhooks::DeliveriesInput {
    webhooks::DeliveriesInput { webhook_id, status, event: None, before: None, limit: None }
}

#[tokio::test]
async fn failed_deliveries_are_logged_and_can_be_resent() {
    let Some(app) = production_app().await else { return };
    let ada = admin(&app).await;
    let input = serde_json::from_value(json!({ "url": "https://example.com/hook", "events": ["*"] })).unwrap();
    let hook = call::<webhooks::Create>(&ada, input).await.unwrap().webhook.id;
    let mut conn = app.db.acquire().await.unwrap();
    // A refused address fails at once, without going to the network.
    sqlx::query("update webhooks set url = 'https://127.0.0.1/hook' where id = $1").bind(hook).execute(&mut *conn).await.unwrap();

    call::<documents::Create>(&ada, serde_json::from_value(json!({ "title": "Template", "isTemplate": true })).unwrap()).await.unwrap();
    let doc = call::<documents::Create>(&ada, serde_json::from_value(json!({ "title": "NDA" })).unwrap()).await.unwrap();
    worker::drain(&app).await.unwrap();

    let log = call::<webhooks::Deliveries>(&ada, deliveries(hook, None)).await.unwrap();
    assert_eq!(log.len(), 1, "templates fire no event");
    let first = &log[0];
    assert_eq!((first.event.as_str(), first.status.as_str(), first.attempts), ("document.created", "pending", 1));
    assert_eq!(first.error.as_deref(), Some("That address is private or reserved"));
    assert!(first.next_attempt_at.is_some());

    sqlx::query("update jobs set max_attempts = 2, run_at = now() where kind = 'webhook'").execute(&mut *conn).await.unwrap();
    worker::drain(&app).await.unwrap();
    let detail = call::<webhooks::GetDelivery>(&ada, webhooks::IdInput { id: first.id }).await.unwrap();
    assert_eq!(detail.delivery.status, "failed");
    assert!(detail.delivery.next_attempt_at.is_none());
    assert_eq!(detail.attempts_log.len(), 2);
    assert_eq!(detail.payload["data"]["documentId"], json!(doc.document.id));
    assert_eq!(detail.payload["data"]["title"], json!("NDA"));
    assert_eq!(call::<webhooks::List>(&ada, Empty {}).await.unwrap()[0].failed, 1);
    assert!(call::<webhooks::Deliveries>(&ada, deliveries(hook, Some(DeliveryStatus::Delivered))).await.unwrap().is_empty());

    let input = webhooks::ResendFailedInput { webhook_id: hook };
    assert_eq!(call::<webhooks::ResendFailed>(&ada, input).await.unwrap().count, 1);
    assert_eq!(call::<webhooks::List>(&ada, Empty {}).await.unwrap()[0].failed, 0, "a resent delivery no longer counts as failed");
    let input = webhooks::ResendFailedInput { webhook_id: hook };
    assert_eq!(call::<webhooks::ResendFailed>(&ada, input).await.unwrap().count, 0);
    let log = call::<webhooks::Deliveries>(&ada, deliveries(hook, None)).await.unwrap();
    assert_eq!(log.len(), 2);
    assert_eq!(log[0].resent_from, Some(first.id));
    let resent = call::<webhooks::GetDelivery>(&ada, webhooks::IdInput { id: log[0].id }).await.unwrap();
    assert_eq!(resent.payload["id"], json!(log[0].id), "a resend has its own id");
    assert_eq!(resent.payload["createdAt"], detail.payload["createdAt"], "and keeps the event's time");
    assert_eq!(resent.payload["data"], detail.payload["data"]);

    let again = call::<webhooks::Resend>(&ada, webhooks::IdInput { id: first.id }).await.unwrap();
    let detail = call::<webhooks::GetDelivery>(&ada, webhooks::IdInput { id: first.id }).await.unwrap();
    assert_eq!(detail.resent_as, vec![log[0].id, again.id]);

    sqlx::query("update webhook_deliveries set created_at = now() - interval '31 days' where id = $1")
        .bind(first.id)
        .execute(&mut *conn)
        .await
        .unwrap();
    worker::schedule_sweep(&mut conn, None).await.unwrap();
    worker::drain(&app).await.unwrap();
    let left: Vec<Uuid> = sqlx::query_scalar("select id from webhook_deliveries order by id").fetch_all(&mut *conn).await.unwrap();
    assert_eq!(left, vec![log[0].id, again.id], "the sweep removes deliveries older than 30 days");
    let actions: Vec<String> = sqlx::query_scalar("select action from audit_log where action like 'webhooks.resend%' order by id")
        .fetch_all(&mut *conn)
        .await
        .unwrap();
    assert_eq!(actions, ["webhooks.resend_failed", "webhooks.resend"]);
}
