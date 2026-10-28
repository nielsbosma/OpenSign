use std::collections::HashMap;

use serde::Serialize;

/// The only configuration that comes from the environment: the connections and the root secret.
/// Everything else is an Admin setting.
#[derive(Clone)]
pub struct Env {
    pub database_url: String,
    pub s3_endpoint: Option<String>,
    pub s3_bucket: String,
    pub s3_access_key: String,
    pub s3_secret_key: String,
    pub s3_region: String,
    pub app_secret: String,
    pub app_url: Option<String>,
    pub converter_url: Option<String>,
    pub port: u16,
    pub role: ProcessRole,
    pub development: bool,
}

impl std::fmt::Debug for Env {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Env").field("s3_bucket", &self.s3_bucket).field("app_url", &self.app_url).field("role", &self.role).finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessRole {
    All,
    Web,
    Worker,
}

#[derive(Debug, Clone, Serialize)]
pub struct EnvProblem {
    pub name: &'static str,
    pub message: String,
    pub hint: &'static str,
}

fn hint(name: &str) -> &'static str {
    match name {
        "DATABASE_URL" => "The connection string of an empty Postgres database, e.g. postgres://user:password@host:5432/opensign",
        "S3_BUCKET" => "The name of the S3-compatible bucket OpenSign stores encrypted documents and backups in",
        "S3_ACCESS_KEY" => "An access key with read and write access to the bucket",
        "S3_SECRET_KEY" => "The secret that belongs to S3_ACCESS_KEY",
        "S3_ENDPOINT" => "Leave empty for AWS S3; set it for R2, MinIO and others, e.g. https://<account>.r2.cloudflarestorage.com",
        "APP_SECRET" => "A long random string that derives every encryption key. Generate it with `openssl rand -base64 48` and never change it.",
        "APP_URL" => "The public https address, e.g. https://sign.example.com",
        "CONVERTER_URL" => "The private address of the Gotenberg service that converts Word documents, e.g. http://converter:3000",
        _ => "",
    }
}

impl Env {
    pub fn from_map(vars: &HashMap<String, String>) -> Result<Env, Vec<EnvProblem>> {
        let mut problems = Vec::new();
        let get = |name: &str| vars.get(name).map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
        let mut required = |name: &'static str| {
            get(name).unwrap_or_else(|| {
                problems.push(EnvProblem { name, message: format!("{name} is not set"), hint: hint(name) });
                String::new()
            })
        };
        let database_url = required("DATABASE_URL");
        let s3_bucket = required("S3_BUCKET");
        let s3_access_key = required("S3_ACCESS_KEY");
        let s3_secret_key = required("S3_SECRET_KEY");
        let app_secret = required("APP_SECRET");

        if !database_url.is_empty() && !(database_url.starts_with("postgres://") || database_url.starts_with("postgresql://")) {
            problems.push(EnvProblem { name: "DATABASE_URL", message: "DATABASE_URL must start with postgres://".into(), hint: hint("DATABASE_URL") });
        }
        if !app_secret.is_empty() && app_secret.chars().count() < opensign_crypto::MIN_SECRET_LEN {
            problems.push(EnvProblem {
                name: "APP_SECRET",
                message: format!("APP_SECRET must be at least {} characters", opensign_crypto::MIN_SECRET_LEN),
                hint: hint("APP_SECRET"),
            });
        }
        let url_var = |name: &'static str, problems: &mut Vec<EnvProblem>| {
            let value = get(name)?.trim_end_matches('/').to_string();
            if !(value.starts_with("http://") || value.starts_with("https://")) {
                problems.push(EnvProblem { name, message: format!("{name} must be an http(s) URL"), hint: hint(name) });
                return None;
            }
            Some(value)
        };
        let s3_endpoint = url_var("S3_ENDPOINT", &mut problems);
        let app_url = url_var("APP_URL", &mut problems);
        let converter_url = url_var("CONVERTER_URL", &mut problems);
        let port = get("PORT").map(|p| p.parse::<u16>()).transpose().unwrap_or_else(|_| {
            problems.push(EnvProblem { name: "PORT", message: "PORT must be a number".into(), hint: "" });
            None
        });
        let role = match get("ROLE").as_deref() {
            None | Some("all") => ProcessRole::All,
            Some("web") => ProcessRole::Web,
            Some("worker") => ProcessRole::Worker,
            Some(_) => {
                problems.push(EnvProblem { name: "ROLE", message: "ROLE must be all, web or worker".into(), hint: "" });
                ProcessRole::All
            }
        };
        let development = get("APP_ENV").as_deref() == Some("development");
        if !development {
            if let Some(url) = &app_url {
                if url.starts_with("http://") && !url.starts_with("http://localhost") {
                    problems.push(EnvProblem { name: "APP_URL", message: "APP_URL must use https in production".into(), hint: hint("APP_URL") });
                }
            }
        }

        if !problems.is_empty() {
            return Err(problems);
        }
        Ok(Env {
            database_url,
            s3_endpoint,
            s3_bucket,
            s3_access_key,
            s3_secret_key,
            s3_region: get("S3_REGION").unwrap_or_else(|| "us-east-1".into()),
            app_secret,
            app_url,
            converter_url,
            port: port.unwrap_or(3000),
            role,
            development,
        })
    }

    pub fn from_process() -> Result<Env, Vec<EnvProblem>> {
        Env::from_map(&std::env::vars().collect())
    }
}

pub fn app_version() -> &'static str {
    option_env!("OPENSIGN_VERSION").filter(|v| !v.is_empty()).unwrap_or(concat!(env!("CARGO_PKG_VERSION"), "-dev"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> HashMap<String, String> {
        [
            ("DATABASE_URL", "postgres://u:p@h/db"),
            ("S3_BUCKET", "b"),
            ("S3_ACCESS_KEY", "k"),
            ("S3_SECRET_KEY", "s"),
            ("APP_SECRET", "0123456789abcdef0123456789abcdef0123"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
    }

    #[test]
    fn accepts_the_minimum() {
        let env = Env::from_map(&base()).expect("valid");
        assert_eq!(env.port, 3000);
        assert_eq!(env.role, ProcessRole::All);
        assert_eq!(env.s3_region, "us-east-1");
    }

    #[test]
    fn explains_every_problem() {
        let mut vars = base();
        vars.remove("S3_BUCKET");
        vars.insert("APP_SECRET".into(), "short".into());
        vars.insert("APP_URL".into(), "http://sign.example.com".into());
        let problems = Env::from_map(&vars).expect_err("invalid");
        let names: Vec<_> = problems.iter().map(|p| p.name).collect();
        assert_eq!(names, ["S3_BUCKET", "APP_SECRET", "APP_URL"]);
    }
}
