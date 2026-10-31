use async_trait::async_trait;
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::PgConnection;
use uuid::Uuid;

use super::{Empty, Ok_};
use crate::audit::{Entry, audit};
use crate::context::{Credential, Ctx};
use crate::error::{AppError, ErrorKind, Result, not_found};
use crate::oauth::{self, decode_pending, issue_code, redirect_with};
use crate::service::{Access, Meta, Service};

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RequestInput {
    /// The `request` parameter of the consent page.
    pub request: String,
}

fn session_only(ctx: &Ctx) -> Result<()> {
    if ctx.actor()?.credential != Credential::Session {
        return Err(AppError::new(ErrorKind::Forbidden, "Approve connections in the app, signed in"));
    }
    Ok(())
}

/// An MCP grant would outlive the person being blocked at the identity provider.
async fn refuse_sso(ctx: &Ctx, conn: &mut PgConnection) -> Result<()> {
    crate::auth::user_by_id(conn, ctx.actor()?.user_id).await?.ok_or_else(crate::error::unauthorized)?.refuse_sso()
}

#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConsentView {
    pub client_name: String,
    pub client_uri: Option<String>,
    /// Where the browser goes after approval. Show it, so a look-alike client name cannot hide it.
    pub redirect_host: String,
    /// The part of that host someone had to register, such as `example.co.uk`. Whoever holds it
    /// receives the access; the labels in front of it can say anything.
    pub redirect_domain: String,
}

pub struct Describe;

#[async_trait]
impl Service for Describe {
    type Input = RequestInput;
    type Output = ConsentView;
    const META: Meta =
        Meta::query("oauth.describe", "Describe a pending MCP client connection for the consent screen.", Access::Authenticated)
            .not_a_tool("OAuth consent is given by the person in the browser, never by a client.");

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: RequestInput) -> Result<ConsentView> {
        session_only(ctx)?;
        refuse_sso(ctx, conn).await?;
        let pending = decode_pending(&ctx.app, &input.request, ctx.now)?;
        let client = oauth::client(conn, &pending.client_id).await?.ok_or_else(|| not_found("Client"))?;
        let redirect_host = url::Url::parse(&pending.redirect_uri).ok().and_then(|u| u.host_str().map(str::to_string)).unwrap_or_default();
        let redirect_domain = oauth::registrable_domain(&redirect_host);
        Ok(ConsentView {
            client_name: client.name,
            client_uri: client.metadata["client_uri"].as_str().map(str::to_string),
            redirect_host,
            redirect_domain,
        })
    }
}

#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RedirectOutput {
    pub redirect_url: String,
}

pub struct Approve;

#[async_trait]
impl Service for Approve {
    type Input = RequestInput;
    type Output = RedirectOutput;
    const META: Meta = Meta::mutation(
        "oauth.approve",
        "Allow an MCP client to act as you. Returns where to send the browser next.",
        Access::Authenticated,
    )
    .not_a_tool("OAuth consent is given by the person in the browser, never by a client.");

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: RequestInput) -> Result<RedirectOutput> {
        session_only(ctx)?;
        refuse_sso(ctx, conn).await?;
        crate::auth::require_step_up(ctx, conn).await?;
        let actor = ctx.actor()?;
        let pending = decode_pending(&ctx.app, &input.request, ctx.now)?;
        let code = issue_code(conn, &pending, actor.user_id, ctx.now).await?;
        let client = oauth::client(conn, &pending.client_id).await?.ok_or_else(|| not_found("Client"))?;
        audit(
            ctx,
            conn,
            Entry::new("oauth.approved", "oauth_client", &pending.client_id)
                .metadata(json!({ "client": client.name, "redirectUri": pending.redirect_uri })),
        )
        .await?;
        Ok(RedirectOutput { redirect_url: redirect_with(&pending, &[("code", &code)])? })
    }
}

pub struct Deny;

#[async_trait]
impl Service for Deny {
    type Input = RequestInput;
    type Output = RedirectOutput;
    const META: Meta = Meta::mutation("oauth.deny", "Refuse an MCP client connection.", Access::Authenticated)
        .not_a_tool("OAuth consent is given by the person in the browser, never by a client.");

    async fn run(ctx: &Ctx, _conn: &mut PgConnection, input: RequestInput) -> Result<RedirectOutput> {
        session_only(ctx)?;
        let pending = decode_pending(&ctx.app, &input.request, ctx.now)?;
        Ok(RedirectOutput {
            redirect_url: redirect_with(&pending, &[("error", "access_denied"), ("error_description", "The user refused access")])?,
        })
    }
}

#[derive(Serialize, JsonSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Connection {
    pub id: Uuid,
    pub client_name: String,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
}

pub struct ListConnections;

#[async_trait]
impl Service for ListConnections {
    type Input = Empty;
    type Output = Vec<Connection>;
    const META: Meta = Meta::query("account.mcp_connections", "List the MCP clients connected to your account.", Access::Authenticated)
        .not_a_tool("Connected AI apps are managed in the app, so an agent cannot see or change which clients hold access.");

    async fn run(ctx: &Ctx, conn: &mut PgConnection, _: Empty) -> Result<Vec<Connection>> {
        Ok(sqlx::query_as(
            "select g.id, c.name as client_name, g.created_at, g.last_used_at from oauth_grants g join oauth_clients c on c.client_id = g.client_id
             where g.user_id = $1 and g.revoked_at is null and g.refresh_expires_at > now() order by g.created_at desc",
        )
        .bind(ctx.actor()?.user_id)
        .fetch_all(&mut *conn)
        .await?)
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct IdInput {
    pub id: Uuid,
}

pub struct RevokeConnection;

#[async_trait]
impl Service for RevokeConnection {
    type Input = IdInput;
    type Output = Ok_;
    const META: Meta =
        Meta::mutation("account.revoke_mcp_connection", "Disconnect an MCP client from your account.", Access::Authenticated)
            .not_a_tool("Connected AI apps are managed in the app, so an agent cannot see or change which clients hold access.");

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: IdInput) -> Result<Ok_> {
        let done = sqlx::query("update oauth_grants set revoked_at = $3 where id = $1 and user_id = $2 and revoked_at is null")
            .bind(input.id)
            .bind(ctx.actor()?.user_id)
            .bind(ctx.now)
            .execute(&mut *conn)
            .await?
            .rows_affected();
        if done == 0 {
            return Err(not_found("Connection"));
        }
        audit(ctx, conn, Entry::new("oauth.revoked", "oauth_grant", input.id)).await?;
        Ok(Ok_ { ok: true })
    }
}
