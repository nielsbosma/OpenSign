//! The values filled in on documents as data: one document as typed JSON, many as a table for CSV
//! or Excel. Personal numbers and uploaded file contents never leave this way; uploads are named.

use std::collections::{BTreeMap, HashMap};

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

use super::{DocumentRow, FieldKind, FieldRow, PartyRow};
use crate::error::{AppError, Result};

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PartyData {
    pub id: Uuid,
    /// "Party 1", "Party 2" and so on, in signing order. Field keys and export columns use it.
    pub slot: String,
    pub role: String,
    pub name: String,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub company: Option<String>,
    pub status: String,
    pub signed_at: Option<DateTime<Utc>>,
    pub declined_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FieldData {
    /// The field's label, made unique in the document: a label used by several parties gets the
    /// party's slot, such as "Name (Party 2)". The column name in exports.
    pub key: String,
    pub label: String,
    pub kind: String,
    pub party_id: Option<Uuid>,
    /// The field, or every button of a radio group.
    pub field_ids: Vec<Uuid>,
    /// Numbers as numbers, checkboxes as true or false, several choices as a list, dates as
    /// YYYY-MM-DD, uploads by file name. Null while nothing is filled in.
    #[schemars(with = "Option<serde_json::Value>")]
    pub value: Value,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DocumentData {
    pub id: Uuid,
    pub title: String,
    pub status: String,
    pub external_id: Option<String>,
    pub template_id: Option<Uuid>,
    pub folder_id: Option<Uuid>,
    pub archived_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub sent_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub parties: Vec<PartyData>,
    /// Every field in page order, with its value. Signature fields are left out: `signedAt` on the party says when they signed.
    pub fields: Vec<FieldData>,
    /// The same values keyed by `key`, for quick lookups.
    #[schemars(with = "BTreeMap<String, serde_json::Value>")]
    pub values: BTreeMap<String, Value>,
}

fn default_label(kind: FieldKind) -> &'static str {
    match kind {
        FieldKind::Signature => "Signature",
        FieldKind::Initials => "Initials",
        FieldKind::Text => "Text",
        FieldKind::Date => "Date Signed",
        FieldKind::Checkbox => "Checkbox",
        FieldKind::Name => "Name",
        FieldKind::Email => "Email",
        FieldKind::Company => "Company",
        FieldKind::Number => "Number",
        FieldKind::Select => "Dropdown",
        FieldKind::Multiselect => "Multiple Choice",
        FieldKind::Radio => "Radio",
        FieldKind::Phone => "Phone",
        FieldKind::DateInput => "Date",
        FieldKind::Image => "Image",
        FieldKind::File => "File",
        FieldKind::Formula => "Formula",
    }
}

fn typed(kind: FieldKind, value: Option<&str>) -> Value {
    let Some(v) = value.map(str::trim).filter(|v| !v.is_empty()) else { return Value::Null };
    match kind {
        FieldKind::Number | FieldKind::Formula => {
            serde_json::from_str::<serde_json::Number>(v).map(Value::Number).unwrap_or_else(|_| v.into())
        }
        FieldKind::Checkbox => Value::Bool(v == "true"),
        FieldKind::Multiselect => serde_json::from_str::<Vec<String>>(v).map(Value::from).unwrap_or_else(|_| v.into()),
        _ => v.into(),
    }
}

pub fn document_data(document: &DocumentRow, parties: &[PartyRow], fields: &[FieldRow]) -> DocumentData {
    let slots: HashMap<Uuid, String> = parties.iter().enumerate().map(|(i, p)| (p.id, format!("Party {}", i + 1))).collect();
    let mut entries: Vec<FieldData> = Vec::new();
    let mut groups: HashMap<(Option<Uuid>, &str), usize> = HashMap::new();
    for field in fields {
        let kind = FieldKind::parse(&field.kind).unwrap_or(FieldKind::Text);
        if kind == FieldKind::Signature {
            continue;
        }
        if kind == FieldKind::Radio
            && let Some(group) = field.group_key.as_deref()
        {
            if let Some(&at) = groups.get(&(field.party_id, group)) {
                let entry = &mut entries[at];
                entry.field_ids.push(field.id);
                if entry.label.is_empty() {
                    entry.label = field.label.trim().to_string();
                }
                if entry.value.is_null() {
                    entry.value = typed(kind, field.value.as_deref());
                }
                continue;
            }
            groups.insert((field.party_id, group), entries.len());
        }
        entries.push(FieldData {
            key: String::new(),
            label: field.label.trim().to_string(),
            kind: kind.as_str().to_string(),
            party_id: field.party_id,
            field_ids: vec![field.id],
            value: typed(kind, field.value.as_deref()),
        });
    }
    for entry in &mut entries {
        if entry.label.is_empty() {
            entry.label = default_label(FieldKind::parse(&entry.kind).unwrap_or(FieldKind::Text)).to_string();
        }
    }
    let mut by_label: HashMap<String, Vec<Option<Uuid>>> = HashMap::new();
    for entry in &entries {
        by_label.entry(entry.label.clone()).or_default().push(entry.party_id);
    }
    let mut used: HashMap<String, usize> = HashMap::new();
    for entry in &mut entries {
        let owners = &by_label[&entry.label];
        let mut key = entry.label.clone();
        if owners.len() > 1 && owners.iter().any(|o| *o != entry.party_id) {
            let slot = entry.party_id.and_then(|p| slots.get(&p)).map_or("Sender", String::as_str);
            key = format!("{key} ({slot})");
        }
        let seen = used.entry(key.clone()).or_insert(0);
        *seen += 1;
        if *seen > 1 {
            key = format!("{key} ({seen})");
        }
        entry.key = key;
    }
    let values = entries.iter().map(|e| (e.key.clone(), e.value.clone())).collect();
    DocumentData {
        id: document.id,
        title: document.title.clone(),
        status: document.status.clone(),
        external_id: document.external_id.clone(),
        template_id: document.template_id,
        folder_id: document.folder_id,
        archived_at: document.archived_at,
        created_at: document.created_at,
        sent_at: document.sent_at,
        completed_at: document.completed_at,
        parties: parties
            .iter()
            .map(|p| PartyData {
                id: p.id,
                slot: slots[&p.id].clone(),
                role: p.role.clone(),
                name: p.name.clone(),
                email: p.email.clone(),
                phone: p.phone.clone(),
                company: p.company.clone(),
                status: p.status.clone(),
                signed_at: p.signed_at,
                declined_at: p.declined_at,
            })
            .collect(),
        fields: entries,
        values,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Cell {
    Empty,
    Text(String),
    Number(f64, String),
    Bool(bool),
}

impl Cell {
    fn of(value: &Value) -> Cell {
        match value {
            Value::Null => Cell::Empty,
            Value::Bool(b) => Cell::Bool(*b),
            Value::Number(n) => Cell::Number(n.as_f64().unwrap_or_default(), n.to_string()),
            Value::String(s) => Cell::Text(s.clone()),
            Value::Array(items) => Cell::Text(items.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(", ")),
            Value::Object(_) => Cell::Text(value.to_string()),
        }
    }

    fn text(value: Option<&str>) -> Cell {
        value.filter(|v| !v.is_empty()).map_or(Cell::Empty, |v| Cell::Text(v.to_string()))
    }

    fn time(value: Option<DateTime<Utc>>) -> Cell {
        value.map_or(Cell::Empty, |t| Cell::Text(t.format("%Y-%m-%d %H:%M:%S").to_string()))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Table {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Cell>>,
}

/// One row per document: the document, each party's name, email and signing time, then one
/// column per field key in the order they first appear.
pub fn table(documents: &[DocumentData]) -> Table {
    let mut columns: Vec<String> =
        ["Document ID", "Title", "Reference", "Status", "Created (UTC)", "Sent (UTC)", "Completed (UTC)"].map(String::from).to_vec();
    let slots = documents.iter().map(|d| d.parties.len()).max().unwrap_or(0);
    for i in 1..=slots {
        columns.extend([format!("Party {i} Name"), format!("Party {i} Email"), format!("Party {i} Signed (UTC)")]);
    }
    let fixed = columns.len();
    let mut keys: Vec<&str> = Vec::new();
    for field in documents.iter().flat_map(|d| &d.fields) {
        if !keys.contains(&field.key.as_str()) {
            keys.push(&field.key);
        }
    }
    columns.extend(keys.iter().map(|k| k.to_string()));
    let rows = documents
        .iter()
        .map(|d| {
            let mut row = vec![
                Cell::Text(d.id.to_string()),
                Cell::Text(d.title.clone()),
                Cell::text(d.external_id.as_deref()),
                Cell::Text(d.status.clone()),
                Cell::time(Some(d.created_at)),
                Cell::time(d.sent_at),
                Cell::time(d.completed_at),
            ];
            for i in 0..slots {
                match d.parties.get(i) {
                    Some(p) => row.extend([Cell::Text(p.name.clone()), Cell::text(p.email.as_deref()), Cell::time(p.signed_at)]),
                    None => row.extend([Cell::Empty, Cell::Empty, Cell::Empty]),
                }
            }
            row.extend(keys.iter().map(|k| d.values.get(*k).map_or(Cell::Empty, Cell::of)));
            debug_assert_eq!(row.len(), fixed + keys.len());
            row
        })
        .collect();
    Table { columns, rows }
}

/// A text that a spreadsheet would run as a formula gets a leading apostrophe, so a signer cannot
/// plant `=HYPERLINK(...)` or a DDE call in someone's Excel. Negative numbers stay as they are.
pub fn spreadsheet_safe(value: &str) -> String {
    let risky = value.starts_with(['=', '+', '-', '@', '\t', '\r', '\n', '\u{ff1d}', '\u{ff0b}', '\u{ff0d}', '\u{ff20}']);
    let negative_number = value.strip_prefix('-').is_some_and(|n| {
        let mut parts = n.splitn(2, '.');
        let whole = parts.next().unwrap_or_default();
        !whole.is_empty() && whole.chars().all(|c| c.is_ascii_digit()) && parts.next().is_none_or(|f| f.chars().all(|c| c.is_ascii_digit()))
    });
    if risky && !negative_number { format!("'{value}") } else { value.to_string() }
}

/// UTF-8 with a byte order mark, so Excel reads å, ü and 中 correctly when the file is opened.
pub fn csv(table: &Table) -> Result<Vec<u8>> {
    let mut writer = csv::WriterBuilder::new().terminator(csv::Terminator::CRLF).from_writer(b"\xEF\xBB\xBF".to_vec());
    writer.write_record(table.columns.iter().map(|c| spreadsheet_safe(c))).map_err(AppError::internal)?;
    for row in &table.rows {
        writer
            .write_record(row.iter().map(|cell| match cell {
                Cell::Empty => String::new(),
                Cell::Text(s) => spreadsheet_safe(s),
                Cell::Number(_, s) => s.clone(),
                Cell::Bool(b) => b.to_string(),
            }))
            .map_err(AppError::internal)?;
    }
    writer.into_inner().map_err(|e| AppError::internal(e.into_error()))
}

/// Text is always written as text, never as a formula, so nothing in a cell runs when opened.
pub fn xlsx(table: &Table) -> Result<Vec<u8>> {
    use rust_xlsxwriter::{Format, Workbook};
    let mut workbook = Workbook::new();
    let bold = Format::new().set_bold();
    let sheet = workbook.add_worksheet();
    sheet.set_name("Data").map_err(AppError::internal)?;
    for (c, name) in table.columns.iter().enumerate() {
        sheet.write_string_with_format(0, c as u16, name, &bold).map_err(AppError::internal)?;
    }
    for (r, row) in table.rows.iter().enumerate() {
        let r = r as u32 + 1;
        for (c, cell) in row.iter().enumerate() {
            let c = c as u16;
            match cell {
                Cell::Empty => continue,
                Cell::Text(s) => sheet.write_string(r, c, s),
                Cell::Number(n, _) => sheet.write_number(r, c, *n),
                Cell::Bool(b) => sheet.write_boolean(r, c, *b),
            }
            .map_err(AppError::internal)?;
        }
    }
    sheet.set_freeze_panes(1, 0).map_err(AppError::internal)?;
    sheet.set_autofit_max_width(400);
    sheet.autofit();
    workbook.save_to_buffer().map_err(AppError::internal)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use serde_json::json;

    use super::*;

    fn party(name: &str, order: i32) -> PartyRow {
        PartyRow {
            id: Uuid::now_v7(),
            document_id: Uuid::nil(),
            role: "signer".into(),
            name: name.into(),
            email: Some(format!("{}@example.test", name.to_lowercase())),
            phone: None,
            personal_number: Some("encrypted".into()),
            company: None,
            sign_order: order,
            delivery: "email".into(),
            auth_to_view: "none".into(),
            auth_to_sign: "standard".into(),
            status: "signed".into(),
            link_version: 1,
            link_expires_at: None,
            notified_at: None,
            viewed_at: None,
            signed_at: Some(DateTime::parse_from_rfc3339("2026-10-29T10:00:00Z").unwrap().with_timezone(&Utc)),
            declined_at: None,
            decline_reason: None,
            signature: None,
            evidence: None,
            language: None,
        }
    }

    fn field(party: Option<Uuid>, kind: &str, label: &str, value: Option<&str>) -> FieldRow {
        FieldRow {
            id: Uuid::now_v7(),
            party_id: party,
            kind: kind.into(),
            page: 1,
            x: 0.1,
            y: 0.1,
            width: 0.1,
            height: 0.05,
            required: false,
            label: label.into(),
            value: value.map(String::from),
            options: json!([]),
            validation: json!({}),
            format: None,
            group_key: None,
            read_only: false,
            conditions: json!([]),
            formula: None,
            file_id: None,
        }
    }

    fn document() -> DocumentRow {
        DocumentRow {
            id: Uuid::now_v7(),
            title: "Lease".into(),
            status: "completed".into(),
            is_template: false,
            template_id: None,
            owner_id: None,
            message: String::new(),
            language: "en".into(),
            sequential: false,
            expires_at: None,
            reminder_days: None,
            source_file_id: None,
            sealed_file_id: None,
            original_sha256: None,
            sealed_sha256: None,
            page_count: Some(1),
            external_id: Some("=cmd|' /C calc'!A0".into()),
            metadata: json!({}),
            sent_at: None,
            completed_at: None,
            canceled_at: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            legal_hold: false,
            legal_hold_reason: None,
            legal_hold_at: None,
            review_links: false,
            folder_id: None,
            archived_at: None,
        }
    }

    #[test]
    fn values_are_typed_and_keys_unique() {
        let (anna, bo) = (party("Anna", 1), party("Bo", 2));
        let mut yes = field(Some(anna.id), "radio", "Pets", None);
        yes.group_key = Some("g".into());
        let mut no = field(Some(anna.id), "radio", "", Some("No"));
        no.group_key = Some("g".into());
        let fields = vec![
            field(Some(anna.id), "number", "Rent", Some("-1234.5")),
            field(Some(anna.id), "checkbox", "Agree", Some("true")),
            field(Some(anna.id), "multiselect", "Rooms", Some(r#"["Kitchen","Bath"]"#)),
            field(Some(anna.id), "name", "Name", Some("Anna")),
            field(Some(bo.id), "name", "Name", Some("Bo")),
            field(Some(anna.id), "text", "Note", Some("a")),
            field(Some(anna.id), "text", "Note", None),
            field(Some(anna.id), "signature", "Sign", Some("signed")),
            field(Some(anna.id), "date_input", "", Some("2026-11-01")),
            yes,
            no,
        ];
        let data = document_data(&document(), &[anna, bo], &fields);
        let keys: Vec<&str> = data.fields.iter().map(|f| f.key.as_str()).collect();
        assert_eq!(keys, ["Rent", "Agree", "Rooms", "Name (Party 1)", "Name (Party 2)", "Note", "Note (2)", "Date", "Pets"]);
        assert_eq!(data.values["Rent"], json!(-1234.5));
        assert_eq!(data.values["Agree"], json!(true));
        assert_eq!(data.values["Rooms"], json!(["Kitchen", "Bath"]));
        assert_eq!(data.values["Note (2)"], Value::Null);
        assert_eq!(data.values["Pets"], json!("No"));
        assert_eq!(data.fields.last().unwrap().field_ids.len(), 2);
        let as_json = serde_json::to_string(&data).unwrap();
        assert!(!as_json.contains("encrypted") && !as_json.contains("personalNumber"));
    }

    #[test]
    fn tables_line_up_and_csv_is_safe() {
        let anna = party("Anna", 1);
        let first = document_data(&document(), std::slice::from_ref(&anna), &[field(Some(anna.id), "text", "City", Some("=1+2"))]);
        let second = document_data(&document(), &[], &[field(None, "number", "Amount", Some("-5"))]);
        let t = table(&[first, second]);
        assert_eq!(t.columns.len(), 7 + 3 + 2);
        assert_eq!(t.columns[10..], ["City".to_string(), "Amount".to_string()]);
        assert!(t.rows.iter().all(|r| r.len() == t.columns.len()));
        let text = String::from_utf8(csv(&t).unwrap()).unwrap();
        assert!(text.starts_with('\u{feff}'));
        assert!(text.contains("'=1+2") && text.contains("'=cmd|"));
        assert!(text.contains(",-5\r\n"));
        let book = xlsx(&t).unwrap();
        assert!(book.starts_with(b"PK"));
        let sheet = crate::sheet::read("data.xlsx", &book).unwrap();
        assert_eq!(sheet.rows[0][10], "=1+2");
        assert_eq!(sheet.rows[1][11], "-5");
    }

    #[test]
    fn formula_guard() {
        for (input, out) in [
            ("=SUM(A1)", "'=SUM(A1)"),
            ("+46701234567", "'+46701234567"),
            ("@cmd", "'@cmd"),
            ("-1", "-1"),
            ("-1.25", "-1.25"),
            ("-1+1", "'-1+1"),
            ("Anna", "Anna"),
            ("", ""),
        ] {
            assert_eq!(spreadsheet_safe(input), out, "{input}");
        }
    }
}
