use serde_json::json;

use crate::context::App;
use crate::error::{AppError, ErrorKind, Result};
use crate::i18n::Lang;
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
    pub lang: Lang,
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

/// `base_url` is where the app is reached; the logo, when set, is loaded from there.
pub fn render(message: &Message, org: &OrgSettings, base_url: &str) -> Rendered {
    let color =
        if org.brand_color.len() == 7 && org.brand_color.starts_with('#') && org.brand_color[1..].chars().all(|c| c.is_ascii_hexdigit()) {
            org.brand_color.as_str()
        } else {
            "#4f46e5"
        };
    let org_name = if org.name.is_empty() { "OpenSign" } else { org.name.as_str() };
    let heading = match crate::services::branding::logo_path(org) {
        Some(path) if base_url.starts_with("https://") || base_url.starts_with("http://") => format!(
            r#"<img src="{}{}" alt="{}" height="40" style="display:block;height:40px;max-width:200px;border:0;margin:0 0 24px">"#,
            escape(base_url.trim_end_matches('/')),
            escape(&path),
            escape(org_name)
        ),
        _ => format!(r#"<p style="margin:0 0 24px;font-weight:600">{}</p>"#, escape(org_name)),
    };
    let mut html = format!(
        r#"<!doctype html><html lang="{}" dir="{}"><body style="margin:0;background:#f5f5f4;font-family:-apple-system,Segoe UI,Roboto,sans-serif;color:#1c1917"><table role="presentation" width="100%" cellpadding="0" cellspacing="0"><tr><td align="center" style="padding:32px 16px"><table role="presentation" width="560" cellpadding="0" cellspacing="0" style="background:#fff;border:1px solid #e7e5e4;border-radius:12px"><tr><td style="padding:32px">{heading}"#,
        message.lang.code(),
        if message.lang.rtl() { "rtl" } else { "ltr" },
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

const NOT_SET_UP: &str = "Email is not set up. An admin connects Resend or an SMTP server in Admin settings.";

pub fn is_smtp(settings: &EmailSettings) -> bool {
    settings.provider == "smtp"
}

pub fn is_configured(settings: &EmailSettings) -> bool {
    !settings.from_address.is_empty()
        && if is_smtp(settings) {
            !settings.smtp_host.trim().is_empty() && settings.smtp_port != 0
        } else {
            settings.api_key.as_deref().is_some_and(|k| !k.is_empty())
        }
}

/// Sends through Resend or SMTP. In development without a Resend key, mail goes to Mailpit instead.
pub async fn send(app: &App, settings: &EmailSettings, org: &OrgSettings, message: &Message) -> Result<()> {
    let rendered = render(message, org, &app.public_url());
    let from_name = header_safe(if settings.from_name.is_empty() { &org.name } else { &settings.from_name });
    let from_address = header_safe(&settings.from_address);
    if is_smtp(settings) {
        if !is_configured(settings) {
            return Err(AppError::new(ErrorKind::Unavailable, NOT_SET_UP));
        }
        return smtp::send(app, settings, &from_name, &from_address, message, rendered).await;
    }
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
        return Err(AppError::new(ErrorKind::Unavailable, NOT_SET_UP));
    }
    let from = if from_name.is_empty() {
        from_address.clone()
    } else {
        format!("{} <{}>", from_name.replace(['<', '>', '"', ',', ';'], ""), from_address)
    };
    let to = match &message.to_name {
        Some(name) if !name.is_empty() => {
            format!("{} <{}>", header_safe(name).replace(['<', '>', '"', ',', ';'], ""), header_safe(&message.to))
        }
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

mod smtp {
    use std::net::SocketAddr;
    use std::time::Duration;

    use lettre::message::header::ContentType;
    use lettre::message::{Attachment, Mailbox, MultiPart};
    use lettre::transport::smtp::authentication::Credentials;
    use lettre::transport::smtp::client::{Tls, TlsParameters};
    use lettre::transport::smtp::extension::ClientId;
    use lettre::{AsyncSmtpTransport, AsyncTransport, Tokio1Executor};

    use super::{Message, Rendered, header_safe};
    use crate::context::App;
    use crate::error::{AppError, ErrorKind, Result, bad_request};
    use crate::settings::EmailSettings;

    /// A host name or IP address, without a scheme, port or path.
    pub fn valid_host(host: &str) -> bool {
        let host = host.trim_start_matches('[').trim_end_matches(']');
        !host.is_empty()
            && host.len() <= 253
            && (host.parse::<std::net::IpAddr>().is_ok() || host.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-'))
    }

    fn mailbox(name: &str, address: &str) -> Result<Mailbox> {
        let address =
            address.parse().map_err(|_| bad_request("{address} is not an address an email can go to").params([("address", &address)]))?;
        let name = name.replace(['<', '>', '"', ',', ';'], "");
        Ok(Mailbox::new((!name.is_empty()).then_some(name), address))
    }

    /// Resolves once and connects to that address, so a name cannot point somewhere else between the
    /// check and the connection. TLS still verifies the certificate against the configured host name.
    async fn resolve(host: &str, port: u16, allow_private: bool) -> Result<SocketAddr> {
        let unreachable = || AppError::new(ErrorKind::Unavailable, format!("{host} could not be found"));
        let addrs: Vec<SocketAddr> = tokio::net::lookup_host((host, port)).await.map_err(|e| unreachable().with_cause(e))?.collect();
        if addrs.is_empty() {
            return Err(unreachable());
        }
        addrs.into_iter().find(|a| allow_private || !crate::net::is_forbidden(a.ip())).ok_or_else(|| {
            AppError::new(
                ErrorKind::Unavailable,
                format!("{host} resolves only to private or reserved addresses, which OpenSign does not connect to"),
            )
        })
    }

    fn describe(host: &str, e: lettre::transport::smtp::Error) -> AppError {
        let message = if e.is_timeout() {
            format!("{host} did not answer in time")
        } else if e.is_tls() {
            format!("A secure connection to {host} failed. Check the security setting and that its certificate is valid for {host}.")
        } else if e.is_client() {
            format!("{host} cannot be used as configured ({e})")
        } else if e.is_permanent() || e.is_transient() {
            format!("The SMTP server refused the email: {e}")
        } else {
            format!("{host} is not reachable on that port")
        };
        AppError::new(ErrorKind::Unavailable, message).with_cause(e)
    }

    pub async fn send(
        app: &App,
        settings: &EmailSettings,
        from_name: &str,
        from_address: &str,
        message: &Message,
        rendered: Rendered,
    ) -> Result<()> {
        let host = settings.smtp_host.trim();
        if !valid_host(host) {
            return Err(bad_request("Enter the SMTP server's host name, such as smtp.example.com"));
        }
        let mut builder = lettre::Message::builder()
            .from(mailbox(from_name, from_address)?)
            .to(mailbox(&message.to_name.as_deref().map(header_safe).unwrap_or_default(), &header_safe(&message.to))?)
            .subject(header_safe(&message.subject));
        if !settings.reply_to.is_empty() {
            builder = builder.reply_to(mailbox("", &header_safe(&settings.reply_to))?);
        }
        let body = MultiPart::alternative_plain_html(rendered.text, rendered.html);
        let email = if message.attachments.is_empty() {
            builder.multipart(body)
        } else {
            let mut mixed = MultiPart::mixed().multipart(body);
            for a in &message.attachments {
                let filename = header_safe(&a.filename).replace(['/', '\\'], "_");
                let mime = if filename.to_ascii_lowercase().ends_with(".pdf") { "application/pdf" } else { "application/octet-stream" };
                let content_type = ContentType::parse(mime).map_err(AppError::internal)?;
                mixed = mixed.singlepart(Attachment::new(filename).body(a.content.clone(), content_type));
            }
            builder.multipart(mixed)
        }
        .map_err(AppError::internal)?;
        let hello = reqwest::Url::parse(&app.public_url())
            .ok()
            .and_then(|u| u.host_str().map(str::to_string))
            .map_or_else(ClientId::default, ClientId::Domain);
        deliver(settings, email, app.env.development, hello).await
    }

    async fn deliver(settings: &EmailSettings, email: lettre::Message, allow_private: bool, hello: ClientId) -> Result<()> {
        let host = settings.smtp_host.trim().trim_start_matches('[').trim_end_matches(']');
        let addr = resolve(host, settings.smtp_port, allow_private).await?;
        let tls = TlsParameters::new(host.into()).map_err(|e| describe(host, e))?;
        let tls = if settings.smtp_security == "tls" { Tls::Wrapper(tls) } else { Tls::Required(tls) };
        let mut transport = AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(addr.ip().to_string())
            .port(addr.port())
            .tls(tls)
            .hello_name(hello)
            .timeout(Some(Duration::from_secs(30)));
        if !settings.smtp_username.is_empty() {
            transport =
                transport.credentials(Credentials::new(settings.smtp_username.clone(), settings.smtp_password.clone().unwrap_or_default()));
        }
        transport.build().send(email).await.map_err(|e| describe(host, e))?;
        Ok(())
    }

    #[cfg(test)]
    #[allow(clippy::unwrap_used)]
    mod tests {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

        use super::*;

        fn settings(port: u16) -> EmailSettings {
            EmailSettings {
                provider: "smtp".into(),
                smtp_host: "127.0.0.1".into(),
                smtp_port: port,
                smtp_username: "user".into(),
                smtp_password: Some("secret".into()),
                ..Default::default()
            }
        }

        fn email() -> lettre::Message {
            lettre::Message::builder()
                .from(mailbox("", "a@example.com").unwrap())
                .to(mailbox("", "b@example.com").unwrap())
                .subject("s")
                .body(String::from("b"))
                .unwrap()
        }

        #[tokio::test]
        async fn refuses_private_addresses_outside_development() {
            let err = deliver(&settings(25), email(), false, ClientId::default()).await.unwrap_err();
            assert!(err.message.contains("private or reserved"), "{}", err.message);
        }

        #[tokio::test]
        async fn never_falls_back_to_plain_text() {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let port = listener.local_addr().unwrap().port();
            let server = tokio::spawn(async move {
                let (stream, _) = listener.accept().await.unwrap();
                let (read, mut write) = stream.into_split();
                let mut lines = BufReader::new(read).lines();
                let mut seen = Vec::new();
                write.write_all(b"220 test ESMTP\r\n").await.unwrap();
                while let Ok(Some(line)) = lines.next_line().await {
                    seen.push(line.clone());
                    let reply: &[u8] = if line.starts_with("EHLO") { b"250-test\r\n250 AUTH PLAIN LOGIN\r\n" } else { b"221 bye\r\n" };
                    if write.write_all(reply).await.is_err() || line.starts_with("QUIT") {
                        break;
                    }
                }
                seen
            });
            let err = deliver(&settings(port), email(), true, ClientId::default()).await.unwrap_err();
            assert!(err.message.contains("STARTTLS"), "{}", err.message);
            let seen = server.await.unwrap();
            assert!(seen.iter().all(|l| !l.starts_with("AUTH") && !l.starts_with("MAIL")), "{seen:?}");
        }
    }
}

pub use smtp::valid_host as valid_smtp_host;

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
                lang: crate::i18n::Lang::En,
            },
            &org,
            "https://sign.example.com",
        );
        assert!(!rendered.html.contains("<script>"));
        assert!(!rendered.html.contains("javascript:"));
        assert!(!rendered.html.contains("url(x)"));
        assert!(rendered.html.contains("&lt;Acme&gt;"));
    }

    #[test]
    fn shows_the_logo_in_place_of_the_name() {
        let id = uuid::Uuid::now_v7();
        let org = OrgSettings { name: "Acme \"AB\"".into(), logo_file_id: Some(id), ..Default::default() };
        let message = Message {
            to: "a@b.c".into(),
            to_name: None,
            subject: "s".into(),
            paragraphs: vec![],
            button: None,
            footer: None,
            attachments: vec![],
            lang: crate::i18n::Lang::En,
        };
        let html = render(&message, &org, "https://sign.example.com/").html;
        assert!(html.contains(&format!(r#"src="https://sign.example.com/api/logo/{}.png" alt="Acme &quot;AB&quot;""#, id.simple())));
        assert!(!render(&message, &org, "javascript:x").html.contains("<img"));
    }

    #[test]
    fn smtp_hosts_are_bare_names() {
        assert!(valid_smtp_host("smtp.example.com"));
        assert!(valid_smtp_host("10.0.0.5"));
        assert!(valid_smtp_host("[::1]"));
        assert!(!valid_smtp_host("smtp://smtp.example.com"));
        assert!(!valid_smtp_host("smtp.example.com:587"));
        assert!(!valid_smtp_host("smtp.example.com/x"));
        assert!(!valid_smtp_host(""));
    }

    #[test]
    fn strips_header_injection() {
        assert_eq!(header_safe("Hi\r\nBcc: x@y.z"), "HiBcc: x@y.z");
    }
}
