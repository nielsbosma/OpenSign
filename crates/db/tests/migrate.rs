#![allow(clippy::unwrap_used)]

use opensign_db::{migrate, migration_state, sqlx, test_db};

#[tokio::test]
async fn migrates_an_empty_database_and_is_idempotent() {
    let Some(db) = test_db().await else { return };
    let before = migration_state(&db).await.unwrap();
    assert!(before.is_empty_database());
    migrate(&db).await.unwrap();
    migrate(&db).await.unwrap();
    assert!(migration_state(&db).await.unwrap().is_current());
}

#[tokio::test]
async fn audit_log_is_append_only() {
    let Some(db) = test_db().await else { return };
    migrate(&db).await.unwrap();
    sqlx::query("insert into audit_log (actor_label, surface, action, entity_type, prev_hash, hash) values ('system', 'system', 'x.y', 'x', '\x00', '\x01')")
        .execute(&db)
        .await
        .unwrap();
    assert!(sqlx::query("update audit_log set action = 'z'").execute(&db).await.is_err());
    assert!(sqlx::query("delete from audit_log").execute(&db).await.is_err());
    assert!(sqlx::query("truncate audit_log").execute(&db).await.is_err());
}

#[tokio::test]
async fn refuses_a_newer_database() {
    let Some(db) = test_db().await else { return };
    migrate(&db).await.unwrap();
    sqlx::query("insert into _sqlx_migrations (version, description, success, checksum, execution_time) values (99999, 'future', true, '\x00', 0)")
        .execute(&db)
        .await
        .unwrap();
    assert!(matches!(migration_state(&db).await, Err(opensign_db::DbError::TooNew(99999))));
}
