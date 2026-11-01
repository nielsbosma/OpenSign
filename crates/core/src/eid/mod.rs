//! Electronic identification of signers: Swedish BankID directly or through TIC Identity, Freja eID
//! directly, and the other Nordic eIDs through an OpenID Connect broker.

pub mod bankid;
pub mod freja;
pub mod jws;
pub mod oidc;
pub mod tic;

use serde::{Deserialize, Serialize};

/// What to tell the person during an identification. The signing page shows `code` in their language;
/// `message` is the English for everyone else. Failures are stored by code, so each is listed in `FAILURES`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EidText {
    pub code: &'static str,
    pub message: &'static str,
}

impl EidText {
    pub const fn new(code: &'static str, message: &'static str) -> Self {
        Self { code, message }
    }

    pub const FAILED: Self = Self::new("failed", "The identification did not go through. Try again.");
    pub const CANCELED: Self = Self::new("canceled", "The identification was canceled.");
    pub const CANCELED_RETRY: Self = Self::new("canceled_retry", "The identification was canceled. Try again.");
    pub const NO_RESPONSE: Self = Self::new("no_response", "The BankID app did not respond in time. Try again.");
    pub const BLOCKED: Self = Self::new("blocked", "The BankID you are trying to use is blocked or too old. Get a new one from your bank.");
    pub const NOT_FOUND: Self =
        Self::new("not_found", "The BankID app could not be found. Install it and get a BankID from your bank, then try again.");
    pub const DECLINED_CALL: Self = Self::new("declined_call", "You declined the call. Try again.");
    pub const FREJA_DECLINED: Self = Self::new("freja_declined", "The identification was declined in Freja eID.");
    pub const FREJA_TIMEOUT: Self = Self::new("freja_timeout", "Freja eID did not respond in time. Try again.");
    pub const OTHER_PERSON: Self = Self::new("other_person", "This eID belongs to someone other than the person the document was sent to.");
    pub const ALREADY_USED: Self = Self::new("already_used", "This identification was already used. Start again.");
    pub const METHOD_CHANGED: Self = Self::new("method_changed", "The sender changed how you identify. Start again.");
    pub const TOO_LONG: Self = Self::new("too_long", "The identification took too long. Try again.");
    pub const TOO_MANY_CODES: Self = Self::new("too_many_codes", "That was the wrong code too many times. Start again.");
    pub const OTHER_BROWSER: Self = Self::new(
        "other_browser",
        "Finish the identification in the same browser where you started it. Go back to the signing link and start again.",
    );
    pub const BROKER_CANCELED: Self = Self::new("broker_canceled", "The identification was canceled or did not go through.");
    pub const BROKER_NO_RESULT: Self = Self::new("broker_no_result", "The eID broker sent no result");
    pub const NO_PERSONAL_NUMBER: Self = Self::new("no_personal_number", "TIC Identity did not return a Swedish personal number");
    pub const BROKER_NO_PERSON: Self = Self::new("broker_no_person", "The eID broker did not say who you are");

    const FAILURES: &[Self] = &[
        Self::FAILED,
        Self::CANCELED,
        Self::CANCELED_RETRY,
        Self::NO_RESPONSE,
        Self::BLOCKED,
        Self::NOT_FOUND,
        Self::DECLINED_CALL,
        Self::FREJA_DECLINED,
        Self::FREJA_TIMEOUT,
        Self::OTHER_PERSON,
        Self::ALREADY_USED,
        Self::METHOD_CHANGED,
        Self::TOO_LONG,
        Self::TOO_MANY_CODES,
        Self::OTHER_BROWSER,
        Self::BROKER_CANCELED,
        Self::BROKER_NO_RESULT,
        Self::NO_PERSONAL_NUMBER,
        Self::BROKER_NO_PERSON,
    ];

    pub fn by_code(code: &str) -> Option<Self> {
        Self::FAILURES.iter().copied().find(|t| t.code == code)
    }

    pub fn by_message(message: &str) -> Option<Self> {
        Self::FAILURES.iter().copied().find(|t| t.message == message)
    }
}

/// The person an eID vouched for.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Identity {
    pub name: String,
    pub personal_number: Option<String>,
}

fn pem(label: &str, der: &[u8]) -> String {
    use base64::Engine;
    let body = base64::engine::general_purpose::STANDARD.encode(der);
    let lines: Vec<&str> = body.as_bytes().chunks(64).map(|c| std::str::from_utf8(c).unwrap_or_default()).collect();
    format!(
        "-----BEGIN {label}-----
{}
-----END {label}-----
",
        lines.join(
            "
"
        )
    )
}

/// The private key and certificate chain of a PKCS#12 file (.p12, .pfx), as PEM.
pub fn pkcs12_to_pem(der: &[u8], password: &str) -> Result<String, String> {
    let store = p12_keystore::KeyStore::from_pkcs12(der, password)
        .map_err(|_| "The certificate file could not be opened. Check the password.".to_string())?;
    let (_, chain) = store.private_key_chain().ok_or("The certificate file holds no private key")?;
    if chain.chain().is_empty() {
        return Err("The certificate file holds no certificate".into());
    }
    let mut out = pem("PRIVATE KEY", chain.key());
    for cert in chain.chain() {
        out.push_str(&pem("CERTIFICATE", cert.as_der()));
    }
    Ok(out)
}

/// Whether `method` is switched on and has what it needs to work, with the reason when not.
pub fn ready(settings: &crate::settings::EidSettings, method: crate::documents::AuthMethod) -> crate::error::Result<()> {
    use crate::documents::AuthMethod;
    match method {
        AuthMethod::SeBankid if bankid_via_tic(settings) => tic::check(settings).map(|_| ()),
        AuthMethod::SeBankid => bankid::BankId::from_settings(settings).map(|_| ()),
        AuthMethod::Freja => freja::Freja::from_settings(settings).map(|_| ()),
        AuthMethod::Broker => {
            let complete = settings.broker_enabled
                && !settings.broker_issuer.trim().is_empty()
                && !settings.broker_client_id.trim().is_empty()
                && settings.broker_client_secret.as_deref().is_some_and(|s| !s.is_empty());
            if complete {
                Ok(())
            } else {
                Err(crate::error::bad_request("The eID broker is not set up. An admin completes it in Admin settings."))
            }
        }
        _ => Ok(()),
    }
}

/// BankID goes through TIC Identity when it is switched on and BankID with an agreement of our own
/// is not.
pub fn bankid_via_tic(settings: &crate::settings::EidSettings) -> bool {
    !settings.bankid_enabled && settings.tic_enabled
}

/// A Swedish personal identity (or coordination) number as 12 digits, if the check digit is right.
/// `+` before the last four digits marks someone 100 or older.
pub fn se_personal_number(input: &str, this_year: i32) -> Option<String> {
    let compact: String = input.chars().filter(|c| !c.is_whitespace()).collect();
    let centenarian = compact.contains('+');
    let digits: String = compact.chars().filter(|c| *c != '-' && *c != '+').collect();
    if !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let full = match digits.len() {
        12 => digits,
        10 => {
            let yy: i32 = digits[..2].parse().ok()?;
            let mut year = this_year - (this_year - yy).rem_euclid(100);
            if centenarian {
                year -= 100;
            }
            format!("{year:04}{}", &digits[2..])
        }
        _ => return None,
    };
    let month: u32 = full[4..6].parse().ok()?;
    let day: u32 = full[6..8].parse().ok()?;
    let day = if day > 60 { day - 60 } else { day };
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) || !luhn(&full[2..]) {
        return None;
    }
    Some(full)
}

fn luhn(digits: &str) -> bool {
    let sum: u32 = digits
        .bytes()
        .enumerate()
        .map(|(i, b)| {
            let d = u32::from(b - b'0') * if i % 2 == 0 { 2 } else { 1 };
            if d > 9 { d - 9 } else { d }
        })
        .sum();
    sum.is_multiple_of(10)
}

/// Whether two personal numbers name the same person, in whatever format each was written.
pub fn same_person(expected: &str, proven: &str, this_year: i32) -> bool {
    if let (Some(a), Some(b)) = (se_personal_number(expected, this_year), se_personal_number(proven, this_year)) {
        return a == b;
    }
    let clean = |s: &str| s.chars().filter(char::is_ascii_alphanumeric).collect::<String>().to_ascii_uppercase();
    let (a, b) = (clean(expected), clean(proven));
    !a.is_empty() && opensign_crypto::constant_time_eq(a.as_bytes(), b.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_failures_read_back() {
        for text in EidText::FAILURES {
            assert_eq!(EidText::by_code(text.code), Some(*text));
            assert_eq!(EidText::by_message(text.message), Some(*text));
        }
        assert_eq!(EidText::by_code("A provider's own words"), None);
    }

    #[test]
    fn swedish_personal_numbers() {
        assert_eq!(se_personal_number("19900101-0017", 2026).as_deref(), Some("199001010017"));
        assert_eq!(se_personal_number("900101-0017", 2026).as_deref(), Some("199001010017"));
        assert_eq!(se_personal_number("900101+0017", 2026).as_deref(), Some("189001010017"));
        assert_eq!(se_personal_number("19900161-0014", 2026).as_deref(), Some("199001610014"), "coordination number");
        assert_eq!(se_personal_number("19900101-0018", 2026), None, "check digit");
        assert_eq!(se_personal_number("19901301-0017", 2026), None, "month");
        assert_eq!(se_personal_number("abc", 2026), None);
    }

    #[test]
    fn matches_people_across_formats() {
        assert!(same_person("900101-0017", "199001010017", 2026));
        assert!(!same_person("900101-0017", "199001010025", 2026));
        assert!(same_person("12345678901", "123456 78901", 2026));
        assert!(!same_person("", "", 2026));
    }
}
