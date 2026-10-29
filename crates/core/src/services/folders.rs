use async_trait::async_trait;
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::PgConnection;
use uuid::Uuid;

use super::Ok_;
use crate::audit::{Entry, audit};
use crate::context::Ctx;
use crate::documents::{Need, VISIBLE, load_document, status, visibility};
use crate::error::{Result, bad_request, conflict, forbidden, not_found};
use crate::permissions::Permission;
use crate::service::{Access, Meta, Service};

pub const MAX_DEPTH: i32 = 3;
const MAX_ITEMS: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    Documents,
    Templates,
}

impl Scope {
    fn as_str(self) -> &'static str {
        match self {
            Scope::Documents => "documents",
            Scope::Templates => "templates",
        }
    }

    fn of(is_template: bool) -> Scope {
        if is_template { Scope::Templates } else { Scope::Documents }
    }
}

#[derive(Serialize, JsonSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct FolderView {
    pub id: Uuid,
    pub name: String,
    pub parent_id: Option<Uuid>,
    pub scope: String,
    pub created_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    /// Documents or templates you can see directly in this folder, not counting archived ones or subfolders.
    pub item_count: i64,
    #[sqlx(default)]
    pub can_edit: bool,
}

#[derive(sqlx::FromRow)]
struct FolderRow {
    id: Uuid,
    name: String,
    parent_id: Option<Uuid>,
    scope: String,
    created_by: Option<Uuid>,
}

async fn folder(conn: &mut PgConnection, id: Uuid) -> Result<FolderRow> {
    sqlx::query_as("select id, name, parent_id, scope, created_by from folders where id = $1")
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or_else(|| not_found("Folder"))
}

/// Template folders are changed by whoever manages templates. Document folders by the person who
/// made them and by those who manage all documents.
fn may_change(ctx: &Ctx, scope: &str, created_by: Option<Uuid>) -> Result<bool> {
    let actor = ctx.actor()?;
    Ok(if scope == Scope::Templates.as_str() {
        actor.can(Permission::TemplatesManage)
    } else {
        actor.can(Permission::DocumentsManageAll) || (actor.can(Permission::DocumentsCreate) && created_by == Some(actor.user_id))
    })
}

fn may_create(ctx: &Ctx, scope: Scope) -> bool {
    ctx.can(match scope {
        Scope::Templates => Permission::TemplatesManage,
        Scope::Documents => Permission::DocumentsCreate,
    })
}

fn name(value: &str) -> Result<String> {
    let name = value.trim();
    if name.is_empty() {
        return Err(bad_request("Give the folder a name"));
    }
    if name.chars().count() > 100 {
        return Err(bad_request("The folder name is too long (at most 100 characters)"));
    }
    if name.chars().any(char::is_control) {
        return Err(bad_request("The folder name contains characters that are not allowed"));
    }
    Ok(name.to_string())
}

/// How deep a folder sits: 1 for a top-level folder.
async fn depth(conn: &mut PgConnection, id: Uuid) -> Result<i32> {
    Ok(sqlx::query_scalar(
        "with recursive up as (select id, parent_id, 1 as depth from folders where id = $1
            union all select f.id, f.parent_id, up.depth + 1 from folders f join up on f.id = up.parent_id where up.depth < 10)
         select coalesce(max(depth), 0) from up",
    )
    .bind(id)
    .fetch_one(&mut *conn)
    .await?)
}

/// The folder and everything below it, with each one's level under it (the folder itself is 1).
async fn subtree(conn: &mut PgConnection, id: Uuid) -> Result<Vec<(Uuid, i32)>> {
    Ok(sqlx::query_as(
        "with recursive down as (select id, 1 as level from folders where id = $1
            union all select f.id, down.level + 1 from folders f join down on f.parent_id = down.id where down.level < 10)
         select id, level from down",
    )
    .bind(id)
    .fetch_all(&mut *conn)
    .await?)
}

/// Checks that a folder exists and holds the given kind of item.
pub async fn check_folder(conn: &mut PgConnection, id: Uuid, is_template: bool) -> Result<()> {
    if folder(conn, id).await?.scope != Scope::of(is_template).as_str() {
        return Err(bad_request(if is_template { "Templates go in template folders" } else { "Documents go in document folders" }));
    }
    Ok(())
}

async fn check_parent(conn: &mut PgConnection, parent_id: Option<Uuid>, scope: &str, levels: i32) -> Result<()> {
    let Some(parent_id) = parent_id else { return Ok(()) };
    let parent = folder(conn, parent_id).await?;
    if parent.scope != scope {
        return Err(bad_request("Template folders and document folders cannot be mixed"));
    }
    if depth(conn, parent_id).await? + levels > MAX_DEPTH {
        return Err(bad_request(format!("Folders can be nested at most {MAX_DEPTH} levels deep")));
    }
    Ok(())
}

async fn check_unique(conn: &mut PgConnection, scope: &str, parent_id: Option<Uuid>, name: &str, except: Option<Uuid>) -> Result<()> {
    let taken: bool = sqlx::query_scalar(
        "select exists (select 1 from folders where scope = $1 and parent_id is not distinct from $2 and lower(name) = lower($3)
            and ($4::uuid is null or id <> $4))",
    )
    .bind(scope)
    .bind(parent_id)
    .bind(name)
    .bind(except)
    .fetch_one(&mut *conn)
    .await?;
    if taken {
        return Err(conflict("There is already a folder with that name here"));
    }
    Ok(())
}

async fn reload(ctx: &Ctx, conn: &mut PgConnection, id: Uuid) -> Result<FolderView> {
    let row = folder(conn, id).await?;
    let mut all = list(ctx, conn, if row.scope == Scope::Templates.as_str() { Scope::Templates } else { Scope::Documents }).await?;
    all.retain(|f| f.id == id);
    all.pop().ok_or_else(|| not_found("Folder"))
}

async fn list(ctx: &Ctx, conn: &mut PgConnection, scope: Scope) -> Result<Vec<FolderView>> {
    let (read_all, user_id, can_create) = visibility(ctx)?;
    let mut rows: Vec<FolderView> = sqlx::query_as(&format!(
        "select f.id, f.name, f.parent_id, f.scope, f.created_by, f.created_at,
                (select count(*) from documents d where d.folder_id = f.id and d.archived_at is null and {VISIBLE}) as item_count
         from folders f where f.scope = $4 order by lower(f.name), f.id"
    ))
    .bind(read_all)
    .bind(user_id)
    .bind(can_create)
    .bind(scope.as_str())
    .fetch_all(&mut *conn)
    .await?;
    for row in &mut rows {
        row.can_edit = may_change(ctx, &row.scope, row.created_by)?;
    }
    Ok(rows)
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ListInput {
    /// documents or templates.
    pub scope: Scope,
}

pub struct List;

#[async_trait]
impl Service for List {
    type Input = ListInput;
    type Output = Vec<FolderView>;
    const META: Meta = Meta::query(
        "folders.list",
        "List every document folder or every template folder, sorted by name, with their parent and how many items you can see in each. Folders nest up to three levels through parentId.",
        Access::Authenticated,
    );

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: ListInput) -> Result<Vec<FolderView>> {
        list(ctx, conn, input.scope).await
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateInput {
    /// documents or templates.
    pub scope: Scope,
    pub name: String,
    /// Put the new folder inside this one; leave out for a top-level folder.
    pub parent_id: Option<Uuid>,
}

pub struct Create;

#[async_trait]
impl Service for Create {
    type Input = CreateInput;
    type Output = FolderView;
    const META: Meta = Meta::mutation(
        "folders.create",
        "Create a folder for documents or for templates, optionally inside another folder of the same kind. Folders are shared by the whole organisation but never give access to the documents in them.",
        Access::Authenticated,
    );

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: CreateInput) -> Result<FolderView> {
        if !may_create(ctx, input.scope) {
            return Err(forbidden());
        }
        let name = name(&input.name)?;
        let scope = input.scope.as_str();
        check_parent(conn, input.parent_id, scope, 1).await?;
        check_unique(conn, scope, input.parent_id, &name, None).await?;
        let id = Uuid::now_v7();
        sqlx::query("insert into folders (id, name, parent_id, scope, created_by) values ($1, $2, $3, $4, $5)")
            .bind(id)
            .bind(&name)
            .bind(input.parent_id)
            .bind(scope)
            .bind(ctx.actor()?.user_id)
            .execute(&mut *conn)
            .await?;
        audit(
            ctx,
            conn,
            Entry::new("folder.created", "folder", id).after(json!({ "name": name, "scope": scope, "parent": input.parent_id })),
        )
        .await?;
        reload(ctx, conn, id).await
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInput {
    pub id: Uuid,
    pub name: String,
    /// The folder to put it in, or null to make it a top-level folder.
    pub parent_id: Option<Uuid>,
}

pub struct Update;

#[async_trait]
impl Service for Update {
    type Input = UpdateInput;
    type Output = FolderView;
    const META: Meta = Meta::mutation(
        "folders.update",
        "Rename a folder or move it into another folder (null parentId for the top level). Give both the name and the parent you want it to have.",
        Access::Authenticated,
    );

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: UpdateInput) -> Result<FolderView> {
        let before = folder(conn, input.id).await?;
        if !may_change(ctx, &before.scope, before.created_by)? {
            return Err(forbidden());
        }
        let name = name(&input.name)?;
        if input.parent_id != before.parent_id {
            let below = subtree(conn, before.id).await?;
            if let Some(parent) = input.parent_id
                && below.iter().any(|(id, _)| *id == parent)
            {
                return Err(bad_request("A folder cannot be moved into itself or one of its subfolders"));
            }
            let height = below.iter().map(|(_, level)| *level).max().unwrap_or(1);
            check_parent(conn, input.parent_id, &before.scope, height).await?;
        }
        check_unique(conn, &before.scope, input.parent_id, &name, Some(before.id)).await?;
        sqlx::query("update folders set name = $2, parent_id = $3, updated_at = now() where id = $1")
            .bind(before.id)
            .bind(&name)
            .bind(input.parent_id)
            .execute(&mut *conn)
            .await?;
        audit(
            ctx,
            conn,
            Entry::new("folder.updated", "folder", before.id)
                .before(json!({ "name": before.name, "parent": before.parent_id }))
                .after(json!({ "name": name, "parent": input.parent_id })),
        )
        .await?;
        reload(ctx, conn, before.id).await
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct IdInput {
    pub id: Uuid,
}

pub struct Delete;

#[async_trait]
impl Service for Delete {
    type Input = IdInput;
    type Output = Ok_;
    const META: Meta = Meta::mutation(
        "folders.delete",
        "Delete a folder. Nothing in it is deleted: its documents, templates and subfolders move up to the folder it was in, or to the top level.",
        Access::Authenticated,
    );

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: IdInput) -> Result<Ok_> {
        let before = folder(conn, input.id).await?;
        if !may_change(ctx, &before.scope, before.created_by)? {
            return Err(forbidden());
        }
        let moved = sqlx::query("update documents set folder_id = $2 where folder_id = $1")
            .bind(before.id)
            .bind(before.parent_id)
            .execute(&mut *conn)
            .await?
            .rows_affected();
        let children: Vec<(Uuid, String)> =
            sqlx::query_as("select id, name from folders where parent_id = $1").bind(before.id).fetch_all(&mut *conn).await?;
        for (id, child) in &children {
            let taken: bool = sqlx::query_scalar(
                "select exists (select 1 from folders where scope = $1 and parent_id is not distinct from $2 and lower(name) = lower($3))",
            )
            .bind(&before.scope)
            .bind(before.parent_id)
            .bind(child)
            .fetch_one(&mut *conn)
            .await?;
            let renamed = if taken { format!("{child} ({})", before.name) } else { child.clone() };
            sqlx::query("update folders set parent_id = $2, name = $3, updated_at = now() where id = $1")
                .bind(id)
                .bind(before.parent_id)
                .bind(renamed)
                .execute(&mut *conn)
                .await?;
        }
        sqlx::query("delete from folders where id = $1").bind(before.id).execute(&mut *conn).await?;
        audit(
            ctx,
            conn,
            Entry::new("folder.deleted", "folder", before.id)
                .before(json!({ "name": before.name, "scope": before.scope, "parent": before.parent_id }))
                .metadata(json!({ "itemsMoved": moved, "foldersMoved": children.len() })),
        )
        .await?;
        Ok(Ok_::default())
    }
}

fn check_ids(ids: &[Uuid]) -> Result<Vec<Uuid>> {
    let mut ids = ids.to_vec();
    ids.sort();
    ids.dedup();
    if ids.is_empty() || ids.len() > MAX_ITEMS {
        return Err(bad_request(format!("Choose between 1 and {MAX_ITEMS} items")));
    }
    Ok(ids)
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MoveInput {
    /// Documents or templates to move, at most 200.
    pub ids: Vec<Uuid>,
    /// The folder to move them to, or null for no folder. Documents go in document folders, templates in template folders.
    pub folder_id: Option<Uuid>,
}

#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MoveOutput {
    pub moved: usize,
}

pub struct Move;

#[async_trait]
impl Service for Move {
    type Input = MoveInput;
    type Output = MoveOutput;
    const META: Meta = Meta::mutation(
        "documents.move",
        "Move documents or templates into a folder, or out of any folder with a null folderId. Works at any status. You need to be allowed to change each one.",
        Access::Permission(Permission::DocumentsCreate),
    );

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: MoveInput) -> Result<MoveOutput> {
        let ids = check_ids(&input.ids)?;
        let target = match input.folder_id {
            Some(id) => Some(folder(conn, id).await?),
            None => None,
        };
        for id in &ids {
            let document = load_document(ctx, conn, *id, Need::Edit).await?;
            if let Some(folder_id) = input.folder_id {
                check_folder(conn, folder_id, document.is_template).await?;
            }
            if document.folder_id == input.folder_id {
                continue;
            }
            sqlx::query("update documents set folder_id = $2 where id = $1")
                .bind(document.id)
                .bind(input.folder_id)
                .execute(&mut *conn)
                .await?;
            audit(
                ctx,
                conn,
                Entry::new("document.moved", "document", document.id)
                    .before(json!({ "folder": document.folder_id }))
                    .after(json!({ "folder": input.folder_id, "folderName": target.as_ref().map(|t| &t.name) })),
            )
            .await?;
        }
        Ok(MoveOutput { moved: ids.len() })
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveInput {
    /// Documents or templates, at most 200.
    pub ids: Vec<Uuid>,
    /// true archives, false restores from the archive.
    pub archived: bool,
}

#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveOutput {
    pub changed: usize,
}

pub struct Archive;

#[async_trait]
impl Service for Archive {
    type Input = ArchiveInput;
    type Output = ArchiveOutput;
    const META: Meta = Meta::mutation(
        "documents.archive",
        "Archive finished documents (completed, declined, canceled or expired) or templates so they leave the normal lists, or restore them with archived false. Archived templates cannot be used until restored. Retention rules and legal holds apply to archived documents as before.",
        Access::Permission(Permission::DocumentsCreate),
    );

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: ArchiveInput) -> Result<ArchiveOutput> {
        let ids = check_ids(&input.ids)?;
        let mut changed = 0;
        for id in &ids {
            let document = load_document(ctx, conn, *id, Need::Edit).await?;
            if input.archived && !document.is_template && !status::FINISHED.contains(&document.status.as_str()) {
                return Err(conflict(format!("Only finished documents can be archived, and \"{}\" is not finished", document.title)));
            }
            if document.archived_at.is_some() == input.archived {
                continue;
            }
            if input.archived && document.is_template {
                let busy: bool = sqlx::query_scalar(
                    "select exists (select 1 from bulk_sends where template_id = $1 and status in ('queued', 'running'))",
                )
                .bind(document.id)
                .fetch_one(&mut *conn)
                .await?;
                if busy {
                    return Err(conflict(format!(
                        "\"{}\" has a bulk send in progress. Archive it when the bulk send is done.",
                        document.title
                    )));
                }
            }
            sqlx::query("update documents set archived_at = $2 where id = $1")
                .bind(document.id)
                .bind(input.archived.then_some(ctx.now))
                .execute(&mut *conn)
                .await?;
            let action = if input.archived { "document.archived" } else { "document.unarchived" };
            audit(ctx, conn, Entry::new(action, "document", document.id)).await?;
            changed += 1;
        }
        Ok(ArchiveOutput { changed })
    }
}
