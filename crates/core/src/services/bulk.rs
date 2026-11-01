use std::collections::{BTreeMap, HashMap};

use async_trait::async_trait;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use chrono::{DateTime, Utc};
use opensign_crypto::KeyPurpose;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::PgConnection;
use uuid::Uuid;

use super::documents::{self as docs, FieldInput, PartyInput};
use super::settings;
use crate::ai::{self, Message, Prompt, json_in};
use crate::audit::{Entry, audit};
use crate::auth::{actor_for, normalize_email, user_by_id};
use crate::context::{Credential, Ctx};
use crate::documents::{Delivery, FieldKind, FieldRow, Need, PartyRow, fields, load_document, parties};
use crate::error::{AppError, ErrorKind, Result, bad_request, conflict, forbidden, not_found};
use crate::jobs::enqueue;
use crate::permissions::Permission;
use crate::service::{Access, Meta, Service};
use crate::settings::{AiSettings, SecuritySettings};
use crate::sheet::{self, Sheet};

const ROWS_PER_RUN: usize = 10;
const PARTY_ATTRS: [(&str, &str); 6] = [
    ("name", "Name"),
    ("email", "Email"),
    ("phone", "Mobile"),
    ("company", "Company"),
    ("personalNumber", "Personal Number"),
    ("language", "Language"),
];

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Target {
    /// `party.<n>.<name|email|phone|company|personalNumber|language>`, `field.<label>`, `title` or `externalId`.
    pub key: String,
    pub label: String,
    /// The template party it belongs to, or "Document".
    pub group: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ColumnMapping {
    pub column: String,
    /// A target key, or empty to ignore the column.
    pub target: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FileInput {
    pub template_id: Uuid,
    /// A .csv or .xlsx file whose first row names the columns. Up to 1000 rows.
    pub file_name: String,
    pub content_base64: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PreviewInput {
    #[serde(flatten)]
    pub file: FileInput,
    /// Ask the AI provider to match columns the built-in rules could not. Sends the column names, and a few sample
    /// rows only if an Admin allows document text to leave the server.
    #[serde(default)]
    pub use_ai: bool,
}

#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RowProblem {
    /// 1-based, counting the header row as row 1, as a spreadsheet shows it.
    pub row: usize,
    /// What is wrong, in English.
    pub message: String,
    /// Which problem this is, for showing it in another language.
    pub code: String,
    /// The values that fill the placeholders of the text for `code`.
    pub params: BTreeMap<String, String>,
}

#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PreviewOutput {
    pub columns: Vec<String>,
    pub sample_rows: Vec<Vec<String>>,
    pub row_count: usize,
    pub targets: Vec<Target>,
    pub mapping: Vec<ColumnMapping>,
    pub ai_used: bool,
    /// Rows that cannot be sent with this mapping. At most 20.
    pub problems: Vec<RowProblem>,
}

struct Template {
    id: Uuid,
    title: String,
    language: String,
    parties: Vec<PartyRow>,
    fields: Vec<FieldRow>,
}

async fn template(ctx: &Ctx, conn: &mut PgConnection, id: Uuid) -> Result<Template> {
    let document = load_document(ctx, conn, id, Need::Read).await?;
    if !document.is_template {
        return Err(bad_request("Bulk send starts from a template"));
    }
    if document.archived_at.is_some() {
        return Err(bad_request("That template is archived. Restore it before using it."));
    }
    let parties = parties(conn, id).await?;
    if !parties.iter().any(PartyRow::must_act) {
        return Err(bad_request("The template needs at least one party who signs or approves"));
    }
    Ok(Template { id, title: document.title, language: document.language, parties, fields: fields(conn, id).await? })
}

fn sender_fields(t: &Template) -> impl Iterator<Item = &FieldRow> {
    t.fields.iter().filter(|f| {
        f.party_id.is_none()
            && !f.label.trim().is_empty()
            && f.kind != FieldKind::Signature.as_str()
            && f.kind != FieldKind::Formula.as_str()
    })
}

fn targets(t: &Template) -> Vec<Target> {
    let doc = |key: &str, label: &str| Target { key: key.into(), label: label.into(), group: "Document".into() };
    let mut out = vec![doc("title", "Title"), doc("externalId", "Your Reference")];
    for (i, p) in t.parties.iter().enumerate() {
        for (attr, label) in PARTY_ATTRS {
            out.push(Target { key: format!("party.{i}.{attr}"), label: label.into(), group: p.name.clone() });
        }
    }
    for f in sender_fields(t) {
        if !out.iter().any(|o| o.key == format!("field.{}", f.label)) {
            out.push(doc(&format!("field.{}", f.label), &f.label));
        }
    }
    out
}

fn normalize(s: &str) -> String {
    s.to_lowercase().chars().filter(|c| c.is_alphanumeric()).collect()
}

const SYNONYMS: [(&str, &[&str]); 8] = [
    ("language", &["language", "lang", "sprak", "språk", "locale"]),
    ("email", &["email", "epost", "mail", "emailaddress", "epostadress", "mejl"]),
    ("phone", &["phone", "mobile", "mobil", "telefon", "tel", "cell", "mobilnummer", "telefonnummer", "phonenumber", "sms"]),
    ("personalNumber", &["personnummer", "personalnumber", "ssn", "pnr", "personnr", "nationalid", "idnumber"]),
    ("company", &["company", "foretag", "företag", "organisation", "organization", "bolag", "firma", "companyname"]),
    ("name", &["name", "namn", "fullname", "fullstandigtnamn", "fullständigtnamn", "contact", "kontakt", "kontaktperson"]),
    ("title", &["title", "titel", "subject", "amne", "ämne", "documenttitle", "dokumenttitel"]),
    (
        "externalId",
        &[
            "externalid",
            "reference",
            "referens",
            "ref",
            "crmid",
            "kundnummer",
            "customerid",
            "ordernumber",
            "ordernummer",
            "avtalsnummer",
            "contractnumber",
            "arendenummer",
            "ärendenummer",
        ],
    ),
];

fn attr_of(header: &str) -> Option<&'static str> {
    SYNONYMS.iter().find(|(_, words)| words.iter().any(|w| header == *w || header.ends_with(w))).map(|(attr, _)| *attr)
}

/// Matches columns by name: a field label, a party attribute (prefixed with the party's name when the
/// template has several, e.g. "Tenant email"), the title or a reference.
pub fn suggest(columns: &[String], targets: &[Target], parties: &[String]) -> Vec<ColumnMapping> {
    let mut used: Vec<String> = Vec::new();
    let mut out = Vec::new();
    for column in columns {
        let header = normalize(column);
        let mut pick = targets
            .iter()
            .find(|t| t.key.starts_with("field.") && normalize(&t.label) == header && !used.contains(&t.key))
            .map(|t| t.key.clone());
        if pick.is_none() {
            pick = parties.iter().position(|p| normalize(p) == header).map(|i| format!("party.{i}.name")).filter(|k| !used.contains(k));
        }
        if pick.is_none() {
            pick = attr_of(&header).and_then(|attr| match attr {
                "title" | "externalId" => Some(attr.to_string()).filter(|k| !used.contains(k)),
                _ => {
                    let named = parties.iter().position(|p| {
                        let p = normalize(p);
                        !p.is_empty() && header.starts_with(&p)
                    });
                    let order: Vec<usize> = match named {
                        Some(i) => vec![i],
                        None => (0..parties.len()).collect(),
                    };
                    order.into_iter().map(|i| format!("party.{i}.{attr}")).find(|k| !used.contains(k))
                }
            });
        }
        if let Some(key) = &pick {
            used.push(key.clone());
        }
        out.push(ColumnMapping { column: column.clone(), target: pick.unwrap_or_default() });
    }
    out
}

const AI_PROMPT: &str = "You match spreadsheet columns to the fields of a signing template. Reply with JSON only: \
{\"mapping\": {\"<column>\": \"<target key>\"}}. Use only the target keys given. Leave out columns that match nothing. \
Use each target at most once. Columns may be in Swedish or English.";

async fn ai_suggest(ctx: &Ctx, conn: &mut PgConnection, sheet: &Sheet, targets: &[Target], mapping: &mut [ColumnMapping]) -> Result<bool> {
    if !ctx.can(Permission::AiUse) {
        return Err(forbidden());
    }
    let s: AiSettings = settings(ctx, conn).await?;
    let mut request = json!({
        "columns": sheet.columns,
        "targets": targets.iter().map(|t| json!({ "key": t.key, "label": format!("{} - {}", t.group, t.label) })).collect::<Vec<_>>(),
        "currentGuess": mapping.iter().filter(|m| !m.target.is_empty()).map(|m| (m.column.clone(), m.target.clone())).collect::<BTreeMap<_, _>>(),
    });
    if s.allow_document_text {
        request["sampleRows"] = json!(sheet.rows.iter().take(3).collect::<Vec<_>>());
    }
    let reply = ai::complete(
        ctx,
        conn,
        &s,
        Prompt { kind: "bulk_mapping", system: AI_PROMPT, messages: &[Message::user(request.to_string())], tools: &[], max_tokens: 2000 },
    )
    .await?;
    let Some(Value::Object(suggested)) = json_in(&reply.text()).and_then(|v| v.get("mapping").cloned()) else {
        return Ok(false);
    };
    let mut used: Vec<String> = Vec::new();
    for m in mapping.iter_mut() {
        if let Some(key) = suggested.get(&m.column).and_then(Value::as_str)
            && targets.iter().any(|t| t.key == key)
            && !used.iter().any(|u| u == key)
        {
            m.target = key.to_string();
        }
        if !m.target.is_empty() {
            used.push(m.target.clone());
        }
    }
    // A target the model gave twice keeps only its first column.
    let mut seen: Vec<String> = Vec::new();
    for m in mapping.iter_mut() {
        if !m.target.is_empty() && seen.contains(&m.target) {
            m.target.clear();
        } else if !m.target.is_empty() {
            seen.push(m.target.clone());
        }
    }
    Ok(true)
}

async fn decode(file: &FileInput) -> Result<Sheet> {
    let name = file.file_name.trim();
    if name.is_empty() || name.len() > 255 {
        return Err(bad_request("The file needs a name"));
    }
    if file.content_base64.len() > sheet::MAX_BYTES / 3 * 4 + 8 {
        return Err(bad_request("The file is larger than 5 MB"));
    }
    let bytes = B64.decode(file.content_base64.trim()).map_err(|_| bad_request("contentBase64 is not valid base64"))?;
    let name = name.to_string();
    tokio::task::spawn_blocking(move || sheet::read(&name, &bytes)).await.map_err(AppError::internal)?
}

fn check_mapping(mapping: &[ColumnMapping], sheet: &Sheet, targets: &[Target]) -> Result<HashMap<String, usize>> {
    let mut by_target = HashMap::new();
    for m in mapping.iter().filter(|m| !m.target.is_empty()) {
        let column =
            sheet.columns.iter().position(|c| *c == m.column).ok_or_else(|| bad_request(format!("There is no column “{}”", m.column)))?;
        if !targets.iter().any(|t| t.key == m.target) {
            return Err(bad_request(format!("“{}” is not something the template can take", m.target)));
        }
        if by_target.insert(m.target.clone(), column).is_some() {
            return Err(bad_request("Map each template field from one column only"));
        }
    }
    if by_target.is_empty() {
        return Err(bad_request("Map at least one column"));
    }
    Ok(by_target)
}

type Values = BTreeMap<String, String>;

fn row_values(row: &[String], by_target: &HashMap<String, usize>) -> Values {
    by_target.iter().map(|(t, &c)| (t.clone(), row.get(c).cloned().unwrap_or_default())).collect()
}

fn party_value<'a>(values: &'a Values, i: usize, attr: &str) -> Option<&'a str> {
    values.get(&format!("party.{i}.{attr}")).map(String::as_str)
}

/// Spreadsheets hold local numbers such as 070-123 45 67; Swedish documents read them as +46.
fn phone(raw: &str, language: &str) -> String {
    let compact: String = raw.chars().filter(|c| !matches!(c, ' ' | '-' | '(' | ')' | '.')).collect();
    match compact.strip_prefix('0') {
        Some(rest) if language == "sv" && !rest.starts_with('0') && !rest.is_empty() => format!("+46{rest}"),
        _ => raw.to_string(),
    }
}

fn problem(code: &str, text: &str, name: &str, value: Option<&str>) -> RowProblem {
    let mut params = BTreeMap::from([("name".to_string(), name.to_string())]);
    if let Some(value) = value {
        params.insert("value".into(), value.into());
    }
    let message = params.iter().fold(text.to_string(), |text, (key, value)| text.replace(&format!("{{{key}}}"), value));
    RowProblem { row: 0, message, code: code.into(), params }
}

fn problems(t: &Template, values: &Values) -> Vec<RowProblem> {
    let mut out = Vec::new();
    for (i, p) in t.parties.iter().enumerate() {
        if party_value(values, i, "name").is_some_and(str::is_empty) {
            out.push(problem("no_name", "{name} has no name", &p.name, None));
        }
        let email = party_value(values, i, "email").map(str::to_string).or_else(|| p.email.clone()).unwrap_or_default();
        let phone = party_value(values, i, "phone").map(|v| phone(v, &t.language)).or_else(|| p.phone.clone()).unwrap_or_default();
        match Delivery::parse(&p.delivery) {
            Some(Delivery::Email) if email.is_empty() => out.push(problem("no_email", "{name} has no email address", &p.name, None)),
            Some(Delivery::Sms) if phone.is_empty() => out.push(problem("no_mobile", "{name} has no mobile number", &p.name, None)),
            _ => {}
        }
        if !email.is_empty() && normalize_email(&email).is_err() {
            out.push(problem("bad_email", "{name}'s email address “{value}” is not valid", &p.name, Some(&email)));
        }
        if !phone.is_empty() && crate::sms::normalize_phone(&phone).is_err() {
            out.push(problem("bad_mobile", "{name}'s mobile number “{value}” is not valid", &p.name, Some(&phone)));
        }
        if let Some(lang) = party_value(values, i, "language").map(str::trim).filter(|l| !l.is_empty())
            && crate::i18n::Lang::parse(lang).is_none()
        {
            out.push(problem("bad_language", "{name}'s language “{value}” is not one OpenSign speaks", &p.name, Some(lang)));
        }
    }
    out
}

fn all_problems(t: &Template, sheet: &Sheet, by_target: &HashMap<String, usize>, limit: usize) -> Vec<RowProblem> {
    let mut out = Vec::new();
    for (i, row) in sheet.rows.iter().enumerate() {
        for problem in problems(t, &row_values(row, by_target)) {
            out.push(RowProblem { row: i + 2, ..problem });
            if out.len() >= limit {
                return out;
            }
        }
    }
    out
}

pub struct Preview;

#[async_trait]
impl Service for Preview {
    type Input = PreviewInput;
    type Output = PreviewOutput;
    const META: Meta = Meta::mutation(
        "bulk.preview",
        "Read a CSV or Excel file for a bulk send from a template: its columns, a few rows, the template fields they can fill, a suggested column mapping and rows that would fail. Sends nothing.",
        Access::Permission(Permission::DocumentsCreate),
    )
    .without_transaction().tool();

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: PreviewInput) -> Result<PreviewOutput> {
        let t = template(ctx, conn, input.file.template_id).await?;
        let sheet = decode(&input.file).await?;
        let targets = targets(&t);
        let names: Vec<String> = t.parties.iter().map(|p| p.name.clone()).collect();
        let mut mapping = suggest(&sheet.columns, &targets, &names);
        let ai_used = input.use_ai && ai_suggest(ctx, conn, &sheet, &targets, &mut mapping).await?;
        let problems = match check_mapping(&mapping, &sheet, &targets) {
            Ok(by_target) => all_problems(&t, &sheet, &by_target, 20),
            Err(_) => Vec::new(),
        };
        Ok(PreviewOutput {
            columns: sheet.columns,
            sample_rows: sheet.rows.iter().take(5).cloned().collect(),
            row_count: sheet.rows.len(),
            targets,
            mapping,
            ai_used,
            problems,
        })
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StartInput {
    #[serde(flatten)]
    pub file: FileInput,
    pub mapping: Vec<ColumnMapping>,
    /// The documents' title, with `{Column}` for a column's value, e.g. "Lease {Address}". Defaults to the
    /// template's title and the first party's name. A mapped title column wins.
    pub title_pattern: Option<String>,
}

#[derive(Serialize, JsonSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct BulkRow {
    pub row_number: i32,
    pub label: String,
    pub status: String,
    pub document_id: Option<Uuid>,
    pub error: Option<String>,
}

#[derive(Serialize, JsonSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct BulkSend {
    pub id: Uuid,
    pub template_id: Option<Uuid>,
    pub template_title: String,
    pub file_name: String,
    pub status: String,
    pub total: i32,
    pub sent: i32,
    pub failed: i32,
    pub created_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    #[sqlx(skip)]
    pub rows: Vec<BulkRow>,
}

fn row_context(bulk: Uuid, row: i32) -> String {
    format!("bulk:{bulk}:row:{row}")
}

pub struct Start;

#[async_trait]
impl Service for Start {
    type Input = StartInput;
    type Output = BulkSend;
    const META: Meta = Meta::mutation(
        "bulk.start",
        "Send a template to every row of a CSV or Excel file, using a column mapping from bulk.preview. Each row becomes its own document; they are created and sent in the background. Refuses the whole file if any row cannot be sent.",
        Access::Permission(Permission::DocumentsCreate),
    ).tool().open_world();

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: StartInput) -> Result<BulkSend> {
        let t = template(ctx, conn, input.file.template_id).await?;
        let sheet = decode(&input.file).await?;
        if sheet.rows.is_empty() {
            return Err(bad_request("The file has no rows below the header"));
        }
        let targets = targets(&t);
        let by_target = check_mapping(&input.mapping, &sheet, &targets)?;
        let problems = all_problems(&t, &sheet, &by_target, 5);
        if !problems.is_empty() {
            let list: Vec<String> = problems.iter().map(|p| format!("row {}: {}", p.row, p.message)).collect();
            return Err(bad_request(format!("Fix these rows first: {}", list.join("; "))));
        }
        let pattern = input.title_pattern.as_deref().unwrap_or("").trim().chars().take(200).collect::<String>();
        let id = Uuid::now_v7();
        let mapping: Vec<&ColumnMapping> = input.mapping.iter().filter(|m| !m.target.is_empty()).collect();
        sqlx::query(
            "insert into bulk_sends (id, template_id, template_title, owner_id, file_name, mapping, title_pattern, total)
             values ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(id)
        .bind(t.id)
        .bind(&t.title)
        .bind(ctx.actor()?.user_id)
        .bind(input.file.file_name.trim())
        .bind(json!(mapping))
        .bind(&pattern)
        .bind(sheet.rows.len() as i32)
        .execute(&mut *conn)
        .await?;
        for (i, row) in sheet.rows.iter().enumerate() {
            let number = i as i32 + 2;
            let values: BTreeMap<String, String> = sheet.columns.iter().cloned().zip(row.iter().cloned()).collect();
            let label = row_values(row, &by_target)
                .get("party.0.name")
                .cloned()
                .unwrap_or_else(|| t.parties.first().map(|p| p.name.clone()).unwrap_or_default());
            let data = ctx.keys().encrypt_str(KeyPurpose::Settings, &row_context(id, number), &json!(values).to_string());
            sqlx::query("insert into bulk_rows (bulk_id, row_number, label, data) values ($1, $2, $3, $4)")
                .bind(id)
                .bind(number)
                .bind(label)
                .bind(data)
                .execute(&mut *conn)
                .await?;
        }
        enqueue(conn, "bulk_send", json!({ "bulk": id }), None, Some(&format!("bulk:{id}"))).await?;
        audit(
            ctx,
            conn,
            Entry::new("bulk.started", "bulk_send", id)
                .metadata(json!({ "template": t.id, "rows": sheet.rows.len(), "file": input.file.file_name.trim() })),
        )
        .await?;
        load(ctx, conn, id).await
    }
}

pub(crate) async fn load(ctx: &Ctx, conn: &mut PgConnection, id: Uuid) -> Result<BulkSend> {
    let actor = ctx.actor()?;
    let mut bulk: BulkSend = sqlx::query_as(
        "select id, template_id, template_title, file_name, status, total, sent, failed, created_at, finished_at from bulk_sends
         where id = $1 and ($2 or owner_id = $3)",
    )
    .bind(id)
    .bind(actor.can(Permission::DocumentsReadAll))
    .bind(actor.user_id)
    .fetch_optional(&mut *conn)
    .await?
    .ok_or_else(|| not_found("Bulk send"))?;
    bulk.rows =
        sqlx::query_as("select row_number, label, status, document_id, error from bulk_rows where bulk_id = $1 order by row_number")
            .bind(id)
            .fetch_all(&mut *conn)
            .await?;
    Ok(bulk)
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BulkIdInput {
    pub id: Uuid,
}

pub struct Get;

#[async_trait]
impl Service for Get {
    type Input = BulkIdInput;
    type Output = BulkSend;
    const META: Meta = Meta::query(
        "bulk.get",
        "A bulk send with the outcome of every row and the documents it created.",
        Access::Permission(Permission::DocumentsRead),
    )
    .tool();

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: BulkIdInput) -> Result<BulkSend> {
        load(ctx, conn, input.id).await
    }
}

pub struct List;

#[async_trait]
impl Service for List {
    type Input = super::Empty;
    type Output = Vec<BulkSend>;
    const META: Meta = Meta::query(
        "bulk.list",
        "Recent bulk sends you started (all of them with permission to read every document).",
        Access::Permission(Permission::DocumentsRead),
    )
    .tool();

    async fn run(ctx: &Ctx, conn: &mut PgConnection, _: super::Empty) -> Result<Vec<BulkSend>> {
        let actor = ctx.actor()?;
        Ok(sqlx::query_as(
            "select id, template_id, template_title, file_name, status, total, sent, failed, created_at, finished_at from bulk_sends
             where $1 or owner_id = $2 order by created_at desc limit 50",
        )
        .bind(actor.can(Permission::DocumentsReadAll))
        .bind(actor.user_id)
        .fetch_all(&mut *conn)
        .await?)
    }
}

pub struct Cancel;

#[async_trait]
impl Service for Cancel {
    type Input = BulkIdInput;
    type Output = BulkSend;
    const META: Meta = Meta::mutation(
        "bulk.cancel",
        "Stop a bulk send: rows not sent yet are skipped. Documents already sent stay out for signing; cancel those one by one.",
        Access::Permission(Permission::DocumentsCreate),
    )
    .tool()
    .destructive();

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: BulkIdInput) -> Result<BulkSend> {
        let bulk = load(ctx, conn, input.id).await?;
        let actor = ctx.actor()?;
        let owner: Option<Uuid> =
            sqlx::query_scalar("select owner_id from bulk_sends where id = $1").bind(bulk.id).fetch_one(&mut *conn).await?;
        if owner != Some(actor.user_id) && !actor.can(Permission::DocumentsManageAll) {
            return Err(forbidden());
        }
        if matches!(bulk.status.as_str(), "done" | "canceled") {
            return Err(conflict("The bulk send has already finished"));
        }
        sqlx::query("update bulk_rows set status = 'canceled', data = null where bulk_id = $1 and status = 'queued'")
            .bind(bulk.id)
            .execute(&mut *conn)
            .await?;
        sqlx::query("update bulk_sends set status = 'canceled', finished_at = $2 where id = $1")
            .bind(bulk.id)
            .bind(ctx.now)
            .execute(&mut *conn)
            .await?;
        audit(ctx, conn, Entry::new("bulk.canceled", "bulk_send", bulk.id)).await?;
        load(ctx, conn, bulk.id).await
    }
}

fn title_for(pattern: &str, template_title: &str, values: &BTreeMap<String, String>, mapped: &Values) -> String {
    if let Some(title) = mapped.get("title").filter(|t| !t.is_empty()) {
        return title.clone();
    }
    if pattern.is_empty() {
        return match mapped.get("party.0.name").filter(|n| !n.is_empty()) {
            Some(name) => format!("{template_title} - {name}"),
            None => template_title.to_string(),
        };
    }
    let mut out = pattern.to_string();
    for (column, value) in values {
        out = out.replace(&format!("{{{column}}}"), value);
    }
    out
}

/// Creates and sends one row's document as the user who started the bulk send.
async fn send_row(ctx: &Ctx, conn: &mut PgConnection, bulk: &BulkJob, values: &BTreeMap<String, String>) -> Result<Uuid> {
    let mapped: Values = bulk.mapping.iter().map(|m| (m.target.clone(), values.get(&m.column).cloned().unwrap_or_default())).collect();
    let template_id = bulk.template_id.ok_or_else(|| not_found("Template"))?;
    let (created, _) = docs::create(
        ctx,
        conn,
        docs::CreateInput {
            title: title_for(&bulk.title_pattern, &bulk.template_title, values, &mapped),
            message: None,
            language: None,
            template_id: Some(template_id),
            is_template: false,
            sequential: None,
            external_id: mapped.get("externalId").filter(|e| !e.is_empty()).cloned(),
            metadata: Some(json!({ "bulkSend": bulk.id })),
            folder_id: None,
        },
        docs::Origin::BulkSend,
    )
    .await?;
    let document_id = created.document.id;
    let rows = parties(conn, document_id).await?;
    let inputs = rows
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let pick =
                |attr: &str, current: Option<&String>| party_value(&mapped, i, attr).map(str::to_string).or_else(|| current.cloned());
            PartyInput {
                id: Some(p.id),
                role: p.role(),
                name: pick("name", Some(&p.name)).unwrap_or_default(),
                email: pick("email", p.email.as_ref()),
                phone: party_value(&mapped, i, "phone").map(|v| phone(v, &bulk.language)).or_else(|| p.phone.clone()),
                personal_number: party_value(&mapped, i, "personalNumber").map(str::to_string),
                company: pick("company", p.company.as_ref()),
                sign_order: Some(p.sign_order),
                delivery: Delivery::parse(&p.delivery).unwrap_or(Delivery::Email),
                auth_to_view: p.auth_to_view(),
                auth_to_sign: p.auth_to_sign(),
                attachments: None,
                language: party_value(&mapped, i, "language")
                    .filter(|l| !l.trim().is_empty())
                    .map(str::to_string)
                    .or_else(|| p.language.clone()),
                webcam_photo: None,
            }
        })
        .collect();
    docs::SetParties::run(ctx, conn, docs::SetPartiesInput { document_id, parties: inputs }).await?;
    if mapped.keys().any(|k| k.starts_with("field.")) {
        let current = fields(conn, document_id).await?;
        let inputs = current
            .into_iter()
            .map(|f| {
                let mut input = FieldInput::from_row(&f)?;
                if f.party_id.is_none()
                    && let Some(value) = mapped.get(&format!("field.{}", f.label))
                {
                    input.value = Some(value.clone());
                }
                Ok(input)
            })
            .collect::<Result<Vec<_>>>()?;
        docs::SetFields::run(ctx, conn, docs::SetFieldsInput { document_id, fields: inputs }).await?;
    }
    docs::Send::run(ctx, conn, docs::SendInput { id: document_id, expires_at: None }).await?;
    Ok(document_id)
}

#[derive(sqlx::FromRow)]
struct BulkJob {
    id: Uuid,
    template_id: Option<Uuid>,
    template_title: String,
    owner_id: Option<Uuid>,
    #[sqlx(json)]
    mapping: Vec<ColumnMapping>,
    title_pattern: String,
    status: String,
    language: String,
}

async fn process_row(ctx: &Ctx, as_user: Option<&Ctx>, bulk: &BulkJob, number: i32, data: Option<&str>) -> Result<Uuid> {
    let user_ctx =
        as_user.ok_or_else(|| AppError::new(ErrorKind::Forbidden, "The user who started the bulk send can no longer send documents"))?;
    let sealed = data.ok_or_else(|| bad_request("The row's data is missing"))?;
    let values: BTreeMap<String, String> = ctx
        .keys()
        .decrypt_str(KeyPurpose::Settings, &row_context(bulk.id, number), sealed)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .ok_or_else(|| bad_request("The row's data could not be read"))?;
    let mut tx = ctx.db().begin().await?;
    let document = send_row(user_ctx, &mut tx, bulk, &values).await?;
    tx.commit().await?;
    Ok(document)
}

/// Works through the next rows of a bulk send, each in its own transaction, then queues itself again.
pub async fn run_job(ctx: &Ctx, bulk_id: Uuid) -> Result<()> {
    let mut conn = ctx.db().acquire().await?;
    let Some(bulk) = sqlx::query_as::<_, BulkJob>(
        "select b.id, b.template_id, b.template_title, b.owner_id, b.mapping, b.title_pattern, b.status, coalesce(d.language, 'en') as language
         from bulk_sends b left join documents d on d.id = b.template_id where b.id = $1",
    )
    .bind(bulk_id)
    .fetch_optional(&mut *conn)
    .await?
    else {
        return Ok(());
    };
    if matches!(bulk.status.as_str(), "done" | "canceled") {
        return Ok(());
    }
    let security: SecuritySettings = settings(ctx, &mut conn).await?;
    let user = match bulk.owner_id {
        Some(id) => user_by_id(&mut conn, id).await?,
        None => None,
    };
    let actor = user.as_ref().and_then(|u| actor_for(u, &security, Credential::Session)).filter(|a| a.can(Permission::DocumentsCreate));
    sqlx::query("update bulk_sends set status = 'running' where id = $1 and status = 'queued'").bind(bulk.id).execute(&mut *conn).await?;
    let rows: Vec<(i32, Option<String>)> =
        sqlx::query_as("select row_number, data from bulk_rows where bulk_id = $1 and status = 'queued' order by row_number limit $2")
            .bind(bulk.id)
            .bind(ROWS_PER_RUN as i64)
            .fetch_all(&mut *conn)
            .await?;
    let as_user = actor.map(|a| ctx.clone().with_actor(a));
    for (number, data) in &rows {
        let outcome = process_row(ctx, as_user.as_ref(), &bulk, *number, data.as_deref()).await;
        if let Err(e) = &outcome
            && e.kind == ErrorKind::Internal
        {
            tracing::error!(bulk = %bulk.id, row = number, error = ?e, "bulk send row failed");
        }
        let mut tx = ctx.db().begin().await?;
        let (status, document, error) = match &outcome {
            Ok(id) => ("sent", Some(*id), None),
            Err(e) => ("failed", None, Some(e.message.clone())),
        };
        sqlx::query("update bulk_rows set status = $3, document_id = $4, error = $5, data = null where bulk_id = $1 and row_number = $2")
            .bind(bulk.id)
            .bind(number)
            .bind(status)
            .bind(document)
            .bind(error)
            .execute(&mut *tx)
            .await?;
        let column = if outcome.is_ok() { "sent" } else { "failed" };
        sqlx::query(&format!("update bulk_sends set {column} = {column} + 1 where id = $1")).bind(bulk.id).execute(&mut *tx).await?;
        tx.commit().await?;
    }
    let left: i64 = sqlx::query_scalar("select count(*) from bulk_rows where bulk_id = $1 and status = 'queued'")
        .bind(bulk.id)
        .fetch_one(&mut *conn)
        .await?;
    if left > 0 {
        enqueue(&mut conn, "bulk_send", json!({ "bulk": bulk.id }), None, None).await?;
    } else {
        sqlx::query("update bulk_sends set status = 'done', finished_at = $2 where id = $1 and status = 'running'")
            .bind(bulk.id)
            .bind(ctx.now)
            .execute(&mut *conn)
            .await?;
        let counts: (i32, i32) =
            sqlx::query_as("select sent, failed from bulk_sends where id = $1").bind(bulk.id).fetch_one(&mut *conn).await?;
        audit(ctx, &mut conn, Entry::new("bulk.finished", "bulk_send", bulk.id).metadata(json!({ "sent": counts.0, "failed": counts.1 })))
            .await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(key: &str, label: &str) -> Target {
        Target { key: key.into(), label: label.into(), group: String::new() }
    }

    #[test]
    fn matches_columns_in_swedish_and_by_party_name() {
        let parties = vec!["Hyresgäst".to_string(), "Borgensman".to_string()];
        let mut targets = vec![target("title", "Title"), target("externalId", "Your Reference"), target("field.Adress", "Adress")];
        for i in 0..2 {
            for (attr, label) in PARTY_ATTRS {
                targets.push(target(&format!("party.{i}.{attr}"), label));
            }
        }
        let columns: Vec<String> =
            ["Hyresgäst", "E-post", "Mobil", "Borgensman namn", "Borgensman e-post", "Adress", "Kundnummer", "Övrigt"]
                .map(String::from)
                .to_vec();
        let got: Vec<String> = suggest(&columns, &targets, &parties).into_iter().map(|m| m.target).collect();
        assert_eq!(
            got,
            ["party.0.name", "party.0.email", "party.0.phone", "party.1.name", "party.1.email", "field.Adress", "externalId", ""]
        );
    }

    #[test]
    fn reads_swedish_local_numbers() {
        assert_eq!(phone("070-123 45 67", "sv"), "+46701234567");
        assert_eq!(phone("+46 70 123 45 67", "sv"), "+46 70 123 45 67");
        assert_eq!(phone("0044 20 1234 5678", "sv"), "0044 20 1234 5678");
        assert_eq!(phone("070-123 45 67", "en"), "070-123 45 67");
    }

    #[test]
    fn titles_from_pattern_or_first_party() {
        let values: BTreeMap<String, String> = [("Adress".to_string(), "Storgatan 1".to_string())].into();
        let mapped: Values = [("party.0.name".to_string(), "Göran".to_string())].into();
        assert_eq!(title_for("", "Hyresavtal", &values, &mapped), "Hyresavtal - Göran");
        assert_eq!(title_for("Hyresavtal {Adress}", "Hyresavtal", &values, &mapped), "Hyresavtal Storgatan 1");
    }
}
