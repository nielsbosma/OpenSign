//! Swedish BankID, relying-party API v6.0. The connection is mutually authenticated with the
//! relying party's certificate, and only BankID's own CA is trusted for the server.

use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use hmac::{Hmac, Mac};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::Sha256;

use super::Identity;
use crate::error::{AppError, ErrorKind, Result, bad_request, conflict};
use crate::settings::EidSettings;

const PRODUCTION: &str = "https://appapi2.bankid.com/rp/v6.0";
const TEST: &str = "https://appapi2.test.bankid.com/rp/v6.0";
/// BankID's server roots, checked against the live servers when they were added.
const ROOT: &str = include_str!("certs/bankid-root.pem");
const TEST_ROOT: &str = include_str!("certs/bankid-test-root.pem");
/// BankID's public certificate for the test environment (FP Testcert 5), so test mode works with no setup.
const TEST_RP: &str = include_str!("certs/bankid-test-rp.pem");

/// Integration tests point BankID at a local stand-in.
#[cfg(feature = "testing")]
pub static TEST_BASE: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

pub struct BankId {
    client: reqwest::Client,
    base: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Order {
    pub order_ref: String,
    pub auto_start_token: String,
    pub qr_start_token: String,
    pub qr_start_secret: String,
}

#[derive(Debug)]
pub enum Collected {
    Pending { hint: Option<String> },
    Failed { hint: Option<String> },
    Complete { identity: Identity, evidence: Value },
}

fn unavailable(message: &str) -> AppError {
    AppError::new(ErrorKind::Unavailable, message.to_string())
}

impl BankId {
    pub fn from_settings(settings: &EidSettings) -> Result<Self> {
        if !settings.bankid_enabled {
            return Err(bad_request("Swedish BankID is not set up. An admin enables it in Admin settings."));
        }
        #[cfg(feature = "testing")]
        if let Some(base) = TEST_BASE.lock().ok().and_then(|b| b.clone()) {
            return Ok(Self { client: reqwest::Client::new(), base });
        }
        let own = settings.bankid_certificate.as_deref().filter(|c| !c.trim().is_empty());
        let identity = match (own, settings.bankid_test) {
            (Some(pem), _) => pem,
            (None, true) => TEST_RP,
            (None, false) => {
                return Err(unavailable("Swedish BankID needs your relying-party certificate. An admin adds it in Admin settings."));
            }
        };
        let ca = match settings.bankid_ca_certificate.trim() {
            "" if settings.bankid_test => TEST_ROOT,
            "" => ROOT,
            own => own,
        };
        let client = crate::net::client_with_certificate(identity, Some(ca))
            .map_err(|e| unavailable(&format!("Swedish BankID is not set up correctly: {e}")))?;
        Ok(Self { client, base: if settings.bankid_test { TEST } else { PRODUCTION }.into() })
    }

    async fn post(&self, method: &str, body: Value) -> Result<Value> {
        let response = self
            .client
            .post(format!("{}/{method}", self.base))
            .json(&body)
            .send()
            .await
            .map_err(|e| unavailable("BankID is not reachable right now. Try again in a moment.").with_cause(e))?;
        let status = response.status();
        let body: Value = response.json().await.unwrap_or(Value::Null);
        if status.is_success() {
            return Ok(body);
        }
        match body["errorCode"].as_str() {
            Some("alreadyInProgress") => Err(conflict(
                "A BankID identification for this person is already in progress. Cancel it in the BankID app, or wait a moment.",
            )),
            Some("invalidParameters") => Err(bad_request("BankID refused the request. Check the personal number on this party.")),
            Some(code) => Err(unavailable(&format!("BankID could not start ({code}). Try again in a moment."))),
            None => Err(unavailable(&format!("BankID answered with {status}. Try again in a moment."))),
        }
    }

    /// Starts an identification, or a signature of `visible_text` when it is given. With a
    /// personal number, only that person's BankID can complete it.
    pub async fn start(&self, end_user_ip: &str, personal_number: Option<&str>, visible_text: Option<&str>) -> Result<Order> {
        let mut body = json!({ "endUserIp": end_user_ip });
        if let Some(number) = personal_number {
            body["requirement"] = json!({ "personalNumber": number });
        }
        let method = match visible_text {
            Some(text) => {
                body["userVisibleData"] = json!(B64.encode(text));
                "sign"
            }
            None => "auth",
        };
        serde_json::from_value(self.post(method, body).await?).map_err(|_| unavailable("BankID answered in an unexpected way"))
    }

    pub async fn collect(&self, order_ref: &str) -> Result<Collected> {
        let body = self.post("collect", json!({ "orderRef": order_ref })).await?;
        let hint = body["hintCode"].as_str().map(str::to_string);
        Ok(match body["status"].as_str() {
            Some("pending") => Collected::Pending { hint },
            Some("complete") => {
                let data = &body["completionData"];
                let user = &data["user"];
                let (Some(name), Some(number)) = (user["name"].as_str(), user["personalNumber"].as_str()) else {
                    return Err(unavailable("BankID answered without the person's details"));
                };
                Collected::Complete {
                    identity: Identity { name: name.to_string(), personal_number: Some(number.to_string()) },
                    evidence: json!({
                        "provider": "se_bankid",
                        "orderRef": order_ref,
                        "signature": data["signature"],
                        "ocspResponse": data["ocspResponse"],
                        "bankIdIssueDate": data["bankIdIssueDate"],
                        "device": data["device"],
                        "stepUp": data["stepUp"],
                    }),
                }
            }
            _ => Collected::Failed { hint },
        })
    }

    pub async fn cancel(&self, order_ref: &str) -> Result<()> {
        self.post("cancel", json!({ "orderRef": order_ref })).await.map(|_| ())
    }
}

/// The animated QR code's content `seconds` after the order started.
pub fn qr_code(start_token: &str, start_secret: &str, seconds: i64) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(start_secret.as_bytes()).expect("HMAC takes any key length");
    mac.update(seconds.to_string().as_bytes());
    format!("bankid.{start_token}.{seconds}.{}", hex::encode(mac.finalize().into_bytes()))
}

/// What to tell the person for a BankID hint code, in BankID's recommended wording.
pub fn message(hint: Option<&str>, failed: bool) -> &'static str {
    match (hint, failed) {
        (Some("outstandingTransaction" | "noClient"), false) => "Start your BankID app.",
        (Some("started"), false) => "Searching for BankID. It may take a little while.",
        (Some("userMrtd"), false) => "Process your machine-readable travel document using the BankID app.",
        (Some("userCallConfirm"), false) => "Confirm the call in your BankID app.",
        (Some("userSign"), false) => "Enter your security code in the BankID app and select Identify or Sign.",
        (_, false) => "Waiting for BankID.",
        (Some("expiredTransaction"), true) => "The BankID app did not respond in time. Try again.",
        (Some("certificateErr"), true) => "The BankID you are trying to use is blocked or too old. Get a new one from your bank.",
        (Some("userCancel"), true) => "The identification was canceled.",
        (Some("cancelled"), true) => "The identification was canceled. Try again.",
        (Some("startFailed"), true) => "The BankID app could not be found. Install it and get a BankID from your bank, then try again.",
        (Some("userDeclinedCall"), true) => "You declined the call. Try again.",
        (_, true) => "The identification did not go through. Try again.",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qr_codes_follow_the_spec() {
        // The worked example in BankID's relying-party guidelines.
        let code = qr_code("67df3917-fa0d-44e5-b327-edcc928297f8", "d28db9a7-4cde-429e-a983-359be676944c", 0);
        assert_eq!(code, "bankid.67df3917-fa0d-44e5-b327-edcc928297f8.0.dc69358e712458a66a7525beef148ae8526b1c71610eff2c16cdffb4cdac9bf8");
    }
}
