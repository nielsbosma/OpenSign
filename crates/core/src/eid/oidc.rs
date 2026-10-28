//! OpenID Connect authorization code flow with PKCE against an eID broker. The ID token's
//! signature, issuer, audience, expiry and nonce are all checked, even though it arrives straight
//! from the token endpoint.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64URL;
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::Identity;
use super::jws::{self, Key};
use crate::error::{AppError, ErrorKind, Result};

#[derive(Debug, Clone, Deserialize)]
pub struct Discovery {
    pub issuer: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub jwks_uri: String,
    #[serde(default)]
    pub token_endpoint_auth_methods_supported: Vec<String>,
}

/// A configured OpenID Connect provider: the generic broker, or TIC Identity.
pub struct Provider {
    pub http: reqwest::Client,
    pub issuer: String,
    pub client_id: String,
    pub client_secret: String,
    pub scope: &'static str,
    pub acr_values: String,
    /// Off only for stand-ins in tests, which listen on loopback over http.
    pub require_public: bool,
}

impl Provider {
    pub async fn discover(&self) -> Result<Discovery> {
        discover_checked(&self.http, &self.issuer, self.require_public).await
    }
}

fn unavailable(message: impl Into<String>) -> AppError {
    AppError::new(ErrorKind::Unavailable, message.into())
}

fn https(url: &str) -> bool {
    crate::net::check_public_url(url).is_ok()
}

pub async fn discover(http: &reqwest::Client, issuer: &str) -> Result<Discovery> {
    discover_checked(http, issuer, true).await
}

async fn discover_checked(http: &reqwest::Client, issuer: &str, require_public: bool) -> Result<Discovery> {
    let issuer = issuer.trim_end_matches('/');
    let url = format!("{issuer}/.well-known/openid-configuration");
    let found: Discovery = http
        .get(&url)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|e| unavailable("The eID broker is not reachable right now").with_cause(e))?
        .json()
        .await
        .map_err(|e| unavailable("The eID broker's configuration is not valid").with_cause(e))?;
    if found.issuer.trim_end_matches('/') != issuer {
        return Err(unavailable("The eID broker names a different issuer than the one configured"));
    }
    if require_public && ![&found.authorization_endpoint, &found.token_endpoint, &found.jwks_uri].iter().all(|u| https(u)) {
        return Err(unavailable("The eID broker's endpoints must be public https addresses"));
    }
    Ok(found)
}

pub fn pkce_challenge(verifier: &str) -> String {
    B64URL.encode(Sha256::digest(verifier.as_bytes()))
}

pub struct AuthorizeRequest<'a> {
    pub client_id: &'a str,
    pub redirect_uri: &'a str,
    pub state: &'a str,
    pub nonce: &'a str,
    pub verifier: &'a str,
    pub scope: &'a str,
    pub acr_values: &'a str,
    pub login_hint: Option<&'a str>,
}

pub fn authorize_url(discovery: &Discovery, request: &AuthorizeRequest) -> Result<String> {
    let mut url =
        reqwest::Url::parse(&discovery.authorization_endpoint).map_err(|_| unavailable("The eID broker's address is not valid"))?;
    {
        let mut query = url.query_pairs_mut();
        query
            .append_pair("response_type", "code")
            .append_pair("client_id", request.client_id)
            .append_pair("redirect_uri", request.redirect_uri)
            .append_pair("scope", request.scope)
            .append_pair("state", request.state)
            .append_pair("nonce", request.nonce)
            .append_pair("code_challenge", &pkce_challenge(request.verifier))
            .append_pair("code_challenge_method", "S256")
            .append_pair("response_mode", "query")
            .append_pair("prompt", "login");
        if !request.acr_values.trim().is_empty() {
            query.append_pair("acr_values", request.acr_values.trim());
        }
        if let Some(hint) = request.login_hint {
            query.append_pair("login_hint", hint);
        }
    }
    Ok(url.to_string())
}

pub struct Exchange<'a> {
    pub client_id: &'a str,
    pub client_secret: &'a str,
    pub code: &'a str,
    pub verifier: &'a str,
    pub redirect_uri: &'a str,
}

pub async fn exchange(http: &reqwest::Client, discovery: &Discovery, request: &Exchange<'_>) -> Result<String> {
    let mut form = vec![
        ("grant_type", "authorization_code"),
        ("code", request.code),
        ("redirect_uri", request.redirect_uri),
        ("code_verifier", request.verifier),
    ];
    let mut post = http.post(&discovery.token_endpoint);
    if client_secret_post(discovery) {
        form.extend([("client_id", request.client_id), ("client_secret", request.client_secret)]);
    } else {
        post = post.basic_auth(form_encode(request.client_id), Some(form_encode(request.client_secret)));
    }
    let response = post.form(&form).send().await.map_err(|e| unavailable("The eID broker is not reachable right now").with_cause(e))?;
    if !response.status().is_success() {
        return Err(unavailable(format!("The eID broker refused the sign-in ({})", response.status())));
    }
    let body: Value = response.json().await.map_err(|e| unavailable("The eID broker answered in an unexpected way").with_cause(e))?;
    body["id_token"].as_str().map(str::to_string).ok_or_else(|| unavailable("The eID broker sent no ID token"))
}

/// Basic auth is the default (OpenID Connect Discovery 1.0, section 3); the secret goes in the body
/// only for a provider that does not take Basic.
fn client_secret_post(discovery: &Discovery) -> bool {
    let methods = &discovery.token_endpoint_auth_methods_supported;
    methods.iter().any(|m| m == "client_secret_post") && !methods.iter().any(|m| m == "client_secret_basic")
}

/// RFC 6749 2.3.1: client credentials are form-encoded before they go into Basic auth.
fn form_encode(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
}

pub async fn keys(http: &reqwest::Client, discovery: &Discovery) -> Result<Vec<Key>> {
    let jwks: Value = http
        .get(&discovery.jwks_uri)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|e| unavailable("The eID broker's keys are not reachable").with_cause(e))?
        .json()
        .await
        .map_err(|e| unavailable("The eID broker's keys are not valid").with_cause(e))?;
    Ok(Key::from_jwks(&jwks))
}

const SKEW: i64 = 60;

/// The claims of a valid ID token for this client and nonce.
pub fn verify_id_token(token: &str, keys: &[Key], issuer: &str, client_id: &str, nonce: &str, now: i64) -> Result<Value> {
    let claims = jws::verify(token, keys).map_err(|e| unavailable("The eID broker's ID token could not be verified").with_cause(e))?;
    let reject = |why: &str| Err(unavailable(format!("The eID broker's ID token was refused: {why}")));
    if claims["iss"].as_str().map(|s| s.trim_end_matches('/')) != Some(issuer.trim_end_matches('/')) {
        return reject("wrong issuer");
    }
    let audience_ok = match &claims["aud"] {
        Value::String(aud) => aud == client_id,
        Value::Array(auds) => {
            auds.iter().any(|a| a.as_str() == Some(client_id)) && (auds.len() == 1 || claims["azp"].as_str() == Some(client_id))
        }
        _ => false,
    };
    if !audience_ok {
        return reject("wrong audience");
    }
    match claims["exp"].as_i64() {
        Some(exp) if exp + SKEW > now => {}
        _ => return reject("expired"),
    }
    if claims["iat"].as_i64().is_some_and(|iat| iat > now + SKEW) {
        return reject("issued in the future");
    }
    if !claims["nonce"].as_str().is_some_and(|n| opensign_crypto::constant_time_eq(n.as_bytes(), nonce.as_bytes())) {
        return reject("wrong nonce");
    }
    Ok(claims)
}

/// The person in the claims. Brokers name the national identity number differently.
pub fn identity(claims: &Value) -> Option<Identity> {
    let name = name(claims)?;
    let number = ["ssn", "nin", "socialno", "personal_number", "personalNumber", "national_id", "cprNumberIdentifier", "hetu"]
        .iter()
        .find_map(|k| claims[*k].as_str())
        .map(str::to_string);
    Some(Identity { name, personal_number: number })
}

pub fn name(claims: &Value) -> Option<String> {
    claims["name"].as_str().filter(|n| !n.trim().is_empty()).map(|n| n.trim().to_string()).or_else(|| {
        let given = claims["given_name"].as_str().unwrap_or_default();
        let family = claims["family_name"].as_str().unwrap_or_default();
        let full = format!("{given} {family}");
        (!full.trim().is_empty()).then(|| full.trim().to_string())
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::eid::jws::testing::{jwks, keypair, sign};

    #[test]
    fn checks_every_claim() {
        let (private, _) = keypair();
        let keys = Key::from_jwks(&jwks(&private));
        let now = 1_800_000_000;
        let good = json!({ "iss": "https://broker.example", "aud": "client", "exp": now + 300, "iat": now, "nonce": "n1", "name": "Ada Lovelace", "ssn": "199001010017" });
        let token = |claims: &Value| sign(&private, &json!({ "alg": "RS256", "kid": "k1" }), claims);
        let check = |claims: &Value| verify_id_token(&token(claims), &keys, "https://broker.example/", "client", "n1", now);

        let claims = check(&good).expect("valid");
        assert_eq!(identity(&claims), Some(Identity { name: "Ada Lovelace".into(), personal_number: Some("199001010017".into()) }));
        for (field, value) in [
            ("iss", json!("https://evil.example")),
            ("aud", json!("other")),
            ("aud", json!(["client", "other"])),
            ("exp", json!(now - 120)),
            ("iat", json!(now + 600)),
            ("nonce", json!("n2")),
            ("nonce", Value::Null),
        ] {
            let mut bad = good.clone();
            bad[field] = value.clone();
            assert!(check(&bad).is_err(), "{field} = {value}");
        }
        let mut multi = good.clone();
        multi["aud"] = json!(["client", "other"]);
        multi["azp"] = json!("client");
        assert!(check(&multi).is_ok());
    }

    #[test]
    fn authorize_url_carries_pkce_and_nonce() {
        let discovery = Discovery {
            issuer: "https://broker.example".into(),
            authorization_endpoint: "https://broker.example/authorize".into(),
            token_endpoint: "https://broker.example/token".into(),
            jwks_uri: "https://broker.example/jwks".into(),
            token_endpoint_auth_methods_supported: Vec::new(),
        };
        let url = authorize_url(
            &discovery,
            &AuthorizeRequest {
                client_id: "c",
                redirect_uri: "https://sign.example/api/eid/callback",
                state: "s",
                nonce: "n",
                verifier: "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk",
                scope: "openid",
                acr_values: "urn:grn:authn:no:bankid",
                login_hint: None,
            },
        )
        .expect("url");
        assert!(url.contains("code_challenge=E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"), "RFC 7636 appendix B");
        assert!(
            url.contains("code_challenge_method=S256")
                && url.contains("nonce=n")
                && url.contains("acr_values=urn%3Agrn%3Aauthn%3Ano%3Abankid")
        );
    }
}
