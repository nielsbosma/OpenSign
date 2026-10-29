//! Running Composio automations: the document data a rule can use, filling its arguments, and the
//! run itself. Runs are deliveries in the webhook log, so they share its retries and history.

use std::collections::BTreeSet;
use std::time::Instant;

use serde_json::{Map, Value, json};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::audit::{Entry, audit};
use crate::composio::{self, Composio};
use crate::context::Ctx;
use crate::documents::notify::{Attempt, EXCERPT_BYTES, excerpt_text, record_attempt, sanitize_filename};
use crate::documents::{DOCUMENT_COLUMNS, DocumentRow, FieldKind, fields, parties, status, values};
use crate::error::{AppError, Result, bad_request};
use crate::jobs::Job;
use crate::services::settings;
use crate::settings::AutomationSettings;
use crate::storage::{file_record, read_file};

/// The placeholder that stands for the sealed PDF, as a file a tool can read.
pub const SEALED_PDF: &str = "sealedPdf";
const MAX_PDF_BYTES: i64 = 50 * 1024 * 1024;

/// What a rule's arguments can refer to, besides `fields.<key>` and indexes into `parties`.
pub const PLACEHOLDERS: &[(&str, &str)] = &[
    ("event", "The event, such as document.completed"),
    ("eventTime", "When the event happened, as an ISO time"),
    ("document.id", "The document's id"),
    ("document.title", "The document's title"),
    ("document.reference", "Your own reference (external id), if one was given"),
    ("document.status", "draft, pending, completed, declined, canceled or expired"),
    ("document.url", "A link to the document in OpenSign, for people with access"),
    ("document.createdAt", "When the document was created"),
    ("document.sentAt", "When it was sent"),
    ("document.completedAt", "When the last party signed"),
    ("document.sealedSha256", "The SHA-256 of the sealed PDF"),
    ("party.name", "The party the event is about (signed, viewed, declined)"),
    ("party.email", "Their email address"),
    ("party.phone", "Their phone number"),
    ("party.company", "Their company"),
    ("declineReason", "Why the party declined"),
    ("parties", "Every party's name, or the whole list when it is the only thing in the value"),
    ("owner.name", "The person who sent the document"),
    ("owner.email", "Their email address"),
    (SEALED_PDF, "The sealed PDF as a file, for a tool's file input. Only on document.completed."),
];

/// The data a rule can send for one event. Personal numbers, signatures and evidence are never in it.
pub async fn context(ctx: &Ctx, conn: &mut PgConnection, event: &str, payload: &Value) -> Result<Option<(DocumentRow, Value)>> {
    let data = &payload["data"];
    let Some(id) = data["documentId"].as_str().and_then(|id| Uuid::try_parse(id).ok()) else { return Ok(None) };
    let document: Option<DocumentRow> = sqlx::query_as(&format!("select {DOCUMENT_COLUMNS} from documents d where id = $1"))
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
    let Some(document) = document else { return Ok(None) };
    let all = parties(conn, id).await?;
    let party = |p: &crate::documents::PartyRow| {
        json!({
            "id": p.id, "name": p.name, "email": p.email, "phone": p.phone, "company": p.company, "role": p.role,
            "status": p.status, "signedAt": p.signed_at, "declinedAt": p.declined_at,
        })
    };
    let acting = data["partyId"].as_str().and_then(|id| Uuid::try_parse(id).ok()).and_then(|id| all.iter().find(|p| p.id == id));
    let owner: Option<(String, String)> = match document.owner_id {
        Some(owner) => sqlx::query_as("select name, email from users where id = $1").bind(owner).fetch_optional(&mut *conn).await?,
        None => None,
    };
    let mut values = Map::new();
    for field in fields(conn, id).await? {
        let Some(kind) = FieldKind::parse(&field.kind).filter(|k| *k != FieldKind::Signature) else { continue };
        let label = field.label.trim();
        let Some(value) = field.value.as_deref().filter(|v| !v.trim().is_empty() && !label.is_empty()) else { continue };
        let typed = match kind {
            FieldKind::Number | FieldKind::Formula => serde_json::from_str::<serde_json::Number>(value).map(Value::Number).ok(),
            FieldKind::Checkbox => Some(Value::Bool(value == "true")),
            FieldKind::Multiselect => serde_json::from_str::<Vec<String>>(value).ok().map(Value::from),
            _ => None,
        };
        let text = values::display(kind, field.format.as_deref(), value);
        values.entry(label.to_string()).or_insert_with(|| typed.unwrap_or(Value::String(text)));
    }
    let context = json!({
        "event": event,
        "eventTime": payload["createdAt"],
        "document": {
            "id": document.id,
            "title": document.title,
            "reference": document.external_id,
            "status": document.status,
            "url": format!("{}/documents/{}", ctx.app.public_url(), document.id),
            "createdAt": document.created_at,
            "sentAt": document.sent_at,
            "completedAt": document.completed_at,
            "sealedSha256": document.sealed_sha256,
        },
        "party": acting.map(party),
        "declineReason": acting.and_then(|p| p.decline_reason.clone()),
        "parties": all.iter().map(party).collect::<Vec<_>>(),
        "owner": owner.map(|(name, email)| json!({ "name": name, "email": email })),
        "fields": values,
    });
    Ok(Some((document, context)))
}

fn lookup<'a>(context: &'a Value, path: &str) -> Option<&'a Value> {
    if let Some(key) = path.strip_prefix("fields.") {
        return context["fields"].get(key);
    }
    let mut at = context;
    for part in path.split('.') {
        at = match at {
            Value::Array(items) => items.get(part.parse::<usize>().ok()?)?,
            Value::Object(map) => map.get(part)?,
            _ => return None,
        };
    }
    Some(at)
}

fn as_text(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        Value::Array(items) => items.iter().map(as_text).filter(|s| !s.is_empty()).collect::<Vec<_>>().join(", "),
        Value::Object(map) => map.get("name").map(as_text).unwrap_or_else(|| value.to_string()),
        other => other.to_string(),
    }
}

/// The `{{…}}` placeholders in a string, trimmed.
pub fn placeholders(text: &str) -> Vec<&str> {
    let mut found = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("{{") {
        let Some(end) = rest[start + 2..].find("}}") else { break };
        found.push(rest[start + 2..start + 2 + end].trim());
        rest = &rest[start + 2 + end + 2..];
    }
    found
}

/// Every placeholder used anywhere in the arguments.
pub fn used(arguments: &Value, into: &mut BTreeSet<String>) {
    match arguments {
        Value::String(s) => into.extend(placeholders(s).into_iter().map(str::to_string)),
        Value::Array(items) => items.iter().for_each(|v| used(v, into)),
        Value::Object(map) => map.values().for_each(|v| used(v, into)),
        _ => {}
    }
}

/// Whether a placeholder names something a rule can use.
pub fn known(placeholder: &str) -> bool {
    if placeholder.strip_prefix("fields.").is_some_and(|k| !k.trim().is_empty()) {
        return true;
    }
    if let Some(rest) = placeholder.strip_prefix("parties.") {
        let mut parts = rest.splitn(2, '.');
        let index_ok = parts.next().is_some_and(|i| i.parse::<usize>().is_ok());
        let field_ok = parts.next().is_none_or(|f| matches!(f, "name" | "email" | "phone" | "company" | "role" | "status" | "signedAt"));
        return index_ok && field_ok;
    }
    PLACEHOLDERS.iter().any(|(name, _)| *name == placeholder)
}

/// Fills the placeholders. A value that is only one placeholder takes its value as it is (a number,
/// a list, the sealed PDF file); otherwise each is written into the text.
pub fn render(template: &Value, context: &Value, file: Option<&Value>) -> Value {
    match template {
        Value::String(s) => {
            let found = placeholders(s);
            let whole = found.len() == 1 && s.trim().starts_with("{{") && s.trim().ends_with("}}");
            if whole {
                let name = found[0];
                if name == SEALED_PDF {
                    return file.cloned().unwrap_or(Value::Null);
                }
                return lookup(context, name).cloned().unwrap_or(Value::Null);
            }
            let mut out = String::with_capacity(s.len());
            let mut rest = s.as_str();
            while let Some(start) = rest.find("{{") {
                let Some(end) = rest[start + 2..].find("}}") else { break };
                out.push_str(&rest[..start]);
                let name = rest[start + 2..start + 2 + end].trim();
                out.push_str(&lookup(context, name).map(as_text).unwrap_or_default());
                rest = &rest[start + 2 + end + 2..];
            }
            out.push_str(rest);
            Value::String(out)
        }
        Value::Array(items) => Value::Array(items.iter().map(|v| render(v, context, file)).collect()),
        Value::Object(map) => Value::Object(map.iter().map(|(k, v)| (k.clone(), render(v, context, file))).collect()),
        other => other.clone(),
    }
}

#[derive(sqlx::FromRow)]
struct PendingRun {
    automation_id: Uuid,
    event: String,
    payload: Value,
    name: String,
    toolkit: String,
    tool: String,
    tool_version: Option<String>,
    connected_account_id: String,
    arguments: Value,
    active: bool,
}

fn excerpt(body: &Value) -> Option<String> {
    let text = if let Value::String(s) = body { s.clone() } else { serde_json::to_string_pretty(body).unwrap_or_default() };
    excerpt_text(&text.as_bytes()[..text.len().min(EXCERPT_BYTES)])
}

fn stop(error: impl Into<String>) -> Attempt {
    Attempt { status_code: None, error: Some(error.into()), response: None, duration_ms: 0, retry: false }
}

/// Runs one automation delivery: fills the rule's arguments from the document and calls the tool.
pub async fn run(ctx: &Ctx, conn: &mut PgConnection, delivery_id: Uuid, job: &Job) -> Result<()> {
    let run: Option<PendingRun> = sqlx::query_as(
        "select d.automation_id, d.event, d.payload, a.name, a.toolkit, a.tool, a.tool_version, a.connected_account_id, a.arguments, a.active
         from webhook_deliveries d join automations a on a.id = d.automation_id where d.id = $1 and d.delivered_at is null",
    )
    .bind(delivery_id)
    .fetch_optional(&mut *conn)
    .await?;
    let Some(run) = run else { return Ok(()) };
    if !run.active {
        return Ok(());
    }
    let automations: AutomationSettings = settings(ctx, conn).await?;
    let Some(key) = automations.key() else {
        return record_attempt(ctx, conn, delivery_id, job, stop("Automations are turned off in Settings")).await;
    };
    let Some((document, data)) = context(ctx, conn, &run.event, &run.payload).await? else {
        return record_attempt(ctx, conn, delivery_id, job, stop("The document no longer exists")).await;
    };
    let mut sent = BTreeSet::new();
    used(&run.arguments, &mut sent);
    let composio = Composio::new(&ctx.app, key);
    let started = Instant::now();
    let result = async {
        let file = if sent.contains(SEALED_PDF) {
            let sealed = document.sealed_file_id.filter(|_| document.status == status::COMPLETED);
            let Some(sealed) = sealed else { return Err(bad_request("The sealed PDF exists only once the document is completed")) };
            let record = file_record(conn, sealed).await?;
            if record.size_bytes > MAX_PDF_BYTES {
                return Err(bad_request("The sealed PDF is larger than 50 MB"));
            }
            let bytes = read_file(&ctx.app.storage, ctx.keys(), &record).await?;
            let name = format!("{}.pdf", sanitize_filename(&document.title));
            Some(composio.upload(&run.toolkit, &run.tool, &name, "application/pdf", bytes).await?)
        } else {
            None
        };
        let arguments = render(&run.arguments, &data, file.as_ref());
        let user = composio::user_id(ctx.keys());
        composio.execute(&run.tool, run.tool_version.as_deref(), &run.connected_account_id, &user, arguments).await
    }
    .await;
    let duration_ms = i32::try_from(started.elapsed().as_millis()).unwrap_or(i32::MAX);
    let attempt = match &result {
        Ok(done) => Attempt {
            status_code: Some(i32::from(done.status)),
            error: done.error.clone(),
            response: excerpt(&done.body),
            duration_ms,
            // A refused key or a bad argument stays refused; anything else may pass next time.
            retry: !matches!(done.status, 400 | 401 | 403 | 404 | 422),
        },
        Err(e) => Attempt {
            status_code: None,
            error: Some(e.message.clone()),
            response: None,
            duration_ms,
            retry: !matches!(e.kind, crate::error::ErrorKind::BadRequest | crate::error::ErrorKind::NotFound),
        },
    };
    let error = attempt.error.clone();
    let retry = attempt.retry;
    record_attempt(ctx, conn, delivery_id, job, attempt).await?;
    audit(
        ctx,
        conn,
        Entry::new("automations.run", "document", document.id).metadata(json!({
            "automation": run.automation_id,
            "name": run.name,
            "event": run.event,
            "app": run.toolkit,
            "tool": run.tool,
            "sent": sent,
            "ok": error.is_none(),
        })),
    )
    .await?;
    match error {
        Some(e) if retry => Err(AppError::internal(anyhow::anyhow!("automation run failed: {e}"))),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context() -> Value {
        json!({
            "event": "document.completed",
            "document": { "title": "NDA", "reference": null },
            "parties": [{ "name": "Anna", "email": "anna@example.com" }, { "name": "Bo" }],
            "fields": { "Total": 1250.5, "Name (Party 2)": "Bo B", "Extras": ["A", "B"] },
        })
    }

    #[test]
    fn whole_placeholders_keep_their_type() {
        let c = context();
        assert_eq!(render(&json!("{{fields.Total}}"), &c, None), json!(1250.5));
        assert_eq!(render(&json!(" {{ fields.Extras }} "), &c, None), json!(["A", "B"]));
        assert_eq!(render(&json!("{{parties}}"), &c, None)[1]["name"], "Bo");
        assert_eq!(render(&json!("{{sealedPdf}}"), &c, Some(&json!({ "s3key": "k" }))), json!({ "s3key": "k" }));
        assert_eq!(render(&json!("{{document.reference}}"), &c, None), Value::Null);
    }

    #[test]
    fn placeholders_in_text_are_written_out() {
        let c = context();
        let out = render(&json!({ "text": "Signed: {{document.title}} by {{parties}}, total {{fields.Total}}", "n": 3 }), &c, None);
        assert_eq!(out, json!({ "text": "Signed: NDA by Anna, Bo, total 1250.5", "n": 3 }));
        assert_eq!(render(&json!("{{parties.0.email}} / {{fields.Name (Party 2)}} / {{nothing}}"), &c, None), json!("anna@example.com / Bo B / "));
        assert_eq!(render(&json!("open {{ only"), &c, None), json!("open {{ only"));
    }

    #[test]
    fn only_known_placeholders_are_accepted() {
        for ok in ["document.title", "fields.Any Label", "parties.0", "parties.1.email", "sealedPdf", "owner.email"] {
            assert!(known(ok), "{ok}");
        }
        for bad in ["document.secret", "parties.x", "parties.0.personalNumber", "fields.", "party.personalNumber", ""] {
            assert!(!known(bad), "{bad}");
        }
        let mut found = BTreeSet::new();
        used(&json!({ "a": ["{{document.title}} {{fields.X}}"], "b": "{{sealedPdf}}" }), &mut found);
        assert_eq!(found.into_iter().collect::<Vec<_>>(), ["document.title", "fields.X", "sealedPdf"]);
    }
}
