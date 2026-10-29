use std::collections::BTreeSet;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

macro_rules! permissions {
    ($($variant:ident = $name:literal : $description:literal,)*) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema)]
        pub enum Permission {
            $(#[serde(rename = $name)] $variant,)*
        }

        impl Permission {
            pub const ALL: &'static [Permission] = &[$(Permission::$variant,)*];

            pub fn as_str(self) -> &'static str {
                match self { $(Permission::$variant => $name,)* }
            }

            pub fn description(self) -> &'static str {
                match self { $(Permission::$variant => $description,)* }
            }

            pub fn parse(value: &str) -> Option<Permission> {
                match value { $($name => Some(Permission::$variant),)* _ => None }
            }
        }
    };
}

permissions! {
    OrgRead = "org.read": "See organisation details and the dashboard",
    UsersRead = "users.read": "See who has an account",
    UsersManage = "users.manage": "Invite people, change roles and deactivate accounts",
    SettingsManage = "settings.manage": "Change Admin settings: organisation, email, signing certificate, providers, AI, security",
    AuditRead = "audit.read": "Browse and verify the audit log",
    DocumentsRead = "documents.read": "See your own documents and those shared with you",
    DocumentsCreate = "documents.create": "Upload documents and send them for signing",
    DocumentsReadAll = "documents.read_all": "See every document, not only your own and those shared with you",
    DocumentsManageAll = "documents.manage_all": "Change, cancel and delete any document",
    DocumentsExport = "documents.export": "Export the values filled in on documents you can see as CSV, Excel or JSON",
    DocumentsLegalHold = "documents.legal_hold": "Place documents on legal hold so nobody can delete them, and release holds",
    TemplatesManage = "templates.manage": "Create and change shared templates",
    ApiKeysManage = "api_keys.manage": "Create API keys that act with your permissions",
    WebhooksManage = "webhooks.manage": "Add and remove webhooks",
    AiUse = "ai.use": "Use the AI assistant and document review",
    SystemRead = "system.read": "See version, migrations, backups and background jobs",
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Admin,
    Manager,
    Sender,
    Auditor,
    Viewer,
}

pub struct RoleInfo {
    pub label: &'static str,
    pub description: &'static str,
    pub permissions: &'static [Permission],
}

use Permission::*;

impl Role {
    pub const ALL: [Role; 5] = [Role::Admin, Role::Manager, Role::Sender, Role::Auditor, Role::Viewer];

    pub fn as_str(self) -> &'static str {
        match self {
            Role::Admin => "admin",
            Role::Manager => "manager",
            Role::Sender => "sender",
            Role::Auditor => "auditor",
            Role::Viewer => "viewer",
        }
    }

    pub fn parse(value: &str) -> Option<Role> {
        Role::ALL.into_iter().find(|r| r.as_str() == value)
    }

    pub fn info(self) -> RoleInfo {
        match self {
            Role::Admin => RoleInfo {
                label: "Admin",
                description: "Everything, including users, settings and the signing certificate",
                permissions: Permission::ALL,
            },
            Role::Manager => RoleInfo {
                label: "Manager",
                description: "Sends documents and manages every document and shared template",
                permissions: &[
                    OrgRead,
                    UsersRead,
                    DocumentsRead,
                    DocumentsCreate,
                    DocumentsReadAll,
                    DocumentsManageAll,
                    DocumentsExport,
                    DocumentsLegalHold,
                    TemplatesManage,
                    ApiKeysManage,
                    WebhooksManage,
                    AiUse,
                ],
            },
            Role::Sender => RoleInfo {
                label: "Sender",
                description: "Sends documents and sees their own and those shared with them",
                permissions: &[OrgRead, UsersRead, DocumentsRead, DocumentsCreate, DocumentsExport, ApiKeysManage, AiUse],
            },
            Role::Auditor => RoleInfo {
                label: "Auditor",
                description: "Reads every document and the audit log, changes nothing",
                permissions: &[OrgRead, UsersRead, DocumentsRead, DocumentsReadAll, DocumentsExport, AuditRead, SystemRead],
            },
            Role::Viewer => RoleInfo {
                label: "Viewer",
                description: "Sees documents shared with them",
                permissions: &[OrgRead, UsersRead, DocumentsRead],
            },
        }
    }

    pub fn permissions(self) -> BTreeSet<Permission> {
        self.info().permissions.iter().copied().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admin_has_everything_and_names_round_trip() {
        assert_eq!(Role::Admin.permissions().len(), Permission::ALL.len());
        for p in Permission::ALL {
            assert_eq!(Permission::parse(p.as_str()), Some(*p));
            assert_eq!(serde_json::to_string(p).ok(), Some(format!("\"{}\"", p.as_str())));
        }
        for r in Role::ALL {
            assert_eq!(Role::parse(r.as_str()), Some(r));
        }
    }

    #[test]
    fn read_only_roles_cannot_write() {
        for role in [Role::Auditor, Role::Viewer] {
            let p = role.permissions();
            assert!(!p.contains(&DocumentsCreate) && !p.contains(&DocumentsManageAll) && !p.contains(&SettingsManage));
        }
    }
}
