use serde_json::json;

use crate::context::App;
use crate::error::{AppError, ErrorKind, Result};
use crate::settings::{EmailSettings, OrgSettings};

pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// Strips characters that could start a new header line.
fn header_safe(text: &str) -> String {
    text.chars().filter(|c| !c.is_control()).collect()
}

pub struct Message {
    pub to: String,
    pub to_name: Option<String>,
    pub subject: String,
    /// Plain paragraphs; each is escaped.
    pub paragraphs: Vec<String>,
    pub button: Option<(String, String)>,
    pub footer: Option<String>,
    pub attachments: Vec<Attachment>,
}

pub struct Attachment {
    pub filename: String,
    pub content: Vec<u8>,
}

fn encoded_attachments(message: &Message, name_key: &str, content_key: &str) -> serde_json::Value {
    use base64::Engine;
    serde_json::Value::Array(
        message
            .attachments
            .iter()
            .map(|a| json!({ name_key: header_safe(&a.filename).replace(['/', '\\'], "_"), content_key: base64::engine::general_purpose::STANDARD.encode(&a.content) }))
            .collect(),
    )
}

pub struct Rendered {
    pub html: String,
    pub text: String,
}

pub fn render(message: &Message, org: &OrgSettings) -> Rendered {
    let color =
        if org.brand_color.len() == 7 && org.brand_color.starts_with('#') && org.brand_color[1..].chars().all(|c| c.is_ascii_hexdigit()) {
            org.brand_color.as_str()
        } else {
            "#4f46e5"
        };
    let org_name = if org.name.is_empty() { "OpenSign" } else { org.name.as_str() };
    let mut html = format!(
        r#"<!doctype html><html><body style="margin:0;background:#f5f5f4;font-family:-apple-system,Segoe UI,Roboto,sans-serif;color:#1c1917"><table role="presentation" width="100%" cellpadding="0" cellspacing="0"><tr><td align="center" style="padding:32px 16px"><table role="presentation" width="560" cellpadding="0" cellspacing="0" style="background:#fff;border:1px solid #e7e5e4;border-radius:12px"><tr><td style="padding:32px"><p style="margin:0 0 24px;font-weight:600">{}</p>"#,
        escape(org_name)
    );
    let mut text = String::new();
    for p in &message.paragraphs {
        html.push_str(&format!(r#"<p style="margin:0 0 16px;line-height:1.5;white-space:pre-line">{}</p>"#, escape(p)));
        text.push_str(p);
        text.push_str("\n\n");
    }
    if let Some((label, url)) = &message.button
        && (url.starts_with("https://") || url.starts_with("http://"))
    {
        html.push_str(&format!(
                r#"<p style="margin:24px 0"><a href="{}" style="background:{color};color:#fff;text-decoration:none;padding:12px 20px;border-radius:8px;display:inline-block;font-weight:600">{}</a></p>"#,
                escape(url),
                escape(label)
            ));
        text.push_str(&format!("{label}: {url}\n\n"));
    }
    if let Some(footer) = &message.footer {
        html.push_str(&format!(r#"<p style="margin:24px 0 0;font-size:12px;color:#78716c">{}</p>"#, escape(footer)));
        text.push_str(footer);
        text.push('\n');
    }
    html.push_str("</td></tr></table></td></tr></table></body></html>");
    Rendered { html, text }
}

fn dev_mailpit_url() -> String {
    std::env::var("MAILPIT_URL").unwrap_or_else(|_| "http://localhost:8026".into())
}

/// Sends through Resend. In development without a Resend key, mail goes to Mailpit instead.
pub async fn send(app: &App, settings: &EmailSettings, org: &OrgSettings, message: &Message) -> Result<()> {
    let rendered = render(message, org);
    let from_name = header_safe(if settings.from_name.is_empty() { &org.name } else { &settings.from_name });
    let from_address = header_safe(&settings.from_address);
    let api_key = settings.api_key.clone().unwrap_or_default();

    if api_key.is_empty() && app.env.development {
        let response = app
            .http
            .internal
            .post(format!("{}/api/v1/send", dev_mailpit_url()))
            .json(&json!({
                "From": { "Email": if from_address.is_empty() { "opensign@localhost" } else { &from_address }, "Name": from_name },
                "To": [{ "Email": header_safe(&message.to), "Name": message.to_name.as_deref().map(header_safe).unwrap_or_default() }],
                "Subject": header_safe(&message.subject),
                "HTML": rendered.html,
                "Text": rendered.text,
                "Attachments": encoded_attachments(message, "Filename", "Content"),
            }))
            .send()
            .await
            .map_err(AppError::internal)?;
        if !response.status().is_success() {
            return Err(AppError::internal(anyhow::anyhow!("Mailpit answered {}", response.status())));
        }
        return Ok(());
    }

    if api_key.is_empty() || from_address.is_empty() {
        return Err(AppError::new(ErrorKind::Unavailable, "Email is not set up. An admin adds a Resend API key in Admin settings."));
    }
    let from =
        if from_name.is_empty() { from_address.clone() } else { format!("{} <{}>", from_name.replace(['<', '>', '"'], ""), from_address) };
    let to = match &message.to_name {
        Some(name) if !name.is_empty() => format!("{} <{}>", header_safe(name).replace(['<', '>', '"'], ""), header_safe(&message.to)),
        _ => header_safe(&message.to),
    };
    let mut body = json!({
        "from": from,
        "to": [to],
        "subject": header_safe(&message.subject),
        "html": rendered.html,
        "text": rendered.text,
    });
    if !message.attachments.is_empty() {
        body["attachments"] = encoded_attachments(message, "filename", "content");
    }
    if !settings.reply_to.is_empty() {
        body["reply_to"] = json!(header_safe(&settings.reply_to));
    }
    let response =
        app.http.public.post("https://api.resend.com/emails").bearer_auth(api_key).json(&body).send().await.map_err(AppError::internal)?;
    let status = response.status();
    if !status.is_success() {
        let detail: serde_json::Value = response.json().await.unwrap_or_default();
        let msg = detail["message"].as_str().unwrap_or("unknown error");
        return Err(AppError::new(ErrorKind::Unavailable, format!("Resend refused the email ({status}): {msg}")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_everything_user_supplied() {
        let org = OrgSettings { name: "<Acme>".into(), brand_color: "red;background:url(x)".into(), ..Default::default() };
        let rendered = render(
            &Message {
                to: "a@b.c".into(),
                to_name: None,
                subject: "s".into(),
                paragraphs: vec!["<script>alert(1)</script>".into()],
                button: Some(("Sign".into(), "javascript:alert(1)".into())),
                footer: None,
                attachments: vec![],
            },
            &org,
        );
        assert!(!rendered.html.contains("<script>"));
        assert!(!rendered.html.contains("javascript:"));
        assert!(!rendered.html.contains("url(x)"));
        assert!(rendered.html.contains("&lt;Acme&gt;"));
    }

    #[test]
    fn strips_header_injection() {
        assert_eq!(header_safe("Hi\r\nBcc: x@y.z"), "HiBcc: x@y.z");
    }
}
