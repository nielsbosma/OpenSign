//! Electronic identification of signers: Swedish BankID and Freja eID directly, and the other
//! Nordic eIDs through an OpenID Connect broker.

pub mod bankid;
pub mod freja;
pub mod jws;
pub mod oidc;

use serde::{Deserialize, Serialize};

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
