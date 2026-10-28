#![allow(clippy::unwrap_used)]

use opensign_core::audit::{Entry, audit, verify_chain};
use opensign_core::testing::{app, system_ctx};
use serde_json::json;

#[tokio::test]
async fn chain_verifies_and_detects_edits() {
    let Some(app) = app().await else { return };
    let ctx = system_ctx(&app);
    let mut conn = app.db.acquire().await.unwrap();
    for i in 0..5 {
        audit(&ctx, &mut conn, Entry::new("test.write", "thing", i).after(json!({ "n": i, "ratio": 0.5, "password": "x" }))).await.unwrap();
    }
    let report = verify_chain(&mut conn).await.unwrap();
    assert!(report.valid, "{report:?}");
    assert_eq!(report.entries, 5);

    let stored: serde_json::Value =
        sqlx::query_scalar("select after from audit_log where entity_id = '2'").fetch_one(&mut *conn).await.unwrap();
    assert_eq!(stored["password"], "••••");

    sqlx::query("alter table audit_log disable trigger audit_log_append_only").execute(&mut *conn).await.unwrap();
    sqlx::query("update audit_log set after = '{\"n\": 99}' where entity_id = '2'").execute(&mut *conn).await.unwrap();
    let report = verify_chain(&mut conn).await.unwrap();
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
    let report = verify_chain(&mut conn).await.unwrap();
    assert!(report.valid && report.entries == 20, "{report:?}");
}
