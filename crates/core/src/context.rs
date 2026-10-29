use std::collections::BTreeSet;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use opensign_crypto::Keys;
use opensign_db::Db;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::env::Env;
use crate::permissions::{Permission, Role};
use crate::settings::SettingsCache;
use crate::storage::Storage;

/// Shared by every request and job in the process.
pub struct App {
    pub env: Env,
    pub db: Db,
    pub keys: Keys,
    pub storage: Storage,
    pub settings: SettingsCache,
    pub http: crate::net::SafeHttp,
}

impl App {
    pub fn public_url(&self) -> String {
        self.env.app_url.clone().unwrap_or_else(|| format!("http://localhost:{}", self.env.port))
    }
}

/// Where a call came from. Recorded on every audit entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Surface {
    Web,
    Api,
    Compat,
    Mcp,
    Ai,
    Signer,
    Worker,
    System,
}

impl Surface {
    pub fn as_str(self) -> &'static str {
        match self {
            Surface::Web => "web",
            Surface::Api => "api",
            Surface::Compat => "compat",
            Surface::Mcp => "mcp",
            Surface::Ai => "ai",
            Surface::Signer => "signer",
            Surface::Worker => "worker",
            Surface::System => "system",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Credential {
    Session,
    ApiKey(Uuid),
    OAuth,
}

#[derive(Debug, Clone)]
pub struct Actor {
    pub user_id: Uuid,
    pub email: String,
    pub name: String,
    pub role: Role,
    /// The role's permissions, narrowed to an API key's scopes when one is used.
    pub permissions: BTreeSet<Permission>,
    pub must_enable_two_factor: bool,
    pub credential: Credential,
}

impl Actor {
    pub fn can(&self, permission: Permission) -> bool {
        self.permissions.contains(&permission)
    }
}

#[derive(Clone)]
pub struct Ctx {
    pub app: Arc<App>,
    pub actor: Option<Actor>,
    pub surface: Surface,
    pub ip: Option<String>,
    pub user_agent: Option<String>,
    pub session_id: Option<String>,
    pub request_id: Option<String>,
    pub now: DateTime<Utc>,
}

impl Ctx {
    pub fn new(app: Arc<App>, surface: Surface) -> Self {
        Self { app, actor: None, surface, ip: None, user_agent: None, session_id: None, request_id: None, now: now() }
    }

    pub fn with_actor(mut self, actor: Actor) -> Self {
        self.actor = Some(actor);
        self
    }

    pub fn can(&self, permission: Permission) -> bool {
        self.actor.as_ref().is_some_and(|a| a.can(permission))
    }

    /// A person signed in to the web app, as opposed to an agent, the assistant or an API credential.
    pub fn in_app(&self) -> bool {
        self.surface != Surface::Ai && self.actor.as_ref().is_some_and(|a| a.credential == Credential::Session)
    }

    pub fn actor(&self) -> crate::error::Result<&Actor> {
        self.actor.as_ref().ok_or_else(crate::error::unauthorized)
    }

    pub fn db(&self) -> &Db {
        &self.app.db
    }

    pub fn keys(&self) -> &Keys {
        &self.app.keys
    }
}

/// Postgres keeps microseconds; truncating here keeps hashed timestamps identical after a round trip.
pub fn now() -> DateTime<Utc> {
    let now = Utc::now();
    DateTime::from_timestamp_micros(now.timestamp_micros()).unwrap_or(now)
}
