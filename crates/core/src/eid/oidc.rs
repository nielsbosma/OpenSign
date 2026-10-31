//! OpenID Connect authorization code flow with PKCE, against an eID broker or the provider people
//! sign in with. The ID token's signature, issuer, audience, expiry and nonce are all checked, even
//! though it arrives straight from the token endpoint.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

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

/// A configured OpenID Connect provider: the generic broker, TIC Identity, or single sign-on.
pub struct Provider {
    pub http: reqwest::Client,
    pub issuer: String,
    pub client_id: String,
    pub client_secret: String,
    pub scope: String,
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
    // A pinned setting skips validation, so this holds for those too.
    if require_public && !https(issuer) {
        return Err(unavailable("The identity provider's issuer must be a public https address"));
    }
    let url = format!("{issuer}/.well-known/openid-configuration");
    let found: Discovery = http
        .get(&url)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|e| unavailable("The identity provider is not reachable right now").with_cause(e))?
        .json()
        .await
        .map_err(|e| unavailable("The identity provider's configuration is not valid").with_cause(e))?;
    if found.issuer.trim_end_matches('/') != issuer {
        return Err(unavailable("The identity provider names a different issuer than the one configured"));
    }
    if require_public && ![&found.authorization_endpoint, &found.token_endpoint, &found.jwks_uri].iter().all(|u| https(u)) {
        return Err(unavailable("The identity provider's endpoints must be public https addresses"));
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
    pub prompt: Option<&'a str>,
}

pub fn authorize_url(discovery: &Discovery, request: &AuthorizeRequest) -> Result<String> {
    let mut url =
        reqwest::Url::parse(&discovery.authorization_endpoint).map_err(|_| unavailable("The identity provider's address is not valid"))?;
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
            .append_pair("response_mode", "query");
        if let Some(prompt) = request.prompt {
            query.append_pair("prompt", prompt);
        }
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
    let response =
        post.form(&form).send().await.map_err(|e| unavailable("The identity provider is not reachable right now").with_cause(e))?;
    if !response.status().is_success() {
        return Err(unavailable(format!("The identity provider refused the sign-in ({})", response.status())));
    }
    let body: Value =
        response.json().await.map_err(|e| unavailable("The identity provider answered in an unexpected way").with_cause(e))?;
    body["id_token"].as_str().map(str::to_string).ok_or_else(|| unavailable("The identity provider sent no ID token"))
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
        .map_err(|e| unavailable("The identity provider's keys are not reachable").with_cause(e))?
        .json()
        .await
        .map_err(|e| unavailable("The identity provider's keys are not valid").with_cause(e))?;
    Ok(Key::from_jwks(&jwks))
}

const CACHE_FOR: Duration = Duration::from_secs(300);
/// A token signed with an unknown key may mean the provider rotated, but anyone can send one, so
/// this is as often as such a token makes us ask.
const REFRESH_KEYS_AFTER: Duration = Duration::from_secs(60);

type Cache<T> = Mutex<Option<HashMap<String, (Instant, T)>>>;

static DISCOVERIES: Cache<Discovery> = Mutex::new(None);
static KEYS: Cache<Vec<Key>> = Mutex::new(None);

fn cached<T: Clone>(cache: &Cache<T>, key: &str, within: Duration) -> Option<T> {
    let cache = cache.lock().unwrap_or_else(|e| e.into_inner());
    cache.as_ref()?.get(key).filter(|(at, _)| at.elapsed() < within).map(|(_, value)| value.clone())
}

fn remember<T>(cache: &Cache<T>, key: &str, value: T) {
    cache.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_default().insert(key.to_string(), (Instant::now(), value));
}

impl Provider {
    /// As `discover`, remembered for a few minutes, for sign-ins that should not wait on the provider twice.
    pub async fn discover_cached(&self) -> Result<Discovery> {
        let key = format!("{}|{}", self.require_public, self.issuer.trim_end_matches('/'));
        if let Some(found) = cached(&DISCOVERIES, &key, CACHE_FOR) {
            return Ok(found);
        }
        let found = self.discover().await?;
        remember(&DISCOVERIES, &key, found.clone());
        Ok(found)
    }

    /// The claims of a valid ID token, with the provider's keys remembered for a few minutes. When
    /// they do not verify it, newer keys are fetched and tried once, since providers rotate.
    pub async fn verify_cached(&self, discovery: &Discovery, token: &str, nonce: &str, now: i64) -> Result<Value> {
        let verify = |keys: &[Key]| verify_id_token(token, keys, &self.issuer, &self.client_id, nonce, &self.acr_values, now);
        let uri = &discovery.jwks_uri;
        if let Some(known) = cached(&KEYS, uri, CACHE_FOR) {
            let result = verify(&known);
            if result.is_ok() || cached(&KEYS, uri, REFRESH_KEYS_AFTER).is_some() {
                return result;
            }
        }
        let fresh = keys(&self.http, discovery).await?;
        remember(&KEYS, uri, fresh.clone());
        verify(&fresh)
    }
}

const SKEW: i64 = 60;

/// The claims of a valid ID token for this client and nonce. With `acr_values`, the token must say
/// one of them was used: the browser can drop them from the authorization request and pick a
/// weaker method at the broker.
pub fn verify_id_token(token: &str, keys: &[Key], issuer: &str, client_id: &str, nonce: &str, acr_values: &str, now: i64) -> Result<Value> {
    let claims =
        jws::verify(token, keys).map_err(|e| unavailable("The identity provider's ID token could not be verified").with_cause(e))?;
    let reject = |why: &str| Err(unavailable(format!("The identity provider's ID token was refused: {why}")));
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
    let mut accepted = acr_values.split_whitespace().peekable();
    if accepted.peek().is_some()
        && !claims["acr"]
            .as_str()
            .is_some_and(|acr| accepted.any(|a| acr == a || acr.strip_prefix(a).is_some_and(|rest| rest.starts_with(':'))))
    {
        return reject("the identification method was not one of those required");
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
        let check = |claims: &Value| verify_id_token(&token(claims), &keys, "https://broker.example/", "client", "n1", "", now);

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
        let required =
            |claims: &Value| verify_id_token(&token(claims), &keys, "https://broker.example", "client", "n1", "urn:a urn:b", now);
        assert!(required(&good).is_err(), "no acr when one is required");
        let mut strong = good.clone();
        strong["acr"] = json!("urn:b");
        assert!(required(&strong).is_ok());
        strong["acr"] = json!("urn:b:same-device");
        assert!(required(&strong).is_ok(), "a more specific method under a required one");
        strong["acr"] = json!("urn:weak");
        assert!(required(&strong).is_err());
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
                prompt: Some("login"),
            },
        )
        .expect("url");
        assert!(url.contains("code_challenge=E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"), "RFC 7636 appendix B");
        assert!(
            url.contains("code_challenge_method=S256")
                && url.contains("nonce=n")
                && url.contains("acr_values=urn%3Agrn%3Aauthn%3Ano%3Abankid")
                && url.contains("prompt=login")
        );
    }
}
