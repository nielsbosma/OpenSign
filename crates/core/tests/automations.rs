#![allow(clippy::unwrap_used)]

use std::sync::{Arc, Mutex};

use axum::Json;
use axum::extract::{Path, State};
use axum::routing::{get, post, put};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use lopdf::{Document, Object, Stream, dictionary};
use opensign_core::composio::{self, TEST_BASE};
use opensign_core::context::{Actor, App, Credential, Ctx, Surface};
use opensign_core::error::ErrorKind;
use opensign_core::services::{Empty, admin, automations, documents as docs};
use opensign_core::testing::app;
use opensign_core::{Role, call, worker};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use uuid::Uuid;

#[derive(Default)]
struct StandIn {
    user_id: String,
    executed: Vec<Value>,
    uploaded: usize,
    fail_next: bool,
}

type Shared = Arc<Mutex<StandIn>>;

async fn composio_stand_in(state: Shared) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let presigned = format!("{base}/s3/k1");
    let account = |s: &StandIn, id: &str| {
        let owner = if id == "ca_drive" { s.user_id.clone() } else { "someone-else".into() };
        json!({ "id": id, "user_id": owner, "status": "ACTIVE", "toolkit": { "slug": "googledrive" }, "state": { "val": { "access_token": "secret" } } })
    };
    let router = axum::Router::new()
        .route("/toolkits", get(|| async { Json(json!({ "items": [{ "slug": "googledrive", "name": "Google Drive", "meta": { "logo": "https://logos.composio.dev/drive.png", "categories": [{ "id": "files", "name": "Files" }] } }] })) }))
        .route(
            "/connected_accounts",
            get(move |State(s): State<Shared>| async move {
                let s = s.lock().unwrap();
                Json(json!({ "items": [account(&s, "ca_drive")] }))
            }),
        )
        .route(
            "/connected_accounts/{id}",
            get(move |State(s): State<Shared>, Path(id): Path<String>| async move { Json(account(&s.lock().unwrap(), &id)) })
                .delete(|| async { Json(json!({ "success": true })) }),
        )
        .route(
            "/tools/{slug}",
            get(|Path(slug): Path<String>| async move {
                Json(json!({
                    "slug": slug, "name": "Upload File", "description": "Uploads a file", "version": "20261001_00",
                    "toolkit": { "slug": if slug.starts_with("GOOGLEDRIVE") { "googledrive" } else { "slack" } },
                    "input_parameters": { "type": "object", "required": ["file_to_upload"], "properties": {
                        "file_to_upload": { "type": "object", "file_uploadable": true }, "description": { "type": "string" }, "total": { "type": "number" },
                    } },
                }))
            }),
        )
        .route("/files/upload/request", post(move || async move { Json(json!({ "key": "k1", "new_presigned_url": presigned, "type": "new" })) }))
        .route(
            "/s3/k1",
            put(|State(s): State<Shared>, body: axum::body::Bytes| async move {
                s.lock().unwrap().uploaded = body.len();
            }),
        )
        .route(
            "/tools/execute/{slug}",
            post(|State(s): State<Shared>, Json(body): Json<Value>| async move {
                let mut s = s.lock().unwrap();
                s.executed.push(body);
                if std::mem::take(&mut s.fail_next) {
                    return (axum::http::StatusCode::BAD_GATEWAY, Json(json!({ "error": { "message": "Drive is down" } })));
                }
                (axum::http::StatusCode::OK, Json(json!({ "successful": true, "data": { "id": "file-1" }, "log_id": "log_1" })))
            }),
        )
        .with_state(state);
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    base
}

fn sample_pdf() -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let content = doc.add_object(Stream::new(dictionary! {}, b"0 0 m 10 10 l S".to_vec()));
    let page = doc.add_object(dictionary! {
        "Type" => "Page", "Parent" => pages_id, "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()], "Contents" => content,
    });
    doc.objects.insert(pages_id, Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => vec![page.into()], "Count" => 1 }));
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog);
    let mut out = Vec::new();
    doc.save_to(&mut out).unwrap();
    out
}

async fn person(app: &Arc<App>, role: Role) -> Ctx {
    let id = Uuid::now_v7();
    let email = format!("{}@example.com", id.simple());
    sqlx::query("insert into users (id, email, name, role, password_hash) values ($1, $2, 'Ada Admin', $3, 'x')")
        .bind(id)
        .bind(&email)
        .bind(role.as_str())
        .execute(&app.db)
        .await
        .unwrap();
    let actor = Actor {
        user_id: id,
        email,
        name: "Ada Admin".into(),
        role,
        permissions: role.permissions(),
        must_enable_two_factor: false,
        credential: Credential::Session,
    };
    Ctx::new(app.clone(), Surface::Api).with_actor(actor)
}

fn input<T: DeserializeOwned>(value: Value) -> T {
    serde_json::from_value(value).unwrap()
}

fn error<T>(result: opensign_core::Result<T>) -> (ErrorKind, String) {
    match result {
        Err(e) => (e.kind, e.message),
        Ok(_) => panic!("expected an error"),
    }
}

#[tokio::test]
async fn rules_send_document_data_and_the_sealed_pdf_to_composio() {
    let Some(app) = app().await else { return };
    let state: Shared = Arc::new(Mutex::new(StandIn { user_id: composio::user_id(&app.keys), ..Default::default() }));
    *TEST_BASE.lock().unwrap() = Some(composio_stand_in(state.clone()).await);
    let ada = person(&app, Role::Admin).await;
    let manager = person(&app, Role::Manager).await;

    let (kind, _) = error(call::<automations::Apps>(&ada, input(json!({}))).await);
    assert_eq!(kind, ErrorKind::Unavailable, "off by default");
    let (_, message) = error(call::<admin::UpdateSettings>(&ada, input(json!({ "group": "automations", "values": { "enabled": true } }))).await);
    assert!(message.contains("Composio API key"), "{message}");
    let settings = json!({ "group": "automations", "values": { "enabled": true, "composioApiKey": "ak_test" } });
    let saved = call::<admin::UpdateSettings>(&ada, input(settings)).await.unwrap();
    assert_eq!(saved.values["composioApiKey"], json!(true), "the key is never shown");
    let status = call::<automations::Status>(&ada, Empty {}).await.unwrap();
    assert!(status.enabled && status.key_set);
    assert_eq!(error(call::<automations::List>(&manager, Empty {}).await).0, ErrorKind::Forbidden);

    let apps = call::<automations::Apps>(&ada, input(json!({ "search": "drive" }))).await.unwrap();
    assert_eq!((apps[0].slug.as_str(), apps[0].categories.clone()), ("googledrive", vec!["Files".to_string()]));
    let connections = call::<automations::Connections>(&ada, Empty {}).await.unwrap();
    assert_eq!((connections[0].id.as_str(), connections[0].status.as_str()), ("ca_drive", "ACTIVE"));
    assert!(!serde_json::to_string(&connections).unwrap().contains("secret"), "account credentials stay at Composio");

    let rule = |connection: &str, event: &str, arguments: Value| {
        input::<automations::AutomationInput>(json!({
            "name": "Save to Drive", "event": event, "connectionId": connection, "tool": "GOOGLEDRIVE_UPLOAD_FILE", "arguments": arguments,
        }))
    };
    let file_args = json!({ "file_to_upload": "{{sealedPdf}}", "description": "{{document.title}} signed by {{parties}}", "total": "{{fields.Total}}" });
    let (kind, _) = error(call::<automations::Create>(&ada, rule("ca_other", "document.completed", file_args.clone())).await);
    assert_eq!(kind, ErrorKind::NotFound, "another install's connection is not usable");
    let (_, message) =
        error(call::<automations::Create>(&ada, rule("ca_drive", "document.completed", json!({ "file_to_upload": "{{party.personalNumber}}" }))).await);
    assert!(message.contains("party.personalNumber"), "{message}");
    let (_, message) = error(call::<automations::Create>(&ada, rule("ca_drive", "document.sent", file_args.clone())).await);
    assert!(message.contains("document.completed"), "{message}");
    let (_, message) = error(call::<automations::Create>(&ada, rule("ca_drive", "document.completed", json!({ "description": "x" }))).await);
    assert!(message.contains("file_to_upload"), "{message}");
    let (_, message) =
        error(call::<automations::Create>(&ada, rule("ca_drive", "document.completed", json!({ "file_to_upload": "x", "folder": "y" }))).await);
    assert!(message.contains("folder"), "{message}");

    let created = call::<automations::Create>(&ada, rule("ca_drive", "document.completed", file_args)).await.unwrap();
    assert_eq!((created.app.as_str(), created.tool_version.as_deref()), ("googledrive", Some("20261001_00")));

    let doc = call::<docs::Create>(&ada, input(json!({ "title": "NDA" }))).await.unwrap().document.id;
    let file = call::<docs::Upload>(
        &ada,
        docs::UploadInput { document_id: doc, name: "nda.pdf".into(), content_base64: B64.encode(sample_pdf()), ..Default::default() },
    )
    .await
    .unwrap()
    .file
    .id;
    call::<docs::SetParties>(
        &ada,
        input(json!({ "documentId": doc, "parties": [{ "name": "Anna Andersson", "email": "anna@example.com", "personalNumber": "19800101-1234" }] })),
    )
    .await
    .unwrap();
    let mut conn = app.db.acquire().await.unwrap();
    let party: Uuid = sqlx::query_scalar("select id from parties where document_id = $1").bind(doc).fetch_one(&mut *conn).await.unwrap();
    sqlx::query("insert into fields (id, document_id, party_id, kind, page, x, y, width, height, label, value) values ($1, $2, $3, 'number', 1, 0.1, 0.1, 0.2, 0.05, 'Total', '1250.5')")
        .bind(Uuid::now_v7())
        .bind(doc)
        .bind(party)
        .execute(&mut *conn)
        .await
        .unwrap();
    sqlx::query("update documents set status = 'completed', sealed_file_id = $2, completed_at = now() where id = $1")
        .bind(doc)
        .bind(file)
        .execute(&mut *conn)
        .await
        .unwrap();
    worker::drain(&app).await.unwrap();

    state.lock().unwrap().fail_next = true;
    let run = call::<automations::Test>(&ada, input(json!({ "id": created.id, "documentId": doc }))).await.unwrap().id;
    worker::drain(&app).await.unwrap();
    let detail = call::<automations::GetRun>(&ada, input(json!({ "id": run }))).await.unwrap();
    assert_eq!(detail.delivery.status, "pending", "a Composio outage is retried");
    assert_eq!(detail.attempts_log[0].status_code, Some(502));
    assert!(detail.attempts_log[0].error.as_deref().unwrap().contains("Drive is down"));

    sqlx::query("update jobs set run_at = now() where kind = 'automation' and done_at is null").execute(&mut *conn).await.unwrap();
    worker::drain(&app).await.unwrap();
    let detail = call::<automations::GetRun>(&ada, input(json!({ "id": run }))).await.unwrap();
    assert_eq!((detail.delivery.status.as_str(), detail.delivery.attempts), ("delivered", 2));
    assert!(detail.attempts_log[1].response_excerpt.as_deref().unwrap().contains("file-1"));
    {
        let s = state.lock().unwrap();
        let body = s.executed.last().unwrap();
        assert_eq!(body["connected_account_id"], "ca_drive");
        assert_eq!(body["user_id"], json!(composio::user_id(&app.keys)));
        assert_eq!(body["version"], "20261001_00");
        assert_eq!(body["arguments"]["file_to_upload"], json!({ "name": "NDA.pdf", "mimetype": "application/pdf", "s3key": "k1" }));
        assert_eq!(body["arguments"]["description"], "NDA signed by Anna Andersson");
        assert_eq!(body["arguments"]["total"], json!(1250.5));
        assert!(!body.to_string().contains("19800101"), "personal numbers are never sent");
        assert!(s.uploaded > 0);
    }
    let list = call::<automations::List>(&ada, Empty {}).await.unwrap();
    assert_eq!((list[0].last_status.as_deref(), list[0].failed), (Some("delivered"), 0));
    let actions: Vec<(String, Value)> =
        sqlx::query_as("select action, metadata from audit_log where action = 'automations.run' order by id").fetch_all(&mut *conn).await.unwrap();
    assert_eq!(actions.len(), 2);
    assert_eq!(actions[1].1["sent"], json!(["document.title", "fields.Total", "parties", "sealedPdf"]));

    let disconnected = call::<automations::Disconnect>(&ada, input(json!({ "id": "ca_drive" }))).await;
    assert_eq!(disconnected.unwrap().paused, 1);
    assert!(!call::<automations::List>(&ada, Empty {}).await.unwrap()[0].active);
    let rerun = call::<automations::Resend>(&ada, input(json!({ "id": run }))).await.unwrap().id;
    worker::drain(&app).await.unwrap();
    let detail = call::<automations::GetRun>(&ada, input(json!({ "id": rerun }))).await.unwrap();
    assert_eq!(detail.delivery.attempts, 0, "a paused rule does not run");
    assert_eq!(state.lock().unwrap().executed.len(), 2);

    call::<automations::Delete>(&ada, input(json!({ "id": created.id }))).await.unwrap();
    let left: i64 = sqlx::query_scalar("select count(*) from webhook_deliveries where automation_id is not null").fetch_one(&mut *conn).await.unwrap();
    assert_eq!(left, 0, "runs go with their rule");
}
