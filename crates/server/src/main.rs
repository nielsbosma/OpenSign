use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use opensign_core::context::App;
use opensign_core::env::{Env, ProcessRole};
use opensign_core::net::SafeHttp;
use opensign_core::settings::SettingsCache;
use opensign_core::storage::Storage;
use opensign_crypto::Keys;
use opensign_server::{AppState, router};
use tracing_subscriber::EnvFilter;

async fn worker_loop(app: Arc<App>) {
    if let Ok(mut conn) = app.db.acquire().await {
        let _ = opensign_core::worker::schedule_sweep(&mut conn, None).await;
    }
    loop {
        match opensign_core::worker::run_next(&app).await {
            Ok(true) => {}
            Ok(false) => tokio::time::sleep(Duration::from_secs(2)).await,
            Err(e) => {
                tracing::warn!(error = %e, "worker could not claim a job");
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }
    }
}

async fn shutdown() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut s) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            s.recv().await;
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! { _ = ctrl_c => {}, _ = terminate => {} }
}

/// `opensign health`: the image's health check, since the runtime image has no shell or curl.
fn health_check() -> anyhow::Result<()> {
    use std::io::{Read, Write};
    let port: u16 = std::env::var("PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(3000);
    let mut stream = std::net::TcpStream::connect_timeout(&SocketAddr::from(([127, 0, 0, 1], port)), Duration::from_secs(3))?;
    stream.set_read_timeout(Some(Duration::from_secs(3)))?;
    stream.write_all(
        b"GET /api/health HTTP/1.0
Host: localhost

",
    )?;
    let mut head = [0u8; 12];
    stream.read_exact(&mut head)?;
    anyhow::ensure!(head.ends_with(b" 200"), "unhealthy: {}", String::from_utf8_lossy(&head));
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    if std::env::args().nth(1).as_deref() == Some("schema") {
        use std::io::Write;
        writeln!(std::io::stdout(), "{}", serde_json::to_string_pretty(&opensign_server::service_schemas())?)?;
        return Ok(());
    }
    if std::env::args().nth(1).as_deref() == Some("health") {
        return health_check();
    }
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,sqlx=warn")))
        .init();
    let env = match Env::from_process() {
        Ok(env) => env,
        Err(problems) => {
            for p in &problems {
                tracing::error!("{}: {} {}", p.name, p.message, p.hint);
            }
            anyhow::bail!("OpenSign cannot start until the environment is fixed");
        }
    };
    let keys = Keys::derive(&env.app_secret).context("APP_SECRET")?;
    let db = opensign_db::connect(&env.database_url, 20).await?;
    opensign_db::migrate(&db).await.context("database migrations")?;
    let storage = Storage::from_env(&env).map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let settings = SettingsCache::from_env(std::env::vars());
    let users: i64 = sqlx::query_scalar("select count(*) from users").fetch_one(&db).await?;
    if users == 0 {
        tracing::warn!("No admin yet. Open {} and enter the setup code {}", env.app_url.as_deref().unwrap_or("the app"), keys.setup_code());
    }
    let port = env.port;
    let role = env.role;
    let app = Arc::new(App { env, db, keys, storage, settings, http: SafeHttp::new() });
    if let Err(e) = app.storage.check().await {
        tracing::error!(error = %e, cause = ?e.cause, "the S3 bucket is not reachable; uploads and downloads will fail");
    }

    if role != ProcessRole::Web {
        tokio::spawn(worker_loop(app.clone()));
    }
    if role == ProcessRole::Worker {
        shutdown().await;
        return Ok(());
    }
    let listener = tokio::net::TcpListener::bind(SocketAddr::from(([0, 0, 0, 0], port))).await?;
    tracing::info!(version = opensign_core::env::app_version(), "listening on port {port}");
    axum::serve(listener, router(AppState::new(app)).into_make_service_with_connect_info::<SocketAddr>())
        .with_graceful_shutdown(shutdown())
        .await?;
    Ok(())
}
