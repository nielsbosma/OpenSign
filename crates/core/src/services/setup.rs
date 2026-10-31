use async_trait::async_trait;
use opensign_crypto::constant_time_eq;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sqlx::PgConnection;
use uuid::Uuid;

use super::{Empty, settings};
use crate::audit::{Entry, audit};
use crate::auth::normalize_email;
use crate::context::Ctx;
use crate::error::{AppError, ErrorKind, Result, bad_request};
use crate::permissions::Role;
use crate::service::{Access, Meta, Service};
use crate::settings::{EmailSettings, OrgSettings};

const SETUP_LOCK: i64 = 0x0073_6574_7570;

#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SetupStatus {
    pub needs_setup: bool,
    pub email_configured: bool,
}

pub struct Status;

#[async_trait]
impl Service for Status {
    type Input = Empty;
    type Output = SetupStatus;
    const META: Meta = Meta::query("setup.status", "Whether the first admin still has to be created.", Access::Public);

    async fn run(ctx: &Ctx, conn: &mut PgConnection, _: Empty) -> Result<SetupStatus> {
        let users: i64 = sqlx::query_scalar("select count(*) from users").fetch_one(&mut *conn).await?;
        let email: EmailSettings = settings(ctx, conn).await?;
        Ok(SetupStatus { needs_setup: users == 0, email_configured: email.verified_at.is_some() })
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CompleteInput {
    /// The one-time code from the container log, or `SETUP_CODE` when that is set.
    pub setup_code: String,
    pub org_name: String,
    pub name: String,
    pub email: String,
    pub password: String,
}

#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Created {
    pub user_id: Uuid,
}

pub struct Complete;

#[async_trait]
impl Service for Complete {
    type Input = CompleteInput;
    type Output = Created;
    const META: Meta =
        Meta::mutation("setup.create_first_admin", "Create the first admin on a fresh install. Locks for good afterwards.", Access::Public);

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: CompleteInput) -> Result<Created> {
        let valid = match &ctx.app.env.setup_code {
            Some(code) => constant_time_eq(input.setup_code.trim().as_bytes(), code.as_bytes()),
            None => ctx.keys().check_setup_code(&input.setup_code),
        };
        if !valid {
            return Err(AppError::new(ErrorKind::Forbidden, "That setup code is not right. Find it in the container log."));
        }
        sqlx::query("select pg_advisory_xact_lock($1)").bind(SETUP_LOCK).execute(&mut *conn).await?;
        let users: i64 = sqlx::query_scalar("select count(*) from users").fetch_one(&mut *conn).await?;
        if users > 0 {
            return Err(AppError::new(ErrorKind::Forbidden, "Setup is already complete. Sign in instead."));
        }
        let name = input.name.trim();
        let org_name = input.org_name.trim();
        if name.is_empty() || org_name.is_empty() || name.len() > 200 || org_name.len() > 200 {
            return Err(bad_request("Enter your name and your organisation's name"));
        }
        let email = normalize_email(&input.email)?;
        let hash = crate::auth::new_password_hash(&input.password).await?;
        let id = Uuid::now_v7();
        sqlx::query("insert into users (id, email, name, password_hash, role) values ($1, $2, $3, $4, $5)")
            .bind(id)
            .bind(&email)
            .bind(name)
            .bind(hash)
            .bind(Role::Admin.as_str())
            .execute(&mut *conn)
            .await?;
        let mut org: OrgSettings = settings(ctx, conn).await?;
        org.name = org_name.to_string();
        ctx.app.settings.save(conn, ctx.keys(), &org, Some(id)).await?;
        audit(ctx, conn, Entry::new("setup.first_admin_created", "user", id).after(serde_json::json!({ "email": email, "org": org_name })))
            .await?;
        Ok(Created { user_id: id })
    }
}
