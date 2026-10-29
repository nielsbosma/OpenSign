use async_trait::async_trait;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use super::{Empty, Ok_, settings};
use crate::audit::{ChainReport, Entry, audit, verify_chain};
use crate::context::Ctx;
use crate::email::{self, Message};
use crate::error::{AppError, Result, bad_request};
use crate::permissions::Permission;
use crate::service::{Access, Meta, Service};
use crate::settings::{
    AiSettings, EidSettings, EmailSettings, OrgSettings, RetentionSettings, SecuritySettings, SettingsGroup, SigningSettings, SmsSettings,
    public_view,
};

#[derive(Debug, Clone, Copy, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Group {
    Org,
    Email,
    Security,
    Retention,
    Updates,
    Signing,
    Ai,
    Sms,
    Eid,
}

#[derive(Deserialize, JsonSchema)]
pub struct GroupInput {
    pub group: Group,
}

#[derive(Serialize, JsonSchema)]
pub struct GroupView {
    /// Secret fields show `true` when set; their values are never returned.
    pub values: Value,
    /// Fields fixed by OPENSIGN_* environment variables.
    pub pinned: Vec<String>,
}

async fn view<G: SettingsGroup>(ctx: &Ctx, conn: &mut PgConnection) -> Result<GroupView> {
    let group: G = settings(ctx, conn).await?;
    Ok(GroupView { values: public_view(&group), pinned: ctx.app.settings.pinned::<G>() })
}

pub struct GetSettings;

#[async_trait]
impl Service for GetSettings {
    type Input = GroupInput;
    type Output = GroupView;
    const META: Meta = Meta::query(
        "settings.get",
        "Read one group of Admin settings. Secrets show only whether they are set.",
        Access::Permission(Permission::SettingsManage),
    )
    .tool();

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: GroupInput) -> Result<GroupView> {
        match input.group {
            Group::Org => view::<OrgSettings>(ctx, conn).await,
            Group::Email => view::<EmailSettings>(ctx, conn).await,
            Group::Security => view::<SecuritySettings>(ctx, conn).await,
            Group::Retention => view::<RetentionSettings>(ctx, conn).await,
            Group::Updates => view::<crate::settings::UpdateSettings>(ctx, conn).await,
            Group::Signing => view::<SigningSettings>(ctx, conn).await,
            Group::Ai => view::<AiSettings>(ctx, conn).await,
            Group::Sms => view::<SmsSettings>(ctx, conn).await,
            Group::Eid => view::<EidSettings>(ctx, conn).await,
        }
    }
}

#[derive(Deserialize, JsonSchema)]
pub struct UpdateInput {
    pub group: Group,
    /// The fields to change. Leave a secret out (or send null) to keep it; send "" to clear it.
    pub values: Value,
}

fn merge(current: Value, patch: &Value, secrets: &[&str]) -> Value {
    let mut merged = current;
    if let (Value::Object(target), Value::Object(changes)) = (&mut merged, patch) {
        for (k, v) in changes {
            if secrets.contains(&k.as_str()) && (v.is_null() || v.is_boolean()) {
                continue;
            }
            target.insert(k.clone(), v.clone());
        }
    }
    merged
}

/// Secrets never pass through an agent or an API call: there they can be cleared, not set, and the
/// fields that decide where a kept secret goes cannot change.
fn guard_secrets<G: SettingsGroup>(current: &Value, patch: &Value) -> Result<()> {
    let Value::Object(changes) = patch else { return Ok(()) };
    let filled = |v: &Value| v.as_str().is_some_and(|s| !s.is_empty());
    if let Some(field) =
        changes.iter().find(|(k, v)| (G::SECRETS.contains(&k.as_str()) || k.ends_with("Password")) && filled(v)).map(|(k, _)| k)
    {
        return Err(bad_request(format!(
            "{field} is a secret. Set it signed in to OpenSign; through MCP and API keys a secret can only be cleared with an empty string."
        )));
    }
    for (secret, targets) in G::SECRET_TARGETS {
        let kept = !current[*secret].is_null() && changes.get(*secret) != Some(&Value::String(String::new()));
        if let Some(field) = targets.iter().find(|f| changes.get(**f).is_some_and(|v| !v.is_null() && *v != current[**f]))
            && kept
        {
            return Err(bad_request(format!(
                "Changing {field} would send the saved {secret} somewhere new. Change it signed in to OpenSign, or clear {secret} with an empty string in the same call."
            )));
        }
    }
    Ok(())
}

async fn update<G: SettingsGroup>(ctx: &Ctx, conn: &mut PgConnection, patch: &Value) -> Result<GroupView> {
    let pinned = ctx.app.settings.pinned::<G>();
    if let Value::Object(changes) = patch
        && let Some(field) = changes.keys().find(|k| pinned.contains(k))
    {
        return Err(bad_request(format!("{field} is fixed by an environment variable")));
    }
    let current: G = settings(ctx, conn).await?;
    if !ctx.in_app() {
        guard_secrets::<G>(&serde_json::to_value(&current).map_err(AppError::internal)?, patch)?;
    }
    let before = public_view(&current);
    let mut base = serde_json::to_value(&current).map_err(AppError::internal)?;
    if let Value::Object(map) = &mut base {
        for secret in G::SECRETS {
            map.insert(secret.to_string(), Value::Null);
        }
    }
    let mut patch = patch.clone();
    if G::NAME == "email"
        && let Value::Object(changes) = &mut patch
    {
        // Only a successful test email marks email as working; a new provider, server, key or sender needs a new test.
        changes.remove("verifiedAt");
        let changed = |field: &str| changes.get(field).is_some_and(|v| !v.is_null() && !v.is_boolean() && Some(v) != base.get(field));
        if ["provider", "apiKey", "smtpHost", "smtpPort", "smtpSecurity", "smtpUsername", "smtpPassword", "fromAddress"]
            .iter()
            .any(|f| changed(f))
        {
            changes.insert("verifiedAt".into(), Value::Null);
        }
    }
    if G::NAME == "security"
        && let Some(Value::Array(origins)) = patch.get_mut("embedOrigins")
    {
        // Typed with a trailing slash or in capitals is still the same origin; anything else is refused below.
        for origin in origins.iter_mut() {
            if let Some(canonical) = origin.as_str().and_then(crate::settings::canonical_origin) {
                *origin = Value::String(canonical);
            }
        }
        let mut seen = std::collections::HashSet::new();
        origins.retain(|o| o.as_str().is_none_or(|s| !s.trim().is_empty() && seen.insert(s.to_string())));
    }
    if G::NAME == "eid"
        && let Value::Object(changes) = &mut patch
    {
        // A .p12/.pfx arrives as base64 with its password; only the PEM it unlocks is stored.
        for (field, password_field) in
            [("bankidCertificate", "bankidCertificatePassword"), ("frejaCertificate", "frejaCertificatePassword")]
        {
            let password = changes.remove(password_field);
            let Some(Value::String(upload)) = changes.get(field) else { continue };
            if upload.is_empty() || upload.trim_start().starts_with("-----BEGIN") {
                continue;
            }
            let der = B64.decode(upload.trim()).map_err(|_| bad_request("The certificate file is neither PEM nor PKCS#12"))?;
            let password = password.as_ref().and_then(Value::as_str).unwrap_or_default();
            let pem = crate::eid::pkcs12_to_pem(&der, password).map_err(bad_request)?;
            changes.insert(field.into(), Value::String(pem));
        }
    }
    let next: G = serde_json::from_value(merge(base, &patch, G::SECRETS)).map_err(|e| bad_request(format!("Invalid settings: {e}")))?;
    validate::<G>(&next)?;
    ctx.app.settings.save(conn, ctx.keys(), &next, Some(ctx.actor()?.user_id)).await?;
    let saved: G = settings(ctx, conn).await?;
    let after = public_view(&saved);
    audit(
        ctx,
        conn,
        Entry {
            action: "settings.update",
            entity_type: "settings",
            entity_id: Some(G::NAME.into()),
            before: Some(before),
            after: Some(after.clone()),
            metadata: None,
            secret_keys: G::SECRETS,
        },
    )
    .await?;
    Ok(GroupView { values: after, pinned })
}

pub(crate) fn validate<G: SettingsGroup>(group: &G) -> Result<()> {
    let value = serde_json::to_value(group).map_err(AppError::internal)?;
    let url = |field: &str| -> Result<()> {
        if let Some(u) = value.get(field).and_then(Value::as_str).filter(|u| !u.is_empty()) {
            crate::net::check_outbound_url(u, G::NAME == "signing").map_err(bad_request)?;
        }
        Ok(())
    };
    match G::NAME {
        "signing" => url("tsaUrl"),
        // A LiteLLM or other self-hosted proxy usually runs next to OpenSign on the private network.
        "ai" if matches!(value["provider"].as_str(), Some("litellm" | "openai-compatible")) => {
            match value["baseUrl"].as_str().filter(|u| !u.is_empty()) {
                Some(u) => match reqwest::Url::parse(u) {
                    Ok(parsed) if matches!(parsed.scheme(), "http" | "https") && parsed.username().is_empty() => Ok(()),
                    _ => Err(bad_request("Enter the proxy's http or https address")),
                },
                None => Err(bad_request("Enter the address of the OpenAI-compatible endpoint")),
            }
        }
        "ai" => url("baseUrl"),
        "eid" => {
            url("brokerIssuer")?;
            let eid: EidSettings = serde_json::from_value(value.clone()).map_err(AppError::internal)?;
            let present = |s: &str| !s.trim().is_empty();
            for (name, ca) in [("BankID server CA", &eid.bankid_ca_certificate), ("Freja server CA", &eid.freja_ca_certificate)] {
                if present(ca) && reqwest::Certificate::from_pem_bundle(ca.as_bytes()).map_or(true, |c| c.is_empty()) {
                    return Err(bad_request(format!("The {name} must be a PEM certificate")));
                }
            }
            for (name, identity) in [("BankID", &eid.bankid_certificate), ("Freja eID", &eid.freja_certificate)] {
                if let Some(pem) = identity.as_deref().filter(|p| present(p)) {
                    crate::net::client_with_certificate(pem, None).map_err(|e| bad_request(format!("{name}: {e}")))?;
                }
            }
            if present(&eid.tic_tenant) && crate::eid::tic::tenant(&eid.tic_tenant).is_none() {
                return Err(bad_request("Enter the TIC tenant's name, as in https://id.tic.io/<tenant>"));
            }
            if present(&eid.freja_signing_certificate) {
                crate::eid::jws::Key::from_certificate_pem(&eid.freja_signing_certificate)
                    .map_err(|e| bad_request(format!("Freja's signing certificate is not usable: {e}")))?;
            }
            Ok(())
        }
        "updates" => {
            for key in ["deployHookUrl", "updateUrl"] {
                if let Some(url) = value.get(key).and_then(Value::as_str).filter(|u| !u.is_empty()) {
                    crate::net::check_public_url(url).map_err(bad_request)?;
                }
            }
            Ok(())
        }
        "security" => {
            let security: SecuritySettings = serde_json::from_value(value.clone()).map_err(AppError::internal)?;
            if security.embed_origins.len() > 20 {
                return Err(bad_request("List at most 20 sites allowed to embed"));
            }
            if let Some(bad) = security.embed_origins.iter().find(|o| crate::settings::canonical_origin(o).as_deref() != Some(o.as_str())) {
                return Err(bad_request(format!(
                    "{bad} is not a site address. Enter each as https://app.example.com, without a path or wildcards."
                )));
            }
            if security.embed_signing && security.embed_origins.is_empty() {
                return Err(bad_request("List the sites allowed to embed the signing page before turning embedding on"));
            }
            Ok(())
        }
        "retention" => {
            let rules: RetentionSettings = serde_json::from_value(value.clone()).map_err(AppError::internal)?;
            if [rules.completed_days, rules.ended_days, rules.draft_days].iter().any(|d| *d > 36_500) {
                return Err(bad_request("Keep retention periods under 100 years"));
            }
            Ok(())
        }
        "email" => {
            let email: EmailSettings = serde_json::from_value(value.clone()).map_err(AppError::internal)?;
            for address in [&email.from_address, &email.reply_to].into_iter().filter(|s| !s.is_empty()) {
                crate::auth::normalize_email(address)?;
            }
            match email.provider.as_str() {
                "resend" => Ok(()),
                "smtp" => {
                    if !email.smtp_host.is_empty() && !email::valid_smtp_host(email.smtp_host.trim()) {
                        return Err(bad_request("Enter the SMTP server's host name only, such as smtp.example.com"));
                    }
                    if email.smtp_port == 0 {
                        return Err(bad_request("Enter the SMTP server's port, usually 587 or 465"));
                    }
                    if !matches!(email.smtp_security.as_str(), "starttls" | "tls") {
                        return Err(bad_request("SMTP security must be STARTTLS or TLS"));
                    }
                    Ok(())
                }
                _ => Err(bad_request("Choose Resend or SMTP")),
            }
        }
        _ => Ok(()),
    }
}

pub struct UpdateSettings;

#[async_trait]
impl Service for UpdateSettings {
    type Input = UpdateInput;
    type Output = GroupView;
    const META: Meta = Meta::mutation(
        "settings.update",
        "Change Admin settings in one group. Every change is audited with secrets masked.",
        Access::Permission(Permission::SettingsManage),
    )
    .tool()
    .destructive();

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: UpdateInput) -> Result<GroupView> {
        if !input.values.is_object() {
            return Err(bad_request("values must be an object"));
        }
        match input.group {
            Group::Org => {
                // The logo is changed only through branding.set_logo, which checks the image.
                let mut patch = input.values.clone();
                if let Value::Object(map) = &mut patch {
                    map.remove("logoFileId");
                }
                update::<OrgSettings>(ctx, conn, &patch).await
            }
            Group::Email => {
                let mut patch = input.values.clone();
                if let Value::Object(map) = &mut patch {
                    map.remove("verifiedAt");
                    map.insert("verifiedAt".into(), Value::Null);
                }
                update::<EmailSettings>(ctx, conn, &patch).await
            }
            Group::Security => update::<SecuritySettings>(ctx, conn, &input.values).await,
            Group::Retention => update::<RetentionSettings>(ctx, conn, &input.values).await,
            Group::Updates => update::<crate::settings::UpdateSettings>(ctx, conn, &input.values).await,
            Group::Signing => update::<SigningSettings>(ctx, conn, &input.values).await,
            Group::Ai => update::<AiSettings>(ctx, conn, &input.values).await,
            Group::Sms => update::<SmsSettings>(ctx, conn, &input.values).await,
            Group::Eid => update::<EidSettings>(ctx, conn, &input.values).await,
        }
    }
}

#[derive(Deserialize, JsonSchema)]
pub struct TestEmailInput {
    /// Defaults to your own address.
    pub to: Option<String>,
}

pub struct SendTestEmail;

#[async_trait]
impl Service for SendTestEmail {
    type Input = TestEmailInput;
    type Output = Ok_;
    const META: Meta = Meta::mutation(
        "settings.send_test_email",
        "Send a test email through the configured provider, Resend or SMTP. A successful test marks email as set up.",
        Access::Permission(Permission::SettingsManage),
    )
    .tool()
    .open_world();

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: TestEmailInput) -> Result<Ok_> {
        let to = match input.to {
            Some(to) => crate::auth::normalize_email(&to)?,
            None => ctx.actor()?.email.clone(),
        };
        let mut email_settings: EmailSettings = settings(ctx, conn).await?;
        let org: OrgSettings = settings(ctx, conn).await?;
        email::send(
            &ctx.app,
            &email_settings,
            &org,
            &Message {
                to: to.clone(),
                to_name: None,
                subject: "OpenSign test email".into(),
                paragraphs: vec!["Email from OpenSign works. Signing invitations and notifications will be sent like this.".into()],
                button: None,
                footer: None,
                attachments: vec![],
                lang: crate::i18n::Lang::En,
            },
        )
        .await?;
        email_settings.verified_at = Some(ctx.now);
        email_settings.api_key = None;
        email_settings.smtp_password = None;
        ctx.app.settings.save(conn, ctx.keys(), &email_settings, Some(ctx.actor()?.user_id)).await?;
        audit(ctx, conn, Entry::new("settings.email_verified", "settings", "email").metadata(serde_json::json!({ "to": to }))).await?;
        Ok(Ok_::default())
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ListAuditInput {
    /// Return entries older than this id (for paging).
    pub before_id: Option<i64>,
    pub limit: Option<i64>,
    pub entity_type: Option<String>,
    pub entity_id: Option<String>,
    pub action: Option<String>,
    pub actor_id: Option<Uuid>,
}

#[derive(Serialize, JsonSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct AuditView {
    pub id: i64,
    pub occurred_at: DateTime<Utc>,
    pub actor_id: Option<Uuid>,
    pub actor_label: String,
    pub surface: String,
    pub action: String,
    pub entity_type: String,
    pub entity_id: Option<String>,
    pub before: Option<Value>,
    pub after: Option<Value>,
    pub metadata: Option<Value>,
    #[schemars(with = "String")]
    #[serde(serialize_with = "hex_bytes")]
    pub hash: Vec<u8>,
}

fn hex_bytes<S: serde::Serializer>(bytes: &[u8], s: S) -> std::result::Result<S::Ok, S::Error> {
    s.serialize_str(&hex::encode(bytes))
}

pub struct ListAudit;

#[async_trait]
impl Service for ListAudit {
    type Input = ListAuditInput;
    type Output = Vec<AuditView>;
    const META: Meta = Meta::query(
        "audit.list",
        "Browse the audit log, newest first. Filter by entity, action or actor.",
        Access::Permission(Permission::AuditRead),
    )
    .tool();

    async fn run(_ctx: &Ctx, conn: &mut PgConnection, input: ListAuditInput) -> Result<Vec<AuditView>> {
        Ok(sqlx::query_as(
            "select id, occurred_at, actor_id, actor_label, surface, action, entity_type, entity_id, before, after, metadata, hash from audit_log
             where ($1::bigint is null or id < $1) and ($2::text is null or entity_type = $2) and ($3::text is null or entity_id = $3)
               and ($4::text is null or action = $4) and ($5::uuid is null or actor_id = $5)
             order by id desc limit $6",
        )
        .bind(input.before_id)
        .bind(input.entity_type)
        .bind(input.entity_id)
        .bind(input.action)
        .bind(input.actor_id)
        .bind(input.limit.unwrap_or(50).clamp(1, 500))
        .fetch_all(&mut *conn)
        .await?)
    }
}

pub struct VerifyAudit;

#[async_trait]
impl Service for VerifyAudit {
    type Input = Empty;
    type Output = ChainReport;
    const META: Meta = Meta::query(
        "audit.verify",
        "Recompute the audit log's hash chain and report the first entry that was changed or removed, if any.",
        Access::Permission(Permission::AuditRead),
    )
    .tool();

    async fn run(ctx: &Ctx, conn: &mut PgConnection, _: Empty) -> Result<ChainReport> {
        verify_chain(conn, ctx.keys()).await
    }
}
