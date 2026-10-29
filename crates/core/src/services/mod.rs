pub mod admin;
pub mod ai;
pub mod auth;
pub mod branding;
pub mod bulk;
pub mod documents;
pub mod eid;
pub mod folders;
pub mod oauth;
pub mod passkeys;
pub mod retention;
pub mod review;
pub mod setup;
pub mod signing;
pub mod system;
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
        erase::<documents::Update>(),
        erase::<documents::SetParties>(),
        erase::<documents::SetFields>(),
        erase::<documents::ListAuthMethods>(),
        erase::<documents::Send>(),
        erase::<documents::Cancel>(),
        erase::<documents::Remind>(),
        erase::<documents::Delete>(),
        erase::<folders::List>(),
        erase::<folders::Create>(),
        erase::<folders::Update>(),
        erase::<folders::Delete>(),
        erase::<folders::Move>(),
        erase::<folders::Archive>(),
        erase::<retention::SetLegalHold>(),
        erase::<retention::Preview>(),
        erase::<system::GetVersion>(),
        erase::<system::Status>(),
        erase::<system::Update>(),
        erase::<documents::Share>(),
        erase::<documents::Unshare>(),
        erase::<documents::Download>(),
        erase::<documents::IssueSigningLink>(),
        erase::<documents::SigningPreview>(),
        erase::<documents::SignInPerson>(),
        erase::<passkeys::BeginRegistration>(),
        erase::<passkeys::FinishRegistration>(),
        erase::<passkeys::List>(),
        erase::<passkeys::Remove>(),
        erase::<passkeys::BeginSignIn>(),
        erase::<passkeys::SignIn>(),
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
        erase::<signing::Sign>(),
        erase::<signing::Decline>(),
        erase::<signing::RemoveAttachment>(),
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
    ]
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::service::Access;

    #[test]
    fn names_are_unique_dotted_and_described() {
        let mut seen = HashSet::new();
        for s in registry() {
            let meta = s.meta();
            assert!(seen.insert(meta.name), "duplicate {}", meta.name);
            assert!(meta.name.contains('.') && meta.name.chars().all(|c| c.is_ascii_lowercase() || c == '.' || c == '_'), "{}", meta.name);
            assert!(meta.description.ends_with('.'), "{} needs a full-sentence description", meta.name);
            if meta.access == Access::Public {
                assert!(!meta.tool, "public service {} must not be a tool", meta.name);
            }
            let schema = serde_json::to_value(s.input_schema()).expect("schema");
            assert_eq!(schema["type"], "object", "{} input must be an object", meta.name);
        }
    }
}
