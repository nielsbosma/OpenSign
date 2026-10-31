use chrono::{DateTime, Utc};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::context::App;
use crate::error::{AppError, ErrorKind, Result, bad_request, too_many};
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

/// Refuses a number in a country the install does not send to.
pub fn check_country(settings: &SmsSettings, phone: &str) -> Result<()> {
    let digits = phone.trim_start_matches('+');
    let mut allowed =
        settings.allowed_countries.split([',', ';', ' ']).map(|c| c.trim().trim_start_matches('+')).filter(|c| !c.is_empty()).peekable();
    if allowed.peek().is_none() || allowed.any(|code| digits.starts_with(code)) {
        return Ok(());
    }
    Err(bad_request("SMS cannot be sent to this country. Use a different phone number, or ask an admin to allow the country."))
}

/// Takes one message from the budgets and returns its entry, which `release` gives back if the
/// message could not be sent. Concurrent callers can overshoot by a few messages, which is fine
/// for a brake on cost.
pub async fn reserve(
    conn: &mut PgConnection,
    settings: &SmsSettings,
    phone: &str,
    share_link: Option<Uuid>,
    now: DateTime<Utc>,
) -> Result<Uuid> {
    check_country(settings, phone)?;
    let id = Uuid::now_v7();
    let taken = sqlx::query(
        "insert into sms_log (id, share_link_id, sent_at) select $1, $2, $3
         where ($4 = 0 or (select count(*) from sms_log where sent_at > $3 - interval '1 hour') < $4)
           and ($5 = 0 or (select count(*) from sms_log where sent_at > $3 - interval '1 day') < $5)
           and ($6 = 0 or $2::uuid is null or (select count(*) from sms_log where share_link_id = $2 and sent_at > $3 - interval '1 hour') < $6)",
    )
    .bind(id)
    .bind(share_link)
    .bind(now)
    .bind(settings.hourly_limit as i64)
    .bind(settings.daily_limit as i64)
    .bind(settings.share_link_hourly_limit as i64)
    .execute(&mut *conn)
    .await?;
    if taken.rows_affected() == 0 {
        tracing::warn!(share_link = ?share_link, "an SMS budget in Admin settings was reached");
        return Err(too_many("Too many text messages were sent just now. Try again later."));
    }
    Ok(id)
}

pub async fn release(conn: &mut PgConnection, id: Uuid) -> Result<()> {
    sqlx::query("delete from sms_log where id = $1").bind(id).execute(&mut *conn).await?;
    Ok(())
}

pub async fn prune(conn: &mut PgConnection, now: DateTime<Utc>) -> Result<()> {
    sqlx::query("delete from sms_log where sent_at < $1 - interval '2 days'").bind(now).execute(&mut *conn).await?;
    Ok(())
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

    #[test]
    fn only_allowed_countries_pass() {
        let settings = |allowed: &str| SmsSettings { allowed_countries: allowed.into(), ..SmsSettings::default() };
        assert!(check_country(&settings(""), "+882123456789").is_ok());
        assert!(check_country(&settings("46, +47"), "+46701234567").is_ok());
        assert!(check_country(&settings("46, +47"), "+4712345678").is_ok());
        assert!(check_country(&settings("46, 47"), "+882123456789").is_err());
    }
}
