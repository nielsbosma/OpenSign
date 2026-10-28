use std::time::Duration;

pub use sqlx;
use sqlx::migrate::{MigrateError, Migrator};
use sqlx::postgres::{PgConnectOptions, PgPool, PgPoolOptions};
use sqlx::{ConnectOptions, Executor};

pub type Db = PgPool;

pub static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("cannot connect to the database: {0}")]
    Connect(#[source] sqlx::Error),
    #[error("the database is newer than this version of OpenSign (it has migration {0}); run a newer image")]
    TooNew(i64),
    #[error(transparent)]
    Migrate(#[from] MigrateError),
    #[error(transparent)]
    Sql(#[from] sqlx::Error),
}

pub async fn connect(url: &str, max_connections: u32) -> Result<Db, DbError> {
    let options: PgConnectOptions = url.parse::<PgConnectOptions>().map_err(DbError::Connect)?.disable_statement_logging();
    PgPoolOptions::new()
        .max_connections(max_connections)
        .acquire_timeout(Duration::from_secs(10))
        .after_connect(|conn, _| {
            Box::pin(async move {
                conn.execute("set statement_timeout = '60s'; set idle_in_transaction_session_timeout = '120s'").await?;
                Ok(())
            })
        })
        .connect_with(options)
        .await
        .map_err(DbError::Connect)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationState {
    pub applied: Vec<i64>,
    pub pending: Vec<i64>,
}

impl MigrationState {
    pub fn is_empty_database(&self) -> bool {
        self.applied.is_empty()
    }

    pub fn is_current(&self) -> bool {
        self.pending.is_empty()
    }
}

/// What is applied and what this build would apply. Refuses a database migrated by a newer build.
pub async fn migration_state(db: &Db) -> Result<MigrationState, DbError> {
    let table_exists: bool = sqlx::query_scalar(
        "select exists (select 1 from information_schema.tables where table_schema = current_schema() and table_name = '_sqlx_migrations')",
    )
    .fetch_one(db)
    .await?;
    let applied: Vec<i64> = if table_exists {
        sqlx::query_scalar("select version from _sqlx_migrations where success order by version").fetch_all(db).await?
    } else {
        Vec::new()
    };
    let known: Vec<i64> = MIGRATOR.iter().filter(|m| m.migration_type.is_up_migration()).map(|m| m.version).collect();
    if let Some(unknown) = applied.iter().find(|v| !known.contains(v)) {
        return Err(DbError::TooNew(*unknown));
    }
    let pending = known.into_iter().filter(|v| !applied.contains(v)).collect();
    Ok(MigrationState { applied, pending })
}

/// Applies pending migrations, each in its own transaction, under sqlx's advisory lock so only one
/// container migrates at a time.
pub async fn migrate(db: &Db) -> Result<(), DbError> {
    migration_state(db).await?;
    MIGRATOR.run(db).await?;
    Ok(())
}

/// A fresh, empty schema per test on `TEST_DATABASE_URL`, or `None` (and the test is skipped)
/// when no test database is configured. A configured database that cannot be reached fails loudly.
#[cfg(feature = "testing")]
pub async fn test_db() -> Option<Db> {
    let url = std::env::var("TEST_DATABASE_URL").ok()?;
    let admin = connect(&url, 1).await.expect("TEST_DATABASE_URL is set but the database cannot be reached");
    let schema = format!("t_{}", std::process::id()) + &format!("_{}", NEXT_SCHEMA.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
    sqlx::raw_sql(&format!("drop schema if exists {schema} cascade; create schema {schema}"))
        .execute(&admin)
        .await
        .expect("create a test schema");
    admin.close().await;
    let options: PgConnectOptions = url
        .parse::<PgConnectOptions>()
        .expect("TEST_DATABASE_URL is a valid URL")
        .disable_statement_logging()
        .options([("search_path", schema.as_str())]);
    Some(PgPoolOptions::new().max_connections(5).connect_with(options).await.expect("connect to the test schema"))
}

#[cfg(feature = "testing")]
static NEXT_SCHEMA: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
