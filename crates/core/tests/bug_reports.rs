#![allow(clippy::unwrap_used)]

use std::sync::{Arc, Mutex};

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::post;
use opensign_core::context::{Actor, App, Credential, Ctx, Surface};
use opensign_core::error::ErrorKind;
use opensign_core::services::{Empty, admin, bugreports};
use opensign_core::settings::BugReportSettings;
use opensign_core::testing::app;
use opensign_core::{Role, call};
use serde_json::{Value, json};
use uuid::Uuid;

#[derive(Default)]
struct Relay {
    drafts: Vec<Value>,
    answer: Option<(StatusCode, Value)>,
}

type Shared = Arc<Mutex<Relay>>;

async fn relay(state: Shared) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let continue_base = base.clone();
    let router = axum::Router::new()
        .route(
            "/bugreport/v1/drafts",
            post(move |State(s): State<Shared>, headers: axum::http::HeaderMap, Json(body): Json<Value>| {
                let continue_base = continue_base.clone();
                async move {
                    assert!(headers.get("origin").is_none() && headers.get("cookie").is_none(), "server to server only");
                    let mut s = s.lock().unwrap();
                    s.drafts.push(body);
                    let id = s.drafts.len();
                    s.answer.clone().map_or_else(
                        || {
                            (
                                StatusCode::CREATED,
                                Json(json!({ "id": format!("d{id}"), "continueUrl": format!("{continue_base}/bugreport/d/d{id}#key") })),
                            )
                        },
                        |(status, body)| (status, Json(body)),
                    )
                }
            }),
        )
        .with_state(state);
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    base
}

async fn person(app: &Arc<App>, role: Role, credential: Credential) -> Ctx {
    let id = Uuid::now_v7();
    let email = format!("{}@example.com", id.simple());
    sqlx::query("insert into users (id, email, name, role, password_hash) values ($1, $2, 'Ada Reporter', $3, 'x')")
        .bind(id)
        .bind(&email)
        .bind(role.as_str())
        .execute(&app.db)
        .await
        .unwrap();
    let actor = Actor {
        user_id: id,
        email,
        name: "Ada Reporter".into(),
        role,
        permissions: role.permissions(),
        must_enable_two_factor: false,
        credential,
    };
    Ctx::new(app.clone(), Surface::Web).with_actor(actor)
}

async fn point_at(app: &Arc<App>, base: &str, enabled: bool) {
    let settings = BugReportSettings { enabled, relay_url: base.into(), ..Default::default() };
    let mut conn = app.db.acquire().await.unwrap();
    app.settings.save(&mut conn, &app.keys, &settings, None).await.unwrap();
}

fn report() -> bugreports::PrepareInput {
    serde_json::from_value(json!({
        "title": "Send does nothing",
        "description": "I press Send for Signing and nothing happens.",
        "route": "/documents/:id",
        "language": "sv",
        "browser": "Firefox 146 (Windows)",
        "viewport": { "width": 1440, "height": 900, "dpr": 2 },
        "comments": [{ "text": "This button", "label": "Send for Signing", "tag": "button" }],
        "console": ["error: failed for ada@example.com at http://localhost:3000/documents/0190a1b2-c3d4-7e5f-8a9b-0c1d2e3f4a5b"],
        "requests": ["documents.send → 500"],
        "screenshot": { "type": "image/png", "data": "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==" },
        "consent": true,
    }))
    .unwrap()
}

fn error<T>(result: opensign_core::Result<T>) -> (ErrorKind, String) {
    match result {
        Err(e) => (e.kind, e.message),
        Ok(_) => panic!("expected an error"),
    }
}

#[tokio::test]
async fn a_report_goes_to_the_relay_scrubbed_and_audited() {
    let Some(app) = app().await else { return };
    let state: Shared = Arc::default();
    let base = relay(state.clone()).await;
    point_at(&app, &base, true).await;
    let viewer = person(&app, Role::Viewer, Credential::Session).await;

    let status = call::<bugreports::Status>(&viewer, Empty {}).await.unwrap();
    assert!(status.enabled);
    assert_eq!(status.repository, "nielsbosma/OpenSign");

    let prepared = call::<bugreports::Prepare>(&viewer, report()).await.unwrap();
    assert_eq!(prepared.continue_url, format!("{base}/bugreport/d/d1#key"));

    let draft = state.lock().unwrap().drafts[0].clone();
    assert_eq!(draft["v"], 1);
    assert_eq!(draft["project"]["repo"], "nielsbosma/OpenSign");
    assert_eq!(draft["route"], "/documents/:id");
    assert_eq!(draft["lang"], "sv");
    assert_eq!(draft["console"], json!(["error: failed for [removed] at [removed]"]));
    assert_eq!(draft["screenshot"]["type"], "image/png");
    let text = draft.to_string();
    let email = &viewer.actor.as_ref().unwrap().email;
    for leak in [email.as_str(), "Ada Reporter", "localhost", "0190a1b2"] {
        assert!(!text.contains(leak), "the draft contains {leak}");
    }

    let audited: (String, Value) =
        sqlx::query_as("select entity_id, metadata from audit_log where action = 'bugreports.send'").fetch_one(&app.db).await.unwrap();
    assert_eq!(audited.0, "d1");
    assert_eq!(audited.1["route"], "/documents/:id");
    assert_eq!(audited.1["screenshot"], true);
}

#[tokio::test]
async fn reports_are_refused_when_off_or_not_from_a_person_in_the_app() {
    let Some(app) = app().await else { return };
    let state: Shared = Arc::default();
    let base = relay(state.clone()).await;
    point_at(&app, &base, false).await;
    let admin = person(&app, Role::Admin, Credential::Session).await;

    assert!(!call::<bugreports::Status>(&admin, Empty {}).await.unwrap().enabled);
    let (kind, message) = error(call::<bugreports::Prepare>(&admin, report()).await);
    assert_eq!(kind, ErrorKind::Forbidden);
    assert!(message.starts_with("Bug reports are turned off"));

    point_at(&app, &base, true).await;
    let key = person(&app, Role::Admin, Credential::ApiKey(Uuid::now_v7())).await;
    assert_eq!(error(call::<bugreports::Prepare>(&key, report()).await).0, ErrorKind::Forbidden);

    let mut unticked = report();
    unticked.consent = false;
    assert_eq!(error(call::<bugreports::Prepare>(&admin, unticked).await).1, "Confirm that the report may be published");
    assert!(state.lock().unwrap().drafts.is_empty(), "nothing reached the relay");

    // Where screenshots go is for a person in the app to change, not an agent or an API key.
    let moved = serde_json::from_value(json!({ "group": "bug_reports", "values": { "relayUrl": "https://evil.example" } })).unwrap();
    assert_eq!(error(call::<admin::UpdateSettings>(&key, moved).await).0, ErrorKind::Forbidden);
    let bad = serde_json::from_value(json!({ "group": "bug_reports", "values": { "repository": "not a repo" } })).unwrap();
    assert_eq!(error(call::<admin::UpdateSettings>(&admin, bad).await).0, ErrorKind::BadRequest);
    let http = serde_json::from_value(json!({ "group": "bug_reports", "values": { "relayUrl": "http://relay.example" } })).unwrap();
    assert_eq!(error(call::<admin::UpdateSettings>(&admin, http).await).0, ErrorKind::BadRequest);
}

#[tokio::test]
async fn reports_are_limited_per_person_and_per_install() {
    let Some(app) = app().await else { return };
    let state: Shared = Arc::default();
    let base = relay(state.clone()).await;
    point_at(&app, &base, true).await;

    let first = person(&app, Role::Sender, Credential::Session).await;
    for _ in 0..bugreports::PER_USER_PER_HOUR {
        call::<bugreports::Prepare>(&first, report()).await.unwrap();
    }
    let (kind, message) = error(call::<bugreports::Prepare>(&first, report()).await);
    assert_eq!(kind, ErrorKind::TooManyRequests);
    assert!(message.starts_with("You have sent many bug reports"));

    for _ in 1..(bugreports::PER_INSTANCE_PER_HOUR / bugreports::PER_USER_PER_HOUR) {
        let other = person(&app, Role::Sender, Credential::Session).await;
        for _ in 0..bugreports::PER_USER_PER_HOUR {
            call::<bugreports::Prepare>(&other, report()).await.unwrap();
        }
    }
    let late = person(&app, Role::Sender, Credential::Session).await;
    let (kind, message) = error(call::<bugreports::Prepare>(&late, report()).await);
    assert_eq!(kind, ErrorKind::TooManyRequests);
    assert!(message.starts_with("Many bug reports were sent from this OpenSign"));
}

#[tokio::test]
async fn relay_answers_are_translated_and_its_links_checked() {
    let Some(app) = app().await else { return };
    let state: Shared = Arc::default();
    let base = relay(state.clone()).await;
    point_at(&app, &base, true).await;
    let user = person(&app, Role::Viewer, Credential::Session).await;

    for (status, body, kind, message) in [
        (
            StatusCode::SERVICE_UNAVAILABLE,
            json!({ "error": "relay_disabled" }),
            ErrorKind::Unavailable,
            "Bug reports are switched off for now.",
        ),
        (
            StatusCode::TOO_MANY_REQUESTS,
            json!({ "error": "rate_limited" }),
            ErrorKind::TooManyRequests,
            "Many bug reports were sent recently. Try again in an hour.",
        ),
        (
            StatusCode::PAYLOAD_TOO_LARGE,
            json!({}),
            ErrorKind::BadRequest,
            "The report is too large. Leave out the screenshot or shorten the text.",
        ),
        (
            StatusCode::CREATED,
            json!({ "id": "x", "continueUrl": "https://evil.example/bugreport/d/x#k" }),
            ErrorKind::Unavailable,
            "The bug report service answered in an unexpected way",
        ),
    ] {
        state.lock().unwrap().answer = Some((status, body));
        assert_eq!(error(call::<bugreports::Prepare>(&user, report()).await), (kind, message.to_string()));
    }
    let sent: i64 = sqlx::query_scalar("select count(*) from audit_log where action = 'bugreports.send'").fetch_one(&app.db).await.unwrap();
    assert_eq!(sent, 0, "only reports the relay took are recorded");
}
