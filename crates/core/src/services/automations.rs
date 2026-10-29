use std::collections::BTreeSet;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::PgConnection;
use uuid::Uuid;

use super::webhooks::{DeliveryDetail, DeliveryStatus, DeliveryView, FAILED, Log, LogFilter, ResendOutput};
use super::{Empty, Ok_, settings};
use crate::audit::{Entry, audit};
use crate::automations::{PLACEHOLDERS, known, used};
use crate::composio::{self, Composio};
use crate::context::Ctx;
use crate::documents::notify::{Target, queue_delivery};
use crate::error::{AppError, ErrorKind, Result, bad_request, not_found};
use crate::permissions::Permission;
use crate::service::{Access, Meta, Service};
use crate::settings::AutomationSettings;

const ACCESS: Access = Access::Permission(Permission::AutomationsManage);
const MAX_ARGUMENTS_BYTES: usize = 20_000;
const MAX_FILTER_IDS: usize = 50;

/// The events a rule can run on.
pub const EVENTS: &[&str] = &[
    "document.created",
    "document.sent",
    "document.viewed",
    "document.signed",
    "document.completed",
    "document.declined",
    "document.canceled",
    "document.expired",
];

async fn client(ctx: &Ctx, conn: &mut PgConnection) -> Result<Composio> {
    let automations: AutomationSettings = settings(ctx, conn).await?;
    let key = automations.key().ok_or_else(|| {
        AppError::new(
            ErrorKind::Unavailable,
            "Automations are off. An admin turns them on and adds a Composio API key in Settings, Automations.",
        )
    })?;
    Ok(Composio::new(&ctx.app, key))
}

#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StatusView {
    pub enabled: bool,
    /// A Composio API key is saved. It is never shown.
    pub key_set: bool,
    /// The placeholders a rule's arguments can use, besides `fields.<label>` and `parties.<n>.<name|email|phone|company>`.
    pub placeholders: Vec<PlaceholderView>,
    pub events: Vec<String>,
}

#[derive(Serialize, JsonSchema)]
pub struct PlaceholderView {
    pub name: String,
    pub description: String,
}

pub struct Status;

#[async_trait]
impl Service for Status {
    type Input = Empty;
    type Output = StatusView;
    const META: Meta = Meta::query(
        "automations.status",
        "Whether Composio automations are turned on and have an API key, with the events and the placeholders a rule's arguments can use.",
        ACCESS,
    );

    async fn run(ctx: &Ctx, conn: &mut PgConnection, _: Empty) -> Result<StatusView> {
        let automations: AutomationSettings = settings(ctx, conn).await?;
        Ok(StatusView {
            enabled: automations.enabled,
            key_set: automations.composio_api_key.as_deref().is_some_and(|k| !k.trim().is_empty()),
            placeholders: PLACEHOLDERS.iter().map(|(n, d)| PlaceholderView { name: n.to_string(), description: d.to_string() }).collect(),
            events: EVENTS.iter().map(|e| e.to_string()).collect(),
        })
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AppsInput {
    /// Part of an app's name, such as "drive" or "hubspot".
    pub search: Option<String>,
    /// At most 100; 30 when left out.
    pub limit: Option<u32>,
}

#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AppView {
    /// Composio's toolkit slug, such as googledrive.
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub logo: Option<String>,
    pub categories: Vec<String>,
    /// Works without connecting an account.
    pub no_auth: bool,
    pub tools_count: Option<u64>,
}

fn app_view(t: &Value) -> Option<AppView> {
    Some(AppView {
        slug: t["slug"].as_str()?.to_string(),
        name: t["name"].as_str().unwrap_or_default().to_string(),
        description: t["meta"]["description"].as_str().map(str::to_string),
        logo: t["meta"]["logo"].as_str().filter(|l| l.starts_with("https://")).map(str::to_string),
        categories: t["meta"]["categories"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|c| c["name"].as_str().or_else(|| c.as_str()).map(str::to_string))
            .collect(),
        no_auth: t["no_auth"].as_bool().unwrap_or(false),
        tools_count: t["meta"]["tools_count"].as_u64(),
    })
}

pub struct Apps;

#[async_trait]
impl Service for Apps {
    type Input = AppsInput;
    type Output = Vec<AppView>;
    const META: Meta = Meta::query(
        "automations.apps",
        "Search the apps Composio can connect to (Google Drive, Slack, HubSpot, Salesforce and hundreds more), most used first.",
        ACCESS,
    );

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: AppsInput) -> Result<Vec<AppView>> {
        let composio = client(ctx, conn).await?;
        let toolkits = composio.toolkits(input.search.as_deref(), input.limit.unwrap_or(30).clamp(1, 100)).await?;
        Ok(toolkits.iter().filter_map(app_view).collect())
    }
}

#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionView {
    /// Composio's connected account id. Rules refer to it.
    pub id: String,
    /// The app's toolkit slug.
    pub app: String,
    /// ACTIVE once the account is connected; INITIATED while the sign-in is not finished; EXPIRED or FAILED when it needs connecting again.
    pub status: String,
    pub created_at: Option<String>,
    /// Rules that use this connection.
    pub automations: i64,
}

pub struct Connections;

#[async_trait]
impl Service for Connections {
    type Input = Empty;
    type Output = Vec<ConnectionView>;
    const META: Meta = Meta::query(
        "automations.connections",
        "List the app accounts connected to this OpenSign through Composio, with their status.",
        ACCESS,
    );

    async fn run(ctx: &Ctx, conn: &mut PgConnection, _: Empty) -> Result<Vec<ConnectionView>> {
        let composio = client(ctx, conn).await?;
        let accounts = composio.accounts(&composio::user_id(ctx.keys())).await?;
        let mut out = Vec::new();
        for a in accounts {
            let Some(id) = a["id"].as_str() else { continue };
            let automations: i64 = sqlx::query_scalar("select count(*) from automations where connected_account_id = $1")
                .bind(id)
                .fetch_one(&mut *conn)
                .await?;
            out.push(ConnectionView {
                id: id.to_string(),
                app: a["toolkit"]["slug"].as_str().unwrap_or_default().to_string(),
                status: a["status"].as_str().unwrap_or("UNKNOWN").to_string(),
                created_at: a["created_at"].as_str().map(str::to_string),
                automations,
            });
        }
        Ok(out)
    }
}

/// The account, only if it belongs to this install's Composio user.
async fn own_account(ctx: &Ctx, composio: &Composio, id: &str) -> Result<Value> {
    let account = composio.account(id).await.map_err(|e| if e.kind == ErrorKind::NotFound { not_found("Connection") } else { e })?;
    if account["user_id"].as_str() != Some(composio::user_id(ctx.keys()).as_str()) {
        return Err(not_found("Connection"));
    }
    Ok(account)
}

#[derive(Deserialize, JsonSchema)]
pub struct ConnectInput {
    /// The app's toolkit slug from automations.apps, such as googledrive.
    pub app: String,
}

#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConnectOutput {
    /// Open this in the browser to sign in to the app. Composio sends you back to Settings, Automations.
    pub redirect_url: String,
    pub connection_id: String,
}

pub struct Connect;

#[async_trait]
impl Service for Connect {
    type Input = ConnectInput;
    type Output = ConnectOutput;
    const META: Meta = Meta::mutation(
        "automations.connect",
        "Start connecting an app account through Composio. Returns a link where a person signs in to the app; the connection is ready once its status is ACTIVE.",
        ACCESS,
    )
    .without_transaction();

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: ConnectInput) -> Result<ConnectOutput> {
        let composio = client(ctx, conn).await?;
        let app = input.app.trim().to_lowercase();
        let auth_config = composio.auth_config(&app).await?;
        let callback = format!("{}/settings/automations?connected={app}", ctx.app.public_url());
        let (redirect_url, connection_id) = composio.link(&auth_config, &composio::user_id(ctx.keys()), &callback).await?;
        audit(ctx, conn, Entry::new("automations.connect", "automation_connection", connection_id.clone()).metadata(json!({ "app": app })))
            .await?;
        Ok(ConnectOutput { redirect_url, connection_id })
    }
}

#[derive(Deserialize, JsonSchema)]
pub struct ConnectionInput {
    /// The connection id from automations.connections.
    pub id: String,
}

#[derive(Serialize, JsonSchema)]
pub struct DisconnectOutput {
    /// Rules that used the connection and were turned off.
    pub paused: u64,
}

pub struct Disconnect;

#[async_trait]
impl Service for Disconnect {
    type Input = ConnectionInput;
    type Output = DisconnectOutput;
    const META: Meta =
        Meta::mutation("automations.disconnect", "Remove a connected app account from Composio. Rules that use it are turned off.", ACCESS)
            .without_transaction();

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: ConnectionInput) -> Result<DisconnectOutput> {
        let composio = client(ctx, conn).await?;
        let account = own_account(ctx, &composio, &input.id).await?;
        composio.delete_account(&input.id).await?;
        let paused = sqlx::query("update automations set active = false, updated_at = now() where connected_account_id = $1 and active")
            .bind(&input.id)
            .execute(&mut *conn)
            .await?
            .rows_affected();
        audit(
            ctx,
            conn,
            Entry::new("automations.disconnect", "automation_connection", input.id.clone())
                .metadata(json!({ "app": account["toolkit"]["slug"], "paused": paused })),
        )
        .await?;
        Ok(DisconnectOutput { paused })
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ToolsInput {
    /// The app's toolkit slug.
    pub app: String,
    /// Words describing what the tool does, such as "upload file".
    pub search: Option<String>,
    /// At most 100; 50 when left out.
    pub limit: Option<u32>,
}

#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ToolSummary {
    pub slug: String,
    pub name: String,
    pub description: String,
}

pub struct Tools;

#[async_trait]
impl Service for Tools {
    type Input = ToolsInput;
    type Output = Vec<ToolSummary>;
    const META: Meta = Meta::query(
        "automations.tools",
        "List what an app can do through Composio (its tools), such as uploading a file or updating a deal, to pick one for a rule.",
        ACCESS,
    );

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: ToolsInput) -> Result<Vec<ToolSummary>> {
        let composio = client(ctx, conn).await?;
        let tools = composio.tools(input.app.trim(), input.search.as_deref(), input.limit.unwrap_or(50).clamp(1, 100)).await?;
        Ok(tools
            .iter()
            .filter(|t| t["is_deprecated"].as_bool() != Some(true))
            .filter_map(|t| {
                Some(ToolSummary {
                    slug: t["slug"].as_str()?.to_string(),
                    name: t["name"].as_str().unwrap_or_default().to_string(),
                    description: t["description"].as_str().unwrap_or_default().chars().take(400).collect(),
                })
            })
            .collect())
    }
}

#[derive(Deserialize, JsonSchema)]
pub struct ToolInput {
    /// The tool's slug, such as GOOGLEDRIVE_UPLOAD_FILE.
    pub tool: String,
}

#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ToolView {
    pub slug: String,
    pub name: String,
    pub description: String,
    pub app: String,
    pub version: Option<String>,
    /// The tool's arguments as a JSON schema, to build a rule's arguments from.
    pub input_schema: Value,
}

fn tool_view(tool: &Value) -> Result<ToolView> {
    Ok(ToolView {
        slug: tool["slug"].as_str().ok_or_else(|| not_found("Tool"))?.to_string(),
        name: tool["name"].as_str().unwrap_or_default().to_string(),
        description: tool["description"].as_str().unwrap_or_default().to_string(),
        app: tool["toolkit"]["slug"].as_str().unwrap_or_default().to_string(),
        version: tool["version"].as_str().map(str::to_string),
        input_schema: composio::input_schema(tool),
    })
}

pub struct Tool;

#[async_trait]
impl Service for Tool {
    type Input = ToolInput;
    type Output = ToolView;
    const META: Meta =
        Meta::query("automations.tool", "One Composio tool with the JSON schema of its arguments, to fill in a rule's arguments.", ACCESS);

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: ToolInput) -> Result<ToolView> {
        let composio = client(ctx, conn).await?;
        tool_view(&composio.tool(input.tool.trim()).await?)
    }
}

#[derive(Serialize, JsonSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct AutomationView {
    pub id: Uuid,
    pub name: String,
    pub event: String,
    /// Runs only for documents made from these templates. Empty: every document.
    pub template_ids: Vec<Uuid>,
    /// Runs only for documents in these folders. Empty: every folder.
    pub folder_ids: Vec<Uuid>,
    /// The app's toolkit slug.
    pub app: String,
    pub tool: String,
    pub tool_version: Option<String>,
    pub connection_id: String,
    /// The tool's arguments. Strings may hold placeholders such as {{document.title}}.
    pub arguments: Value,
    pub active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_run_at: Option<DateTime<Utc>>,
    /// pending, delivered or failed, for the latest run.
    pub last_status: Option<String>,
    pub last_error: Option<String>,
    /// Runs that were given up and not resent.
    pub failed: i64,
}

const VIEW: &str = "select a.id, a.name, a.event, a.template_ids, a.folder_ids, a.toolkit as app, a.tool, a.tool_version,
    a.connected_account_id as connection_id, a.arguments, a.active, a.created_at, a.updated_at,
    r.created_at as last_run_at, r.status as last_status, r.error as last_error,
    (select count(*) from webhook_deliveries f where f.automation_id = a.id and FAILED) as failed
  from automations a left join lateral (
    select created_at, error, case when delivered_at is not null then 'delivered' when next_attempt_at is not null then 'pending' else 'failed' end as status
    from webhook_deliveries where automation_id = a.id order by created_at desc limit 1) r on true";

fn view_sql(filter: &str) -> String {
    format!("{} {filter}", VIEW.replace("FAILED", FAILED))
}

async fn load(conn: &mut PgConnection, id: Uuid) -> Result<AutomationView> {
    sqlx::query_as(&view_sql("where a.id = $1")).bind(id).fetch_optional(&mut *conn).await?.ok_or_else(|| not_found("Automation"))
}

pub struct List;

#[async_trait]
impl Service for List {
    type Input = Empty;
    type Output = Vec<AutomationView>;
    const META: Meta = Meta::query(
        "automations.list",
        "List automation rules: the event, the app and tool they run, their arguments, and the result of the latest run.",
        ACCESS,
    );

    async fn run(_ctx: &Ctx, conn: &mut PgConnection, _: Empty) -> Result<Vec<AutomationView>> {
        Ok(sqlx::query_as(&view_sql("order by a.created_at")).fetch_all(&mut *conn).await?)
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AutomationInput {
    pub name: String,
    /// One of document.created, document.sent, document.viewed, document.signed, document.completed, document.declined, document.canceled, document.expired.
    pub event: String,
    /// Only documents made from these templates. Empty or left out: every document.
    #[serde(default)]
    pub template_ids: Vec<Uuid>,
    /// Only documents in these folders. Empty or left out: every folder.
    #[serde(default)]
    pub folder_ids: Vec<Uuid>,
    /// An ACTIVE connection from automations.connections.
    pub connection_id: String,
    /// The tool's slug from automations.tools. It must belong to the connection's app.
    pub tool: String,
    /// The tool's arguments, by the names in its input schema. Strings may use placeholders such as {{document.title}}, {{fields.Total}}, {{parties.0.email}} or {{sealedPdf}} (the sealed PDF as a file); see automations.status for the list.
    #[serde(default)]
    pub arguments: Value,
    /// Off: the rule is kept but does not run. On when left out.
    pub active: Option<bool>,
}

struct Checked {
    name: String,
    app: String,
    tool: String,
    version: Option<String>,
    arguments: Value,
}

async fn check(ctx: &Ctx, conn: &mut PgConnection, input: &AutomationInput) -> Result<Checked> {
    let name = input.name.trim().to_string();
    if name.is_empty() || name.chars().count() > 200 {
        return Err(bad_request("Give the automation a name of up to 200 characters"));
    }
    if !EVENTS.contains(&input.event.as_str()) {
        return Err(bad_request(format!("Choose an event from: {}", EVENTS.join(", "))));
    }
    if input.template_ids.len() > MAX_FILTER_IDS || input.folder_ids.len() > MAX_FILTER_IDS {
        return Err(bad_request(format!("Pick at most {MAX_FILTER_IDS} templates and {MAX_FILTER_IDS} folders")));
    }
    let templates: i64 = sqlx::query_scalar("select count(*) from documents where id = any($1) and is_template")
        .bind(&input.template_ids)
        .fetch_one(&mut *conn)
        .await?;
    let folders: i64 =
        sqlx::query_scalar("select count(*) from folders where id = any($1)").bind(&input.folder_ids).fetch_one(&mut *conn).await?;
    if templates as usize != unique(&input.template_ids) || folders as usize != unique(&input.folder_ids) {
        return Err(bad_request("A template or folder in the rule does not exist"));
    }
    let arguments = if input.arguments.is_null() { json!({}) } else { input.arguments.clone() };
    if !arguments.is_object() {
        return Err(bad_request("The arguments must be an object of the tool's inputs"));
    }
    if serde_json::to_vec(&arguments).map_or(0, |b| b.len()) > MAX_ARGUMENTS_BYTES {
        return Err(bad_request("The arguments are too long; keep them under 20 KB"));
    }
    let mut placeholders = BTreeSet::new();
    used(&arguments, &mut placeholders);
    if let Some(bad) = placeholders.iter().find(|p| !known(p)) {
        return Err(bad_request(format!("{{{{{bad}}}}} is not a placeholder OpenSign knows. See the list under Placeholders.")));
    }
    if placeholders.contains(crate::automations::SEALED_PDF) && input.event != "document.completed" {
        return Err(bad_request("{{sealedPdf}} is only there when the document is completed. Use the document.completed event."));
    }
    let composio = client(ctx, conn).await?;
    let account = own_account(ctx, &composio, input.connection_id.trim()).await?;
    let app = account["toolkit"]["slug"].as_str().unwrap_or_default().to_string();
    let tool = tool_view(&composio.tool(input.tool.trim()).await?)?;
    if !tool.app.eq_ignore_ascii_case(&app) {
        return Err(bad_request(format!("{} is a {} tool, but the connection is to {app}", tool.slug, tool.app)));
    }
    let properties = tool.input_schema["properties"].as_object();
    if let Some(unknown) =
        arguments.as_object().into_iter().flatten().map(|(k, _)| k).find(|k| properties.is_none_or(|p| !p.contains_key(*k)))
    {
        return Err(bad_request(format!("{} has no input called {unknown}", tool.slug)));
    }
    let missing: Vec<&str> = tool.input_schema["required"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter(|k| arguments.get(*k).is_none_or(|v| v.is_null() || v.as_str().is_some_and(|s| s.trim().is_empty())))
        .collect();
    if !missing.is_empty() {
        return Err(bad_request(format!("Fill in the tool's required inputs: {}", missing.join(", "))));
    }
    Ok(Checked { name, app, tool: tool.slug, version: tool.version, arguments })
}

fn unique(ids: &[Uuid]) -> usize {
    ids.iter().collect::<BTreeSet<_>>().len()
}

fn dedup(ids: &[Uuid]) -> Vec<Uuid> {
    ids.iter().copied().collect::<BTreeSet<_>>().into_iter().collect()
}

pub struct Create;

#[async_trait]
impl Service for Create {
    type Input = AutomationInput;
    type Output = AutomationView;
    const META: Meta = Meta::mutation(
        "automations.create",
        "Add an automation rule: when a document event happens (optionally only for some templates or folders), run a Composio tool on a connected app with arguments filled from the document. The tool and its required inputs are checked with Composio first.",
        ACCESS,
    )
    .without_transaction();

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: AutomationInput) -> Result<AutomationView> {
        let checked = check(ctx, conn, &input).await?;
        let id = Uuid::now_v7();
        sqlx::query(
            "insert into automations (id, name, event, template_ids, folder_ids, toolkit, tool, tool_version, connected_account_id, arguments, active, created_by)
             values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
        )
        .bind(id)
        .bind(&checked.name)
        .bind(&input.event)
        .bind(dedup(&input.template_ids))
        .bind(dedup(&input.folder_ids))
        .bind(&checked.app)
        .bind(&checked.tool)
        .bind(&checked.version)
        .bind(input.connection_id.trim())
        .bind(&checked.arguments)
        .bind(input.active.unwrap_or(true))
        .bind(ctx.actor()?.user_id)
        .execute(&mut *conn)
        .await?;
        let view = load(conn, id).await?;
        audit(ctx, conn, Entry::new("automations.create", "automation", id).after(rule_audit(&view))).await?;
        Ok(view)
    }
}

fn rule_audit(view: &AutomationView) -> Value {
    json!({
        "name": view.name, "event": view.event, "app": view.app, "tool": view.tool, "active": view.active,
        "templates": view.template_ids, "folders": view.folder_ids, "arguments": view.arguments,
    })
}

#[derive(Deserialize, JsonSchema)]
pub struct UpdateInput {
    pub id: Uuid,
    #[serde(flatten)]
    pub rule: AutomationInput,
}

pub struct Update;

#[async_trait]
impl Service for Update {
    type Input = UpdateInput;
    type Output = AutomationView;
    const META: Meta = Meta::mutation(
        "automations.update",
        "Change an automation rule. Send the whole rule, as for automations.create; set active to false to pause it.",
        ACCESS,
    )
    .without_transaction();

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: UpdateInput) -> Result<AutomationView> {
        let before = load(conn, input.id).await?;
        let rule = &input.rule;
        let checked = check(ctx, conn, rule).await?;
        sqlx::query(
            "update automations set name = $2, event = $3, template_ids = $4, folder_ids = $5, toolkit = $6, tool = $7, tool_version = $8,
               connected_account_id = $9, arguments = $10, active = $11, updated_at = now() where id = $1",
        )
        .bind(input.id)
        .bind(&checked.name)
        .bind(&rule.event)
        .bind(dedup(&rule.template_ids))
        .bind(dedup(&rule.folder_ids))
        .bind(&checked.app)
        .bind(&checked.tool)
        .bind(&checked.version)
        .bind(rule.connection_id.trim())
        .bind(&checked.arguments)
        .bind(rule.active.unwrap_or(before.active))
        .execute(&mut *conn)
        .await?;
        let view = load(conn, input.id).await?;
        audit(ctx, conn, Entry::new("automations.update", "automation", input.id).before(rule_audit(&before)).after(rule_audit(&view)))
            .await?;
        Ok(view)
    }
}

#[derive(Deserialize, JsonSchema)]
pub struct IdInput {
    pub id: Uuid,
}

pub struct Delete;

#[async_trait]
impl Service for Delete {
    type Input = IdInput;
    type Output = Ok_;
    const META: Meta =
        Meta::mutation("automations.delete", "Remove an automation rule and its run history. The connected app account stays.", ACCESS);

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: IdInput) -> Result<Ok_> {
        let before = load(conn, input.id).await?;
        sqlx::query("delete from automations where id = $1").bind(input.id).execute(&mut *conn).await?;
        audit(ctx, conn, Entry::new("automations.delete", "automation", input.id).before(rule_audit(&before))).await?;
        Ok(Ok_::default())
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TestInput {
    pub id: Uuid,
    /// The document whose data the run uses. It must not be a template.
    pub document_id: Uuid,
}

pub struct Test;

#[async_trait]
impl Service for Test {
    type Input = TestInput;
    type Output = ResendOutput;
    const META: Meta = Meta::mutation(
        "automations.test",
        "Run an automation now with one document's data, as if its event had just happened, to check it end to end. The tool really runs. Returns the run's id.",
        ACCESS,
    );

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: TestInput) -> Result<ResendOutput> {
        let rule = load(conn, input.id).await?;
        let document: Option<(String, Option<String>)> =
            sqlx::query_as("select title, external_id from documents where id = $1 and not is_template")
                .bind(input.document_id)
                .fetch_optional(&mut *conn)
                .await?;
        let (title, external_id) = document.ok_or_else(|| not_found("Document"))?;
        let data = json!({ "documentId": input.document_id, "title": title, "externalId": external_id, "test": true });
        let id = queue_delivery(conn, Target::Automation(rule.id), &rule.event, &data, ctx.now, None).await?;
        audit(
            ctx,
            conn,
            Entry::new("automations.test", "automation", rule.id).metadata(json!({ "document": input.document_id, "run": id })),
        )
        .await?;
        Ok(ResendOutput { id })
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RunsInput {
    pub automation_id: Uuid,
    /// Only runs with this status. "failed" also includes runs waiting for a retry after a failed attempt.
    pub status: Option<DeliveryStatus>,
    /// For the next page: the id of the last run of the previous page.
    pub before: Option<Uuid>,
    /// At most 200; 50 when left out.
    pub limit: Option<i64>,
}

pub struct Runs;

#[async_trait]
impl Service for Runs {
    type Input = RunsInput;
    type Output = Vec<DeliveryView>;
    const META: Meta = Meta::query(
        "automations.runs",
        "List an automation's runs from the last 30 days, newest first: event, status, attempts, Composio's latest answer or error, and the next retry.",
        ACCESS,
    );

    async fn run(_ctx: &Ctx, conn: &mut PgConnection, input: RunsInput) -> Result<Vec<DeliveryView>> {
        let filter = LogFilter { status: input.status, event: None, before: input.before, limit: input.limit };
        super::webhooks::deliveries(conn, Log::Automation, input.automation_id, filter).await
    }
}

pub struct GetRun;

#[async_trait]
impl Service for GetRun {
    type Input = IdInput;
    type Output = DeliveryDetail;
    const META: Meta = Meta::query(
        "automations.run",
        "One automation run with its event and every attempt: time, Composio's status code, error, duration and the start of its answer.",
        ACCESS,
    );

    async fn run(_ctx: &Ctx, conn: &mut PgConnection, input: IdInput) -> Result<DeliveryDetail> {
        super::webhooks::delivery(conn, Log::Automation, input.id).await
    }
}

pub struct Resend;

#[async_trait]
impl Service for Resend {
    type Input = IdInput;
    type Output = ResendOutput;
    const META: Meta = Meta::mutation(
        "automations.resend",
        "Run an automation again for an earlier run's event, as a new run. The tool really runs again, with the document's current data.",
        ACCESS,
    );

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: IdInput) -> Result<ResendOutput> {
        let id = super::webhooks::resend(conn, Log::Automation, input.id).await?.ok_or_else(|| not_found("Run"))?;
        audit(ctx, conn, Entry::new("automations.resend", "automation_run", input.id).metadata(json!({ "resentAs": id }))).await?;
        Ok(ResendOutput { id })
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ResendFailedInput {
    pub automation_id: Uuid,
}

#[derive(Serialize, JsonSchema)]
pub struct ResendFailedOutput {
    /// How many runs were queued again.
    pub count: usize,
}

pub struct ResendFailed;

#[async_trait]
impl Service for ResendFailed {
    type Input = ResendFailedInput;
    type Output = ResendFailedOutput;
    const META: Meta = Meta::mutation(
        "automations.resend_failed",
        "Run again every run of an automation that was given up and not resent yet, up to 500 at a time. Use it after fixing the connection or the rule.",
        ACCESS,
    );

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: ResendFailedInput) -> Result<ResendFailedOutput> {
        load(conn, input.automation_id).await?;
        let count = super::webhooks::resend_failed(conn, Log::Automation, input.automation_id).await?;
        if count > 0 {
            audit(
                ctx,
                conn,
                Entry::new("automations.resend_failed", "automation", input.automation_id).metadata(json!({ "count": count })),
            )
            .await?;
        }
        Ok(ResendFailedOutput { count })
    }
}
