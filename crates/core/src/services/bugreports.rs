//! Report a Bug: a signed-in user's report goes to the bug report relay, which shows it to them
//! again, signs them in with GitHub and opens the issue in their name. Everything that leaves the
//! instance is checked and scrubbed here first: the route pattern instead of the real path, no host
//! name, no user identity, and technical lines without addresses, ids or tokens.

use std::sync::LazyLock;

use async_trait::async_trait;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use regex::Regex;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::PgConnection;

use super::settings;
use crate::audit::{Entry, audit};
use crate::context::Ctx;
use crate::env::app_version;
use crate::error::{AppError, ErrorKind, Result, bad_request, too_many};
use crate::service::{Access, Meta, Service};
use crate::settings::BugReportSettings;

pub const MAX_TITLE: usize = 120;
pub const MAX_DESCRIPTION: usize = 10_000;
pub const MAX_COMMENTS: usize = 30;
pub const MAX_LINE: usize = 500;
pub const MAX_CONSOLE: usize = 30;
pub const MAX_REQUESTS: usize = 20;
const MAX_DIALOGS: usize = 10;
const MAX_LABEL: usize = 120;
/// The screenshot in base64. The whole call may be 12 MB (`rpc::body_limit`).
pub const MAX_SCREENSHOT_BASE64: usize = 10 * 1024 * 1024;
/// All text together, so the issue stays under GitHub's 65,536 characters.
const MAX_TEXT: usize = 60_000;
const MAX_PIXELS_SIDE: u32 = 8192;
pub const PER_USER_PER_HOUR: i64 = 10;
pub const PER_INSTANCE_PER_HOUR: i64 = 30;
const LANGUAGES: &[&str] = &["en", "zh", "hi", "es", "fr", "ar", "bn", "pt", "ru", "ur", "sv"];
const REMOVED: &str = "[removed]";

/// `owner/name` as GitHub allows them.
pub fn valid_repository(repository: &str) -> bool {
    static PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[A-Za-z0-9-]{1,39}/[A-Za-z0-9._-]{1,100}$").expect("regex"));
    PATTERN.is_match(repository) && !repository.ends_with("/.") && !repository.ends_with("/..")
}

/// The relay's base URL: https on a public host. In development plain http and private addresses
/// are allowed too, so a stand-in relay can run next to the app.
pub fn relay_base(url: &str) -> std::result::Result<reqwest::Url, String> {
    relay_base_for(url, false)
}

fn relay_base_for(url: &str, development: bool) -> std::result::Result<reqwest::Url, String> {
    let parsed = crate::net::check_webhook_url(url.trim(), development)?;
    if parsed.query().is_some() || parsed.fragment().is_some() {
        return Err("Enter the relay's address without a query".into());
    }
    Ok(parsed)
}

/// Replaces what could identify a person, a document or this instance in a technical line.
pub fn scrub(text: &str, host: Option<&str>) -> String {
    static RULES: LazyLock<Vec<Regex>> = LazyLock::new(|| {
        [
            // Any absolute URL: it names a host and may carry a query or token.
            r"\b[A-Za-z][A-Za-z0-9+.-]*://\S+",
            r"[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}",
            r"eyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+(?:\.[A-Za-z0-9_-]*)?",
            r"(?i)\b(?:bearer|basic|token)\s+\S+",
            r"(?i)\b[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\b",
            r"\S*\?\S*=\S*",
            r"\b[0-9a-fA-F]{24,}\b",
        ]
        .iter()
        .map(|r| Regex::new(r).expect("regex"))
        .collect()
    });
    static BASE64_RUN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[A-Za-z0-9+/_-]{24,}={0,2}").expect("regex"));
    static NUMBER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\+?\d[\d \-]{8,}\d").expect("regex"));
    static DATE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\d{4}-\d{2}-\d{2}").expect("regex"));
    let mut out = text.to_string();
    if let Some(host) = host.filter(|h| h.len() > 2) {
        out = out.replace(host, REMOVED);
    }
    for rule in RULES.iter() {
        out = rule.replace_all(&out, REMOVED).into_owned();
    }
    // A long run with digits and letters is a key or an encoded value; a long plain word is not.
    out = BASE64_RUN
        .replace_all(&out, |c: &regex::Captures| {
            let run = &c[0];
            if run.bytes().any(|b| b.is_ascii_digit()) && run.bytes().any(|b| b.is_ascii_alphabetic()) {
                REMOVED.to_string()
            } else {
                run.to_string()
            }
        })
        .into_owned();
    // Personal and phone numbers: ten digits or more with the usual separators, unlike a date.
    out = NUMBER
        .replace_all(&out, |c: &regex::Captures| {
            let run = &c[0];
            if run.bytes().filter(u8::is_ascii_digit).count() >= 10 && !DATE.is_match(run) { REMOVED.to_string() } else { run.to_string() }
        })
        .into_owned();
    out.chars().filter(|c| !c.is_control() || *c == '\n' || *c == '\t').collect()
}

/// A route pattern such as `/documents/:id`: lowercase words and `:params`, never a real id.
pub fn valid_route(route: &str) -> bool {
    static PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(?:/(?:[a-z][a-z-]*|:[A-Za-z]+|\*))*/?$").expect("regex"));
    route.starts_with('/') && route.len() <= 200 && PATTERN.is_match(route)
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Comment {
    /// What the user wrote about the element.
    pub text: String,
    /// The element's visible text or accessible name, shortened.
    #[serde(default)]
    pub label: String,
    /// The element's tag, such as `button`.
    #[serde(default)]
    pub tag: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema, Clone, Copy)]
pub struct Viewport {
    pub width: u32,
    pub height: u32,
    pub dpr: f32,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Screenshot {
    /// `image/png` or `image/jpeg`.
    #[serde(rename = "type")]
    pub kind: String,
    /// The image in base64, with the user's masks already burned in.
    pub data: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PrepareInput {
    #[serde(default)]
    pub title: String,
    pub description: String,
    /// The page's route pattern, such as `/documents/:id`. Never the real path.
    pub route: String,
    /// The app's language, such as `sv`.
    pub language: String,
    /// Browser name, major version and system, such as `Firefox 146 (Windows)`.
    #[serde(default)]
    pub browser: String,
    pub viewport: Option<Viewport>,
    /// Titles of the dialogs that were open.
    #[serde(default)]
    pub dialogs: Vec<String>,
    #[serde(default)]
    pub comments: Vec<Comment>,
    /// Recent console errors and warnings.
    #[serde(default)]
    pub console: Vec<String>,
    /// Recent failed calls, as `service → status`.
    #[serde(default)]
    pub requests: Vec<String>,
    pub screenshot: Option<Screenshot>,
    /// The user ticked that the report and screenshot may be published.
    pub consent: bool,
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Prepared {
    /// Where the user checks the report, signs in with GitHub and publishes it. Open it in a new window.
    pub continue_url: String,
}

fn too_long(what: &str, max: usize) -> AppError {
    bad_request(format!("{what} is too long (at most {{max}} characters)")).params([("max", &max)])
}

fn line(text: &str, max: usize, what: &str) -> Result<()> {
    if text.chars().count() > max {
        return Err(too_long(what, max));
    }
    Ok(())
}

fn lines(items: &[String], count: usize, max: usize, what: &str, host: Option<&str>) -> Result<Vec<String>> {
    if items.len() > count {
        return Err(bad_request(format!("Send at most {count} {what} lines")));
    }
    items
        .iter()
        .map(|item| {
            line(item, max, what)?;
            Ok(scrub(item, host))
        })
        .filter(|r| r.as_ref().map_or(true, |s| !s.trim().is_empty()))
        .collect()
}

fn image(screenshot: &Screenshot) -> Result<()> {
    if screenshot.data.len() > MAX_SCREENSHOT_BASE64 {
        return Err(bad_request("The screenshot is too large. Leave it out or make the window smaller."));
    }
    let bytes = B64.decode(screenshot.data.as_bytes()).map_err(|_| bad_request("The screenshot is not a PNG or JPEG image"))?;
    let png = bytes.starts_with(b"\x89PNG\r\n\x1a\n");
    let jpeg = bytes.starts_with(&[0xFF, 0xD8, 0xFF]);
    match screenshot.kind.as_str() {
        "image/png" if png => {
            let side = |at: usize| bytes.get(at..at + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]])).unwrap_or(u32::MAX);
            if side(16) > MAX_PIXELS_SIDE || side(20) > MAX_PIXELS_SIDE {
                return Err(bad_request("The screenshot is too large. Leave it out or make the window smaller."));
            }
            Ok(())
        }
        "image/jpeg" if jpeg => Ok(()),
        _ => Err(bad_request("The screenshot is not a PNG or JPEG image")),
    }
}

/// The draft for the relay's `POST /bugreport/v1/drafts`, after every check.
pub fn draft(input: &PrepareInput, repository: &str, host: Option<&str>) -> Result<Value> {
    if !input.consent {
        return Err(bad_request("Confirm that the report may be published"));
    }
    let title = input.title.trim();
    let description = input.description.trim();
    if description.is_empty() {
        return Err(bad_request("Describe the problem"));
    }
    line(title, MAX_TITLE, "The title")?;
    line(description, MAX_DESCRIPTION, "The description")?;
    if !valid_route(&input.route) {
        return Err(bad_request("The page is not a route pattern"));
    }
    if !LANGUAGES.contains(&input.language.as_str()) {
        return Err(bad_request("Unknown language"));
    }
    if input.comments.len() > MAX_COMMENTS {
        return Err(bad_request(format!("Add at most {MAX_COMMENTS} comments")));
    }
    let comments = input
        .comments
        .iter()
        .enumerate()
        .map(|(i, c)| {
            line(&c.text, MAX_LINE, "A comment")?;
            let tag: String = c.tag.chars().filter(char::is_ascii_alphanumeric).take(20).collect::<String>().to_ascii_lowercase();
            let label: String = scrub(&c.label, host).chars().take(MAX_LABEL).collect();
            Ok(json!({ "n": i + 1, "text": c.text.trim(), "label": label.trim(), "tag": tag }))
        })
        .collect::<Result<Vec<_>>>()?;
    let dialogs: Vec<String> =
        lines(&input.dialogs, MAX_DIALOGS, MAX_LABEL, "dialog", host)?.into_iter().map(|d| d.trim().to_string()).collect();
    let console = lines(&input.console, MAX_CONSOLE, MAX_LINE, "console", host)?;
    let requests = lines(&input.requests, MAX_REQUESTS, MAX_LINE, "request", host)?;
    line(&input.browser, 60, "The browser")?;
    let browser: String =
        scrub(&input.browser, host).chars().filter(|c| c.is_alphanumeric() || " ().,/-_".contains(*c)).collect::<String>().trim().into();
    let viewport = input.viewport.filter(|v| v.width <= 20_000 && v.height <= 20_000 && v.dpr.is_finite() && v.dpr > 0.0 && v.dpr <= 8.0);
    let text: usize = title.len()
        + description.len()
        + comments.iter().map(|c| c.to_string().len()).sum::<usize>()
        + [&dialogs, &console, &requests].iter().flat_map(|l| l.iter()).map(String::len).sum::<usize>();
    if text > MAX_TEXT {
        return Err(bad_request("The report is too long. Shorten the description or remove some lines."));
    }
    if let Some(screenshot) = &input.screenshot {
        image(screenshot)?;
    }
    Ok(json!({
        "v": 1,
        "project": { "name": "OpenSign", "version": app_version(), "repo": repository },
        "lang": input.language,
        "title": title,
        "description": description,
        "route": input.route,
        "browser": browser,
        "viewport": viewport,
        "dialogs": dialogs,
        "comments": comments,
        "console": console,
        "requests": requests,
        "screenshot": input.screenshot.as_ref().map(|s| json!({ "type": s.kind, "data": s.data })),
    }))
}

/// The continue link, only on the relay's own origin, so the app never opens anything else.
fn continue_url(base: &reqwest::Url, answer: &Value, development: bool) -> Result<String> {
    let refused = || AppError::new(ErrorKind::Unavailable, "The bug report service answered in an unexpected way");
    let url = answer["continueUrl"].as_str().and_then(|u| reqwest::Url::parse(u).ok()).ok_or_else(refused)?;
    let secure = url.scheme() == "https" || development;
    if !secure || url.origin() != base.origin() || !url.path().starts_with("/bugreport/") || !url.username().is_empty() {
        return Err(refused());
    }
    Ok(url.to_string())
}

fn relay_error(status: reqwest::StatusCode, body: &Value) -> AppError {
    match status.as_u16() {
        503 if body["error"] == "relay_disabled" => AppError::new(ErrorKind::Unavailable, "Bug reports are switched off for now."),
        413 => bad_request("The report is too large. Leave out the screenshot or shorten the text."),
        429 => too_many("Many bug reports were sent recently. Try again in an hour."),
        507 => AppError::new(ErrorKind::Unavailable, "The bug report service is full right now. Try again later."),
        400 | 422 => bad_request("The bug report service refused the report."),
        _ => AppError::new(ErrorKind::Unavailable, "The bug report service is not reachable. Try again later."),
    }
    .with_cause(anyhow::anyhow!("relay answered {status}: {}", body["error"]))
}

async fn sent_in_last_hour(conn: &mut PgConnection, user: Option<uuid::Uuid>) -> Result<i64> {
    Ok(sqlx::query_scalar(
        "select count(*) from audit_log where action = 'bugreports.send' and occurred_at > now() - interval '1 hour'
         and ($1::uuid is null or actor_id = $1)",
    )
    .bind(user)
    .fetch_one(&mut *conn)
    .await?)
}

fn turned_off() -> AppError {
    AppError::new(ErrorKind::Forbidden, "Bug reports are turned off. An admin can turn them on in Settings, Bug Reports.")
}

pub struct Prepare;

#[async_trait]
impl Service for Prepare {
    type Input = PrepareInput;
    type Output = Prepared;
    const META: Meta = Meta::mutation(
        "bugreports.prepare",
        "Send a bug report with an optional screenshot to the bug report relay, which returns a link where the user checks it, signs in with GitHub and publishes it as a public issue in their own name.",
        Access::Authenticated,
    )
    .without_transaction()
    .open_world()
    .not_a_tool("A screenshot of the user's screen goes public on GitHub, so only the person at the screen may send it.");

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: PrepareInput) -> Result<Prepared> {
        if !ctx.in_app() {
            return Err(AppError::new(ErrorKind::Forbidden, "Report a bug in the app, signed in"));
        }
        let reports: BugReportSettings = settings(ctx, conn).await?;
        if !reports.enabled {
            return Err(turned_off());
        }
        let actor = ctx.actor()?.user_id;
        if sent_in_last_hour(conn, Some(actor)).await? >= PER_USER_PER_HOUR {
            return Err(too_many("You have sent many bug reports this past hour. Try again later."));
        }
        if sent_in_last_hour(conn, None).await? >= PER_INSTANCE_PER_HOUR {
            return Err(too_many("Many bug reports were sent from this OpenSign this past hour. Try again later."));
        }
        let development = ctx.app.env.development;
        if !valid_repository(&reports.repository) {
            return Err(AppError::new(ErrorKind::Unavailable, "Bug reports are not set up. An admin checks Settings, Bug Reports."));
        }
        let base = relay_base_for(&reports.relay_url, development).map_err(|e| {
            AppError::new(ErrorKind::Unavailable, "Bug reports are not set up. An admin checks Settings, Bug Reports.")
                .with_cause(anyhow::anyhow!(e))
        })?;
        let host = reqwest::Url::parse(&ctx.app.public_url()).ok().and_then(|u| u.host_str().map(str::to_string));
        let body = draft(&input, &reports.repository, host.as_deref())?;
        let endpoint = base.join("bugreport/v1/drafts").map_err(AppError::internal)?;
        let http = if development { &ctx.app.http.internal } else { &ctx.app.http.public };
        let response = http.post(endpoint).json(&body).timeout(std::time::Duration::from_secs(60)).send().await.map_err(|e| {
            AppError::new(ErrorKind::Unavailable, "The bug report service is not reachable. Try again later.").with_cause(e)
        })?;
        let status = response.status();
        if response.content_length().is_some_and(|l| l > 64 * 1024) {
            return Err(relay_error(reqwest::StatusCode::BAD_GATEWAY, &Value::Null));
        }
        let answer: Value = response.json().await.unwrap_or(Value::Null);
        if !status.is_success() {
            return Err(relay_error(status, &answer));
        }
        let continue_url = continue_url(&base, &answer, development)?;
        let draft_id: String = answer["id"].as_str().unwrap_or_default().chars().filter(char::is_ascii_alphanumeric).take(64).collect();
        audit(
            ctx,
            conn,
            Entry::new("bugreports.send", "bug_report", if draft_id.is_empty() { "draft".into() } else { draft_id }).metadata(json!({
                "route": body["route"],
                "repository": reports.repository,
                "relay": base.origin().ascii_serialization(),
                "screenshot": input.screenshot.is_some(),
                "comments": input.comments.len(),
            })),
        )
        .await?;
        Ok(Prepared { continue_url })
    }
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StatusView {
    /// Report a Bug is offered.
    pub enabled: bool,
    /// The GitHub repository, as `owner/name`, that receives reports.
    pub repository: String,
}

pub struct Status;

#[async_trait]
impl Service for Status {
    type Input = super::Empty;
    type Output = StatusView;
    const META: Meta = Meta::query(
        "bugreports.status",
        "Whether users can report a bug from the user menu, and which GitHub repository receives the reports.",
        Access::Authenticated,
    )
    .tool();

    async fn run(ctx: &Ctx, conn: &mut PgConnection, _input: super::Empty) -> Result<StatusView> {
        let reports: BugReportSettings = settings(ctx, conn).await?;
        Ok(StatusView { enabled: reports.enabled, repository: reports.repository })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrubs_what_identifies() {
        let cases = [
            ("failed for ada@example.com", "failed for [removed]"),
            ("GET https://sign.acme.se/api/rpc/x?token=abc 500", "GET [removed] 500"),
            ("doc 0190a1b2-c3d4-7e5f-8a9b-0c1d2e3f4a5b gone", "doc [removed] gone"),
            ("Authorization: Bearer abc.def", "Authorization: [removed]"),
            ("jwt eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxIn0.sig", "jwt [removed]"),
            ("hash 0123456789abcdef0123456789abcdef", "hash [removed]"),
            ("key c2VjcmV0LWtleS0xMjM0NTY3ODkwMTIzNDU2", "key [removed]"),
            ("pnr 19900101-1234 and +46 70 123 45 67", "pnr [removed] and [removed]"),
            ("on sign.acme.se now", "on [removed] now"),
            ("TypeError: cannot read properties of undefined", "TypeError: cannot read properties of undefined"),
            ("ComponentWithAVeryLongNameIndeed failed", "ComponentWithAVeryLongNameIndeed failed"),
            ("documents.get → 500", "documents.get → 500"),
            ("at 2026-11-07 12:00", "at 2026-11-07 12:00"),
        ];
        for (input, expected) in cases {
            assert_eq!(scrub(input, Some("sign.acme.se")), expected, "{input}");
        }
    }

    #[test]
    fn routes_are_patterns() {
        for good in ["/", "/documents", "/documents/:id", "/settings/admin/:group", "/settings/webhooks/:id"] {
            assert!(valid_route(good), "{good}");
        }
        for bad in ["documents", "/documents/0190a1b2", "/documents?x=1", "/documents#a", "/Documents", "/a b", "/documents/123"] {
            assert!(!valid_route(bad), "{bad}");
        }
    }

    #[test]
    fn repositories_are_owner_and_name() {
        assert!(valid_repository("nielsbosma/OpenSign"));
        assert!(valid_repository("acme-corp/open.sign_fork"));
        for bad in ["nielsbosma", "a/b/c", "https://github.com/a/b", "a/..", "a_b/c", "/b", "a/"] {
            assert!(!valid_repository(bad), "{bad}");
        }
    }

    #[test]
    fn relay_is_https_and_public() {
        assert!(relay_base("https://sliplane.opendeploy.cloud").is_ok());
        for bad in
            ["http://sliplane.opendeploy.cloud", "https://10.0.0.1", "https://localhost", "https://a.example?x=1", "https://u:p@a.example"]
        {
            assert!(relay_base(bad).is_err(), "{bad}");
        }
        assert!(relay_base_for("http://127.0.0.1:9", true).is_ok());
    }

    fn input(value: Value) -> PrepareInput {
        serde_json::from_value(value).expect("input")
    }

    fn base() -> Value {
        json!({ "description": "The button does nothing", "route": "/documents/:id", "language": "sv", "consent": true })
    }

    #[test]
    fn drafts_are_checked() {
        let draft = draft(&input(base()), "nielsbosma/OpenSign", None).expect("draft");
        assert_eq!(draft["project"]["repo"], "nielsbosma/OpenSign");
        assert_eq!(draft["route"], "/documents/:id");
        assert!(draft["screenshot"].is_null());

        let refused = |patch: Value| {
            let mut value = base();
            for (k, v) in patch.as_object().expect("object") {
                value[k] = v.clone();
            }
            super::draft(&input(value), "nielsbosma/OpenSign", None).expect_err("refused").message
        };
        assert_eq!(refused(json!({ "consent": false })), "Confirm that the report may be published");
        assert_eq!(refused(json!({ "description": "  " })), "Describe the problem");
        assert_eq!(refused(json!({ "route": "/documents/0190a1b2-c3d4" })), "The page is not a route pattern");
        assert!(refused(json!({ "title": "x".repeat(121) })).contains("at most 120"));
        assert!(refused(json!({ "description": "x".repeat(10_001) })).contains("at most 10000"));
        assert!(refused(json!({ "comments": (0..31).map(|_| json!({ "text": "x" })).collect::<Vec<_>>() })).contains("at most 30"));
        assert!(refused(json!({ "console": (0..31).map(|_| "x").collect::<Vec<_>>() })).contains("at most 30"));
        assert!(refused(json!({ "requests": ["x".repeat(501)] })).contains("at most 500"));
        assert_eq!(
            refused(json!({ "screenshot": { "type": "image/png", "data": B64.encode(b"<svg/>") } })),
            "The screenshot is not a PNG or JPEG image"
        );
        let huge = "A".repeat(MAX_SCREENSHOT_BASE64 + 4);
        assert!(refused(json!({ "screenshot": { "type": "image/png", "data": huge } })).contains("too large"));
    }

    #[test]
    fn drafts_scrub_technical_lines_but_keep_the_users_words() {
        let mut value = base();
        value["console"] = json!(["Error at https://sign.acme.se/documents/1", ""]);
        value["comments"] = json!([{ "text": "Mail ada@example.com about it", "label": "ada@example.com", "tag": "<Button>" }]);
        let draft = draft(&input(value), "nielsbosma/OpenSign", Some("sign.acme.se")).expect("draft");
        assert_eq!(draft["console"], json!(["Error at [removed]"]));
        assert_eq!(draft["comments"][0]["label"], "[removed]");
        assert_eq!(draft["comments"][0]["tag"], "button");
        assert_eq!(draft["comments"][0]["text"], "Mail ada@example.com about it");
        let text = draft.to_string();
        assert!(!text.contains("sign.acme.se"));
    }

    #[test]
    fn continue_links_stay_on_the_relay() {
        let base = reqwest::Url::parse("https://relay.example").expect("url");
        let ok = json!({ "continueUrl": "https://relay.example/bugreport/d/abc#key" });
        assert_eq!(continue_url(&base, &ok, false).expect("ok"), "https://relay.example/bugreport/d/abc#key");
        for bad in [
            "https://evil.example/bugreport/d/abc",
            "http://relay.example/bugreport/d/abc",
            "https://relay.example/other",
            "javascript:alert(1)",
        ] {
            assert!(continue_url(&base, &json!({ "continueUrl": bad }), false).is_err(), "{bad}");
        }
    }
}
