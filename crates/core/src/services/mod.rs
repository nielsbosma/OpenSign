pub mod admin;
pub mod ai;
pub mod auth;
pub mod automations;
pub mod branding;
pub mod bulk;
pub mod detect;
pub mod docs;
pub mod documents;
pub mod eid;
pub mod exports;
pub mod folders;
pub mod oauth;
pub mod passkeys;
pub mod retention;
pub mod review;
pub mod setup;
pub mod share_links;
pub mod signatures;
pub mod signing;
pub mod system;
pub mod templates;
pub mod transfers;
pub mod users;
pub mod webhooks;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sqlx::PgConnection;

use crate::context::Ctx;
use crate::error::Result;
use crate::service::{DynService, erase};
use crate::settings::SettingsGroup;

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct Empty {}

#[derive(Debug, Serialize, JsonSchema)]
pub struct Ok_ {
    pub ok: bool,
}

impl Default for Ok_ {
    fn default() -> Self {
        Self { ok: true }
    }
}

pub async fn settings<G: SettingsGroup>(ctx: &Ctx, conn: &mut PgConnection) -> Result<G> {
    ctx.app.settings.get::<G>(conn, ctx.keys()).await
}

/// Every service. MCP, the AI assistant and the RPC endpoint expose these by name.
pub fn registry() -> Vec<Box<dyn DynService>> {
    vec![
        erase::<auth::Login>(),
        erase::<auth::VerifyTwoFactor>(),
        erase::<auth::Logout>(),
        erase::<auth::WhoAmI>(),
        erase::<auth::SetLanguage>(),
        erase::<signatures::Get>(),
        erase::<signatures::Set>(),
        erase::<signatures::Remove>(),
        erase::<auth::BeginTwoFactor>(),
        erase::<auth::ConfirmTwoFactor>(),
        erase::<auth::ChangePassword>(),
        erase::<auth::RequestPasswordReset>(),
        erase::<auth::ResetPassword>(),
        erase::<setup::Status>(),
        erase::<setup::Complete>(),
        erase::<users::List>(),
        erase::<users::Invite>(),
        erase::<users::AcceptInvite>(),
        erase::<users::Update>(),
        erase::<users::ChangeEmail>(),
        erase::<users::ConfirmEmailChange>(),
        erase::<users::ResetTwoFactor>(),
        erase::<users::ListInvites>(),
        erase::<users::RevokeInvite>(),
        erase::<users::CreateApiKey>(),
        erase::<users::ListApiKeys>(),
        erase::<users::UpdateApiKey>(),
        erase::<users::RevokeApiKey>(),
        erase::<admin::GetSettings>(),
        erase::<admin::UpdateSettings>(),
        erase::<admin::SendTestEmail>(),
        erase::<branding::Get>(),
        erase::<branding::SetLogo>(),
        erase::<branding::RemoveLogo>(),
        erase::<admin::ListAudit>(),
        erase::<admin::VerifyAudit>(),
        erase::<documents::List>(),
        erase::<documents::Counts>(),
        erase::<documents::Get>(),
        erase::<documents::Create>(),
        erase::<documents::Upload>(),
        erase::<documents::RemoveFile>(),
        erase::<documents::ReplaceFile>(),
        erase::<documents::Update>(),
        erase::<documents::SetParties>(),
        erase::<documents::SetFields>(),
        erase::<detect::DetectFields>(),
        erase::<documents::ListAuthMethods>(),
        erase::<documents::Send>(),
        erase::<documents::Cancel>(),
        erase::<documents::Remind>(),
        erase::<documents::Delete>(),
        erase::<exports::Values>(),
        erase::<exports::Export>(),
        erase::<templates::Versions>(),
        erase::<templates::GetVersion>(),
        erase::<templates::Compare>(),
        erase::<templates::Save>(),
        erase::<templates::Restore>(),
        erase::<folders::List>(),
        erase::<folders::Create>(),
        erase::<folders::Update>(),
        erase::<folders::Delete>(),
        erase::<folders::Move>(),
        erase::<folders::Archive>(),
        erase::<share_links::Get>(),
        erase::<share_links::Set>(),
        erase::<share_links::Rotate>(),
        erase::<share_links::Open>(),
        erase::<share_links::RequestCode>(),
        erase::<share_links::Confirm>(),
        erase::<retention::SetLegalHold>(),
        erase::<retention::Preview>(),
        erase::<system::GetVersion>(),
        erase::<system::Status>(),
        erase::<system::Update>(),
        erase::<documents::Share>(),
        erase::<documents::Unshare>(),
        erase::<documents::Download>(),
        erase::<transfers::CreateDownloadUrl>(),
        erase::<transfers::CreateUploadUrl>(),
        erase::<documents::IssueSigningLink>(),
        erase::<documents::PartyIdentity>(),
        erase::<documents::SigningPreview>(),
        erase::<documents::SignInPerson>(),
        erase::<documents::SignMyself>(),
        erase::<passkeys::BeginRegistration>(),
        erase::<passkeys::FinishRegistration>(),
        erase::<passkeys::List>(),
        erase::<passkeys::Remove>(),
        erase::<passkeys::BeginSignIn>(),
        erase::<passkeys::SignIn>(),
        erase::<passkeys::BeginStepUp>(),
        erase::<passkeys::StepUp>(),
        erase::<bulk::Preview>(),
        erase::<bulk::Start>(),
        erase::<bulk::Get>(),
        erase::<bulk::List>(),
        erase::<bulk::Cancel>(),
        erase::<documents::Verify>(),
        erase::<documents::GetSeal>(),
        erase::<documents::RegenerateSeal>(),
        erase::<signing::Open>(),
        erase::<signing::RequestCode>(),
        erase::<signing::VerifyCode>(),
        erase::<eid::Start>(),
        erase::<eid::Status>(),
        erase::<eid::Cancel>(),
        erase::<eid::Continue>(),
        erase::<signing::Sign>(),
        erase::<signing::Decline>(),
        erase::<signing::RemoveAttachment>(),
        erase::<signing::RemoveFieldFile>(),
        erase::<review::CreateLink>(),
        erase::<review::RevokeLink>(),
        erase::<review::SetReviewLinks>(),
        erase::<webhooks::List>(),
        erase::<webhooks::Create>(),
        erase::<webhooks::Delete>(),
        erase::<webhooks::Test>(),
        erase::<webhooks::Deliveries>(),
        erase::<webhooks::GetDelivery>(),
        erase::<webhooks::Resend>(),
        erase::<webhooks::ResendFailed>(),
        erase::<automations::Status>(),
        erase::<automations::Apps>(),
        erase::<automations::Connections>(),
        erase::<automations::Connect>(),
        erase::<automations::Disconnect>(),
        erase::<automations::Tools>(),
        erase::<automations::Tool>(),
        erase::<automations::List>(),
        erase::<automations::Create>(),
        erase::<automations::Update>(),
        erase::<automations::Delete>(),
        erase::<automations::Test>(),
        erase::<automations::Runs>(),
        erase::<automations::GetRun>(),
        erase::<automations::Resend>(),
        erase::<automations::ResendFailed>(),
        erase::<oauth::Describe>(),
        erase::<oauth::Approve>(),
        erase::<oauth::Deny>(),
        erase::<oauth::ListConnections>(),
        erase::<oauth::RevokeConnection>(),
        erase::<ai::GetStatus>(),
        erase::<ai::Chat>(),
        erase::<ai::CheckSetup>(),
        erase::<ai::Review>(),
        erase::<ai::LatestReview>(),
        erase::<ai::TestProvider>(),
        erase::<ai::ListModels>(),
        erase::<ai::GetUsage>(),
        erase::<docs::Search>(),
        erase::<docs::Read>(),
    ]
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeSet, HashSet};

    use super::*;
    use crate::context::{Actor, Credential};
    use crate::permissions::Role;
    use crate::service::{Access, Exposure, Kind, allows};

    /// Signed-in services agents never get. Changing this list is a security decision: say why in
    /// the service's `not_a_tool` reason and in docs/mcp.md.
    const HIDDEN: &[&str] = &[
        "account.change_password",
        "account.mcp_connections",
        "account.passkey_add",
        "account.passkey_begin",
        "account.passkey_remove",
        "account.passkeys",
        "account.remove_signature",
        "account.revoke_mcp_connection",
        "account.set_signature",
        "account.signature",
        "account.step_up",
        "account.step_up_begin",
        "account.two_factor_begin",
        "account.two_factor_confirm",
        "ai.chat",
        "api_keys.create",
        "auth.logout",
        "documents.party_identity",
        "documents.sign_in_person",
        "documents.sign_myself",
        "oauth.approve",
        "oauth.describe",
        "oauth.deny",
        "seal.regenerate",
        "users.reset_two_factor",
    ];

    /// Calls that only change the caller's own preferences, allowed to read-only roles.
    const PERSONAL: &[&str] = &["account.set_language"];

    /// Open to any signed-in user because the permission depends on the input (a template folder
    /// needs templates.manage, a document folder documents.create; a template needs edit rights to
    /// it); the service checks it.
    const CHECKED_INSIDE: &[&str] = &["folders.create", "folders.update", "folders.delete", "templates.save_version", "templates.restore"];

    #[test]
    fn names_are_unique_dotted_and_described() {
        let mut seen = HashSet::new();
        for s in registry() {
            let meta = s.meta();
            assert!(seen.insert(meta.name), "duplicate {}", meta.name);
            assert!(meta.name.contains('.') && meta.name.chars().all(|c| c.is_ascii_lowercase() || c == '.' || c == '_'), "{}", meta.name);
            assert!(meta.description.ends_with('.'), "{} needs a full-sentence description", meta.name);
            let schema = serde_json::to_value(s.input_schema()).expect("schema");
            assert_eq!(schema["type"], "object", "{} input must be an object", meta.name);
        }
    }

    #[test]
    fn every_service_decides_whether_it_is_a_tool() {
        let mut hidden = BTreeSet::new();
        for s in registry() {
            let meta = s.meta();
            match meta.exposure {
                Exposure::Undecided => panic!("{} must call .tool() or .not_a_tool(reason)", meta.name),
                Exposure::Hidden(reason) => {
                    assert!(reason.ends_with('.'), "{} needs a full-sentence reason", meta.name);
                    if meta.access != Access::Public {
                        hidden.insert(meta.name);
                    }
                }
                Exposure::Tool => assert_ne!(meta.access, Access::Public, "public service {} must not be a tool", meta.name),
            }
            if meta.kind == Kind::Query {
                assert!(!meta.destructive, "{} is a query and cannot be destructive", meta.name);
            }
        }
        assert_eq!(hidden, HIDDEN.iter().copied().collect(), "the hidden list changed; update HIDDEN and docs/mcp.md");
    }

    fn actor(role: Role) -> Actor {
        Actor {
            user_id: uuid::Uuid::nil(),
            email: "a@example.com".into(),
            name: "A".into(),
            role,
            permissions: role.permissions(),
            must_enable_two_factor: false,
            credential: Credential::OAuth,
        }
    }

    fn tools(role: Role) -> BTreeSet<&'static str> {
        let actor = actor(role);
        registry().iter().map(|s| *s.meta()).filter(|m| m.is_tool() && allows(m.access, Some(&actor))).map(|m| m.name).collect()
    }

    #[test]
    fn role_matrix() {
        let all: BTreeSet<&str> = registry().iter().map(|s| *s.meta()).filter(|m| m.is_tool()).map(|m| m.name).collect();
        assert_eq!(tools(Role::Admin), all, "an admin sees every tool");
        assert!(all.is_disjoint(&HIDDEN.iter().copied().collect()));

        let kinds: std::collections::HashMap<&str, Kind> = registry().iter().map(|s| (s.meta().name, s.meta().kind)).collect();
        for role in [Role::Viewer, Role::Auditor] {
            let writes: Vec<_> = tools(role)
                .into_iter()
                .filter(|n| kinds[n] == Kind::Mutation && !PERSONAL.contains(n) && !CHECKED_INSIDE.contains(n))
                .collect();
            assert!(writes.is_empty(), "{role:?} can change {writes:?}");
        }

        let sender = tools(Role::Sender);
        for name in &sender {
            let area = name.split('.').next().unwrap_or_default();
            assert!(!matches!(area, "settings" | "webhooks" | "audit" | "system" | "retention" | "branding"), "a sender has {name}");
            assert!(area != "users" || *name == "users.list", "a sender has {name}");
        }
        assert!(sender.contains("documents.send") && sender.contains("documents.upload_url"));

        let manager = tools(Role::Manager);
        assert!(manager.contains("webhooks.resend") && !manager.contains("settings.update") && !manager.contains("users.invite"));
    }
}
