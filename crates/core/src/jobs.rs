use chrono::{DateTime, Duration, Utc};
use serde::Serialize;
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::error::Result;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Job {
    pub id: Uuid,
    pub kind: String,
    pub payload: Value,
    pub attempts: i32,
    pub max_attempts: i32,
}

/// Queues a job in the caller's transaction, so it only exists if the change that caused it commits.
/// A `unique_key` keeps at most one pending job per key; a job under that key that ran out of
/// attempts is started over instead of blocking the key forever.
pub async fn enqueue(
    conn: &mut PgConnection,
    kind: &str,
    payload: impl Serialize,
    run_at: Option<DateTime<Utc>>,
    unique_key: Option<&str>,
) -> Result<()> {
    sqlx::query(
        "insert into jobs (id, kind, payload, run_at, unique_key) values ($1, $2, $3, coalesce($4, now()), $5)
         on conflict (unique_key) where done_at is null and unique_key is not null do update
           set kind = excluded.kind, payload = excluded.payload, run_at = excluded.run_at, attempts = 0, locked_until = null, last_error = null
           where jobs.attempts >= jobs.max_attempts",
    )
    .bind(Uuid::now_v7())
    .bind(kind)
    .bind(serde_json::to_value(payload).unwrap_or_default())
    .bind(run_at)
    .bind(unique_key)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Claims one due job for `lease` seconds. Other workers skip it while it is locked.
pub async fn claim(conn: &mut PgConnection, lease_seconds: i64) -> Result<Option<Job>> {
    let job = sqlx::query_as::<_, Job>(
        "update jobs set attempts = attempts + 1, locked_until = now() + make_interval(secs => $1)
         where id = (select id from jobs where done_at is null and run_at <= now() and (locked_until is null or locked_until < now())
                     and attempts < max_attempts order by run_at limit 1 for update skip locked)
         returning id, kind, payload, attempts, max_attempts",
    )
    .bind(lease_seconds as f64)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(job)
}

pub async fn complete(conn: &mut PgConnection, id: Uuid) -> Result<()> {
    sqlx::query("update jobs set done_at = now(), locked_until = null, last_error = null where id = $1")
        .bind(id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// Exponential back-off after the given attempt: 30 s, 1 min, 2 min … capped at 6 h.
pub fn retry_delay(attempts: i32) -> Duration {
    Duration::seconds((30i64 << (attempts.clamp(1, 10) - 1)).min(6 * 3600))
}

/// Schedules the next try after `retry_delay`. After the last attempt it stays failed.
pub async fn fail(conn: &mut PgConnection, job: &Job, error: &str) -> Result<()> {
    let delay = retry_delay(job.attempts);
    sqlx::query("update jobs set locked_until = null, last_error = $2, run_at = now() + make_interval(secs => $3) where id = $1")
        .bind(job.id)
        .bind(error.chars().take(2000).collect::<String>())
        .bind(delay.num_seconds() as f64)
        .execute(&mut *conn)
        .await?;
    Ok(())
}
