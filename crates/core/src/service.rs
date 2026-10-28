use async_trait::async_trait;
use schemars::{JsonSchema, Schema};
use serde::Serialize;
use serde::de::DeserializeOwned;
use sqlx::PgConnection;

use crate::context::Ctx;
use crate::error::{AppError, ErrorKind, Result, bad_request, forbidden, unauthorized};
use crate::permissions::Permission;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Query,
    Mutation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    Public,
    Authenticated,
    Permission(Permission),
}

#[derive(Debug, Clone, Copy)]
pub struct Meta {
    /// Dotted name, e.g. `users.update`. The MCP and AI tool name is this with dots as underscores.
    pub name: &'static str,
    /// Written for a model: what it does and when to use it. Becomes the MCP and AI tool description.
    pub description: &'static str,
    pub kind: Kind,
    pub access: Access,
    /// Offered as an MCP and AI tool. Off for credentials, the caller's own session, and public calls.
    pub tool: bool,
    /// Mutations run in one transaction. Off when a failure must still persist writes (failed-login counters).
    pub transaction: bool,
    /// Callable while the user still has to set up enforced two-factor.
    pub allow_without_two_factor: bool,
}

impl Meta {
    pub const fn query(name: &'static str, description: &'static str, access: Access) -> Meta {
        Meta { name, description, kind: Kind::Query, access, tool: !matches!(access, Access::Public), transaction: false, allow_without_two_factor: false }
    }

    pub const fn mutation(name: &'static str, description: &'static str, access: Access) -> Meta {
        Meta { name, description, kind: Kind::Mutation, access, tool: !matches!(access, Access::Public), transaction: true, allow_without_two_factor: false }
    }

    pub const fn not_a_tool(mut self) -> Meta {
        self.tool = false;
        self
    }

    pub const fn without_transaction(mut self) -> Meta {
        self.transaction = false;
        self
    }

    pub const fn before_two_factor(mut self) -> Meta {
        self.allow_without_two_factor = true;
        self
    }

    pub fn tool_name(&self) -> String {
        self.name.replace('.', "_")
    }
}

/// One unit of business logic. The web app, REST API, compat APIs, MCP, the AI assistant and the
/// worker all call services, so validation, permission checks, transactions and audit happen once.
#[async_trait]
pub trait Service: Send + Sync + 'static {
    type Input: DeserializeOwned + JsonSchema + Send + 'static;
    type Output: Serialize + Send + 'static;
    const META: Meta;
    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: Self::Input) -> Result<Self::Output>;
}

pub fn can_access(access: Access, ctx: &Ctx) -> bool {
    match access {
        Access::Public => true,
        Access::Authenticated => ctx.actor.is_some(),
        Access::Permission(p) => ctx.can(p),
    }
}

pub fn authorize(meta: &Meta, ctx: &Ctx) -> Result<()> {
    if meta.access == Access::Public {
        return Ok(());
    }
    let actor = ctx.actor.as_ref().ok_or_else(unauthorized)?;
    if actor.must_enable_two_factor && !meta.allow_without_two_factor {
        return Err(AppError::new(ErrorKind::Forbidden, "Set up two-factor authentication first. Your administrator requires it."));
    }
    if !can_access(meta.access, ctx) {
        return Err(forbidden());
    }
    Ok(())
}

/// Runs a service with typed input: authorization, then a transaction for mutations.
pub async fn call<S: Service>(ctx: &Ctx, input: S::Input) -> Result<S::Output> {
    authorize(&S::META, ctx)?;
    if S::META.kind == Kind::Mutation && S::META.transaction {
        let mut tx = ctx.db().begin().await?;
        let output = S::run(ctx, &mut tx, input).await?;
        tx.commit().await?;
        Ok(output)
    } else {
        let mut conn = ctx.db().acquire().await?;
        S::run(ctx, &mut conn, input).await
    }
}

/// Type-erased service, for the registry that MCP, the AI assistant and the RPC endpoint use.
#[async_trait]
pub trait DynService: Send + Sync {
    fn meta(&self) -> &Meta;
    fn input_schema(&self) -> Schema;
    async fn invoke(&self, ctx: &Ctx, input: serde_json::Value) -> Result<serde_json::Value>;
}

struct Erased<S>(std::marker::PhantomData<S>);

#[async_trait]
impl<S: Service> DynService for Erased<S> {
    fn meta(&self) -> &Meta {
        &S::META
    }

    fn input_schema(&self) -> Schema {
        schemars::schema_for!(S::Input)
    }

    async fn invoke(&self, ctx: &Ctx, input: serde_json::Value) -> Result<serde_json::Value> {
        let input = if input.is_null() { serde_json::json!({}) } else { input };
        let input: S::Input = serde_json::from_value(input).map_err(|e| bad_request(format!("Invalid input: {e}")))?;
        let output = call::<S>(ctx, input).await?;
        serde_json::to_value(output).map_err(AppError::internal)
    }
}

pub fn erase<S: Service>() -> Box<dyn DynService> {
    Box::new(Erased::<S>(std::marker::PhantomData))
}
