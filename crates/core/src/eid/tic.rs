//! TIC Identity (id.tic.io): Swedish BankID through TIC's OpenID Connect provider, for those without
//! a BankID agreement of their own. Only the `nin` claim says who the person is; `sub` is TIC's own
//! hash of it.

use serde_json::Value;

use super::oidc::{self, Provider};
use super::{Identity, se_personal_number};
use crate::error::{Result, bad_request};
use crate::settings::EidSettings;

const HOST: &str = "https://id.tic.io/";
const SCOPE: &str = "openid profile nin";

#[cfg(feature = "testing")]
pub static TEST_ISSUER: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// The tenant as TIC names it, from the bare name or the tenant's address pasted from TIC's portal.
pub fn tenant(input: &str) -> Option<String> {
    let input = input.trim();
    let slug = input.strip_prefix(HOST).unwrap_or(input).trim_end_matches('/');
    let slug = slug.strip_suffix("/.well-known/openid-configuration").unwrap_or(slug);
    let valid = !slug.is_empty() && slug.len() <= 64 && slug.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    valid.then(|| slug.to_string())
}

/// Whether TIC Identity is switched on and has what it needs, with the reason when not.
pub fn check(settings: &EidSettings) -> Result<String> {
    let not_set_up = || bad_request("TIC Identity is not set up. An admin completes it in Admin settings.");
    if !settings.tic_enabled
        || settings.tic_client_id.trim().is_empty()
        || !settings.tic_client_secret.as_deref().is_some_and(|s| !s.is_empty())
    {
        return Err(not_set_up());
    }
    let tenant = tenant(&settings.tic_tenant).ok_or_else(not_set_up)?;
    Ok(format!("{HOST}{tenant}"))
}

pub fn provider(settings: &EidSettings, http: &reqwest::Client) -> Result<Provider> {
    let issuer = check(settings)?;
    let provider = Provider {
        http: http.clone(),
        issuer,
        client_id: settings.tic_client_id.trim().to_string(),
        client_secret: settings.tic_client_secret.clone().unwrap_or_default(),
        scope: SCOPE.into(),
        acr_values: String::new(),
        require_public: true,
    };
    #[cfg(feature = "testing")]
    if let Some(issuer) = TEST_ISSUER.lock().ok().and_then(|b| b.clone()) {
        return Ok(Provider { http: reqwest::Client::new(), issuer, require_public: false, ..provider });
    }
    Ok(provider)
}

/// The person in TIC's ID token, only if it carries a valid Swedish personal number.
pub fn identity(claims: &Value, this_year: i32) -> Option<Identity> {
    if claims["nin_issuing_country"].as_str().is_some_and(|c| !c.eq_ignore_ascii_case("SE")) {
        return None;
    }
    let number = se_personal_number(claims["nin"].as_str()?, this_year)?;
    Some(Identity { name: oidc::name(claims)?, personal_number: Some(number) })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn tenant_from_name_or_address() {
        assert_eq!(tenant("acme").as_deref(), Some("acme"));
        assert_eq!(tenant(" https://id.tic.io/acme-ab/ ").as_deref(), Some("acme-ab"));
        assert_eq!(tenant("https://id.tic.io/acme/.well-known/openid-configuration").as_deref(), Some("acme"));
        for bad in ["", "https://evil.example/acme", "acme/../x", "a b", "acme?x=1"] {
            assert_eq!(tenant(bad), None, "{bad}");
        }
    }

    #[test]
    fn needs_everything_before_it_is_ready() {
        let complete = EidSettings {
            tic_enabled: true,
            tic_tenant: "acme".into(),
            tic_client_id: "client".into(),
            tic_client_secret: Some("secret".into()),
            ..EidSettings::default()
        };
        assert_eq!(check(&complete).expect("ready"), "https://id.tic.io/acme");
        assert!(check(&EidSettings { tic_enabled: false, ..complete.clone() }).is_err());
        assert!(check(&EidSettings { tic_tenant: "../x".into(), ..complete.clone() }).is_err());
        assert!(check(&EidSettings { tic_client_secret: None, ..complete }).is_err());
    }

    #[test]
    fn trusts_only_the_personal_number_claim() {
        let claims = json!({ "sub": "hash", "name": "Anna Andersson", "nin": "199001010017", "nin_type": "personal_number", "nin_issuing_country": "SE" });
        assert_eq!(identity(&claims, 2026), Some(Identity { name: "Anna Andersson".into(), personal_number: Some("199001010017".into()) }));
        let mut foreign = claims.clone();
        foreign["nin_issuing_country"] = json!("NO");
        assert_eq!(identity(&foreign, 2026), None);
        assert_eq!(identity(&json!({ "name": "Anna", "sub": "199001010017", "ssn": "199001010017" }), 2026), None);
        assert_eq!(identity(&json!({ "name": "Anna", "nin": "199001010018" }), 2026), None, "check digit");
        assert_eq!(identity(&json!({ "nin": "199001010017" }), 2026), None, "no name");
    }
}
