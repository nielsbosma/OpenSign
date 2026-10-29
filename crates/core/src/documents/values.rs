//! What a field may hold, and how its value is written on the page. The signing page checks the
//! same rules as the signer types (`web/src/lib/field-values.ts`); both run the cases in
//! `web/src/lib/field-values.cases.json`. The server's check on submit is the one that counts.

use chrono::NaiveDate;
use regex::{Regex, RegexBuilder};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{FieldKind, FieldRow};
use crate::error::{Result, bad_request};

pub const MAX_OPTIONS: usize = 100;
const MAX_OPTION_CHARS: usize = 200;
const MAX_PATTERN_CHARS: usize = 300;
const MAX_MESSAGE_CHARS: usize = 200;
const MAX_TEXT_CHARS: u32 = 1000;
const MAX_DIGITS: usize = 15;

/// The date formats a date field can use. The first is the default.
pub const DATE_FORMATS: &[(&str, &str)] = &[
    ("YYYY-MM-DD", "%Y-%m-%d"),
    ("DD/MM/YYYY", "%d/%m/%Y"),
    ("MM/DD/YYYY", "%m/%d/%Y"),
    ("DD.MM.YYYY", "%d.%m.%Y"),
    ("D MMM YYYY", "%-d %b %Y"),
];

/// Checks on what a signer types. Each applies only to the kinds it makes sense for.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Validation {
    /// A regular expression the whole value must match (text, initials and company fields), such as `[0-9]{3} ?[0-9]{2}` for a Swedish postcode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
    /// Shown to the signer instead of the built-in error when any check fails.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// Fewest characters (text, initials and company fields).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_length: Option<u32>,
    /// Most characters (text, initials and company fields).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_length: Option<u32>,
    /// Smallest number for a number field, or earliest date as YYYY-MM-DD for a date picker.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<String>,
    /// Largest number for a number field, or latest date as YYYY-MM-DD for a date picker.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<String>,
}

impl Validation {
    pub fn is_empty(&self) -> bool {
        *self == Validation::default()
    }
}

/// Why a value was refused. The codes are shared with the signing page, which words them in the
/// signer's language.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Invalid {
    Number,
    Phone,
    Date,
    Option,
    TooShort(u32),
    TooLong(u32),
    Below(String),
    Above(String),
    Pattern,
}

impl Invalid {
    pub fn code(&self) -> &'static str {
        match self {
            Invalid::Number => "number",
            Invalid::Phone => "phone",
            Invalid::Date => "date",
            Invalid::Option => "option",
            Invalid::TooShort(_) => "tooShort",
            Invalid::TooLong(_) => "tooLong",
            Invalid::Below(_) => "below",
            Invalid::Above(_) => "above",
            Invalid::Pattern => "pattern",
        }
    }

    fn english(&self, label: &str) -> String {
        match self {
            Invalid::Number => format!("\u{201c}{label}\u{201d} must be a number"),
            Invalid::Phone => format!("Write \u{201c}{label}\u{201d} with its country code, such as +46 70 123 45 67"),
            Invalid::Date => format!("\u{201c}{label}\u{201d} must be a date"),
            Invalid::Option => format!("Choose \u{201c}{label}\u{201d} from the list"),
            Invalid::TooShort(n) => format!("\u{201c}{label}\u{201d} needs at least {n} characters"),
            Invalid::TooLong(n) => format!("\u{201c}{label}\u{201d} can have at most {n} characters"),
            Invalid::Below(min) => format!("\u{201c}{label}\u{201d} must be at least {min}"),
            Invalid::Above(max) => format!("\u{201c}{label}\u{201d} must be at most {max}"),
            Invalid::Pattern => format!("\u{201c}{label}\u{201d} is not in the expected format"),
        }
    }
}

/// How a field is set up, apart from its place on the page.
#[derive(Debug, Clone, Default)]
pub struct Spec {
    pub options: Vec<String>,
    pub validation: Validation,
    pub format: Option<String>,
}

impl Spec {
    pub fn of(field: &FieldRow) -> Spec {
        Spec {
            options: options_of(&field.options),
            validation: serde_json::from_value(field.validation.clone()).unwrap_or_default(),
            format: field.format.clone(),
        }
    }
}

pub fn options_of(value: &Value) -> Vec<String> {
    value.as_array().into_iter().flatten().filter_map(|o| o.as_str().map(str::to_string)).collect()
}

/// The error a signer or sender sees, in English: the sender's own text if they wrote one.
pub fn error_message(spec: &Spec, label: &str, invalid: &Invalid) -> String {
    match spec.validation.message.as_deref().map(str::trim).filter(|m| !m.is_empty()) {
        Some(message) => message.to_string(),
        None => invalid.english(label),
    }
}

fn text_kind(kind: FieldKind) -> bool {
    matches!(kind, FieldKind::Text | FieldKind::Initials | FieldKind::Company)
}

fn has_options(kind: FieldKind) -> bool {
    matches!(kind, FieldKind::Select | FieldKind::Multiselect | FieldKind::Radio)
}

fn clean_line(value: &str) -> String {
    value.trim().replace(['\n', '\r', '\t'], " ")
}

fn compile(pattern: &str) -> Option<Regex> {
    RegexBuilder::new(&format!("^(?:{pattern})$")).size_limit(1 << 20).dfa_size_limit(1 << 20).nest_limit(50).build().ok()
}

/// Checks how a sender set up a field, and returns its options cleaned. Called when fields are
/// saved, so a broken pattern or an impossible range never reaches a signer.
pub fn check_spec(kind: FieldKind, spec: &Spec, group_key: Option<&str>) -> Result<Vec<String>> {
    let mut options = Vec::with_capacity(spec.options.len());
    for option in &spec.options {
        let option = clean_line(option);
        if option.is_empty() || option.chars().count() > MAX_OPTION_CHARS || option.chars().any(char::is_control) {
            return Err(bad_request(format!("Each option needs a name of at most {MAX_OPTION_CHARS} characters")));
        }
        if options.contains(&option) {
            return Err(bad_request(format!("The option \u{201c}{option}\u{201d} is listed twice")));
        }
        options.push(option);
    }
    match kind {
        FieldKind::Select | FieldKind::Multiselect if options.is_empty() || options.len() > MAX_OPTIONS => {
            return Err(bad_request(format!("A list field needs from 1 to {MAX_OPTIONS} options")));
        }
        FieldKind::Radio if options.len() != 1 => return Err(bad_request("Each radio button holds exactly one option")),
        _ if !has_options(kind) && !options.is_empty() => return Err(bad_request("Only list and radio fields have options")),
        _ => {}
    }
    match (kind, group_key.map(str::trim)) {
        (FieldKind::Radio, Some(key)) if !key.is_empty() && key.len() <= 64 => {}
        (FieldKind::Radio, _) => return Err(bad_request("Each radio button needs a group of at most 64 characters")),
        (_, Some(_)) => return Err(bad_request("Only radio buttons belong to a group")),
        _ => {}
    }
    match (kind, spec.format.as_deref()) {
        (_, None) => {}
        (FieldKind::Date | FieldKind::DateInput, Some(f)) if DATE_FORMATS.iter().any(|(name, _)| *name == f) => {}
        (FieldKind::Date | FieldKind::DateInput, Some(_)) => {
            let names: Vec<&str> = DATE_FORMATS.iter().map(|(name, _)| *name).collect();
            return Err(bad_request(format!("A date format is one of {}", names.join(", "))));
        }
        (FieldKind::Number | FieldKind::Formula, Some(f)) if NumberFormat::parse(f).is_some() => {}
        (FieldKind::Number | FieldKind::Formula, Some(_)) => {
            return Err(bad_request("Write a number format as 1234 would look, such as 1,234.00 or 1 234,00 kr"));
        }
        _ => return Err(bad_request("Only date, number and formula fields have a format")),
    }

    let v = &spec.validation;
    if v.message.as_deref().is_some_and(|m| m.chars().count() > MAX_MESSAGE_CHARS || m.chars().any(char::is_control)) {
        return Err(bad_request(format!("Keep the error text under {MAX_MESSAGE_CHARS} characters")));
    }
    if (v.pattern.is_some() || v.min_length.is_some() || v.max_length.is_some()) && !text_kind(kind) {
        return Err(bad_request("Only text, initials and company fields take a pattern or a length"));
    }
    if let Some(pattern) = &v.pattern {
        if pattern.is_empty() || pattern.chars().count() > MAX_PATTERN_CHARS {
            return Err(bad_request(format!("A pattern is from 1 to {MAX_PATTERN_CHARS} characters")));
        }
        if compile(pattern).is_none() {
            return Err(bad_request("The pattern is not a valid regular expression"));
        }
    }
    if v.max_length.is_some_and(|n| n == 0 || n > MAX_TEXT_CHARS) || v.min_length.is_some_and(|n| n > MAX_TEXT_CHARS) {
        return Err(bad_request(format!("A length is from 1 to {MAX_TEXT_CHARS} characters")));
    }
    if let (Some(min), Some(max)) = (v.min_length, v.max_length)
        && min > max
    {
        return Err(bad_request("The shortest length is longer than the longest"));
    }
    match kind {
        FieldKind::Number => {
            let parse = |s: &Option<String>| -> Result<Option<f64>> {
                s.as_deref()
                    .map(|s| number_value(s).ok_or_else(|| bad_request("The smallest and largest number must be numbers")))
                    .transpose()
            };
            if let (Some(min), Some(max)) = (parse(&v.min)?, parse(&v.max)?)
                && min > max
            {
                return Err(bad_request("The smallest number is larger than the largest"));
            }
        }
        FieldKind::DateInput => {
            let parse = |s: &Option<String>| -> Result<Option<NaiveDate>> {
                s.as_deref()
                    .map(|s| {
                        NaiveDate::parse_from_str(s, "%Y-%m-%d")
                            .map_err(|_| bad_request("The earliest and latest date are written YYYY-MM-DD"))
                    })
                    .transpose()
            };
            if let (Some(min), Some(max)) = (parse(&v.min)?, parse(&v.max)?)
                && min > max
            {
                return Err(bad_request("The earliest date is after the latest"));
            }
        }
        _ if v.min.is_some() || v.max.is_some() => return Err(bad_request("Only number and date fields take a smallest or largest value")),
        _ => {}
    }
    Ok(options)
}

/// A value a signer gave, checked and in its stored form: numbers as plain decimals, phone
/// numbers in E.164, dates as YYYY-MM-DD, several choices as a JSON array. Empty is `None`.
pub fn normalize(kind: FieldKind, spec: &Spec, raw: &str) -> Result<Option<String>, Invalid> {
    let raw = raw.trim();
    if raw.is_empty() || raw == "[]" {
        return Ok(None);
    }
    let v = &spec.validation;
    let value = match kind {
        FieldKind::Number => {
            let format = spec.format.as_deref().and_then(NumberFormat::parse);
            let canonical = canonical_number(raw, format.as_ref()).ok_or(Invalid::Number)?;
            let n = number_value(&canonical).ok_or(Invalid::Number)?;
            if let Some(min) = v.min.as_deref().filter(|m| number_value(m).is_some_and(|m| n < m)) {
                return Err(Invalid::Below(min.to_string()));
            }
            if let Some(max) = v.max.as_deref().filter(|m| number_value(m).is_some_and(|m| n > m)) {
                return Err(Invalid::Above(max.to_string()));
            }
            canonical
        }
        FieldKind::Phone => phone(raw).ok_or(Invalid::Phone)?,
        FieldKind::DateInput => {
            let date = parse_date(raw, spec.format.as_deref()).ok_or(Invalid::Date)?;
            let bound = |s: &Option<String>| s.as_deref().and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());
            if let Some(min) = bound(&v.min).filter(|m| date < *m) {
                return Err(Invalid::Below(min.format("%Y-%m-%d").to_string()));
            }
            if let Some(max) = bound(&v.max).filter(|m| date > *m) {
                return Err(Invalid::Above(max.format("%Y-%m-%d").to_string()));
            }
            date.format("%Y-%m-%d").to_string()
        }
        FieldKind::Select | FieldKind::Radio => option(&spec.options, raw).ok_or(Invalid::Option)?.to_string(),
        FieldKind::Multiselect => {
            let picked: Vec<String> = if raw.starts_with('[') {
                serde_json::from_str(raw).map_err(|_| Invalid::Option)?
            } else {
                raw.split([',', ';', '|']).map(str::to_string).collect()
            };
            let mut chosen = Vec::new();
            for p in picked.iter().map(|p| p.trim()).filter(|p| !p.is_empty()) {
                chosen.push(option(&spec.options, p).ok_or(Invalid::Option)?);
            }
            let ordered: Vec<&String> = spec.options.iter().filter(|o| chosen.contains(o)).collect();
            if ordered.is_empty() {
                return Ok(None);
            }
            serde_json::to_string(&ordered).map_err(|_| Invalid::Option)?
        }
        _ => {
            let length = raw.chars().count() as u32;
            if text_kind(kind) {
                if let Some(min) = v.min_length.filter(|min| length < *min) {
                    return Err(Invalid::TooShort(min));
                }
                if let Some(max) = v.max_length.filter(|max| length > *max) {
                    return Err(Invalid::TooLong(max));
                }
                if let Some(pattern) = &v.pattern
                    && !compile(pattern).is_some_and(|re| re.is_match(raw))
                {
                    return Err(Invalid::Pattern);
                }
            }
            raw.to_string()
        }
    };
    Ok(Some(value))
}

fn option<'a>(options: &'a [String], value: &str) -> Option<&'a String> {
    options.iter().find(|o| *o == value).or_else(|| options.iter().find(|o| o.to_lowercase() == value.to_lowercase()))
}

fn phone(raw: &str) -> Option<String> {
    let compact: String = raw.chars().filter(|c| !matches!(c, ' ' | '-' | '(' | ')' | '.' | '/' | '\u{a0}')).collect();
    let digits = compact.strip_prefix('+').or_else(|| compact.strip_prefix("00"))?;
    let ok = (7..=15).contains(&digits.len()) && digits.chars().all(|c| c.is_ascii_digit()) && !digits.starts_with('0');
    ok.then(|| format!("+{digits}"))
}

fn parse_date(raw: &str, format: Option<&str>) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(raw, "%Y-%m-%d").ok().or_else(|| {
        let pattern = DATE_FORMATS.iter().find(|(name, _)| Some(*name) == format)?.1;
        NaiveDate::parse_from_str(raw, pattern).ok()
    })
}

pub(crate) fn number_value(canonical: &str) -> Option<f64> {
    let clean = canonical.trim();
    let ok = !clean.is_empty()
        && clean.trim_start_matches('-').split('.').count() <= 2
        && clean.trim_start_matches('-').chars().all(|c| c.is_ascii_digit() || c == '.')
        && clean.chars().filter(char::is_ascii_digit).count() <= MAX_DIGITS;
    ok.then(|| clean.parse::<f64>().ok()).flatten().filter(|n| n.is_finite())
}

/// Reads a number as people write it: `1 234,5`, `1,234.50`, `$12` or `12 kr` all work. A
/// single comma or point is the decimal separator; a separator that repeats groups thousands.
fn canonical_number(raw: &str, format: Option<&NumberFormat>) -> Option<String> {
    let mut text = raw.trim().to_string();
    if let Some(f) = format {
        for affix in [f.prefix.trim(), f.suffix.trim()] {
            if !affix.is_empty() {
                text = text.replace(affix, "");
            }
        }
    }
    let unsigned = text.trim().trim_start_matches(['-', '\u{2212}']);
    let negative = unsigned.len() < text.trim().len();
    let body: String = unsigned.chars().filter(|c| !matches!(c, ' ' | '\u{a0}' | '\u{202f}' | '\'')).collect();
    if body.is_empty() || !body.chars().all(|c| c.is_ascii_digit() || c == ',' || c == '.') {
        return None;
    }
    let (commas, points) = (body.matches(',').count(), body.matches('.').count());
    let decimal = match (commas, points) {
        (0, 0) => None,
        (_, 0) if commas > 1 => None,
        (0, _) if points > 1 => None,
        (1, 0) | (0, 1) => {
            let sep = if commas == 1 { ',' } else { '.' };
            let after = body.len() - body.rfind(sep)? - 1;
            if format.is_some_and(|f| f.group == Some(sep)) && after == 3 { None } else { Some(sep) }
        }
        _ => {
            let sep = if body.rfind(',') > body.rfind('.') { ',' } else { '.' };
            if body.matches(sep).count() > 1 {
                return None;
            }
            Some(sep)
        }
    };
    let (int, frac) = match decimal {
        Some(d) => {
            let at = body.rfind(d)?;
            (body[..at].replace([',', '.'], ""), body[at + 1..].to_string())
        }
        None => (body.replace([',', '.'], ""), String::new()),
    };
    if frac.contains([',', '.']) || (int.is_empty() && frac.is_empty()) || int.len() + frac.len() > MAX_DIGITS {
        return None;
    }
    let (int, frac) = match format {
        Some(f) => round_half_up(&int, &frac, f.decimals),
        None => (int, frac),
    };
    let int = int.trim_start_matches('0');
    let frac = frac.trim_end_matches('0');
    let mut out = String::new();
    if negative && (!int.is_empty() || !frac.is_empty()) {
        out.push('-');
    }
    out.push_str(if int.is_empty() { "0" } else { int });
    if !frac.is_empty() {
        out.push('.');
        out.push_str(frac);
    }
    Some(out)
}

/// Rounds a decimal written as digits, half away from zero, as people round money. Floats would
/// round 0.125 differently here and in the browser.
pub(crate) fn round_half_up(int: &str, frac: &str, decimals: usize) -> (String, String) {
    if frac.len() <= decimals {
        return (int.to_string(), frac.to_string());
    }
    let mut digits: Vec<u8> = format!("{int}{}", &frac[..decimals]).into_bytes();
    if frac.as_bytes()[decimals] >= b'5' {
        let mut i = digits.len();
        loop {
            if i == 0 {
                digits.insert(0, b'1');
                break;
            }
            i -= 1;
            if digits[i] == b'9' {
                digits[i] = b'0';
            } else {
                digits[i] += 1;
                break;
            }
        }
    }
    let split = digits.len() - decimals;
    let text = String::from_utf8(digits).unwrap_or_default();
    (text[..split].to_string(), text[split..].to_string())
}

/// A number format written as 1234 would look in it, such as `1,234.00`, `1 234,00 kr` or `$1234`.
#[derive(Debug, Clone, PartialEq)]
pub struct NumberFormat {
    prefix: String,
    group: Option<char>,
    decimal: char,
    decimals: usize,
    suffix: String,
}

impl NumberFormat {
    pub fn parse(format: &str) -> Option<NumberFormat> {
        let start = format.find('1')?;
        let prefix = &format[..start];
        let mut rest = format[start + 1..].chars().peekable();
        let group = match rest.peek() {
            Some(c) if matches!(c, ',' | '.' | ' ' | '\u{a0}' | '\u{202f}' | '\'') => {
                let c = *c;
                rest.next();
                Some(c)
            }
            _ => None,
        };
        let rest: String = rest.collect();
        let rest = rest.strip_prefix("234")?;
        let (decimal, decimals, suffix) = match rest.chars().next() {
            Some(d @ (',' | '.')) if rest[1..].starts_with('0') => {
                let zeros = rest[1..].chars().take_while(|c| *c == '0').count();
                (d, zeros, &rest[1 + zeros..])
            }
            _ => (if group == Some('.') { ',' } else { '.' }, 0, rest),
        };
        let affix_ok = |s: &str| s.chars().count() <= 8 && !s.chars().any(|c| c.is_ascii_digit() || c.is_control());
        if group == Some(decimal) || decimals > 6 || !affix_ok(prefix) || !affix_ok(suffix) {
            return None;
        }
        Some(NumberFormat { prefix: prefix.to_string(), group, decimal, decimals, suffix: suffix.to_string() })
    }

    pub fn decimals(&self) -> usize {
        self.decimals
    }

    pub fn write(&self, canonical: &str) -> Option<String> {
        number_value(canonical)?;
        let negative = canonical.starts_with('-');
        let (int, frac) = canonical.trim_start_matches('-').split_once('.').unwrap_or((canonical.trim_start_matches('-'), ""));
        let (int, frac) = round_half_up(int, frac, self.decimals);
        let frac = format!("{frac:0<width$}", width = self.decimals);
        let int = int.trim_start_matches('0');
        let int = if int.is_empty() { "0" } else { int };
        let mut grouped = String::new();
        for (i, c) in int.chars().enumerate() {
            if i > 0
                && (int.len() - i).is_multiple_of(3)
                && let Some(g) = self.group
            {
                grouped.push(g);
            }
            grouped.push(c);
        }
        let negative = negative && format!("{int}{frac}").chars().any(|c| c != '0');
        let mut out = format!("{}{}{grouped}", if negative { "-" } else { "" }, self.prefix);
        if !frac.is_empty() {
            out.push(self.decimal);
            out.push_str(&frac);
        }
        out.push_str(&self.suffix);
        Some(out)
    }
}

/// A stored value as it is written on the page.
pub fn display(kind: FieldKind, format: Option<&str>, value: &str) -> String {
    match kind {
        FieldKind::Number | FieldKind::Formula => {
            format.and_then(NumberFormat::parse).and_then(|f| f.write(value)).unwrap_or_else(|| value.to_string())
        }
        FieldKind::Date | FieldKind::DateInput => {
            let pattern = DATE_FORMATS.iter().find(|(name, _)| Some(*name) == format).map(|(_, p)| *p);
            match (pattern, NaiveDate::parse_from_str(value, "%Y-%m-%d")) {
                (Some(p), Ok(date)) => date.format(p).to_string(),
                _ => value.to_string(),
            }
        }
        FieldKind::Multiselect => serde_json::from_str::<Vec<String>>(value).map(|v| v.join(", ")).unwrap_or_else(|_| value.to_string()),
        _ => value.to_string(),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Case {
        kind: String,
        #[serde(default)]
        options: Vec<String>,
        #[serde(default)]
        validation: Validation,
        format: Option<String>,
        input: String,
        #[serde(default)]
        stored: Option<String>,
        #[serde(default)]
        error: Option<String>,
        #[serde(default)]
        shown: Option<String>,
    }

    #[derive(Deserialize)]
    struct Cases {
        values: Vec<Case>,
        formats: Vec<Case>,
    }

    fn cases() -> Cases {
        serde_json::from_str(include_str!("../../../../web/src/lib/field-values.cases.json")).unwrap()
    }

    #[test]
    fn shared_value_cases() {
        for c in cases().values {
            let kind = FieldKind::parse(&c.kind).unwrap();
            let spec = Spec { options: c.options.clone(), validation: c.validation.clone(), format: c.format.clone() };
            let got = normalize(kind, &spec, &c.input);
            match &c.error {
                Some(code) => assert_eq!(got.as_ref().err().map(Invalid::code), Some(code.as_str()), "{} {:?}", c.kind, c.input),
                None => assert_eq!(got, Ok(c.stored.clone()), "{} {:?}", c.kind, c.input),
            }
        }
    }

    #[test]
    fn shared_format_cases() {
        for c in cases().formats {
            let kind = FieldKind::parse(&c.kind).unwrap();
            assert_eq!(display(kind, c.format.as_deref(), &c.input), c.shown.clone().unwrap(), "{:?} {:?}", c.format, c.input);
        }
    }

    #[test]
    fn refuses_broken_setups() {
        let spec = |v: Validation| Spec { validation: v, ..Spec::default() };
        assert!(check_spec(FieldKind::Text, &spec(Validation { pattern: Some("(".into()), ..Default::default() }), None).is_err());
        assert!(
            check_spec(FieldKind::Text, &spec(Validation { pattern: Some("a{1000}{1000}".into()), ..Default::default() }), None).is_err()
        );
        assert!(
            check_spec(FieldKind::Number, &spec(Validation { min: Some("5".into()), max: Some("1".into()), ..Default::default() }), None)
                .is_err()
        );
        assert!(check_spec(FieldKind::Number, &spec(Validation { pattern: Some("x".into()), ..Default::default() }), None).is_err());
        assert!(check_spec(FieldKind::Select, &Spec::default(), None).is_err());
        assert!(check_spec(FieldKind::Radio, &Spec { options: vec!["A".into()], ..Spec::default() }, None).is_err());
        assert!(check_spec(FieldKind::Text, &Spec { format: Some("YYYY-MM-DD".into()), ..Spec::default() }, None).is_err());
        assert!(check_spec(FieldKind::Select, &Spec { options: vec!["A".into(), " A ".into()], ..Spec::default() }, None).is_err());
        let cleaned = check_spec(FieldKind::Radio, &Spec { options: vec![" Yes ".into()], ..Spec::default() }, Some("g1")).unwrap();
        assert_eq!(cleaned, vec!["Yes".to_string()]);
        assert!(check_spec(FieldKind::Number, &Spec { format: Some("1 234,00 kr".into()), ..Spec::default() }, None).is_ok());
    }

    #[test]
    fn custom_messages_replace_built_in_ones() {
        let spec =
            Spec { validation: Validation { message: Some("Use your member number".into()), ..Default::default() }, ..Spec::default() };
        assert_eq!(error_message(&spec, "Member", &Invalid::Pattern), "Use your member number");
        assert_eq!(error_message(&Spec::default(), "Member", &Invalid::Pattern), "\u{201c}Member\u{201d} is not in the expected format");
    }
}
