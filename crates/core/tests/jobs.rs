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

#[tokio::test]
async fn the_sweep_does_not_expire_a_document_waiting_for_its_seal() {
    let Some(app) = app().await else { return };
    let mut conn = app.db.acquire().await.unwrap();
    for (title, signed) in [("Signed", true), ("Unsigned", false)] {
        let id = uuid::Uuid::now_v7();
        sqlx::query("insert into documents (id, title, status, expires_at) values ($1, $2, 'pending', now() - interval '1 minute')")
            .bind(id)
            .bind(title)
            .execute(&mut *conn)
            .await
            .unwrap();
        sqlx::query("insert into parties (id, document_id, name, signed_at) values ($1, $2, 'Anna', case when $3 then now() end)")
            .bind(uuid::Uuid::now_v7())
            .bind(id)
            .bind(signed)
            .execute(&mut *conn)
            .await
            .unwrap();
    }
    worker::schedule_sweep(&mut conn, None).await.unwrap();
    worker::drain(&app).await.unwrap();
    let statuses: Vec<(String, String)> =
        sqlx::query_as("select title, status from documents order by title").fetch_all(&mut *conn).await.unwrap();
    assert_eq!(statuses, vec![("Signed".into(), "pending".into()), ("Unsigned".into(), "expired".into())]);
}
