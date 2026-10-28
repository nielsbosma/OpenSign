#![allow(clippy::unwrap_used)]

use std::sync::{Arc, Mutex};

use axum::Json;
use axum::extract::State;
use axum::routing::post;
use base64::Engine;
use opensign_core::auth::load_session;
use opensign_core::call;
use opensign_core::context::{App, Ctx, Surface};
use opensign_core::error::ErrorKind;
use opensign_core::services::auth::{self, LoginResult};
use opensign_core::services::{Empty, admin, ai, documents, setup};
use opensign_core::settings::SecuritySettings;
use opensign_core::testing::app;
use serde_json::{Value, json};

const PASSWORD: &str = "correct horse battery staple 42";

type Seen = Arc<Mutex<Vec<Value>>>;

/// A stand-in for an OpenAI-compatible provider: asks for one tool call, then answers.
async fn fake_llm(State(seen): State<Seen>, Json(body): Json<Value>) -> Json<Value> {
    seen.lock().unwrap().push(body.clone());
    let messages = body["messages"].as_array().unwrap();
    let system = messages[0]["content"].as_str().unwrap_or_default();
    let usage = json!({ "prompt_tokens": 100, "completion_tokens": 20 });
    if system.contains("You review agreements") {
        let reply = "Sure:\n```json\n{\"summary\": \"A short NDA.\", \"risks\": [{\"severity\": \"high\", \"title\": \"No term\", \"detail\": \"Confidentiality never ends\", \"page\": 1, \"suggestion\": \"Add five years\"}], \"keyTerms\": []}\n```";
        return Json(json!({ "choices": [{ "message": { "content": reply } }], "usage": usage }));
    }
    if messages.iter().any(|m| m["role"] == "tool") {
        return Json(json!({ "choices": [{ "message": { "content": "Created the draft." } }], "usage": usage }));
    }
    Json(json!({
        "choices": [{ "message": { "content": null, "tool_calls": [
            { "id": "call_1", "type": "function", "function": { "name": "documents_create", "arguments": "{\"title\":\"Via AI\"}" } },
            { "id": "call_2", "type": "function", "function": { "name": "api_keys_create", "arguments": "{\"name\":\"x\",\"permissions\":[\"documents.create\"]}" } }
        ] } }],
        "usage": usage
    }))
}

async fn admin(app: &Arc<App>) -> Ctx {
    let anon = Ctx::new(app.clone(), Surface::Web);
    let input = setup::CompleteInput {
        setup_code: app.keys.setup_code(),
        org_name: "Acme AB".into(),
        name: "Ada Admin".into(),
        email: "ada@example.com".into(),
        password: PASSWORD.into(),
    };
    call::<setup::Complete>(&anon, input).await.unwrap();
    let result = call::<auth::Login>(&anon, auth::LoginInput { email: "ada@example.com".into(), password: PASSWORD.into() }).await.unwrap();
    let LoginResult::SignedIn { token, .. } = result else { panic!() };
    let mut conn = app.db.acquire().await.unwrap();
    let (actor, session) =
        load_session(&mut conn, &token, &SecuritySettings::default(), opensign_core::context::now()).await.unwrap().unwrap();
    let mut ctx = Ctx::new(app.clone(), Surface::Web).with_actor(actor);
    ctx.session_id = Some(session);
    ctx
}

fn pdf() -> String {
    use lopdf::{Document, Object, Stream, dictionary};
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let font = doc.add_object(dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica" });
    let text = b"BT /F1 12 Tf 72 720 Td (Mutual non-disclosure agreement between Acme AB and Bo Berg) Tj 0 -20 Td (The parties keep all information confidential forever.) Tj ET";
    let content = doc.add_object(Stream::new(dictionary! {}, text.to_vec()));
    let page = doc.add_object(dictionary! {
        "Type" => "Page", "Parent" => pages_id, "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
        "Contents" => content, "Resources" => dictionary! { "Font" => dictionary! { "F1" => font } },
    });
    doc.objects.insert(pages_id, Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => vec![page.into()], "Count" => 1 }));
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog);
    let mut out = Vec::new();
    doc.save_to(&mut out).unwrap();
    base64::engine::general_purpose::STANDARD.encode(out)
}

#[tokio::test]
async fn assistant_and_review() {
    let Some(app) = app().await else { return };
    let seen: Seen = Arc::default();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = axum::Router::new().route("/v1/chat/completions", post(fake_llm)).with_state(seen.clone());
    tokio::spawn(async move { axum::serve(listener, server).await.unwrap() });

    let ada = admin(&app).await;
    let chat = |text: &str| ai::ChatInput {
        messages: vec![ai::ChatMessage { role: "user".into(), content: text.into() }],
        page: Some("/documents".into()),
    };
    let off = call::<ai::Chat>(&ada, chat("hello")).await.err().unwrap();
    assert_eq!(off.kind, ErrorKind::Unavailable, "the AI features stay off until an Admin sets them up");

    let values = json!({ "provider": "openai-compatible", "model": "test-model", "apiKey": "sk-test", "baseUrl": format!("http://127.0.0.1:{port}/v1") });
    call::<admin::UpdateSettings>(&ada, admin::UpdateInput { group: admin::Group::Ai, values }).await.unwrap();
    assert!(call::<ai::GetStatus>(&ada, Empty {}).await.unwrap().configured);

    let out = call::<ai::Chat>(&ada, chat("Create a draft called Via AI")).await.unwrap();
    assert_eq!(out.reply, "Created the draft.");
    assert_eq!(out.actions.len(), 2);
    assert!(out.actions[0].ok, "documents_create ran as the user");
    assert!(!out.actions[1].ok, "api_keys_create is not a tool, so the model cannot mint keys");
    let first = seen.lock().unwrap()[0].clone();
    let tools: Vec<&str> = first["tools"].as_array().unwrap().iter().map(|t| t["function"]["name"].as_str().unwrap()).collect();
    assert!(tools.contains(&"documents_create") && !tools.contains(&"api_keys_create") && !tools.contains(&"ai_chat"));
    let list = call::<documents::List>(&ada, serde_json::from_value(json!({ "search": "Via AI" })).unwrap()).await.unwrap();
    assert_eq!(list.len(), 1);
    let mut conn = app.db.acquire().await.unwrap();
    let surface: String = sqlx::query_scalar("select surface from audit_log where action = 'document.created' order by id desc limit 1")
        .fetch_one(&mut *conn)
        .await
        .unwrap();
    assert_eq!(surface, "ai");

    let doc = call::<documents::Create>(&ada, serde_json::from_value(json!({ "title": "NDA" })).unwrap()).await.unwrap();
    let id = doc.document.id;
    call::<documents::Upload>(
        &ada,
        serde_json::from_value(json!({ "documentId": id, "name": "nda.pdf", "contentBase64": pdf() })).unwrap(),
    )
    .await
    .unwrap();
    let parties = json!({ "documentId": id, "parties": [{ "name": "Bo Berg", "delivery": "sms", "authToSign": "se_bankid" }] });
    call::<documents::SetParties>(&ada, serde_json::from_value(parties).unwrap()).await.unwrap();
    let checks = call::<ai::CheckSetup>(&ada, ai::IdInput { id }).await.unwrap();
    let messages: Vec<&str> = checks.iter().map(|c| c.message.as_str()).collect();
    assert!(messages.iter().any(|m| m.contains("no signature field")), "{messages:?}");
    assert!(messages.iter().any(|m| m.contains("by SMS but has no phone")), "{messages:?}");
    assert!(messages.iter().any(|m| m.contains("no personal number")), "{messages:?}");

    let refused = call::<ai::Review>(&ada, ai::ReviewInput { id, focus: None }).await.err().unwrap();
    assert_eq!(refused.kind, ErrorKind::Forbidden, "document text stays home until an Admin allows it");
    call::<admin::UpdateSettings>(&ada, admin::UpdateInput { group: admin::Group::Ai, values: json!({ "allowDocumentText": true }) })
        .await
        .unwrap();
    let review = call::<ai::Review>(&ada, ai::ReviewInput { id, focus: Some("We are Acme".into()) }).await.unwrap();
    assert_eq!(review.review["risks"][0]["severity"], "high");
    assert!(!review.setup_checks.is_empty());
    let sent = seen.lock().unwrap().last().unwrap().clone();
    let prompt = sent["messages"][1]["content"].as_str().unwrap();
    assert!(prompt.contains("<document>") && prompt.contains("[Page 1]") && prompt.contains("non-disclosure"), "{prompt}");
    assert!(prompt.contains("We are Acme"));
    let latest = call::<ai::LatestReview>(&ada, ai::IdInput { id }).await.unwrap().unwrap();
    assert_eq!(latest.id, review.id);

    call::<admin::UpdateSettings>(&ada, admin::UpdateInput { group: admin::Group::Ai, values: json!({ "monthlyTokenBudget": 100 }) })
        .await
        .unwrap();
    let usage = call::<ai::GetUsage>(&ada, Empty {}).await.unwrap();
    assert!(usage.tokens_this_month >= 360);
    let spent = call::<ai::Chat>(&ada, chat("hello")).await.err().unwrap();
    assert_eq!(spent.kind, ErrorKind::TooManyRequests, "the monthly budget is enforced");
}
