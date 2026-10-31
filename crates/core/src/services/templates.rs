use async_trait::async_trait;
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::audit::{Entry, audit};
use crate::context::Ctx;
use crate::documents::versions::{self, Change, Content};
use crate::documents::{DocumentRow, DocumentView, Need, VISIBLE, load_document, view, visibility};
use crate::error::{Result, bad_request, conflict, not_found};
use crate::service::{Access, Meta, Service};
use crate::storage::{FileRecord, file_record};

const MAX_NOTE: usize = 500;

async fn template(ctx: &Ctx, conn: &mut PgConnection, id: Uuid, need: Need) -> Result<DocumentRow> {
    let document = load_document(ctx, conn, id, need).await?;
    if !document.is_template {
        return Err(bad_request("That document is not a template"));
    }
    Ok(document)
}

#[derive(sqlx::FromRow)]
struct VersionRow {
    id: Uuid,
    number: i32,
    snapshot: Value,
}

async fn latest(conn: &mut PgConnection, template_id: Uuid) -> Result<Option<VersionRow>> {
    Ok(sqlx::query_as("select id, number, snapshot from template_versions where template_id = $1 order by number desc limit 1")
        .bind(template_id)
        .fetch_optional(&mut *conn)
        .await?)
}

async fn version(ctx: &Ctx, conn: &mut PgConnection, template_id: Uuid, number: i32) -> Result<Content> {
    let row: VersionRow = sqlx::query_as("select id, number, snapshot from template_versions where template_id = $1 and number = $2")
        .bind(template_id)
        .bind(number)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or_else(|| not_found("Version"))?;
    versions::open(ctx.keys(), row.id, row.snapshot)
}

async fn insert(
    ctx: &Ctx,
    conn: &mut PgConnection,
    template_id: Uuid,
    content: &Content,
    note: Option<&str>,
    restored_from: Option<i32>,
) -> Result<i32> {
    let id = Uuid::now_v7();
    let number: i32 = sqlx::query_scalar(
        "insert into template_versions (id, template_id, number, snapshot, note, restored_from, created_by)
         select $1, $2, coalesce(max(number), 0) + 1, $3, $4, $5, $6 from template_versions where template_id = $2
         returning number",
    )
    .bind(id)
    .bind(template_id)
    .bind(versions::seal(ctx.keys(), id, content)?)
    .bind(note)
    .bind(restored_from)
    .bind(ctx.actor.as_ref().map(|a| a.user_id))
    .fetch_one(&mut *conn)
    .await?;
    let file_ids: Vec<Uuid> = content.files.iter().map(|f| f.id).collect();
    sqlx::query("insert into template_version_files (version_id, file_id) select $1, unnest($2::uuid[]) on conflict do nothing")
        .bind(id)
        .bind(&file_ids)
        .execute(&mut *conn)
        .await?;
    audit(
        ctx,
        conn,
        Entry::new("template.version_saved", "document", template_id)
            .metadata(json!({ "version": number, "note": note, "restoredFrom": restored_from })),
    )
    .await?;
    Ok(number)
}

/// The template as it is now, and the number of the version that holds it. A version is saved
/// first when the template changed since the last one, so every document can name its version.
pub async fn current_version(ctx: &Ctx, conn: &mut PgConnection, template_id: Uuid) -> Result<(i32, Content)> {
    sqlx::query("select 1 from documents where id = $1 for update").bind(template_id).execute(&mut *conn).await?;
    let content = versions::load(ctx, conn, template_id).await?;
    if let Some(last) = latest(conn, template_id).await?
        && versions::open(ctx.keys(), last.id, last.snapshot)? == content
    {
        return Ok((last.number, content));
    }
    let number = insert(ctx, conn, template_id, &content, None, None).await?;
    Ok((number, content))
}

/// The files of a template version, in order.
pub async fn version_files(ctx: &Ctx, conn: &mut PgConnection, template: &DocumentRow, number: i32) -> Result<Vec<FileRecord>> {
    if !template.is_template {
        return Err(bad_request("That document is not a template"));
    }
    let content = version(ctx, conn, template.id, number).await?;
    let mut records = Vec::with_capacity(content.files.len());
    for f in &content.files {
        records.push(file_record(conn, f.id).await?);
    }
    Ok(records)
}

fn note(value: Option<&str>) -> Result<Option<String>> {
    let Some(value) = value.map(str::trim).filter(|v| !v.is_empty()) else { return Ok(None) };
    if value.chars().count() > MAX_NOTE {
        return Err(bad_request(format!("Keep the note under {MAX_NOTE} characters")));
    }
    if value.chars().any(|c| c.is_control() && c != '\n') {
        return Err(bad_request("The note contains characters that are not allowed"));
    }
    Ok(Some(value.to_string()))
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TemplateInput {
    pub template_id: Uuid,
}

#[derive(Serialize, JsonSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct TemplateVersionSummary {
    pub number: i32,
    pub note: Option<String>,
    /// The version this one restored, if it was made by a restore.
    pub restored_from: Option<i32>,
    pub created_by_name: Option<String>,
    pub created_at: DateTime<Utc>,
    /// Documents you can see that were made from this version.
    pub document_count: i64,
}

#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct VersionsOutput {
    /// Newest first.
    pub versions: Vec<TemplateVersionSummary>,
    /// The template has changed since its newest version (or has none yet).
    pub changed: bool,
}

async fn summaries(ctx: &Ctx, conn: &mut PgConnection, template_id: Uuid, number: Option<i32>) -> Result<Vec<TemplateVersionSummary>> {
    let (read_all, user_id, can_create) = visibility(ctx)?;
    Ok(sqlx::query_as(&format!(
        "select v.number, v.note, v.restored_from, u.name as created_by_name, v.created_at,
            (select count(*) from documents d where d.template_id = v.template_id and d.template_version = v.number and {VISIBLE}) as document_count
         from template_versions v left join users u on u.id = v.created_by
         where v.template_id = $4 and ($5::integer is null or v.number = $5) order by v.number desc"
    ))
    .bind(read_all)
    .bind(user_id)
    .bind(can_create)
    .bind(template_id)
    .bind(number)
    .fetch_all(&mut *conn)
    .await?)
}

pub struct Versions;

#[async_trait]
impl Service for Versions {
    type Input = TemplateInput;
    type Output = VersionsOutput;
    const META: Meta = Meta::query(
        "templates.versions",
        "List a template's saved versions, newest first, with who saved them, their notes and how many documents were made from each, and whether the template has changed since the newest.",
        Access::Authenticated,
    )
    .tool();

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: TemplateInput) -> Result<VersionsOutput> {
        let t = template(ctx, conn, input.template_id, Need::Read).await?;
        let changed = match latest(conn, t.id).await? {
            Some(last) => versions::open(ctx.keys(), last.id, last.snapshot)? != versions::load(ctx, conn, t.id).await?,
            None => true,
        };
        Ok(VersionsOutput { versions: summaries(ctx, conn, t.id, None).await?, changed })
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct VersionInput {
    pub template_id: Uuid,
    pub number: i32,
}

#[derive(Serialize, JsonSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct VersionDocument {
    pub id: Uuid,
    pub title: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TemplateVersion {
    #[serde(flatten)]
    pub summary: TemplateVersionSummary,
    /// The template as it was: settings, files, parties and fields. Personal numbers are masked.
    pub content: Content,
    /// The newest 50 documents you can see that were made from this version.
    pub documents: Vec<VersionDocument>,
}

pub struct GetVersion;

#[async_trait]
impl Service for GetVersion {
    type Input = VersionInput;
    type Output = TemplateVersion;
    const META: Meta = Meta::query(
        "templates.version",
        "Get one saved version of a template: its settings, files, parties and fields as they were, and the documents made from it.",
        Access::Authenticated,
    )
    .tool();

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: VersionInput) -> Result<TemplateVersion> {
        let t = template(ctx, conn, input.template_id, Need::Read).await?;
        let content = versions::masked(version(ctx, conn, t.id, input.number).await?);
        let summary = summaries(ctx, conn, t.id, Some(input.number)).await?.pop().ok_or_else(|| not_found("Version"))?;
        let (read_all, user_id, can_create) = visibility(ctx)?;
        let documents = sqlx::query_as(&format!(
            "select d.id, d.title, d.status, d.created_at from documents d
             where d.template_id = $4 and d.template_version = $5 and {VISIBLE} order by d.created_at desc limit 50"
        ))
        .bind(read_all)
        .bind(user_id)
        .bind(can_create)
        .bind(t.id)
        .bind(input.number)
        .fetch_all(&mut *conn)
        .await?;
        Ok(TemplateVersion { summary, content, documents })
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CompareInput {
    pub template_id: Uuid,
    /// The older version. Leave out to compare from the template as it is now.
    pub from: Option<i32>,
    /// The newer version. Leave out to compare with the template as it is now.
    pub to: Option<i32>,
}

#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CompareOutput {
    pub changes: Vec<Change>,
}

pub struct Compare;

#[async_trait]
impl Service for Compare {
    type Input = CompareInput;
    type Output = CompareOutput;
    const META: Meta = Meta::query(
        "templates.compare",
        "Compare two versions of a template, or a version with the template as it is now: which settings, files, parties and fields were added, removed or changed.",
        Access::Authenticated,
    )
    .tool();

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: CompareInput) -> Result<CompareOutput> {
        let t = template(ctx, conn, input.template_id, Need::Read).await?;
        let from = match input.from {
            Some(n) => version(ctx, conn, t.id, n).await?,
            None => versions::load(ctx, conn, t.id).await?,
        };
        let to = match input.to {
            Some(n) => version(ctx, conn, t.id, n).await?,
            None => versions::load(ctx, conn, t.id).await?,
        };
        Ok(CompareOutput { changes: versions::diff(&from, &to) })
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SaveInput {
    pub template_id: Uuid,
    /// What changed and why, for whoever reads the history.
    pub note: Option<String>,
}

pub struct Save;

#[async_trait]
impl Service for Save {
    type Input = SaveInput;
    type Output = TemplateVersionSummary;
    const META: Meta = Meta::mutation(
        "templates.save_version",
        "Save the template as it is now as a new version, with an optional note. Versions are also saved automatically when a changed template is used.",
        Access::Authenticated,
    )
    .tool();

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: SaveInput) -> Result<TemplateVersionSummary> {
        let t = template(ctx, conn, input.template_id, Need::Edit).await?;
        let note = note(input.note.as_deref())?;
        sqlx::query("select 1 from documents where id = $1 for update").bind(t.id).execute(&mut *conn).await?;
        let content = versions::load(ctx, conn, t.id).await?;
        if let Some(last) = latest(conn, t.id).await?
            && versions::open(ctx.keys(), last.id, last.snapshot)? == content
        {
            if note.is_none() {
                return Err(conflict(format!("Nothing has changed since version {}", last.number)));
            }
            sqlx::query("update template_versions set note = $3 where template_id = $1 and number = $2")
                .bind(t.id)
                .bind(last.number)
                .bind(&note)
                .execute(&mut *conn)
                .await?;
            audit(
                ctx,
                conn,
                Entry::new("template.version_noted", "document", t.id).metadata(json!({ "version": last.number, "note": note })),
            )
            .await?;
            return summaries(ctx, conn, t.id, Some(last.number)).await?.pop().ok_or_else(|| not_found("Version"));
        }
        let number = insert(ctx, conn, t.id, &content, note.as_deref(), None).await?;
        summaries(ctx, conn, t.id, Some(number)).await?.pop().ok_or_else(|| not_found("Version"))
    }
}

pub struct Restore;

#[async_trait]
impl Service for Restore {
    type Input = VersionInput;
    type Output = DocumentView;
    const META: Meta = Meta::mutation(
        "templates.restore",
        "Put a template back the way it was in an earlier version: settings, files, parties and fields. Unsaved changes are saved as a version first, and the restore itself becomes a new version, so nothing is lost.",
        Access::Authenticated,
    )
    .tool();

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: VersionInput) -> Result<DocumentView> {
        let t = template(ctx, conn, input.template_id, Need::Edit).await?;
        super::documents::lock_draft(conn, t.id).await?;
        let target = version(ctx, conn, t.id, input.number).await?;
        let (_, current) = current_version(ctx, conn, t.id).await?;
        if current == target {
            return Err(conflict(format!("The template already matches version {}", input.number)));
        }
        versions::clear(conn, t.id).await?;
        versions::apply(ctx, conn, &target, t.id).await?;
        sqlx::query(
            "update documents set title = $2, message = $3, language = $4, sequential = $5, reminder_days = $6, review_links = $7, updated_at = now() where id = $1",
        )
        .bind(t.id)
        .bind(&target.title)
        .bind(&target.message)
        .bind(&target.language)
        .bind(target.sequential)
        .bind(target.reminder_days)
        .bind(target.review_links)
        .execute(&mut *conn)
        .await?;
        audit(ctx, conn, Entry::new("template.restored", "document", t.id).metadata(json!({ "version": input.number }))).await?;
        let restored = versions::load(ctx, conn, t.id).await?;
        insert(ctx, conn, t.id, &restored, None, Some(input.number)).await?;
        let document = load_document(ctx, conn, t.id, Need::Read).await?;
        view(ctx, conn, document).await
    }
}
