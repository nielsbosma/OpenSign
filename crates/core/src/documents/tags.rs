//! Turns text tags and PDF form fields found in an upload into fields of the draft. The tag
//! syntax is DocuSeal's, so their templates move over unchanged:
//! `{{Name;role=Tenant;type=signature;required=false}}`.

use opensign_pdf::{FormField, FormKind, TextTag};

use super::FieldKind;
use super::values::Validation;

/// A field read from a tag or a form, before it is given to a party and checked like any other.
#[derive(Debug, Clone, PartialEq)]
pub struct Found {
    pub label: String,
    pub kind: FieldKind,
    /// The party it belongs to, by name. `None` goes to the first party who signs.
    pub role: Option<String>,
    /// Zero-based page within the uploaded file.
    pub page: usize,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub required: bool,
    pub read_only: bool,
    pub value: Option<String>,
    pub options: Vec<String>,
    pub format: Option<String>,
    pub validation: Validation,
    pub group_key: Option<String>,
}

/// Smallest size a field gets when its tag is short, in points.
fn minimum_size(kind: FieldKind) -> (f64, f64) {
    match kind {
        FieldKind::Signature => (120.0, 32.0),
        FieldKind::Initials => (40.0, 24.0),
        FieldKind::Image => (100.0, 60.0),
        FieldKind::Checkbox | FieldKind::Radio => (10.0, 10.0),
        _ => (60.0, 12.0),
    }
}

fn truthy(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" | "yes" | "1" => Some(true),
        "false" | "no" | "0" => Some(false),
        _ => None,
    }
}

fn number_format(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "usd" => "$1,234.00".into(),
        "eur" => "€1,234.00".into(),
        "gbp" => "£1,234.00".into(),
        "sek" => "1 234,00 kr".into(),
        _ => value.trim().to_string(),
    }
}

fn split_options(value: &str) -> Vec<String> {
    let separator = if value.contains('|') { '|' } else { ',' };
    value.split(separator).map(str::trim).filter(|o| !o.is_empty()).map(str::to_string).collect()
}

/// Reads one tag. A tag that cannot be used comes back as a sentence saying why.
pub fn parse_tag(tag: &TextTag) -> Result<Found, String> {
    let quoted = format!("{{{{{}}}}}", tag.text.chars().take(60).collect::<String>());
    let mut parts = tag.text.split(';');
    let first = parts.next().unwrap_or_default();
    let mut attributes: Vec<(String, String)> = Vec::new();
    let mut name = String::new();
    if first.contains('=') {
        attributes.push(split_attribute(first));
    } else {
        name = first.trim().to_string();
    }
    for part in parts.filter(|p| !p.trim().is_empty()) {
        if !part.contains('=') {
            return Err(format!("The tag {quoted} on page {} has \u{201c}{}\u{201d} without a value", tag.page + 1, part.trim()));
        }
        attributes.push(split_attribute(part));
    }
    let get = |key: &str| attributes.iter().rev().find(|(k, _)| k == key).map(|(_, v)| v.as_str());
    let kind = match get("type").map(str::to_ascii_lowercase).as_deref() {
        None | Some("text") => FieldKind::Text,
        Some("signature") => FieldKind::Signature,
        Some("initials") => FieldKind::Initials,
        Some("date") => FieldKind::DateInput,
        Some("datenow") => FieldKind::Date,
        Some("number") => FieldKind::Number,
        Some("select") => FieldKind::Select,
        Some("multiple" | "multiselect") => FieldKind::Multiselect,
        Some("radio") => FieldKind::Radio,
        Some("checkbox") => FieldKind::Checkbox,
        Some("phone") => FieldKind::Phone,
        Some("image") => FieldKind::Image,
        Some("file") => FieldKind::File,
        Some("name") => FieldKind::Name,
        Some("email") => FieldKind::Email,
        Some("company") => FieldKind::Company,
        Some(other) => {
            return Err(format!(
                "The tag {quoted} on page {} has the type \u{201c}{other}\u{201d}, which OpenSign does not have",
                tag.page + 1
            ));
        }
    };
    let label = get("title").map(str::trim).filter(|t| !t.is_empty()).map(str::to_string).unwrap_or(name.clone());
    let mut options = match kind {
        FieldKind::Radio => get("option").or(get("options")).map(split_options).unwrap_or_default(),
        FieldKind::Select | FieldKind::Multiselect => get("options").map(split_options).unwrap_or_default(),
        _ => Vec::new(),
    };
    let mut kind = kind;
    if kind == FieldKind::Radio && options.len() > 1 {
        kind = FieldKind::Select;
    }
    if kind == FieldKind::Radio && options.is_empty() {
        options.push(if name.is_empty() { "1".to_string() } else { name.clone() });
    }
    let group_key = (kind == FieldKind::Radio).then(|| {
        let key = if name.is_empty() { "radio" } else { name.as_str() };
        format!("tag:{}", key.chars().take(58).collect::<String>())
    });
    let format = match kind {
        FieldKind::Number => get("format").map(number_format),
        FieldKind::Date | FieldKind::DateInput => get("format").map(|f| f.trim().to_ascii_uppercase()),
        _ => None,
    };
    let validation = Validation {
        min: get("min").filter(|_| matches!(kind, FieldKind::Number | FieldKind::DateInput)).map(str::to_string),
        max: get("max").filter(|_| matches!(kind, FieldKind::Number | FieldKind::DateInput)).map(str::to_string),
        ..Default::default()
    };
    let points =
        |key: &str| get(key).and_then(|v| v.trim().trim_end_matches("px").parse::<f64>().ok()).filter(|v| v.is_finite() && *v > 0.0);
    let (min_w, min_h) = minimum_size(kind);
    let width = points("width").map_or(tag.width.max(min_w / tag.page_width), |w| w / tag.page_width);
    let height = points("height").map_or(tag.height.max(min_h / tag.page_height), |h| h / tag.page_height);
    // A taller field grows upwards from the tag's baseline, so a signature sits on its line.
    let bottom = tag.y + tag.height;
    let y = (bottom - height).max(0.0);
    let x = tag.x.min(1.0 - width.min(1.0)).max(0.0);
    let value = get("default").map(str::trim).filter(|v| !v.is_empty()).map(|v| match kind {
        FieldKind::Checkbox => truthy(v).unwrap_or(false).to_string(),
        FieldKind::Multiselect => serde_json::to_string(&split_options(v)).unwrap_or_default(),
        FieldKind::Radio => options.first().cloned().filter(|o| truthy(v) == Some(true) || o == v).unwrap_or_default(),
        _ => v.to_string(),
    });
    Ok(Found {
        label,
        kind,
        role: get("role").map(str::trim).filter(|r| !r.is_empty()).map(str::to_string),
        page: tag.page,
        x,
        y,
        width: width.min(1.0 - x),
        height: bottom - y,
        required: get("required").and_then(truthy).unwrap_or(kind != FieldKind::Checkbox),
        read_only: get("readonly").and_then(truthy).unwrap_or(false),
        value: value.filter(|v| !v.is_empty()),
        options,
        format,
        validation,
        group_key,
    })
}

fn split_attribute(part: &str) -> (String, String) {
    let (key, value) = part.split_once('=').unwrap_or((part, ""));
    (key.trim().to_ascii_lowercase(), value.trim().to_string())
}

/// A widget of a PDF form as a field. Radio buttons keep their group by the form field's name.
pub fn from_form(field: &FormField) -> Found {
    let kind = match field.kind {
        FormKind::Text => FieldKind::Text,
        FormKind::Checkbox => FieldKind::Checkbox,
        FormKind::Radio => FieldKind::Radio,
        FormKind::Select => FieldKind::Select,
        FormKind::Multiselect => FieldKind::Multiselect,
        FormKind::Signature => FieldKind::Signature,
    };
    let label: String = field.name.chars().take(200).collect();
    Found {
        group_key: (kind == FieldKind::Radio).then(|| format!("form:{}", label.chars().take(58).collect::<String>())),
        label,
        kind,
        role: None,
        page: field.page,
        x: field.x,
        y: field.y,
        width: field.width,
        height: field.height,
        required: field.required,
        read_only: false,
        value: field.value.clone(),
        options: field.options.clone(),
        format: None,
        validation: Validation::default(),
    }
}

/// The party a form field's name points at: "Tenant.Name" or "Tenant name" belongs to Tenant.
pub fn party_by_prefix<'a>(label: &str, names: impl Iterator<Item = &'a str>) -> Option<&'a str> {
    let lower = label.to_lowercase();
    names.filter(|n| !n.trim().is_empty()).find(|name| {
        let name = name.trim().to_lowercase();
        lower.strip_prefix(&name).is_some_and(|rest| rest.is_empty() || rest.starts_with(|c: char| !c.is_alphanumeric()))
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn tag(text: &str) -> TextTag {
        TextTag { page: 0, text: text.into(), x: 0.1, y: 0.5, width: 0.3, height: 0.015, page_width: 600.0, page_height: 800.0 }
    }

    #[test]
    fn reads_docuseal_tags() {
        let f = parse_tag(&tag("Client Sign;type=signature;role=Client")).unwrap();
        assert_eq!((f.kind, f.label.as_str(), f.role.as_deref(), f.required), (FieldKind::Signature, "Client Sign", Some("Client"), true));
        assert!((f.height - 32.0 / 800.0).abs() < 1e-9 && (f.y + f.height - 0.515).abs() < 1e-9, "grows upwards: {f:?}");

        let f = parse_tag(&tag("Start Date;type=date;format=dd/mm/yyyy;required=false")).unwrap();
        assert_eq!((f.kind, f.format.as_deref(), f.required), (FieldKind::DateInput, Some("DD/MM/YYYY"), false));

        let f = parse_tag(&tag("Region;type=select;options=North,South, East")).unwrap();
        assert_eq!(f.options, vec!["North", "South", "East"]);

        let f = parse_tag(&tag("Plan;type=radio;option=Basic;default=Basic")).unwrap();
        assert_eq!(
            (f.kind, f.options.clone(), f.group_key.as_deref(), f.value.as_deref()),
            (FieldKind::Radio, vec!["Basic".to_string()], Some("tag:Plan"), Some("Basic"))
        );

        let f = parse_tag(&tag("type=checkbox")).unwrap();
        assert_eq!((f.kind, f.label.as_str(), f.required), (FieldKind::Checkbox, "", false));

        let f = parse_tag(&tag("Amount;type=number;format=usd;min=0;max=100;width=50;height=20")).unwrap();
        assert_eq!(
            (f.format.as_deref(), f.validation.min.as_deref(), f.validation.max.as_deref()),
            (Some("$1,234.00"), Some("0"), Some("100"))
        );
        assert!((f.width - 50.0 / 600.0).abs() < 1e-9 && (f.height - 20.0 / 800.0).abs() < 1e-9);
        assert!(super::super::values::NumberFormat::parse(f.format.as_deref().unwrap()).is_some());

        let f = parse_tag(&tag("Date;type=datenow;readonly=true;title=Signed on")).unwrap();
        assert_eq!((f.kind, f.label.as_str(), f.read_only), (FieldKind::Date, "Signed on", true));
    }

    #[test]
    fn explains_tags_it_cannot_use() {
        assert!(parse_tag(&tag("Pay;type=payment")).unwrap_err().contains("payment"));
        assert!(parse_tag(&tag("Name;required")).unwrap_err().contains("without a value"));
        let f = parse_tag(&tag("Size;type=radio;options=S|M|L")).unwrap();
        assert_eq!((f.kind, f.options.len()), (FieldKind::Select, 3), "several options on one radio tag become a list");
    }

    #[test]
    fn matches_parties_by_prefix() {
        let names = ["Tenant", "Landlord"];
        assert_eq!(party_by_prefix("Tenant.Name", names.iter().copied()), Some("Tenant"));
        assert_eq!(party_by_prefix("landlord signature", names.iter().copied()), Some("Landlord"));
        assert_eq!(party_by_prefix("Tenants", names.iter().copied()), None);
    }
}
