//! Freja eID authentication API. Requests use the relying party's client certificate; results
//! are only believed when Freja's signature over them checks out.

use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use serde_json::{Value, json};

use super::Identity;
use super::jws::{self, Key};
use crate::error::{AppError, ErrorKind, Result, bad_request, conflict};
use crate::settings::EidSettings;

const PRODUCTION: &str = "https://services.prod.frejaeid.com";
const TEST: &str = "https://services.test.frejaeid.com";

pub struct Freja {
    client: reqwest::Client,
    base: String,
    signer: Key,
}

pub enum User<'a> {
    /// A Swedish personal number as 12 digits.
    Ssn(&'a str),
    Email(&'a str),
    /// Whoever scans the QR code.
    Inferred,
}

pub enum Outcome {
    Pending,
    Failed(&'static str),
    Complete { identity: Identity, evidence: Value },
}

fn unavailable(message: &str) -> AppError {
    AppError::new(ErrorKind::Unavailable, message.to_string())
}

/// The QR code content for an `Inferred` request.
pub fn qr_code(auth_ref: &str) -> String {
    let encoded: String = auth_ref
        .bytes()
        .map(|b| if b.is_ascii_alphanumeric() || b == b'-' { (b as char).to_string() } else { format!("%{b:02X}") })
        .collect();
    format!("frejaeid://bindUserToTransaction?transactionReference={encoded}")
}

impl Freja {
    pub fn from_settings(settings: &EidSettings) -> Result<Self> {
        if !settings.freja_enabled {
            return Err(bad_request("Freja eID is not set up. An admin enables it in Admin settings."));
        }
        let identity = settings.freja_certificate.as_deref().filter(|c| !c.trim().is_empty());
        let (Some(identity), false) = (identity, settings.freja_signing_certificate.trim().is_empty()) else {
            return Err(unavailable("Freja eID is missing its certificates. An admin adds them in Admin settings."));
        };
        let client = crate::net::client_with_certificate(identity, Some(&settings.freja_ca_certificate))
            .map_err(|e| unavailable(&format!("Freja eID is not set up correctly: {e}")))?;
        let signer = Key::from_certificate_pem(&settings.freja_signing_certificate)
            .map_err(|e| unavailable(&format!("Freja's signing certificate is not usable: {e}")))?;
        Ok(Self { client, base: if settings.freja_test { TEST } else { PRODUCTION }.into(), signer })
    }

    #[cfg(any(test, feature = "testing"))]
    pub fn with_client(client: reqwest::Client, base: &str, signer: Key) -> Self {
        Self { client, base: base.into(), signer }
    }

    async fn post(&self, path: &str, field: &str, request: Value) -> Result<Value> {
        let response = self
            .client
            .post(format!("{}{path}", self.base))
            .form(&[(field, B64.encode(request.to_string()))])
            .send()
            .await
            .map_err(|e| unavailable("Freja eID is not reachable right now. Try again in a moment.").with_cause(e))?;
        let status = response.status();
        let body: Value = response.json().await.unwrap_or(Value::Null);
        if status.is_success() {
            return Ok(body);
        }
        match body["code"].as_i64() {
            Some(2000) => Err(conflict("A Freja eID request for this person is already in progress. Wait a moment and try again.")),
            _ if status.is_client_error() => {
                let reason: String = body["message"].as_str().unwrap_or("no reason given").chars().take(200).collect();
                Err(bad_request("Freja eID refused the request: {reason}").params([("reason", &reason)]))
            }
            _ => Err(unavailable(&format!("Freja eID answered with {status}. Try again in a moment."))),
        }
    }

    pub async fn start(&self, user: User<'_>) -> Result<String> {
        let (kind, info) = match user {
            User::Ssn(ssn) => ("SSN", B64.encode(json!({ "country": "SE", "ssn": ssn }).to_string())),
            User::Email(email) => ("EMAIL", email.to_string()),
            User::Inferred => ("INFERRED", "N/A".to_string()),
        };
        let request = json!({
            "userInfoType": kind,
            "userInfo": info,
            "minRegistrationLevel": "EXTENDED",
            "attributesToReturn": [{ "attribute": "BASIC_USER_INFO" }, { "attribute": "SSN" }],
        });
        let body = self.post("/authentication/1.0/initAuthentication", "initAuthRequest", request).await?;
        body["authRef"].as_str().map(str::to_string).ok_or_else(|| unavailable("Freja eID answered in an unexpected way"))
    }

    pub async fn result(&self, auth_ref: &str) -> Result<Outcome> {
        let body = self.post("/authentication/1.0/getOneResult", "getOneAuthResultRequest", json!({ "authRef": auth_ref })).await?;
        Ok(match body["status"].as_str() {
            Some("STARTED" | "DELIVERED_TO_MOBILE") => Outcome::Pending,
            Some("APPROVED") => self.approved(auth_ref, &body)?,
            Some("REJECTED") => Outcome::Failed("The identification was declined in Freja eID."),
            Some("EXPIRED") => Outcome::Failed("Freja eID did not respond in time. Try again."),
            Some("CANCELED" | "RP_CANCELED") => Outcome::Failed("The identification was canceled."),
            _ => Outcome::Failed("The identification did not go through. Try again."),
        })
    }

    /// Only the signed details count: the unsigned copy next to them is ignored.
    fn approved(&self, auth_ref: &str, body: &Value) -> Result<Outcome> {
        let details = body["details"].as_str().ok_or_else(|| unavailable("Freja eID sent an unsigned result"))?;
        let signed = jws::verify(details, std::slice::from_ref(&self.signer))
            .map_err(|e| AppError::new(ErrorKind::Unavailable, "Freja eID's result could not be verified").with_cause(e))?;
        if signed["authRef"].as_str() != Some(auth_ref) || signed["status"].as_str() != Some("APPROVED") {
            return Err(unavailable("Freja eID's signed result is for another request"));
        }
        let attributes = &signed["requestedAttributes"];
        let info = &attributes["basicUserInfo"];
        let name = format!("{} {}", info["name"].as_str().unwrap_or_default(), info["surname"].as_str().unwrap_or_default());
        let ssn = attributes["ssn"]["ssn"].as_str().map(str::to_string);
        if name.trim().is_empty() || ssn.is_none() {
            return Err(unavailable("Freja eID answered without the person's details"));
        }
        Ok(Outcome::Complete {
            identity: Identity { name: name.trim().to_string(), personal_number: ssn },
            evidence: json!({ "provider": "freja", "authRef": auth_ref, "details": details }),
        })
    }

    pub async fn cancel(&self, auth_ref: &str) -> Result<()> {
        self.post("/authentication/1.0/cancel", "cancelAuthRequest", json!({ "authRef": auth_ref })).await.map(|_| ())
    }
}
