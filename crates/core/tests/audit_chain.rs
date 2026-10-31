#![allow(clippy::unwrap_used)]

use opensign_core::audit::{Entry, WitnessStatus, audit, verify_chain, verify_trail, write_witness};
use opensign_core::testing::{app, system_ctx};
use opensign_crypto::Keys;
use serde_json::json;
use sha2::{Digest, Sha256};

#[tokio::test]
async fn chain_verifies_and_detects_edits() {
    let Some(app) = app().await else { return };
    let ctx = system_ctx(&app);
    let mut conn = app.db.acquire().await.unwrap();
    for i in 0..5 {
        audit(&ctx, &mut conn, Entry::new("test.write", "thing", i).after(json!({ "n": i, "ratio": 0.5, "password": "x" }))).await.unwrap();
    }
    let report = verify_chain(&mut conn, &app.keys).await.unwrap();
    assert!(report.valid, "{report:?}");
    assert_eq!(report.entries, 5);

    let stored: serde_json::Value =
        sqlx::query_scalar("select after from audit_log where entity_id = '2'").fetch_one(&mut *conn).await.unwrap();
    assert_eq!(stored["password"], "••••");

    sqlx::query("alter table audit_log disable trigger audit_log_append_only").execute(&mut *conn).await.unwrap();
    sqlx::query("update audit_log set after = '{\"n\": 99}' where entity_id = '2'").execute(&mut *conn).await.unwrap();
    let report = verify_chain(&mut conn, &app.keys).await.unwrap();
    assert!(!report.valid);
    let tampered: i64 = sqlx::query_scalar("select id from audit_log where entity_id = '2'").fetch_one(&mut *conn).await.unwrap();
    assert_eq!(report.broken_at, Some(tampered));
}

#[tokio::test]
async fn concurrent_appends_keep_one_chain() {
    let Some(app) = app().await else { return };
    let mut tasks = Vec::new();
    for i in 0..20 {
        let app = app.clone();
        tasks.push(tokio::spawn(async move {
            let ctx = system_ctx(&app);
            let mut tx = app.db.begin().await.unwrap();
            audit(&ctx, &mut tx, Entry::new("test.concurrent", "thing", i)).await.unwrap();
            tx.commit().await.unwrap();
        }));
    }
    for t in tasks {
        t.await.unwrap();
    }
    let mut conn = app.db.acquire().await.unwrap();
    let report = verify_chain(&mut conn, &app.keys).await.unwrap();
    assert!(report.valid && report.entries == 20, "{report:?}");
}

#[tokio::test]
async fn anchors_must_match_the_chain() {
    let Some(app) = app().await else { return };
    let ctx = system_ctx(&app);
    let mut conn = app.db.acquire().await.unwrap();
    let id = audit(&ctx, &mut conn, Entry::new("test.anchor", "thing", 1)).await.unwrap();
    sqlx::query("insert into audit_anchors (audit_id, head_hash) values ($1, $2)")
        .bind(id)
        .bind(vec![7u8; 32])
        .execute(&mut *conn)
        .await
        .unwrap();
    let report = verify_chain(&mut conn, &app.keys).await.unwrap();
    assert!(!report.valid && report.broken_anchors.len() == 1, "{report:?}");
}

async fn entries(ctx: &opensign_core::context::Ctx, conn: &mut sqlx::PgConnection, count: i32) -> i64 {
    let mut last = 0;
    for i in 0..count {
        last = audit(ctx, conn, Entry::new("test.write", "thing", i)).await.unwrap();
    }
    last
}

async fn head(conn: &mut sqlx::PgConnection, id: i64) -> Vec<u8> {
    sqlx::query_scalar("select hash from audit_log where id = $1").bind(id).fetch_one(&mut *conn).await.unwrap()
}

async fn add_anchor(conn: &mut sqlx::PgConnection, id: i64, hash: &[u8], token: Option<&[u8]>) -> i64 {
    sqlx::query_scalar("insert into audit_anchors (audit_id, head_hash, tsa_token) values ($1, $2, $3) returning id")
        .bind(id)
        .bind(hash)
        .bind(token)
        .fetch_one(&mut *conn)
        .await
        .unwrap()
}

#[tokio::test]
async fn unkeyed_entries_are_refused() {
    let Some(app) = app().await else { return };
    let ctx = system_ctx(&app);
    let mut conn = app.db.acquire().await.unwrap();
    entries(&ctx, &mut conn, 3).await;
    sqlx::query("alter table audit_log disable trigger user").execute(&mut *conn).await.unwrap();
    assert!(sqlx::query("update audit_log set hash_alg = 'sha256'").execute(&mut *conn).await.is_err(), "the constraint refuses them");
    sqlx::query("alter table audit_log drop constraint audit_log_keyed_only").execute(&mut *conn).await.unwrap();

    let rows: Vec<(i64, String)> =
        sqlx::query_as("select id, to_jsonb(audit_log)::text from audit_log order by id").fetch_all(&mut *conn).await.unwrap();
    let mut prev = vec![0u8; 32];
    for (id, row) in rows {
        let hash = Sha256::digest([prev.as_slice(), row.as_bytes()].concat()).to_vec();
        sqlx::query("update audit_log set hash_alg = 'sha256', prev_hash = $2, hash = $3 where id = $1")
            .bind(id)
            .bind(&prev)
            .bind(&hash)
            .execute(&mut *conn)
            .await
            .unwrap();
        prev = hash;
    }
    let report = verify_chain(&mut conn, &app.keys).await.unwrap();
    assert!(!report.valid && report.broken_at.is_some(), "{report:?}");
}

#[tokio::test]
async fn anchor_tokens_are_checked() {
    let Some(app) = app().await else { return };
    let ctx = system_ctx(&app);
    let mut conn = app.db.acquire().await.unwrap();
    let id = entries(&ctx, &mut conn, 2).await;
    let hash = head(&mut conn, id).await;
    let unsigned = add_anchor(&mut conn, id, &hash, None).await;
    let report = verify_chain(&mut conn, &app.keys).await.unwrap();
    assert!(!report.valid && report.unsigned_anchors == [unsigned] && report.broken_anchors.is_empty(), "{report:?}");
    let bad = add_anchor(&mut conn, id, &hash, Some(b"not a time stamp")).await;
    let report = verify_chain(&mut conn, &app.keys).await.unwrap();
    assert!(!report.valid && report.invalid_anchors == [bad], "{report:?}");
    assert!(sqlx::query("truncate audit_anchors").execute(&mut *conn).await.is_err());
}

#[tokio::test]
async fn the_bucket_copy_shows_removed_anchors_and_entries() {
    let Some(app) = app().await else { return };
    let ctx = system_ctx(&app);
    let mut conn = app.db.acquire().await.unwrap();
    let id = entries(&ctx, &mut conn, 3).await;
    let report = verify_trail(&ctx, &mut conn).await.unwrap();
    assert!(report.valid && report.witness == WitnessStatus::None, "{report:?}");

    let hash = head(&mut conn, id).await;
    let anchor = add_anchor(&mut conn, id, &hash, None).await;
    assert_eq!(verify_trail(&ctx, &mut conn).await.unwrap().witness, WitnessStatus::Missing);

    write_witness(&app.storage, &app.keys, anchor, id, &hash, b"token").await.unwrap();
    assert_eq!(verify_trail(&ctx, &mut conn).await.unwrap().witness, WitnessStatus::Ok);

    let other = Keys::derive("another-secret-another-secret-another-0").unwrap();
    write_witness(&app.storage, &other, anchor, id, &hash, b"token").await.unwrap();
    assert_eq!(verify_trail(&ctx, &mut conn).await.unwrap().witness, WitnessStatus::Mismatch);
    write_witness(&app.storage, &app.keys, anchor, id, &hash, b"token").await.unwrap();

    let newer = entries(&ctx, &mut conn, 1).await;
    let newer_hash = head(&mut conn, newer).await;
    add_anchor(&mut conn, newer, &newer_hash, None).await;
    assert_eq!(verify_trail(&ctx, &mut conn).await.unwrap().witness, WitnessStatus::Behind);

    sqlx::query("alter table audit_anchors disable trigger user").execute(&mut *conn).await.unwrap();
    sqlx::query("delete from audit_anchors").execute(&mut *conn).await.unwrap();
    let report = verify_trail(&ctx, &mut conn).await.unwrap();
    assert!(report.broken_at.is_none() && !report.valid && report.witness == WitnessStatus::Mismatch, "{report:?}");

    sqlx::query("alter table audit_log disable trigger user").execute(&mut *conn).await.unwrap();
    sqlx::query("delete from audit_log where id >= $1").bind(id).execute(&mut *conn).await.unwrap();
    let report = verify_trail(&ctx, &mut conn).await.unwrap();
    assert!(report.broken_at.is_none() && !report.valid && report.witness == WitnessStatus::Mismatch, "{report:?}");
}

#[tokio::test]
async fn stopped_anchoring_is_reported_without_failing_the_chain() {
    let Some(app) = app().await else { return };
    let ctx = system_ctx(&app);
    let mut conn = app.db.acquire().await.unwrap();
    entries(&ctx, &mut conn, 1).await;
    assert!(!verify_trail(&ctx, &mut conn).await.unwrap().anchor_stale);
    let mut later = ctx.clone();
    later.now += chrono::Duration::hours(4);
    let report = verify_trail(&later, &mut conn).await.unwrap();
    assert!(report.anchor_stale && report.valid, "{report:?}");
}
