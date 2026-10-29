use std::collections::HashMap;

use async_trait::async_trait;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::PgConnection;
use uuid::Uuid;

use super::documents::IdInput;
use crate::audit::{Entry, audit};
use crate::context::Ctx;
use crate::documents::export::{self, DocumentData};
use crate::documents::{DOCUMENT_COLUMNS, DocumentRow, FieldRow, Need, PARTY_COLUMNS, PartyRow, VISIBLE, load_document, visibility};
use crate::error::{Result, bad_request};
use crate::permissions::Permission;
use crate::service::{Access, Meta, Service};

pub const MAX_DOCUMENTS: usize = 5000;

pub fn values_url(ctx: &Ctx, document_id: Uuid) -> String {
    format!("{}/api/v1/documents/{document_id}/values", ctx.app.public_url())
}

async fn parties_of(conn: &mut PgConnection, ids: &[Uuid]) -> Result<HashMap<Uuid, Vec<PartyRow>>> {
    let rows: Vec<PartyRow> =
        sqlx::query_as(&format!("select {PARTY_COLUMNS} from parties where document_id = any($1) order by sign_order, created_at, id"))
            .bind(ids)
            .fetch_all(&mut *conn)
            .await?;
    let mut by_document: HashMap<Uuid, Vec<PartyRow>> = HashMap::new();
    for row in rows {
        by_document.entry(row.document_id).or_default().push(row);
    }
    Ok(by_document)
}

#[derive(sqlx::FromRow)]
struct DocumentField {
    document_id: Uuid,
    #[sqlx(flatten)]
    field: FieldRow,
}

async fn fields_of(conn: &mut PgConnection, ids: &[Uuid]) -> Result<HashMap<Uuid, Vec<FieldRow>>> {
    let rows: Vec<DocumentField> = sqlx::query_as(&format!(
        "select document_id, {} from fields where document_id = any($1) order by page, y, x, id",
        crate::documents::FIELD_COLUMNS
    ))
    .bind(ids)
    .fetch_all(&mut *conn)
    .await?;
    let mut by_document: HashMap<Uuid, Vec<FieldRow>> = HashMap::new();
    for row in rows {
        by_document.entry(row.document_id).or_default().push(row.field);
    }
    Ok(by_document)
}

async fn data_for(conn: &mut PgConnection, documents: &[DocumentRow]) -> Result<Vec<DocumentData>> {
    let ids: Vec<Uuid> = documents.iter().map(|d| d.id).collect();
    let mut parties = parties_of(conn, &ids).await?;
    let mut fields = fields_of(conn, &ids).await?;
    Ok(documents
        .iter()
        .map(|d| export::document_data(d, &parties.remove(&d.id).unwrap_or_default(), &fields.remove(&d.id).unwrap_or_default()))
        .collect())
}

pub struct Values;

#[async_trait]
impl Service for Values {
    type Input = IdInput;
    type Output = DocumentData;
    const META: Meta = Meta::query(
        "documents.values",
        "Get the values filled in on one document as data: each field keyed by its label, typed (numbers as numbers, checkboxes as true or false, several choices as a list, dates as YYYY-MM-DD, uploads by file name), with the parties and when they signed. Use it after document.completed to read what signers entered.",
        Access::Authenticated,
    )
    .tool();

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: IdInput) -> Result<DocumentData> {
        let document = load_document(ctx, conn, input.id, Need::Read).await?;
        Ok(data_for(conn, std::slice::from_ref(&document)).await?.remove(0))
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ExportFormat {
    #[default]
    Csv,
    Xlsx,
    Json,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExportInput {
    /// csv (the default), xlsx for Excel, or json.
    #[serde(default)]
    pub format: ExportFormat,
    /// Only these documents.
    pub ids: Option<Vec<Uuid>>,
    /// Only documents made from this template.
    pub template_id: Option<Uuid>,
    /// Only documents a bulk send created.
    pub bulk_id: Option<Uuid>,
    /// draft, pending, completed, declined, canceled or expired.
    pub status: Option<String>,
    /// Matches the title, or an exact external id, as in documents.list.
    pub search: Option<String>,
    /// Only documents directly in this folder.
    pub folder_id: Option<Uuid>,
    /// true for archived documents only, false to leave them out. Leave out for both.
    pub archived: Option<bool>,
}

#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExportOutput {
    /// A file name for the export.
    pub name: String,
    pub content_type: String,
    /// How many documents are in it.
    pub count: usize,
    /// The CSV or Excel file, base64. Not set for json.
    pub content_base64: Option<String>,
    /// The documents with their values. Only for json.
    pub documents: Option<Vec<DocumentData>>,
}

fn file_name(base: &str, date: &str, extension: &str) -> String {
    let base: String = base.chars().filter(|c| !c.is_control() && !matches!(c, '/' | '\\' | ':' | '"')).take(80).collect();
    format!("{} {date}.{extension}", base.trim())
}

pub struct Export;

#[async_trait]
impl Service for Export {
    type Input = ExportInput;
    type Output = ExportOutput;
    const META: Meta = Meta::query(
        "documents.export",
        "Export the values filled in on many documents you can see as CSV, Excel (xlsx) or JSON: one row per document with its status, each party's name, email and signing time, and one column per field. Filter by template, bulk send, status, folder, search or archive, as in documents.list. Personal numbers and uploaded files are never included. Every export is audited.",
        Access::Permission(Permission::DocumentsExport),
    )
    .tool();

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: ExportInput) -> Result<ExportOutput> {
        let (read_all, user_id, can_create) = visibility(ctx)?;
        if input.ids.as_ref().is_some_and(|ids| ids.is_empty() || ids.len() > MAX_DOCUMENTS) {
            return Err(bad_request(format!("Choose from 1 to {MAX_DOCUMENTS} documents")));
        }
        let bulk = match input.bulk_id {
            Some(id) => Some(super::bulk::load(ctx, conn, id).await?),
            None => None,
        };
        let template: Option<String> = match input.template_id {
            Some(id) => Some(load_document(ctx, conn, id, Need::Read).await?.title),
            None => None,
        };
        let search = input.search.as_deref().map(str::trim).filter(|s| !s.is_empty());
        let documents: Vec<DocumentRow> = sqlx::query_as(&format!(
            "select {DOCUMENT_COLUMNS} from documents d
             where {VISIBLE} and not d.is_template
               and ($4::uuid[] is null or d.id = any($4))
               and ($5::uuid is null or d.template_id = $5)
               and ($6::uuid is null or d.id in (select document_id from bulk_rows where bulk_id = $6 and document_id is not null))
               and ($7::text is null or d.status = $7)
               and ($8::text is null or d.title ilike $9 escape '\\' or d.external_id = $8)
               and ($10::uuid is null or d.folder_id = $10)
               and ($11::boolean is null or (d.archived_at is not null) = $11)
             order by d.created_at, d.id limit $12"
        ))
        .bind(read_all)
        .bind(user_id)
        .bind(can_create)
        .bind(input.ids.as_deref())
        .bind(input.template_id)
        .bind(input.bulk_id)
        .bind(input.status.as_deref())
        .bind(search)
        .bind(search.map(super::documents::like_pattern))
        .bind(input.folder_id)
        .bind(input.archived)
        .bind(MAX_DOCUMENTS as i64 + 1)
        .fetch_all(&mut *conn)
        .await?;
        if documents.len() > MAX_DOCUMENTS {
            return Err(bad_request(format!("More than {MAX_DOCUMENTS} documents match. Narrow the filters and export in parts.")));
        }
        let data = data_for(conn, &documents).await?;
        let base = match (&bulk, &template, &documents[..]) {
            (Some(b), _, _) => b.template_title.clone(),
            (None, Some(title), _) => title.clone(),
            (None, None, [only]) => only.title.clone(),
            _ => "Documents".to_string(),
        };
        let date = ctx.now.format("%Y-%m-%d").to_string();
        let (name, content_type, content, documents_out) = match input.format {
            ExportFormat::Csv => {
                let bytes = export::csv(&export::table(&data))?;
                (file_name(&base, &date, "csv"), "text/csv; charset=utf-8", Some(B64.encode(bytes)), None)
            }
            ExportFormat::Xlsx => {
                let bytes = export::xlsx(&export::table(&data))?;
                (
                    file_name(&base, &date, "xlsx"),
                    "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
                    Some(B64.encode(bytes)),
                    None,
                )
            }
            ExportFormat::Json => (file_name(&base, &date, "json"), "application/json", None, Some(data)),
        };
        let ids: Vec<Uuid> = documents.iter().map(|d| d.id).collect();
        audit(
            ctx,
            conn,
            Entry::new("documents.exported", "export", Uuid::now_v7()).metadata(json!({
                "format": input.format,
                "count": ids.len(),
                "documents": ids,
                "templateId": input.template_id,
                "bulkId": input.bulk_id,
                "status": input.status,
                "folderId": input.folder_id,
                "archived": input.archived,
                "search": search.is_some(),
            })),
        )
        .await?;
        Ok(ExportOutput {
            name,
            content_type: content_type.to_string(),
            count: ids.len(),
            content_base64: content,
            documents: documents_out,
        })
    }
}
