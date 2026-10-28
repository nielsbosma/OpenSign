//! What is running and whether a newer release exists: the version, the latest GitHub release,
//! the database migrations and the job queue. With a deploy hook set, an admin can update from the
//! System page.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::PgConnection;

use super::{Ok_, settings};
use crate::audit::{Entry, audit};
use crate::context::Ctx;
use crate::env::app_version;
use crate::error::{AppError, ErrorKind, Result, bad_request};
use crate::permissions::Permission;
use crate::service::{Access, Meta, Service};
use crate::settings::UpdateSettings;

pub const REPOSITORY: &str = "nielsbosma/OpenSign";
const CACHE_FOR: Duration = Duration::from_secs(6 * 3600);
const MAX_NOTES: usize = 4000;

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Release {
    pub version: String,
    pub url: String,
    pub published_at: Option<DateTime<Utc>>,
    /// The release notes as written on GitHub (Markdown), shortened.
    pub notes: String,
}

static LATEST: Mutex<Option<(Instant, Option<Release>)>> = Mutex::new(None);

/// `1.2.3` from `v1.2.3` or `1.2.3-dev`; `None` for anything else.
pub fn parse_version(text: &str) -> Option<(u64, u64, u64)> {
    let core = text.trim().trim_start_matches('v').split(['-', '+']).next()?;
    let mut parts = core.split('.').map(|p| p.parse::<u64>().ok());
    let version = (parts.next()??, parts.next()??, parts.next().unwrap_or(Some(0))?);
    parts.next().is_none().then_some(version)
}

pub fn is_newer(latest: &str, current: &str) -> bool {
    match (parse_version(latest), parse_version(current)) {
        (Some(latest), Some(current)) => latest > current,
        _ => false,
    }
}

async fn fetch_latest(ctx: &Ctx) -> Result<Option<Release>> {
    let response = ctx
        .app
        .http
        .public
        .get(format!("https://api.github.com/repos/{REPOSITORY}/releases/latest"))
        .header("accept", "application/vnd.github+json")
        .header("user-agent", format!("OpenSign/{}", app_version()))
        .send()
        .await
        .map_err(|e| AppError::new(ErrorKind::Unavailable, "GitHub is not reachable").with_cause(e))?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if !response.status().is_success() {
        return Err(AppError::new(ErrorKind::Unavailable, format!("GitHub answered {}", response.status())));
    }
    if response.content_length().is_some_and(|l| l > 1_000_000) {
        return Err(AppError::new(ErrorKind::Unavailable, "The release information is too large"));
    }
    let body: Value = response.json().await.map_err(AppError::internal)?;
    let tag = body["tag_name"].as_str().unwrap_or_default();
    if parse_version(tag).is_none() {
        return Ok(None);
    }
    let url = body["html_url"].as_str().filter(|u| u.starts_with("https://github.com/")).unwrap_or_default();
    Ok(Some(Release {
        version: tag.trim_start_matches('v').to_string(),
        url: url.to_string(),
        published_at: body["published_at"].as_str().and_then(|t| t.parse().ok()),
        notes: body["body"].as_str().unwrap_or_default().chars().take(MAX_NOTES).collect(),
    }))
}

/// The latest release, from a six-hour cache unless `refresh`.
pub async fn latest_release(ctx: &Ctx, refresh: bool) -> Result<Option<Release>> {
    if !refresh
        && let Some((at, release)) = LATEST.lock().map_err(|_| AppError::internal(anyhow::anyhow!("poisoned")))?.as_ref()
        && at.elapsed() < CACHE_FOR
    {
        return Ok(release.clone());
    }
    let release = fetch_latest(ctx).await?;
    *LATEST.lock().map_err(|_| AppError::internal(anyhow::anyhow!("poisoned")))? = Some((Instant::now(), release.clone()));
    Ok(release)
}

#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct VersionView {
    pub current: String,
    /// A build from source rather than a release image.
    pub development: bool,
    pub latest: Option<Release>,
    pub update_available: bool,
    /// Why the latest release is unknown: checks turned off, or GitHub unreachable.
    pub check_problem: Option<String>,
    /// An admin can update from the app: a deploy hook is set.
    pub can_update: bool,
}

async fn version_view(ctx: &Ctx, conn: &mut PgConnection, refresh: bool) -> Result<VersionView> {
    let updates: UpdateSettings = settings(ctx, conn).await?;
    let current = app_version().to_string();
    let (latest, check_problem) = if !updates.check_for_updates {
        (None, Some("Update checks are turned off in Settings.".to_string()))
    } else {
        match latest_release(ctx, refresh).await {
            Ok(latest) => (latest, None),
            Err(e) => (None, Some(e.message.clone())),
        }
    };
    let update_available = latest.as_ref().is_some_and(|l| is_newer(&l.version, &current));
    Ok(VersionView {
        development: current.contains("-dev"),
        current,
        latest,
        update_available,
        check_problem,
        can_update: updates.deploy_hook_url.as_deref().is_some_and(|u| !u.is_empty()),
    })
}

#[derive(Deserialize, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct VersionInput {
    /// Ask GitHub now instead of using the result from the last six hours.
    #[serde(default)]
    pub refresh: bool,
}

pub struct GetVersion;

#[async_trait]
impl Service for GetVersion {
    type Input = VersionInput;
    type Output = VersionView;
    const META: Meta = Meta::query(
        "system.version",
        "The running OpenSign version, the latest release on GitHub, and whether an update is available.",
        Access::Permission(Permission::SystemRead),
    )
    .without_transaction();

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: VersionInput) -> Result<VersionView> {
        version_view(ctx, conn, input.refresh).await
    }
}

#[derive(Serialize, JsonSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct MigrationView {
    pub version: i64,
    pub description: String,
    pub installed_on: DateTime<Utc>,
    pub success: bool,
}

#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct JobSummary {
    pub kind: String,
    pub waiting: i64,
    pub failing: i64,
    pub last_error: Option<String>,
}

#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SystemStatus {
    pub version: String,
    pub database: String,
    /// Applied migrations, newest first. Pending ones run when the app starts.
    pub migrations: Vec<MigrationView>,
    pub pending_migrations: usize,
    pub jobs: Vec<JobSummary>,
}

pub struct Status;

#[async_trait]
impl Service for Status {
    type Input = super::Empty;
    type Output = SystemStatus;
    const META: Meta = Meta::query(
        "system.status",
        "The database version, which migrations are applied, and the background job queue with recent failures.",
        Access::Permission(Permission::SystemRead),
    );

    async fn run(ctx: &Ctx, conn: &mut PgConnection, _input: super::Empty) -> Result<SystemStatus> {
        let database: String = sqlx::query_scalar("select current_setting('server_version')").fetch_one(&mut *conn).await?;
        let migrations: Vec<MigrationView> =
            sqlx::query_as("select version, description, installed_on, success from _sqlx_migrations order by version desc")
                .fetch_all(&mut *conn)
                .await?;
        let state = opensign_db::migration_state(&ctx.app.db).await.map_err(AppError::internal)?;
        let jobs = sqlx::query_as::<_, (String, i64, i64, Option<String>)>(
            "select kind, count(*) filter (where done_at is null), count(*) filter (where done_at is null and attempts > 0),
                    (array_agg(last_error order by created_at desc) filter (where last_error is not null and done_at is null))[1]
             from jobs where done_at is null or done_at > now() - interval '1 day' group by kind order by kind",
        )
        .fetch_all(&mut *conn)
        .await?
        .into_iter()
        .map(|(kind, waiting, failing, last_error)| JobSummary { kind, waiting, failing, last_error })
        .collect();
        Ok(SystemStatus { version: app_version().into(), database, migrations, pending_migrations: state.pending.len(), jobs })
    }
}

pub struct Update;

#[async_trait]
impl Service for Update {
    type Input = super::Empty;
    type Output = Ok_;
    const META: Meta = Meta::mutation(
        "system.update",
        "Redeploy OpenSign through the deploy hook in Settings, which pulls the newest image for the service's tag. Migrations run when the new version starts.",
        Access::Permission(Permission::SettingsManage),
    )
    .not_a_tool()
    .without_transaction();

    async fn run(ctx: &Ctx, conn: &mut PgConnection, _input: super::Empty) -> Result<Ok_> {
        let updates: UpdateSettings = settings(ctx, conn).await?;
        let hook = updates.deploy_hook_url.filter(|u| !u.is_empty()).ok_or_else(|| bad_request("Set a deploy hook in Settings first"))?;
        let url = crate::net::check_public_url(&hook).map_err(bad_request)?;
        let target = latest_release(ctx, false).await.ok().flatten().map(|r| r.version);
        let response = ctx
            .app
            .http
            .public
            .get(url)
            .send()
            .await
            .map_err(|e| AppError::new(ErrorKind::Unavailable, "The deploy hook is not reachable").with_cause(e))?;
        if !response.status().is_success() {
            return Err(AppError::new(ErrorKind::Unavailable, format!("The deploy hook answered {}", response.status())));
        }
        audit(ctx, conn, Entry::new("system.update_started", "system", "deploy").metadata(json!({ "from": app_version(), "to": target })))
            .await?;
        Ok(Ok_::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_versions() {
        assert_eq!(parse_version("v1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_version("0.1.0-dev"), Some((0, 1, 0)));
        assert_eq!(parse_version("1.2"), Some((1, 2, 0)));
        assert_eq!(parse_version("latest"), None);
        assert!(is_newer("v0.10.0", "0.9.9"));
        assert!(is_newer("0.2.0", "0.1.0-dev"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("nonsense", "0.1.0"));
    }
}
