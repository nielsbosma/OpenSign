#![allow(clippy::unwrap_used)]

use opensign_core::jobs::enqueue;
use opensign_core::testing::app;
use opensign_core::worker;
use serde_json::json;

#[tokio::test]
async fn the_sweep_schedules_itself_again() {
    let Some(app) = app().await else { return };
    let mut conn = app.db.acquire().await.unwrap();
    worker::schedule_sweep(&mut conn, None).await.unwrap();
    worker::drain(&app).await.unwrap();
    let next: Vec<bool> =
        sqlx::query_scalar("select run_at > now() from jobs where kind = 'sweep' and done_at is null").fetch_all(&mut *conn).await.unwrap();
    assert_eq!(next, vec![true], "one future sweep is pending");
    let anchors: i64 = sqlx::query_scalar("select count(*) from jobs where kind = 'anchor_audit' and done_at is not null")
        .fetch_one(&mut *conn)
        .await
        .unwrap();
    assert_eq!(anchors, 1);
}

#[tokio::test]
async fn an_exhausted_job_does_not_block_its_key() {
    let Some(app) = app().await else { return };
    let mut conn = app.db.acquire().await.unwrap();
    enqueue(&mut conn, "notify_party", json!({ "n": 1 }), None, Some("k")).await.unwrap();
    enqueue(&mut conn, "notify_party", json!({ "n": 2 }), None, Some("k")).await.unwrap();
    let payload: serde_json::Value =
        sqlx::query_scalar("select payload from jobs where unique_key = 'k'").fetch_one(&mut *conn).await.unwrap();
    assert_eq!(payload, json!({ "n": 1 }), "a pending job keeps its key");

    sqlx::query("update jobs set attempts = max_attempts, last_error = 'boom' where unique_key = 'k'").execute(&mut *conn).await.unwrap();
    enqueue(&mut conn, "notify_party", json!({ "n": 3 }), None, Some("k")).await.unwrap();
    let (payload, attempts): (serde_json::Value, i32) =
        sqlx::query_as("select payload, attempts from jobs where unique_key = 'k'").fetch_one(&mut *conn).await.unwrap();
    assert_eq!((payload, attempts), (json!({ "n": 3 }), 0));
}
