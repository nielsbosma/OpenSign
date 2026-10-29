//! A small client for Composio's REST API (v3), which connects OpenSign to hundreds of apps. Each
//! install brings its own Composio API key; it is sent only to Composio and never logged.

use std::time::Duration;

use md5::{Digest, Md5};
use opensign_crypto::{KeyPurpose, Keys};
use reqwest::{Method, StatusCode};
use serde_json::{Value, json};

use crate::context::App;
use crate::error::{AppError, ErrorKind, Result, bad_request};

const BASE: &str = "https://backend.composio.dev/api/v3";

#[cfg(feature = "testing")]
pub static TEST_BASE: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

pub struct Composio {
    http: reqwest::Client,
    base: String,
    key: String,
    /// Off only for a test stand-in on localhost.
    public: bool,
}

/// What a tool call returned, for the run log.
pub struct Execution {
    pub status: u16,
    pub body: Value,
    /// Composio's reason when the tool did not succeed.
    pub error: Option<String>,
}

/// The Composio user that owns this install's connections. Derived from the app secret, so it
/// stays the same across restarts and is not guessable from the install's address.
pub fn user_id(keys: &Keys) -> String {
    format!("opensign-{}", &hex::encode(keys.mac(KeyPurpose::TokenMac, b"composio-user"))[..20])
}

impl Composio {
    pub fn new(app: &App, key: &str) -> Composio {
        let composio = Composio { http: app.http.public.clone(), base: BASE.into(), key: key.trim().to_string(), public: true };
        #[cfg(feature = "testing")]
        if let Some(base) = TEST_BASE.lock().ok().and_then(|b| b.clone()) {
            return Composio { http: app.http.internal.clone(), base, public: false, ..composio };
        }
        composio
    }

    async fn send(&self, method: Method, path: &str, query: &[(&str, String)], body: Option<Value>) -> Result<(StatusCode, Value)> {
        let mut request =
            self.http.request(method, format!("{}{path}", self.base)).header("x-api-key", &self.key).timeout(Duration::from_secs(60));
        if !query.is_empty() {
            request = request.query(query);
        }
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request.send().await.map_err(|e| {
            AppError::new(ErrorKind::Unavailable, "Composio could not be reached. Try again in a moment.").with_cause(e.without_url())
        })?;
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        let body = match serde_json::from_str(&text) {
            Ok(body) => body,
            Err(_) if text.is_empty() => Value::Null,
            Err(_) => Value::String(text),
        };
        Ok((status, body))
    }

    async fn call(&self, method: Method, path: &str, query: &[(&str, String)], body: Option<Value>) -> Result<Value> {
        let (status, body) = self.send(method, path, query, body).await?;
        if status.is_success() {
            return Ok(body);
        }
        Err(match status {
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
                bad_request("Composio refused the API key. Check it under Settings, Automations.")
            }
            StatusCode::TOO_MANY_REQUESTS => {
                AppError::new(ErrorKind::TooManyRequests, "Composio is rate limiting requests. Try again soon.")
            }
            StatusCode::NOT_FOUND => {
                AppError::new(ErrorKind::NotFound, error_text(&body).unwrap_or_else(|| "Not found at Composio".into()))
            }
            _ => bad_request(format!(
                "Composio answered {}: {}",
                status.as_u16(),
                error_text(&body).unwrap_or_else(|| "no reason given".into())
            )),
        })
    }

    pub async fn toolkits(&self, search: Option<&str>, limit: u32) -> Result<Vec<Value>> {
        let mut query = vec![("limit", limit.to_string()), ("sort_by", "usage".to_string())];
        if let Some(search) = search.filter(|s| !s.trim().is_empty()) {
            query.push(("search", search.trim().to_string()));
        }
        Ok(items(self.call(Method::GET, "/toolkits", &query, None).await?))
    }

    pub async fn tools(&self, toolkit: &str, search: Option<&str>, limit: u32) -> Result<Vec<Value>> {
        let mut query = vec![("toolkit_slug", toolkit.to_string()), ("limit", limit.to_string()), ("toolkit_versions", "latest".into())];
        if let Some(search) = search.filter(|s| !s.trim().is_empty()) {
            query.push(("query", search.trim().to_string()));
        }
        Ok(items(self.call(Method::GET, "/tools", &query, None).await?))
    }

    pub async fn tool(&self, slug: &str) -> Result<Value> {
        self.call(Method::GET, &format!("/tools/{}", segment(slug)?), &[("toolkit_versions", "latest".into())], None).await
    }

    /// The Composio-managed auth config for a toolkit, made on first use.
    pub async fn auth_config(&self, toolkit: &str) -> Result<String> {
        let query = [("toolkit_slug", toolkit.to_string()), ("is_composio_managed", "true".into())];
        let existing = items(self.call(Method::GET, "/auth_configs", &query, None).await?);
        if let Some(id) = existing.iter().find(|c| c["status"].as_str() != Some("DISABLED")).and_then(|c| c["id"].as_str()) {
            return Ok(id.to_string());
        }
        let body = json!({ "toolkit": { "slug": toolkit }, "auth_config": { "type": "use_composio_managed_auth" } });
        let created = self.call(Method::POST, "/auth_configs", &[], Some(body)).await?;
        created["auth_config"]["id"]
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| bad_request("Composio did not return an auth config. The app may need its own credentials set up in Composio."))
    }

    /// Starts connecting an account. Returns where to send the admin and the new account's id.
    pub async fn link(&self, auth_config_id: &str, user_id: &str, callback_url: &str) -> Result<(String, String)> {
        let body = json!({ "auth_config_id": auth_config_id, "user_id": user_id, "callback_url": callback_url });
        let link = self.call(Method::POST, "/connected_accounts/link", &[], Some(body)).await?;
        match (link["redirect_url"].as_str(), link["connected_account_id"].as_str()) {
            (Some(url), Some(id)) => Ok((url.to_string(), id.to_string())),
            _ => Err(bad_request("Composio did not return a link to connect the app")),
        }
    }

    pub async fn accounts(&self, user_id: &str) -> Result<Vec<Value>> {
        let query = [("user_ids", user_id.to_string()), ("limit", "200".into())];
        Ok(items(self.call(Method::GET, "/connected_accounts", &query, None).await?))
    }

    pub async fn account(&self, id: &str) -> Result<Value> {
        self.call(Method::GET, &format!("/connected_accounts/{}", segment(id)?), &[], None).await
    }

    pub async fn delete_account(&self, id: &str) -> Result<()> {
        self.call(Method::DELETE, &format!("/connected_accounts/{}", segment(id)?), &[], None).await.map(|_| ())
    }

    /// Puts a file where a tool can read it, and returns the argument value that points at it.
    pub async fn upload(&self, toolkit: &str, tool: &str, filename: &str, mimetype: &str, bytes: Vec<u8>) -> Result<Value> {
        let md5 = hex::encode(Md5::digest(&bytes));
        let body = json!({ "toolkit_slug": toolkit, "tool_slug": tool, "filename": filename, "mimetype": mimetype, "md5": md5 });
        let request = self.call(Method::POST, "/files/upload/request", &[], Some(body)).await?;
        let key = request["key"].as_str().ok_or_else(|| bad_request("Composio did not accept the file"))?.to_string();
        let url = request["new_presigned_url"].as_str().or_else(|| request["newPresignedUrl"].as_str());
        if let Some(url) = url.filter(|_| request["type"].as_str() != Some("existing")) {
            if self.public {
                crate::net::check_public_url(url).map_err(bad_request)?;
            }
            let response =
                self.http.put(url).header("content-type", mimetype).timeout(Duration::from_secs(120)).body(bytes).send().await.map_err(
                    |e| AppError::new(ErrorKind::Unavailable, "The file could not be uploaded to Composio").with_cause(e.without_url()),
                )?;
            if !response.status().is_success() {
                return Err(bad_request(format!("Composio's file storage answered {}", response.status().as_u16())));
            }
        }
        Ok(json!({ "name": filename, "mimetype": mimetype, "s3key": key }))
    }

    pub async fn execute(&self, tool: &str, version: Option<&str>, account: &str, user_id: &str, arguments: Value) -> Result<Execution> {
        let mut body = json!({ "connected_account_id": account, "user_id": user_id, "arguments": arguments });
        if let Some(version) = version {
            body["version"] = json!(version);
        }
        let (status, body) = self.send(Method::POST, &format!("/tools/execute/{}", segment(tool)?), &[], Some(body)).await?;
        let error = if !status.is_success() {
            Some(format!("Composio answered {}: {}", status.as_u16(), error_text(&body).unwrap_or_else(|| "no reason given".into())))
        } else if body["successful"].as_bool() == Some(false) {
            Some(error_text(&body).unwrap_or_else(|| "The tool did not succeed".into()))
        } else {
            None
        };
        Ok(Execution { status: status.as_u16(), body, error })
    }
}

fn items(body: Value) -> Vec<Value> {
    match body {
        Value::Object(mut map) => match map.remove("items") {
            Some(Value::Array(items)) => items,
            _ => Vec::new(),
        },
        Value::Array(items) => items,
        _ => Vec::new(),
    }
}

/// A slug or id as one path segment, so input cannot reach another endpoint.
fn segment(value: &str) -> Result<&str> {
    let ok = !value.is_empty() && value.len() <= 200 && value.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'));
    if ok && value != "." && value != ".." { Ok(value) } else { Err(bad_request("That is not a Composio name")) }
}

fn error_text(body: &Value) -> Option<String> {
    let text = [&body["error"]["message"], &body["error"], &body["message"], &body["detail"]]
        .into_iter()
        .find_map(|v| v.as_str().filter(|s| !s.trim().is_empty()))
        .map(str::to_string)
        .or_else(|| body.as_str().map(str::to_string))?;
    Some(text.chars().take(500).collect())
}

/// A tool's inputs as a JSON schema object, whether Composio sends a schema or a bare map of parameters.
pub fn input_schema(tool: &Value) -> Value {
    let params = &tool["input_parameters"];
    if params.get("properties").is_some() {
        return params.clone();
    }
    let Some(map) = params.as_object() else { return json!({ "type": "object", "properties": {} }) };
    let required: Vec<&String> = map.iter().filter(|(_, p)| p["required"].as_bool() == Some(true)).map(|(k, _)| k).collect();
    json!({ "type": "object", "properties": map, "required": required })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segments_stay_in_their_path() {
        assert!(segment("GOOGLEDRIVE_UPLOAD_FILE").is_ok());
        assert!(segment("ca_Ab-12").is_ok());
        for bad in ["", "..", "a/b", "a?b", "a b", "%2e%2e"] {
            assert!(segment(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn errors_are_read_from_any_shape() {
        assert_eq!(error_text(&json!({ "error": { "message": "Bad key" } })).as_deref(), Some("Bad key"));
        assert_eq!(error_text(&json!({ "error": "Nope", "successful": false })).as_deref(), Some("Nope"));
        assert_eq!(error_text(&json!({ "message": "Missing" })).as_deref(), Some("Missing"));
        assert_eq!(error_text(&json!({})), None);
    }

    #[test]
    fn parameter_maps_become_schemas() {
        let tool = json!({ "input_parameters": { "channel": { "type": "string", "required": true }, "text": { "type": "string" } } });
        let schema = input_schema(&tool);
        assert_eq!(schema["required"], json!(["channel"]));
        assert_eq!(schema["properties"]["text"]["type"], "string");
        let tool = json!({ "input_parameters": { "type": "object", "properties": { "a": {} }, "required": ["a"] } });
        assert_eq!(input_schema(&tool)["required"], json!(["a"]));
    }

    #[test]
    fn user_ids_are_stable() {
        let keys = Keys::derive(crate::testing::TEST_SECRET).expect("keys");
        assert_eq!(user_id(&keys), user_id(&keys));
        assert!(user_id(&keys).starts_with("opensign-") && user_id(&keys).len() == 29);
    }
}
