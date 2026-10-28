use crate::context::App;
use crate::error::{AppError, ErrorKind, Result, bad_request};
use crate::settings::SmsSettings;

/// E.164: a plus sign and 8 to 15 digits. Spaces, dashes and brackets are dropped; a leading 00
/// becomes +.
pub fn normalize_phone(input: &str) -> Result<String> {
    let trimmed: String = input.chars().filter(|c| !matches!(c, ' ' | '-' | '(' | ')' | '.')).collect();
    let digits = trimmed
        .strip_prefix('+')
        .or_else(|| trimmed.strip_prefix("00"))
        .ok_or_else(|| bad_request("Enter the phone number with its country code, e.g. +46 70 123 45 67"))?;
    if !(8..=15).contains(&digits.len()) || !digits.chars().all(|c| c.is_ascii_digit()) || digits.starts_with('0') {
        return Err(bad_request("Enter the phone number with its country code, e.g. +46 70 123 45 67"));
    }
    Ok(format!("+{digits}"))
}

pub fn is_configured(settings: &SmsSettings) -> bool {
    matches!(settings.provider.as_str(), "46elks" | "twilio")
        && !settings.username.is_empty()
        && settings.password.as_deref().is_some_and(|p| !p.is_empty())
}

/// Sender ids: up to 11 letters and digits, or a phone number.
fn sender(settings: &SmsSettings) -> String {
    let s: String = settings.sender.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '+').take(15).collect();
    if s.is_empty() { "OpenSign".into() } else { s }
}

pub async fn send(app: &App, settings: &SmsSettings, to: &str, text: &str) -> Result<()> {
    let to = normalize_phone(to)?;
    if !is_configured(settings) {
        if app.env.development {
            tracing::info!(to = %to, "SMS (no provider configured in development): {text}");
            return Ok(());
        }
        return Err(AppError::new(ErrorKind::Unavailable, "SMS is not set up. An admin chooses an SMS provider in Admin settings."));
    }
    let password = settings.password.clone().unwrap_or_default();
    let request = match settings.provider.as_str() {
        "46elks" => app.http.public.post("https://api.46elks.com/a1/sms").form(&[
            ("from", sender(settings)),
            ("to", to),
            ("message", text.to_string()),
        ]),
        _ => app
            .http
            .public
            .post(format!("https://api.twilio.com/2010-04-01/Accounts/{}/Messages.json", urlencode(&settings.username)))
            .form(&[("From", sender(settings)), ("To", to), ("Body", text.to_string())]),
    };
    let response = request
        .basic_auth(&settings.username, Some(password))
        .send()
        .await
        .map_err(|e| AppError::new(ErrorKind::Unavailable, "The SMS provider is not reachable").with_cause(e))?;
    if !response.status().is_success() {
        let status = response.status();
        return Err(AppError::new(ErrorKind::Unavailable, format!("The SMS provider refused the message ({status})")));
    }
    Ok(())
}

fn urlencode(value: &str) -> String {
    value.chars().filter(|c| c.is_ascii_alphanumeric()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_phone_numbers() {
        assert_eq!(normalize_phone("+46 70-123 45 67").ok().as_deref(), Some("+46701234567"));
        assert_eq!(normalize_phone("0046701234567").ok().as_deref(), Some("+46701234567"));
        assert!(normalize_phone("070 123 45 67").is_err());
        assert!(normalize_phone("+46 70 123 45 67; rm").is_err());
    }
}
