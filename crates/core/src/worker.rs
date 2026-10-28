use std::sync::Arc;

use chrono::Duration;
use serde_json::json;
use uuid::Uuid;

use crate::audit::{Entry, audit};
use crate::context::{App, Ctx, Surface};
use crate::documents::{notify, seal};
use crate::error::{AppError, Result};
use crate::jobs::{self, Job, enqueue};

const LEASE_SECONDS: i64 = 300;
const SWEEP_MINUTES: i64 = 10;

fn id(job: &Job, key: &str) -> Result<Uuid> {
    job.payload[key]
        .as_str()
        .and_then(|v| Uuid::try_parse(v).ok())
        .ok_or_else(|| AppError::internal(anyhow::anyhow!("job {} has no {key}", job.id)))
}

async fn run(ctx: &Ctx, job: &Job) -> Result<()> {
    let mut tx = ctx.db().begin().await?;
    match job.kind.as_str() {
        "notify_party" => notify::notify_party(ctx, &mut tx, id(job, "party")?, job.payload["reminder"].as_bool().unwrap_or(false)).await?,
        "seal_document" => seal::seal_document(ctx, &mut tx, id(job, "document")?, job.attempts).await?,
        "notify_completed" => notify::notify_completed(ctx, &mut tx, id(job, "document")?).await?,
        "notify_declined" => notify::notify_declined(ctx, &mut tx, id(job, "document")?, id(job, "party")?).await?,
        "notify_canceled" => {
            let document = sqlx::query_as(&format!("select {} from documents d where id = $1", crate::documents::DOCUMENT_COLUMNS))
                .bind(id(job, "document")?)
                .fetch_optional(&mut *tx)
                .await?;
            if let Some(document) = document {
                notify::notify_stopped(ctx, &mut tx, &document, None, crate::i18n::Stopped::Canceled).await?;
            }
        }
        "webhook" => {
            drop(tx);
            let mut conn = ctx.db().acquire().await?;
            return notify::deliver_webhook(ctx, &mut conn, id(job, "delivery")?).await;
        }
        "sweep" => sweep(ctx, &mut tx).await?,
        "anchor_audit" => {
            drop(tx);
            let mut conn = ctx.db().acquire().await?;
            return anchor_audit(ctx, &mut conn).await;
        }
        other => return Err(AppError::internal(anyhow::anyhow!("unknown job kind {other}"))),
    }
    tx.commit().await?;
    Ok(())
}

/// Expires documents past their deadline, queues reminders and the audit anchor. `run_next`
/// schedules the next sweep once this one is done, since a pending job holds its unique key.
async fn sweep(ctx: &Ctx, conn: &mut sqlx::PgConnection) -> Result<()> {
    let expired: Vec<Uuid> = sqlx::query_scalar(
        "update documents d set status = 'expired', updated_at = now() where status = 'pending' and expires_at is not null and expires_at < $1
           and exists (select 1 from parties p where p.document_id = d.id and p.role <> 'viewer' and p.signed_at is null)
         returning id",
    )
    .bind(ctx.now)
    .fetch_all(&mut *conn)
    .await?;
    for document in expired {
        audit(ctx, conn, Entry::new("document.expired", "document", document)).await?;
        notify::emit(conn, "document.expired", json!({ "documentId": document })).await?;
    }
    let due: Vec<Uuid> = sqlx::query_scalar(
        "select p.id from parties p join documents d on d.id = p.document_id
         where d.status = 'pending' and d.reminder_days > 0 and p.role <> 'viewer' and p.signed_at is null and p.declined_at is null
           and p.notified_at is not null and p.notified_at < $1 - make_interval(days => d.reminder_days)",
    )
    .bind(ctx.now)
    .fetch_all(&mut *conn)
    .await?;
    for party in due {
        enqueue(conn, "notify_party", json!({ "party": party, "reminder": true }), None, Some(&format!("notify:{party}"))).await?;
    }
    sqlx::query("delete from idempotency_keys where created_at < $1 - interval '1 day'").bind(ctx.now).execute(&mut *conn).await?;
    enqueue(conn, "anchor_audit", json!({}), None, Some("anchor_audit")).await
}

/// Time-stamps the head of the audit chain with the time stamp authority, at most once an hour, so
/// the trail up to that point provably existed then and cannot be quietly rebuilt later.
async fn anchor_audit(ctx: &Ctx, conn: &mut sqlx::PgConnection) -> Result<()> {
    let head: Option<(i64, Vec<u8>)> =
        sqlx::query_as("select id, hash from audit_log order by id desc limit 1").fetch_optional(&mut *conn).await?;
    let Some((head_id, hash)) = head else { return Ok(()) };
    let (anchored, last): (Option<i64>, Option<chrono::DateTime<chrono::Utc>>) =
        sqlx::query_as("select max(audit_id), max(created_at) from audit_anchors").fetch_one(&mut *conn).await?;
    if anchored.is_some_and(|a| a >= head_id) || last.is_some_and(|at| at > ctx.now - Duration::hours(1)) {
        return Ok(());
    }
    let signing: crate::settings::SigningSettings = crate::services::settings(ctx, conn).await?;
    let url = signing.tsa_url.trim();
    if url.is_empty() {
        return Ok(());
    }
    let token = seal::timestamp(ctx, url, &hash).await?;
    sqlx::query("insert into audit_anchors (audit_id, head_hash, tsa_token, tsa_url) values ($1, $2, $3, $4)")
        .bind(head_id)
        .bind(&hash)
        .bind(token)
        .bind(url)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

pub async fn schedule_sweep(conn: &mut sqlx::PgConnection, at: Option<chrono::DateTime<chrono::Utc>>) -> Result<()> {
    enqueue(conn, "sweep", json!({}), at, Some("sweep")).await
}

/// Claims and runs one due job. Returns whether there was one.
pub async fn run_next(app: &Arc<App>) -> Result<bool> {
    let job = {
        let mut conn = app.db.acquire().await?;
        jobs::claim(&mut conn, LEASE_SECONDS).await?
    };
    let Some(job) = job else { return Ok(false) };
    let ctx = Ctx::new(app.clone(), Surface::Worker);
    let result = run(&ctx, &job).await;
    let mut conn = app.db.acquire().await?;
    match result {
        Ok(()) => jobs::complete(&mut conn, job.id).await?,
        Err(e) => {
            tracing::warn!(job = %job.id, kind = %job.kind, attempt = job.attempts, error = %e, cause = ?e.cause, "job failed");
            if job.kind == "sweep" {
                jobs::complete(&mut conn, job.id).await?;
            } else {
                jobs::fail(&mut conn, &job, &e.to_string()).await?;
            }
        }
    }
    if job.kind == "sweep" {
        schedule_sweep(&mut conn, Some(crate::context::now() + Duration::minutes(SWEEP_MINUTES))).await?;
    }
    Ok(true)
}

/// Runs every due job, for tests and one-shot tools.
pub async fn drain(app: &Arc<App>) -> Result<usize> {
    let mut count = 0;
    while run_next(app).await? {
        count += 1;
        if count > 1000 {
            break;
        }
    }
    Ok(count)
}
