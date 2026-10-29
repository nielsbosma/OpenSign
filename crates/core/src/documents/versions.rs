//! What a template version holds, and how it is read from and written to a document.

use std::collections::HashMap;

use opensign_crypto::{KeyPurpose, Keys};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use super::{DOCUMENT_COLUMNS, DocumentRow, decrypt_personal_number, fields, files, mask, parties, personal_number_context};
use crate::context::Ctx;
use crate::error::{AppError, Result};

/// Everything a document made from the template copies: its settings, files, parties with their
/// attachment requests, and fields.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Content {
    pub title: String,
    pub message: String,
    pub language: String,
    pub sequential: bool,
    pub reminder_days: Option<i32>,
    pub review_links: bool,
    pub files: Vec<ContentFile>,
    pub parties: Vec<ContentParty>,
    pub fields: Vec<ContentField>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ContentFile {
    pub id: Uuid,
    pub name: String,
    pub sha256: String,
    pub size_bytes: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ContentParty {
    pub role: String,
    pub name: String,
    pub email: Option<String>,
    pub phone: Option<String>,
    /// Masked when shown; encrypted when stored.
    pub personal_number: Option<String>,
    pub company: Option<String>,
    pub sign_order: i32,
    pub delivery: String,
    pub auth_to_view: String,
    pub auth_to_sign: String,
    pub language: Option<String>,
    pub attachments: Vec<ContentAttachment>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct ContentAttachment {
    #[serde(skip)]
    #[schemars(skip)]
    pub party_id: Uuid,
    pub kind: String,
    pub position: i32,
    pub name: String,
    pub description: String,
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ContentField {
    /// The index of its party in `parties`; none for a field the sender fills in.
    pub party: Option<usize>,
    pub kind: String,
    pub page: i32,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub required: bool,
    pub label: String,
    pub value: Option<String>,
    #[schemars(with = "Vec<String>")]
    pub options: Value,
    #[schemars(with = "super::values::Validation")]
    pub validation: Value,
    pub format: Option<String>,
    pub group_key: Option<String>,
    pub read_only: bool,
    #[schemars(with = "Vec<super::logic::Condition>")]
    pub conditions: Value,
    pub formula: Option<String>,
}

pub async fn load(ctx: &Ctx, conn: &mut PgConnection, document_id: Uuid) -> Result<Content> {
    let document: DocumentRow =
        sqlx::query_as(&format!("select {DOCUMENT_COLUMNS} from documents where id = $1")).bind(document_id).fetch_one(&mut *conn).await?;
    let party_rows = parties(conn, document_id).await?;
    let index: HashMap<Uuid, usize> = party_rows.iter().enumerate().map(|(i, p)| (p.id, i)).collect();
    let requests: Vec<ContentAttachment> = sqlx::query_as(
        "select party_id, kind, position, name, description, required from attachment_requests where document_id = $1 order by party_id, position",
    )
    .bind(document_id)
    .fetch_all(&mut *conn)
    .await?;
    let parties = party_rows
        .iter()
        .map(|p| ContentParty {
            role: p.role.clone(),
            name: p.name.clone(),
            email: p.email.clone(),
            phone: p.phone.clone(),
            personal_number: decrypt_personal_number(ctx.keys(), p),
            company: p.company.clone(),
            sign_order: p.sign_order,
            delivery: p.delivery.clone(),
            auth_to_view: p.auth_to_view.clone(),
            auth_to_sign: p.auth_to_sign.clone(),
            language: p.language.clone(),
            attachments: requests.iter().filter(|r| r.party_id == p.id).cloned().collect(),
        })
        .collect();
    let fields = fields(conn, document_id)
        .await?
        .into_iter()
        .map(|f| ContentField {
            party: f.party_id.and_then(|p| index.get(&p).copied()),
            value: if f.file_id.is_some() { None } else { f.value },
            kind: f.kind,
            page: f.page,
            x: f.x,
            y: f.y,
            width: f.width,
            height: f.height,
            required: f.required,
            label: f.label,
            options: f.options,
            validation: f.validation,
            format: f.format,
            group_key: f.group_key,
            read_only: f.read_only,
            conditions: f.conditions,
            formula: f.formula,
        })
        .collect();
    let files = files(conn, document_id)
        .await?
        .into_iter()
        .map(|f| ContentFile { id: f.id, name: f.name, sha256: f.sha256, size_bytes: f.size_bytes })
        .collect();
    Ok(Content {
        title: document.title,
        message: document.message,
        language: document.language,
        sequential: document.sequential,
        reminder_days: document.reminder_days,
        review_links: document.review_links,
        files,
        parties,
        fields,
    })
}

/// Adds the content's files, parties, attachment requests and fields to a draft. Settings are the
/// caller's business, since a new document and a restored template treat them differently.
pub async fn apply(ctx: &Ctx, conn: &mut PgConnection, content: &Content, to: Uuid) -> Result<()> {
    for (position, file) in content.files.iter().enumerate() {
        sqlx::query("insert into document_files (document_id, file_id, position) values ($1, $2, $3)")
            .bind(to)
            .bind(file.id)
            .bind(position as i32 + 1)
            .execute(&mut *conn)
            .await?;
    }
    let mut ids = Vec::with_capacity(content.parties.len());
    for p in &content.parties {
        let id = Uuid::now_v7();
        ids.push(id);
        sqlx::query(
            "insert into parties (id, document_id, role, name, email, phone, personal_number, company, sign_order, delivery, auth_to_view, auth_to_sign, language)
             values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)",
        )
        .bind(id)
        .bind(to)
        .bind(&p.role)
        .bind(&p.name)
        .bind(&p.email)
        .bind(&p.phone)
        .bind(p.personal_number.as_deref().map(|n| ctx.keys().encrypt_str(KeyPurpose::Settings, &personal_number_context(id), n)))
        .bind(&p.company)
        .bind(p.sign_order)
        .bind(&p.delivery)
        .bind(&p.auth_to_view)
        .bind(&p.auth_to_sign)
        .bind(&p.language)
        .execute(&mut *conn)
        .await?;
        for r in &p.attachments {
            sqlx::query(
                "insert into attachment_requests (id, document_id, party_id, position, kind, name, description, required)
                 values ($1, $2, $3, $4, $5, $6, $7, $8)",
            )
            .bind(Uuid::now_v7())
            .bind(to)
            .bind(id)
            .bind(r.position)
            .bind(&r.kind)
            .bind(&r.name)
            .bind(&r.description)
            .bind(r.required)
            .execute(&mut *conn)
            .await?;
        }
    }
    for f in &content.fields {
        sqlx::query(
            "insert into fields (id, document_id, party_id, kind, page, x, y, width, height, required, label, value, options, validation, format, group_key, read_only, conditions, formula)
             values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19)",
        )
        .bind(Uuid::now_v7())
        .bind(to)
        .bind(f.party.and_then(|i| ids.get(i).copied()))
        .bind(&f.kind)
        .bind(f.page)
        .bind(f.x)
        .bind(f.y)
        .bind(f.width)
        .bind(f.height)
        .bind(f.required)
        .bind(&f.label)
        .bind(&f.value)
        .bind(&f.options)
        .bind(&f.validation)
        .bind(&f.format)
        .bind(&f.group_key)
        .bind(f.read_only)
        .bind(&f.conditions)
        .bind(&f.formula)
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

/// Removes a draft's files, parties (with their requests) and fields.
pub async fn clear(conn: &mut PgConnection, document_id: Uuid) -> Result<()> {
    sqlx::query("delete from fields where document_id = $1").bind(document_id).execute(&mut *conn).await?;
    sqlx::query("delete from parties where document_id = $1").bind(document_id).execute(&mut *conn).await?;
    sqlx::query("delete from document_files where document_id = $1").bind(document_id).execute(&mut *conn).await?;
    Ok(())
}

fn number_context(version_id: Uuid, party: usize) -> String {
    format!("template_version:{version_id}:party:{party}:personal_number")
}

/// The stored form: personal numbers encrypted to this version and party.
pub fn seal(keys: &Keys, version_id: Uuid, content: &Content) -> Result<Value> {
    let mut sealed = content.clone();
    for (i, p) in sealed.parties.iter_mut().enumerate() {
        p.personal_number = p.personal_number.take().map(|n| keys.encrypt_str(KeyPurpose::Settings, &number_context(version_id, i), &n));
    }
    serde_json::to_value(sealed).map_err(AppError::internal)
}

pub fn open(keys: &Keys, version_id: Uuid, stored: Value) -> Result<Content> {
    let mut content: Content = serde_json::from_value(stored).map_err(AppError::internal)?;
    for (i, p) in content.parties.iter_mut().enumerate() {
        p.personal_number = match p.personal_number.take() {
            Some(n) => {
                Some(keys.decrypt_str(KeyPurpose::Settings, &number_context(version_id, i), &n).map_err(AppError::internal)?.to_string())
            }
            None => None,
        };
    }
    Ok(content)
}

/// The content as people see it: personal numbers masked.
pub fn masked(mut content: Content) -> Content {
    for p in &mut content.parties {
        p.personal_number = p.personal_number.as_deref().map(mask);
    }
    content
}

#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Change {
    /// added, removed or changed.
    pub change: String,
    /// setting, file, party or field.
    pub item: String,
    /// The setting's name (title, message, language, sequential, reminderDays, reviewLinks), the
    /// file or party name, or the field's label (its kind when it has none).
    pub name: String,
    /// For a field, the name of its party; none for a field the sender fills in.
    pub party: Option<String>,
    /// For a changed party or field, what changed, such as email, position or required.
    pub details: Vec<String>,
}

fn change(change: &str, item: &str, name: &str, party: Option<String>, details: Vec<String>) -> Change {
    Change { change: change.into(), item: item.into(), name: name.into(), party, details }
}

fn differing(pairs: &[(&str, bool)]) -> Vec<String> {
    pairs.iter().filter(|(_, same)| !same).map(|(name, _)| name.to_string()).collect()
}

/// Pairs items with the same key in order of appearance; the rest were added or removed.
fn pair<'a, T, K: Eq + std::hash::Hash>(from: &'a [T], to: &'a [T], key: impl Fn(&T) -> K) -> Vec<(Option<&'a T>, Option<&'a T>)> {
    let mut waiting: HashMap<K, std::collections::VecDeque<&T>> = HashMap::new();
    for item in to {
        waiting.entry(key(item)).or_default().push_back(item);
    }
    let mut pairs = Vec::new();
    for item in from {
        let matched = waiting.get_mut(&key(item)).and_then(|q| q.pop_front());
        pairs.push((Some(item), matched));
    }
    for item in to {
        if !pairs.iter().any(|(_, t)| t.is_some_and(|t| std::ptr::eq(t, item))) {
            pairs.push((None, Some(item)));
        }
    }
    pairs
}

/// What changed from one version to another, in terms a person can read.
pub fn diff(from: &Content, to: &Content) -> Vec<Change> {
    let mut changes = Vec::new();
    let settings = [
        ("title", from.title == to.title),
        ("message", from.message == to.message),
        ("language", from.language == to.language),
        ("sequential", from.sequential == to.sequential),
        ("reminderDays", from.reminder_days == to.reminder_days),
        ("reviewLinks", from.review_links == to.review_links),
    ];
    for name in differing(&settings) {
        changes.push(change("changed", "setting", &name, None, vec![]));
    }
    for (a, b) in pair(&from.files, &to.files, |f| f.id) {
        match (a, b) {
            (Some(f), None) => changes.push(change("removed", "file", &f.name, None, vec![])),
            (None, Some(f)) => changes.push(change("added", "file", &f.name, None, vec![])),
            _ => {}
        }
    }
    let order_changed = from.files.iter().map(|f| f.id).filter(|id| to.files.iter().any(|f| f.id == *id)).collect::<Vec<_>>()
        != to.files.iter().map(|f| f.id).filter(|id| from.files.iter().any(|f| f.id == *id)).collect::<Vec<_>>();
    if order_changed {
        changes.push(change("changed", "setting", "fileOrder", None, vec![]));
    }
    for (a, b) in pair(&from.parties, &to.parties, |p| p.name.to_lowercase()) {
        match (a, b) {
            (Some(p), None) => changes.push(change("removed", "party", &p.name, None, vec![])),
            (None, Some(p)) => changes.push(change("added", "party", &p.name, None, vec![])),
            (Some(a), Some(b)) => {
                let details = differing(&[
                    ("name", a.name == b.name),
                    ("role", a.role == b.role),
                    ("email", a.email == b.email),
                    ("phone", a.phone == b.phone),
                    ("personalNumber", a.personal_number == b.personal_number),
                    ("company", a.company == b.company),
                    ("signOrder", a.sign_order == b.sign_order),
                    ("delivery", a.delivery == b.delivery),
                    ("authToView", a.auth_to_view == b.auth_to_view),
                    ("authToSign", a.auth_to_sign == b.auth_to_sign),
                    ("language", a.language == b.language),
                    ("attachments", a.attachments == b.attachments),
                ]);
                if !details.is_empty() {
                    changes.push(change("changed", "party", &b.name, None, details));
                }
            }
            (None, None) => {}
        }
    }
    let party_name = |content: &Content, f: &ContentField| f.party.and_then(|i| content.parties.get(i)).map(|p| p.name.clone());
    let from_keyed: Vec<_> = from.fields.iter().map(|f| (party_name(from, f).map(|n| n.to_lowercase()), f)).collect();
    let to_keyed: Vec<_> = to.fields.iter().map(|f| (party_name(to, f).map(|n| n.to_lowercase()), f)).collect();
    for (a, b) in pair(&from_keyed, &to_keyed, |(party, f)| (party.clone(), f.kind.clone(), f.label.clone())) {
        let name = |f: &ContentField| if f.label.is_empty() { f.kind.clone() } else { f.label.clone() };
        match (a, b) {
            (Some((_, f)), None) => changes.push(change("removed", "field", &name(f), party_name(from, f), vec![])),
            (None, Some((_, f))) => changes.push(change("added", "field", &name(f), party_name(to, f), vec![])),
            (Some((_, a)), Some((_, b))) => {
                let details = differing(&[
                    ("position", (a.page, a.x, a.y, a.width, a.height) == (b.page, b.x, b.y, b.width, b.height)),
                    ("required", a.required == b.required),
                    ("value", a.value == b.value),
                    ("options", a.options == b.options),
                    ("validation", a.validation == b.validation),
                    ("format", a.format == b.format),
                    ("group", a.group_key == b.group_key),
                    ("readOnly", a.read_only == b.read_only),
                    ("conditions", a.conditions == b.conditions),
                    ("formula", a.formula == b.formula),
                ]);
                if !details.is_empty() {
                    changes.push(change("changed", "field", &name(b), party_name(to, b), details));
                }
            }
            (None, None) => {}
        }
    }
    changes
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::json;

    fn party(name: &str) -> ContentParty {
        ContentParty {
            role: "signer".into(),
            name: name.into(),
            email: None,
            phone: None,
            personal_number: Some("19800101-1234".into()),
            company: None,
            sign_order: 1,
            delivery: "email".into(),
            auth_to_view: "none".into(),
            auth_to_sign: "standard".into(),
            language: None,
            attachments: vec![],
        }
    }

    fn field(party: Option<usize>, kind: &str, label: &str) -> ContentField {
        ContentField {
            party,
            kind: kind.into(),
            page: 0,
            x: 0.1,
            y: 0.1,
            width: 0.2,
            height: 0.05,
            required: true,
            label: label.into(),
            value: None,
            options: json!([]),
            validation: json!({}),
            format: None,
            group_key: None,
            read_only: false,
            conditions: json!([]),
            formula: None,
        }
    }

    fn content() -> Content {
        Content {
            title: "Lease".into(),
            message: String::new(),
            language: "en".into(),
            sequential: false,
            reminder_days: None,
            review_links: true,
            files: vec![ContentFile { id: Uuid::nil(), name: "lease.pdf".into(), sha256: "x".into(), size_bytes: 1 }],
            parties: vec![party("Tenant"), party("Landlord")],
            fields: vec![field(Some(0), "signature", ""), field(Some(1), "signature", ""), field(Some(0), "text", "Address")],
        }
    }

    #[test]
    fn identical_content_has_no_changes() {
        assert!(diff(&content(), &content()).is_empty());
    }

    #[test]
    fn reports_what_changed_by_name() {
        let before = content();
        let mut after = content();
        after.title = "Lease 2026".into();
        after.parties[0].email = Some("t@example.com".into());
        after.parties.remove(1);
        after.fields = vec![field(Some(0), "signature", ""), field(Some(0), "text", "Address"), field(None, "number", "Rent")];
        after.fields[1].y = 0.5;
        after.fields[1].required = false;
        let changes = diff(&before, &after);
        let summary: Vec<String> = changes.iter().map(|c| format!("{} {} {} {:?}", c.change, c.item, c.name, c.details)).collect();
        assert_eq!(
            summary,
            [
                "changed setting title []",
                "changed party Tenant [\"email\"]",
                "removed party Landlord []",
                "removed field signature []",
                "changed field Address [\"position\", \"required\"]",
                "added field Rent []",
            ]
        );
        assert_eq!(changes[3].party.as_deref(), Some("Landlord"));
    }

    #[test]
    fn personal_numbers_are_sealed_to_the_version() {
        let keys = Keys::derive(&"k".repeat(40)).unwrap();
        let id = Uuid::now_v7();
        let stored = seal(&keys, id, &content()).unwrap();
        assert!(!stored.to_string().contains("19800101"));
        assert_eq!(open(&keys, id, stored.clone()).unwrap(), content());
        assert!(open(&keys, Uuid::now_v7(), stored).is_err());
        assert_eq!(masked(content()).parties[0].personal_number.as_deref(), Some("•••••••••1234"));
    }
}
