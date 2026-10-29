use std::collections::HashMap;
use std::sync::RwLock;
use std::time::{Duration, Instant};

use opensign_crypto::{KeyPurpose, Keys};
use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sqlx::PgConnection;

use crate::error::{AppError, Result, bad_request};

/// A group of Admin settings, stored as one row. Secret fields are encrypted one by one, bound to
/// their group and name, and never returned to a client: reads show only whether they are set.
pub trait SettingsGroup: Serialize + DeserializeOwned + Default + JsonSchema + Send + Sync {
    const NAME: &'static str;
    const SECRETS: &'static [&'static str] = &[];
    /// For each secret, the fields that decide where it is sent. Outside the app they cannot change
    /// while the secret is kept, so an agent cannot point a saved secret at another server.
    const SECRET_TARGETS: &'static [(&'static str, &'static [&'static str])] = &[];
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct OrgSettings {
    pub name: String,
    pub registration_number: String,
    pub country: String,
    pub time_zone: String,
    pub locale: String,
    pub brand_color: String,
    pub logo_file_id: Option<uuid::Uuid>,
}

impl Default for OrgSettings {
    fn default() -> Self {
        Self {
            name: String::new(),
            registration_number: String::new(),
            country: "SE".into(),
            time_zone: "Europe/Stockholm".into(),
            locale: "en".into(),
            brand_color: "#4f46e5".into(),
            logo_file_id: None,
        }
    }
}

impl SettingsGroup for OrgSettings {
    const NAME: &'static str = "org";
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct EmailSettings {
    /// `resend` or `smtp`.
    pub provider: String,
    pub api_key: Option<String>,
    pub smtp_host: String,
    pub smtp_port: u16,
    /// `starttls` (usually port 587) or `tls` (usually port 465). Plain-text SMTP is not offered.
    pub smtp_security: String,
    pub smtp_username: String,
    pub smtp_password: Option<String>,
    pub from_address: String,
    pub from_name: String,
    pub reply_to: String,
    pub verified_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl Default for EmailSettings {
    fn default() -> Self {
        Self {
            provider: "resend".into(),
            api_key: None,
            smtp_host: String::new(),
            smtp_port: 587,
            smtp_security: "starttls".into(),
            smtp_username: String::new(),
            smtp_password: None,
            from_address: String::new(),
            from_name: String::new(),
            reply_to: String::new(),
            verified_at: None,
        }
    }
}

impl SettingsGroup for EmailSettings {
    const NAME: &'static str = "email";
    const SECRETS: &'static [&'static str] = &["apiKey", "smtpPassword"];
    const SECRET_TARGETS: &'static [(&'static str, &'static [&'static str])] =
        &[("apiKey", &["provider"]), ("smtpPassword", &["provider", "smtpHost", "smtpPort", "smtpSecurity", "smtpUsername"])];
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct SecuritySettings {
    pub enforce_two_factor: bool,
    pub session_hours: u32,
    pub idle_minutes: u32,
    pub signer_link_days: u32,
    pub max_upload_mb: u32,
    /// How long a signer's AI review link works; 0 turns review links off.
    pub review_link_days: u32,
}

impl Default for SecuritySettings {
    fn default() -> Self {
        Self {
            enforce_two_factor: false,
            session_hours: 12,
            idle_minutes: 60,
            signer_link_days: 30,
            max_upload_mb: 25,
            review_link_days: 7,
        }
    }
}

impl SettingsGroup for SecuritySettings {
    const NAME: &'static str = "security";
}

/// Release checks and in-app updates.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct UpdateSettings {
    /// Ask GitHub for the latest release (at most every six hours, only the version is sent).
    pub check_for_updates: bool,
    /// A secret URL that redeploys the service when called, such as Sliplane's deploy hook.
    pub deploy_hook_url: Option<String>,
    /// A page that updates this installation to a release given as `ref`, such as the update link
    /// the Sliplane Deployer passes in.
    pub update_url: Option<String>,
}

impl Default for UpdateSettings {
    fn default() -> Self {
        Self { check_for_updates: true, deploy_hook_url: None, update_url: None }
    }
}

impl SettingsGroup for UpdateSettings {
    const NAME: &'static str = "updates";
    const SECRETS: &'static [&'static str] = &["deployHookUrl"];
}

/// How long documents are kept. 0 keeps them; documents on legal hold and templates are always kept.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct RetentionSettings {
    /// Days after completion before a signed document is deleted.
    pub completed_days: u32,
    /// Days after a document was declined, canceled or expired.
    pub ended_days: u32,
    /// Days since a draft was last changed.
    pub draft_days: u32,
}

impl SettingsGroup for RetentionSettings {
    const NAME: &'static str = "retention";
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct SigningSettings {
    pub tsa_url: String,
    pub seal_name: String,
    pub reason: String,
}

impl Default for SigningSettings {
    fn default() -> Self {
        Self { tsa_url: "http://timestamp.digicert.com".into(), seal_name: String::new(), reason: "Signed with OpenSign".into() }
    }
}

impl SettingsGroup for SigningSettings {
    const NAME: &'static str = "signing";
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct AiSettings {
    pub provider: String,
    pub model: String,
    pub api_key: Option<String>,
    pub base_url: String,
    /// Off: the assistant works, but document text never leaves the server.
    pub allow_document_text: bool,
    /// Tokens per calendar month across everyone; 0 means no limit.
    pub monthly_token_budget: u64,
}

impl SettingsGroup for AiSettings {
    const NAME: &'static str = "ai";
    const SECRETS: &'static [&'static str] = &["apiKey"];
    const SECRET_TARGETS: &'static [(&'static str, &'static [&'static str])] = &[("apiKey", &["provider", "baseUrl"])];
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct SmsSettings {
    /// `46elks`, `twilio` or empty.
    pub provider: String,
    pub username: String,
    pub password: Option<String>,
    pub sender: String,
}

impl SettingsGroup for SmsSettings {
    const NAME: &'static str = "sms";
    const SECRETS: &'static [&'static str] = &["password"];
    const SECRET_TARGETS: &'static [(&'static str, &'static [&'static str])] = &[("password", &["provider", "username"])];
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct EidSettings {
    pub bankid_enabled: bool,
    /// BankID's test environment, for test BankIDs only.
    pub bankid_test: bool,
    /// The relying-party certificate and its private key, PEM. Empty in the test environment uses
    /// BankID's public test certificate.
    pub bankid_certificate: Option<String>,
    /// Overrides the built-in BankID server root, PEM. Only this root is trusted for BankID.
    pub bankid_ca_certificate: String,
    pub freja_enabled: bool,
    pub freja_test: bool,
    /// The relying-party certificate and its private key, PEM.
    pub freja_certificate: Option<String>,
    /// Trusted CA for Freja's API server, PEM. Empty trusts the public CAs.
    pub freja_ca_certificate: String,
    /// Freja's result-signing certificate, PEM. Results are refused unless their signature checks out.
    pub freja_signing_certificate: String,
    /// An OpenID Connect eID broker (Idura/Criipto, Signicat) for the other Nordic eIDs.
    pub broker_enabled: bool,
    pub broker_issuer: String,
    pub broker_client_id: String,
    pub broker_client_secret: Option<String>,
    /// Picks the eID at the broker, e.g. `urn:grn:authn:no:bankid`. Empty lets the signer choose.
    pub broker_acr_values: String,
    /// Swedish BankID through TIC Identity (id.tic.io), used when BankID above is off.
    pub tic_enabled: bool,
    /// The tenant's name at TIC, as in `https://id.tic.io/<tenant>`.
    pub tic_tenant: String,
    pub tic_client_id: String,
    pub tic_client_secret: Option<String>,
}

impl SettingsGroup for EidSettings {
    const NAME: &'static str = "eid";
    const SECRETS: &'static [&'static str] = &["bankidCertificate", "frejaCertificate", "brokerClientSecret", "ticClientSecret"];
    const SECRET_TARGETS: &'static [(&'static str, &'static [&'static str])] = &[
        ("bankidCertificate", &["bankidTest", "bankidCaCertificate"]),
        ("frejaCertificate", &["frejaTest", "frejaCaCertificate", "frejaSigningCertificate"]),
        ("brokerClientSecret", &["brokerIssuer", "brokerClientId"]),
        ("ticClientSecret", &["ticTenant", "ticClientId"]),
    ];
}

const TTL: Duration = Duration::from_secs(5);

#[derive(Default)]
pub struct SettingsCache {
    entries: RwLock<HashMap<&'static str, (Instant, Value)>>,
    pins: HashMap<String, String>,
}

fn env_name(group: &str, field: &str) -> String {
    let mut out = format!("OPENSIGN_{}_", group.to_uppercase());
    for c in field.chars() {
        if c.is_uppercase() {
            out.push('_');
        }
        out.push(c.to_ascii_uppercase());
    }
    out
}

fn context(group: &str, field: &str) -> String {
    format!("settings:{group}.{field}")
}

impl SettingsCache {
    /// `OPENSIGN_<GROUP>_<FIELD>` variables pin a setting; pinned fields cannot be changed in the UI.
    pub fn from_env(vars: impl Iterator<Item = (String, String)>) -> Self {
        Self { entries: RwLock::default(), pins: vars.filter(|(k, _)| k.starts_with("OPENSIGN_") && k != "OPENSIGN_VERSION").collect() }
    }

    pub fn pinned<G: SettingsGroup>(&self) -> Vec<String> {
        let defaults = serde_json::to_value(G::default()).unwrap_or_default();
        let Value::Object(fields) = defaults else { return Vec::new() };
        fields.keys().filter(|f| self.pins.contains_key(&env_name(G::NAME, f))).cloned().collect()
    }

    fn apply_pins<G: SettingsGroup>(&self, values: &mut Map<String, Value>) {
        let defaults = serde_json::to_value(G::default()).unwrap_or_default();
        let Value::Object(fields) = defaults else { return };
        for (field, default) in fields {
            if let Some(raw) = self.pins.get(&env_name(G::NAME, &field)) {
                let value = match default {
                    Value::Bool(_) => Value::Bool(matches!(raw.as_str(), "true" | "1" | "yes")),
                    Value::Number(_) => raw
                        .parse::<i64>()
                        .map(Value::from)
                        .ok()
                        .or_else(|| raw.parse::<f64>().ok().and_then(serde_json::Number::from_f64).map(Value::Number))
                        .unwrap_or(default),
                    _ => Value::String(raw.clone()),
                };
                values.insert(field, value);
            }
        }
    }

    pub fn invalidate(&self, group: &str) {
        if let Ok(mut entries) = self.entries.write() {
            entries.retain(|k, _| *k != group);
        }
    }

    pub async fn get<G: SettingsGroup>(&self, conn: &mut PgConnection, keys: &Keys) -> Result<G> {
        if let Some((at, value)) = self.entries.read().ok().and_then(|e| e.get(G::NAME).cloned())
            && at.elapsed() < TTL
        {
            return serde_json::from_value(value).map_err(AppError::internal);
        }
        let row: Option<(Value, Value)> =
            sqlx::query_as(r#"select "values", secrets from settings where "group" = $1"#).bind(G::NAME).fetch_optional(&mut *conn).await?;
        let mut values = match row.as_ref().map(|r| &r.0) {
            Some(Value::Object(map)) => map.clone(),
            _ => Map::new(),
        };
        if let Some((_, Value::Object(secrets))) = &row {
            for (field, sealed) in secrets {
                if let Some(sealed) = sealed.as_str() {
                    let plain = keys.decrypt_str(KeyPurpose::Settings, &context(G::NAME, field), sealed)?;
                    values.insert(field.clone(), Value::String(plain.to_string()));
                }
            }
        }
        self.apply_pins::<G>(&mut values);
        let merged = Value::Object(values);
        let group: G = serde_json::from_value(merged.clone())
            .map_err(|e| AppError::internal(anyhow::anyhow!("settings group {} is invalid (check OPENSIGN_ pins): {e}", G::NAME)))?;
        if let Ok(mut entries) = self.entries.write() {
            entries.insert(G::NAME, (Instant::now(), serde_json::to_value(&group).unwrap_or(merged)));
        }
        Ok(group)
    }

    /// Saves a whole group. A secret sent as `None` keeps its stored value; send an empty string to clear it.
    pub async fn save<G: SettingsGroup>(
        &self,
        conn: &mut PgConnection,
        keys: &Keys,
        group: &G,
        updated_by: Option<uuid::Uuid>,
    ) -> Result<()> {
        let pinned = self.pinned::<G>();
        let Value::Object(mut values) = serde_json::to_value(group).map_err(AppError::internal)? else {
            return Err(bad_request("Settings must be an object"));
        };
        let current: Option<Value> =
            sqlx::query_scalar(r#"select secrets from settings where "group" = $1"#).bind(G::NAME).fetch_optional(&mut *conn).await?;
        let mut secrets = match current {
            Some(Value::Object(map)) => map,
            _ => Map::new(),
        };
        for field in G::SECRETS {
            match values.remove(*field) {
                Some(Value::String(s)) if s.is_empty() => {
                    secrets.remove(*field);
                }
                Some(Value::String(s)) => {
                    secrets.insert(field.to_string(), Value::String(keys.encrypt_str(KeyPurpose::Settings, &context(G::NAME, field), &s)));
                }
                _ => {}
            }
        }
        for field in &pinned {
            values.remove(field);
            secrets.remove(field);
        }
        sqlx::query(
            r#"insert into settings ("group", "values", secrets, updated_by, updated_at) values ($1, $2, $3, $4, now())
               on conflict ("group") do update set "values" = excluded."values", secrets = excluded.secrets, updated_by = excluded.updated_by, updated_at = now()"#,
        )
        .bind(G::NAME)
        .bind(Value::Object(values))
        .bind(Value::Object(secrets))
        .bind(updated_by)
        .execute(&mut *conn)
        .await?;
        self.invalidate(G::NAME);
        Ok(())
    }
}

/// The group as shown to a client: secrets replaced by whether they are set.
pub fn public_view<G: SettingsGroup>(group: &G) -> Value {
    let mut value = serde_json::to_value(group).unwrap_or_default();
    if let Value::Object(map) = &mut value {
        for field in G::SECRETS {
            let set = map.get(*field).is_some_and(|v| v.as_str().is_some_and(|s| !s.is_empty()));
            map.insert(field.to_string(), Value::Bool(set));
        }
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_names_follow_the_pattern() {
        assert_eq!(env_name("email", "apiKey"), "OPENSIGN_EMAIL_API_KEY");
        assert_eq!(env_name("security", "enforceTwoFactor"), "OPENSIGN_SECURITY_ENFORCE_TWO_FACTOR");
    }

    #[test]
    fn pins_override_and_are_listed() {
        let cache = SettingsCache::from_env(
            [
                ("OPENSIGN_SECURITY_ENFORCE_TWO_FACTOR".to_string(), "true".to_string()),
                ("OPENSIGN_SECURITY_SESSION_HOURS".into(), "4".into()),
            ]
            .into_iter(),
        );
        let mut values = Map::new();
        cache.apply_pins::<SecuritySettings>(&mut values);
        let s: SecuritySettings = serde_json::from_value(Value::Object(values)).expect("valid");
        assert!(s.enforce_two_factor);
        assert_eq!(s.session_hours, 4);
        let mut pinned = cache.pinned::<SecuritySettings>();
        pinned.sort();
        assert_eq!(pinned, ["enforceTwoFactor", "sessionHours"]);
    }

    #[test]
    fn public_view_hides_secrets() {
        let s = EmailSettings { api_key: Some("re_secret".into()), from_address: "a@b.c".into(), ..Default::default() };
        let v = public_view(&s);
        assert_eq!(v["apiKey"], Value::Bool(true));
        assert_eq!(v["fromAddress"], "a@b.c");
    }
}
