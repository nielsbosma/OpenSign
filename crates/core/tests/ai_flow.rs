#![allow(clippy::unwrap_used)]

use std::sync::{Arc, Mutex};

use axum::Json;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::routing::{get, post};
use base64::Engine;
use opensign_core::auth::load_session;
use opensign_core::call;
use opensign_core::context::{App, Ctx, Surface};
use opensign_core::error::ErrorKind;
use opensign_core::services::auth::{self, LoginResult};
use opensign_core::services::{Empty, admin, ai, detect, documents, setup};
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
    if system.contains("You find where a document asks") {
        let reply = "{\"fields\": [{\"page\": 0, \"kind\": \"signature\", \"label\": \"Signed by\", \"x\": 0.1, \"y\": 0.8, \"width\": 0.3, \"height\": 0.05}]}";
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

async fn fake_models(State(seen): State<Seen>, headers: HeaderMap) -> Json<Value> {
    let auth = headers.get("authorization").and_then(|h| h.to_str().ok()).unwrap_or_default();
    seen.lock().unwrap().push(json!({ "models": auth }));
    Json(json!({ "data": [
        { "id": "older-model", "created": 1 },
        { "id": "text-embedding-small", "created": 3 },
        { "id": "test-model", "created": 2 },
    ] }))
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
    let server = axum::Router::new()
        .route("/v1/chat/completions", post(fake_llm))
        .route("/v1/models", get(fake_models))
        .route("/other/v1/models", get(fake_models))
        .with_state(seen.clone());
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

    let none = || ai::ModelsInput { provider: None, base_url: None, api_key: None };
    let models = call::<ai::ListModels>(&ada, none()).await.unwrap();
    let ids: Vec<&str> = models.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(ids, ["test-model", "older-model"], "chat models only, newest first");
    let elsewhere = ai::ModelsInput { base_url: Some(format!("http://127.0.0.1:{port}/other/v1")), ..none() };
    call::<ai::ListModels>(&ada, elsewhere).await.unwrap();
    let auths: Vec<Value> = seen.lock().unwrap().drain(..).map(|v| v["models"].clone()).collect();
    assert_eq!(auths, [json!("Bearer sk-test"), json!("")], "the saved key never goes to an unsaved address");

    let out = call::<ai::Chat>(&ada, chat("Create a draft called Via AI")).await.unwrap();
    assert_eq!(out.actions.len(), 1);
    assert!(!out.actions[0].ok, "api_keys_create is not a tool, so the model cannot mint keys");
    assert_eq!(out.proposals.len(), 1);
    let proposal = &out.proposals[0];
    assert_eq!(proposal.tool, "documents.create");
    assert_eq!(proposal.summary, "Create a new draft", "the words come from the server");
    assert!(proposal.details.iter().any(|d| d.label == "title" && d.value == "Via AI"), "the user sees what would happen");
    let first = seen.lock().unwrap()[0].clone();
    let tools: Vec<&str> = first["tools"].as_array().unwrap().iter().map(|t| t["function"]["name"].as_str().unwrap()).collect();
    assert!(tools.contains(&"documents_create") && !tools.contains(&"api_keys_create") && !tools.contains(&"ai_chat"));
    for kept_out in ["users_update", "users_invite", "webhooks_create", "webhooks_list", "documents_signing_link", "documents_share"] {
        assert!(!tools.contains(&kept_out), "the assistant never gets {kept_out}");
    }
    for withheld in ["documents_values", "documents_export", "documents_review", "documents_signing_preview", "documents_detect_fields"] {
        assert!(!tools.contains(&withheld), "{withheld} needs document text to be allowed");
    }
    let search = || serde_json::from_value::<documents::ListInput>(json!({ "search": "Via AI" })).unwrap();
    assert!(call::<documents::List>(&ada, search()).await.unwrap().is_empty(), "nothing happens until the user confirms");

    let mut steered = ada.clone();
    steered.surface = Surface::Ai;
    let refused = call::<documents::Create>(&steered, serde_json::from_value(proposal.input.clone()).unwrap()).await.err().unwrap();
    assert_eq!(refused.kind, ErrorKind::Forbidden, "the assistant's own calls cannot change anything");

    call::<documents::Create>(&ada, serde_json::from_value(proposal.input.clone()).unwrap()).await.unwrap();
    assert_eq!(call::<documents::List>(&ada, search()).await.unwrap().len(), 1, "confirming is an ordinary call as the user");

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
    let by_sms = checks.iter().find(|c| c.code.as_deref() == Some("no_phone")).unwrap();
    assert_eq!(by_sms.message, "Bo Berg gets the document by SMS but has no phone number");
    assert_eq!(by_sms.params["name"], "Bo Berg");

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

    let mut allowed = 0;
    seen.lock().unwrap().clear();
    call::<ai::Chat>(&ada, chat("Create a draft called Via AI")).await.unwrap();
    for tool in seen.lock().unwrap()[0]["tools"].as_array().unwrap() {
        if tool["function"]["name"] == "documents_values" {
            allowed += 1;
        }
    }
    assert_eq!(allowed, 1, "reading field values is back once document text is allowed");

    let found = call::<detect::DetectFields>(&ada, detect::DetectInput { document_id: id, ai: true }).await.unwrap();
    assert!(found.ai_used && found.suggestions.iter().any(|s| s.source == "ai" && s.label == "Signed by"), "{found:?}");
    let sent = seen.lock().unwrap().last().unwrap().clone();
    assert!(sent["messages"][1]["content"].as_str().unwrap().contains("non-disclosure"), "the page text goes to the model");

    let metadata = json!({ "baseUrl": "http://169.254.169.254/v1" });
    let refused =
        call::<admin::UpdateSettings>(&ada, admin::UpdateInput { group: admin::Group::Ai, values: metadata }).await.err().unwrap();
    assert_eq!(refused.kind, ErrorKind::BadRequest, "a proxy may be private, but not a metadata address");

    let user = ada.actor.as_ref().unwrap().user_id;
    sqlx::query("insert into ai_usage (user_id, kind, model, input_tokens, output_tokens, created_at) select $1, 'chat', 'm', 0, 0, now() from generate_series(1, 400)")
        .bind(user)
        .execute(&app.db)
        .await
        .unwrap();
    let busy = call::<ai::Chat>(&ada, chat("hello")).await.err().unwrap();
    assert_eq!(busy.kind, ErrorKind::TooManyRequests, "one person cannot run up the bill alone");
    sqlx::query("update ai_usage set created_at = created_at - interval '2 hours' where user_id = $1 and model = 'm'")
        .bind(user)
        .execute(&app.db)
        .await
        .unwrap();

    call::<admin::UpdateSettings>(&ada, admin::UpdateInput { group: admin::Group::Ai, values: json!({ "monthlyTokenBudget": 100 }) })
        .await
        .unwrap();
    let usage = call::<ai::GetUsage>(&ada, Empty {}).await.unwrap();
    assert!(usage.tokens_this_month >= 360);
    let spent = call::<ai::Chat>(&ada, chat("hello")).await.err().unwrap();
    assert_eq!(spent.kind, ErrorKind::TooManyRequests, "the monthly budget is enforced");
}
