#![allow(clippy::unwrap_used)]

use axum::body::Body;
use axum::http::{Request, Response, StatusCode, header};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use http_body_util::BodyExt;
use opensign_core::testing::app;
use opensign_server::{AppState, router};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tower::ServiceExt;

const PASSWORD: &str = "correct horse battery staple 42";
const ORIGIN: &str = "http://localhost:3000";
const IP: &str = "203.0.113.30";
const REDIRECT: &str = "http://localhost:33418/callback";

async fn send(router: &axum::Router, request: Request<Body>) -> (Response<Body>, Value) {
    let response = router.clone().oneshot(request).await.unwrap();
    let (parts, body) = response.into_parts();
    let bytes = body.collect().await.unwrap().to_bytes();
    (Response::from_parts(parts, Body::empty()), serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

fn post_json(uri: &str, body: Value, headers: &[(&str, &str)]) -> Request<Body> {
    let mut b = Request::post(uri).header(header::CONTENT_TYPE, "application/json").header("x-forwarded-for", IP);
    for (k, v) in headers {
        b = b.header(*k, *v);
    }
    b.body(Body::from(body.to_string())).unwrap()
}

fn post_form(uri: &str, pairs: &[(&str, &str)]) -> Request<Body> {
    let body: String = url::form_urlencoded::Serializer::new(String::new()).extend_pairs(pairs).finish();
    Request::post(uri)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header("x-forwarded-for", IP)
        .body(Body::from(body))
        .unwrap()
}

fn query_param(url: &str, name: &str) -> Option<String> {
    url::Url::parse(url).unwrap().query_pairs().find(|(k, _)| k == name).map(|(_, v)| v.into_owned())
}

async fn mcp(router: &axum::Router, token: &str, body: Value) -> (Response<Body>, Value) {
    send(
        router,
        post_json("/mcp", body, &[("authorization", &format!("Bearer {token}")), ("accept", "application/json, text/event-stream")]),
    )
    .await
}

#[tokio::test]
async fn oauth_and_mcp() {
    let Some(app) = app().await else { return };
    let router = router(AppState::new(app.clone()));
    let first = json!({ "setupCode": app.keys.setup_code(), "orgName": "Acme AB", "name": "Ada Admin", "email": "ada@example.com", "password": PASSWORD });
    send(&router, post_json("/api/rpc/setup.create_first_admin", first, &[])).await;
    let (response, _) =
        send(&router, post_json("/api/rpc/auth.login", json!({ "email": "ada@example.com", "password": PASSWORD }), &[])).await;
    let cookie = response.headers().get(header::SET_COOKIE).unwrap().to_str().unwrap().split(';').next().unwrap().to_string();
    let session = [("cookie", cookie.as_str()), ("origin", ORIGIN)];

    let (response, _) = send(&router, Request::get("/.well-known/oauth-authorization-server").body(Body::empty()).unwrap()).await;
    assert_eq!(response.status(), StatusCode::OK);
    let (_, resource) = send(&router, Request::get("/.well-known/oauth-protected-resource/mcp").body(Body::empty()).unwrap()).await;
    assert_eq!(resource["resource"], "http://localhost:3000/mcp");

    let (response, unauthenticated) = send(&router, post_json("/mcp", json!({ "jsonrpc": "2.0", "id": 1, "method": "ping" }), &[])).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{unauthenticated}");
    assert!(response.headers().get(header::WWW_AUTHENTICATE).unwrap().to_str().unwrap().contains("oauth-protected-resource/mcp"));

    for bad in ["http://evil.example/cb", "javascript:alert(1)", "https://ok.example/cb#frag"] {
        let (response, err) =
            send(&router, post_json("/oauth/register", json!({ "redirect_uris": [bad], "client_name": "Bad" }), &[])).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{bad} is refused");
        assert_eq!(err["error"], "invalid_redirect_uri");
    }
    let (response, client) =
        send(&router, post_json("/oauth/register", json!({ "redirect_uris": [REDIRECT], "client_name": "Claude" }), &[])).await;
    assert_eq!(response.status(), StatusCode::CREATED, "{client}");
    let client_id = client["client_id"].as_str().unwrap().to_string();

    let verifier = "a-long-random-verifier-string-that-is-at-least-43-characters-long";
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let authorize = |method: &str| {
        format!(
            "/oauth/authorize?response_type=code&client_id={client_id}&redirect_uri={}&code_challenge={challenge}&code_challenge_method={method}&state=xyz&resource={}",
            url::form_urlencoded::byte_serialize(REDIRECT.as_bytes()).collect::<String>(),
            url::form_urlencoded::byte_serialize(b"http://localhost:3000/mcp").collect::<String>(),
        )
    };
    let (response, _) = send(&router, Request::get(authorize("plain")).body(Body::empty()).unwrap()).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST, "plain PKCE is refused, here: nobody has approved this client yet");
    assert!(response.headers().get(header::LOCATION).is_none());
    let (response, _) = send(&router, Request::get(authorize("S256").replace(&client_id, "unknown")).body(Body::empty()).unwrap()).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST, "an unknown client is not redirected anywhere");

    let (response, _) = send(&router, Request::get(authorize("S256")).body(Body::empty()).unwrap()).await;
    assert!(response.status().is_redirection());
    let consent = response.headers().get(header::LOCATION).unwrap().to_str().unwrap().to_string();
    assert!(consent.starts_with("http://localhost:3000/oauth/consent?request="));
    let request = query_param(&consent, "request").unwrap();

    let (_, view) = send(&router, post_json("/api/rpc/oauth.describe", json!({ "request": request }), &session)).await;
    assert_eq!(view["clientName"], "Claude");
    assert_eq!(view["redirectHost"], "localhost");
    let (_, tampered) = send(&router, post_json("/api/rpc/oauth.approve", json!({ "request": format!("{request}x") }), &session)).await;
    assert_eq!(tampered["kind"], "bad_request");
    let (response, approved) = send(&router, post_json("/api/rpc/oauth.approve", json!({ "request": request }), &session)).await;
    assert_eq!(response.status(), StatusCode::OK, "{approved}");
    let redirect = approved["redirectUrl"].as_str().unwrap();
    assert_eq!(query_param(redirect, "state").as_deref(), Some("xyz"));
    let code = query_param(redirect, "code").unwrap();

    let bad_verifier = "x".repeat(50);
    let wrong = [
        ("grant_type", "authorization_code"),
        ("client_id", client_id.as_str()),
        ("code", code.as_str()),
        ("code_verifier", bad_verifier.as_str()),
        ("redirect_uri", REDIRECT),
    ];
    let (response, err) = send(&router, post_form("/oauth/token", &wrong)).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(err["error"], "invalid_grant");
    let exchange = [
        ("grant_type", "authorization_code"),
        ("client_id", client_id.as_str()),
        ("code", code.as_str()),
        ("code_verifier", verifier),
        ("redirect_uri", REDIRECT),
    ];
    let (_, err) = send(&router, post_form("/oauth/token", &exchange)).await;
    assert_eq!(err["error"], "invalid_grant", "a code is spent by the first attempt, even a failed one");

    let (_, approved) = send(&router, post_json("/api/rpc/oauth.approve", json!({ "request": request }), &session)).await;
    let code = query_param(approved["redirectUrl"].as_str().unwrap(), "code").unwrap();
    let exchange = [
        ("grant_type", "authorization_code"),
        ("client_id", client_id.as_str()),
        ("code", code.as_str()),
        ("code_verifier", verifier),
        ("redirect_uri", REDIRECT),
    ];
    let (response, tokens) = send(&router, post_form("/oauth/token", &exchange)).await;
    assert_eq!(response.status(), StatusCode::OK, "{tokens}");
    assert_eq!(response.headers().get(header::CACHE_CONTROL).unwrap(), "no-store");
    let access = tokens["access_token"].as_str().unwrap().to_string();
    let refresh = tokens["refresh_token"].as_str().unwrap().to_string();
    let (_, again) = send(&router, post_form("/oauth/token", &exchange)).await;
    assert_eq!(again["error"], "invalid_grant", "codes work once");
    let (response, _) = send(&router, Request::get(authorize("plain")).body(Body::empty()).unwrap()).await;
    let location = response.headers().get(header::LOCATION).unwrap().to_str().unwrap().to_string();
    assert!(location.starts_with(REDIRECT) && location.contains("error=invalid_request"), "an approved client hears why: {location}");

    let (_, init) = mcp(&router, &access, json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": { "name": "test", "version": "1" } } })).await;
    assert_eq!(init["result"]["protocolVersion"], "2025-06-18");
    let (response, _) = mcp(&router, &access, json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })).await;
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let (_, list) = mcp(&router, &access, json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" })).await;
    let names: Vec<&str> = list["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"documents_create") && names.contains(&"documents_send"), "{names:?}");
    assert!(
        !names.contains(&"api_keys_create") && !names.contains(&"auth_login") && !names.contains(&"oauth_approve"),
        "credentials stay out of MCP"
    );
    let (_, created) =
        mcp(&router, &access, json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": { "name": "documents_create", "arguments": { "title": "From MCP" } } })).await;
    assert_eq!(created["result"]["isError"], false, "{created}");
    assert_eq!(created["result"]["structuredContent"]["result"]["title"], "From MCP");
    let (_, failed) = mcp(&router, &access, json!({ "jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": { "name": "documents_get", "arguments": { "id": uuid::Uuid::new_v4() } } })).await;
    assert_eq!(failed["result"]["isError"], true);
    let (_, audit) = send(&router, post_json("/api/rpc/audit.list", json!({ "action": "document.created" }), &session)).await;
    assert!(audit.to_string().contains("\"mcp\""), "the audit log shows the call came through MCP: {audit}");

    let (response, _) = send(
        &router,
        post_json(
            "/mcp",
            json!({ "jsonrpc": "2.0", "id": 1, "method": "ping" }),
            &[("authorization", &format!("Bearer {access}")), ("origin", "https://evil.example")],
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN, "browsers on other sites are refused");

    let (response, rotated) = send(
        &router,
        post_form(
            "/oauth/token",
            &[("grant_type", "refresh_token"), ("client_id", client_id.as_str()), ("refresh_token", refresh.as_str())],
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK, "{rotated}");
    let new_access = rotated["access_token"].as_str().unwrap().to_string();
    assert_ne!(new_access, access);
    let (response, _) = mcp(&router, &access, json!({ "jsonrpc": "2.0", "id": 5, "method": "ping" })).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "the old access token is replaced");
    let (response, _) = mcp(&router, &new_access, json!({ "jsonrpc": "2.0", "id": 6, "method": "ping" })).await;
    assert_eq!(response.status(), StatusCode::OK);

    let (_, reuse) = send(
        &router,
        post_form(
            "/oauth/token",
            &[("grant_type", "refresh_token"), ("client_id", client_id.as_str()), ("refresh_token", refresh.as_str())],
        ),
    )
    .await;
    assert_eq!(reuse["error"], "invalid_grant");
    let (response, _) = mcp(&router, &new_access, json!({ "jsonrpc": "2.0", "id": 7, "method": "ping" })).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "replaying a rotated refresh token revokes the whole grant");

    let (_, approved) = send(&router, post_json("/api/rpc/oauth.approve", json!({ "request": request }), &session)).await;
    let code = query_param(approved["redirectUrl"].as_str().unwrap(), "code").unwrap();
    let exchange =
        [("grant_type", "authorization_code"), ("client_id", client_id.as_str()), ("code", code.as_str()), ("code_verifier", verifier)];
    let (_, tokens) = send(&router, post_form("/oauth/token", &exchange)).await;
    let access = tokens["access_token"].as_str().unwrap().to_string();
    let (_, connections) = send(&router, post_json("/api/rpc/account.mcp_connections", json!({}), &session)).await;
    let live = connections.as_array().unwrap();
    assert_eq!(live.len(), 1, "{connections}");
    send(&router, post_json("/api/rpc/account.revoke_mcp_connection", json!({ "id": live[0]["id"] }), &session)).await;
    let (response, _) = mcp(&router, &access, json!({ "jsonrpc": "2.0", "id": 8, "method": "ping" })).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "disconnecting in the app cuts the client off");

    let (_, key) =
        send(&router, post_json("/api/rpc/api_keys.create", json!({ "name": "Agent", "permissions": ["documents.create"] }), &session))
            .await;
    let (_, list) = mcp(&router, key["key"].as_str().unwrap(), json!({ "jsonrpc": "2.0", "id": 9, "method": "tools/list" })).await;
    let names: Vec<&str> = list["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"documents_create") && !names.contains(&"users_invite"), "an API key gets only its scopes: {names:?}");
}

fn pdf_fixture() -> Vec<u8> {
    use lopdf::{Document, Object, Stream, dictionary};
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let font = doc.add_object(dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica" });
    let content = doc.add_object(Stream::new(dictionary! {}, b"BT /F1 12 Tf 72 720 Td (Agreement) Tj ET".to_vec()));
    let page = doc.add_object(dictionary! {
        "Type" => "Page", "Parent" => pages_id, "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
        "Contents" => content, "Resources" => dictionary! { "Font" => dictionary! { "F1" => font } },
    });
    doc.objects.insert(pages_id, Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => vec![page.into()], "Count" => 1 }));
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog);
    let mut out = Vec::new();
    doc.save_to(&mut out).unwrap();
    out
}

async fn call(router: &axum::Router, token: &str, name: &str, arguments: Value) -> (bool, Value) {
    let (_, reply) = mcp(
        router,
        token,
        json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": name, "arguments": arguments } }),
    )
    .await;
    let result = &reply["result"];
    let ok = result["isError"] == false;
    (ok, if ok { result["structuredContent"]["result"].clone() } else { result["content"][0]["text"].clone() })
}

fn path_of(url: &str) -> String {
    url.strip_prefix("http://localhost:3000").unwrap().to_string()
}

#[tokio::test]
async fn agent_tools() {
    let Some(app) = app().await else { return };
    let router = router(AppState::new(app.clone()));
    let first = json!({ "setupCode": app.keys.setup_code(), "orgName": "Acme AB", "name": "Ada Admin", "email": "ada@example.com", "password": PASSWORD });
    send(&router, post_json("/api/rpc/setup.create_first_admin", first, &[])).await;
    let (response, _) =
        send(&router, post_json("/api/rpc/auth.login", json!({ "email": "ada@example.com", "password": PASSWORD }), &[])).await;
    let cookie = response.headers().get(header::SET_COOKIE).unwrap().to_str().unwrap().split(';').next().unwrap().to_string();
    let session = [("cookie", cookie.as_str()), ("origin", ORIGIN)];
    let scopes =
        json!({ "name": "Agent", "permissions": ["settings.manage", "documents.read", "documents.create", "users.read", "ai.use"] });
    let (_, key) = send(&router, post_json("/api/rpc/api_keys.create", scopes, &session)).await;
    let key = key["key"].as_str().unwrap().to_string();

    let (_, list) = mcp(&router, &key, json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" })).await;
    let tools = list["result"]["tools"].as_array().unwrap();
    let tool = |name: &str| tools.iter().find(|t| t["name"] == name).cloned();
    for name in
        ["settings_update", "system_update", "ai_models", "ai_test", "documents_review", "docs_search", "docs_read", "documents_upload_url"]
    {
        assert!(tool(name).is_some(), "{name} is a tool");
    }
    for name in
        ["api_keys_create", "account_change_password", "account_two_factor_begin", "seal_regenerate", "users_reset_two_factor", "ai_chat"]
    {
        assert!(tool(name).is_none(), "{name} stays hidden");
    }
    assert_eq!(tool("settings_update").unwrap()["annotations"]["destructiveHint"], true);
    assert_eq!(tool("system_update").unwrap()["annotations"]["openWorldHint"], true);
    assert_eq!(tool("docs_search").unwrap()["annotations"]["readOnlyHint"], true);

    let (ok, hits) = call(&router, &key, "docs_search", json!({ "query": "refresh token rotate" })).await;
    assert!(ok, "{hits}");
    assert_eq!(hits[0]["slug"], "mcp", "{hits}");
    let (_, page) = call(&router, &key, "docs_read", json!({ "slug": "mcp", "anchor": hits[0]["anchor"] })).await;
    assert!(page["markdown"].as_str().unwrap().starts_with("## "), "{page}");
    let (_, index) = call(&router, &key, "docs_read", json!({})).await;
    assert!(index["pages"].as_array().unwrap().len() > 20);

    let (ok, refused) = call(&router, &key, "settings_update", json!({ "group": "ai", "values": { "apiKey": "sk-from-chat" } })).await;
    assert!(!ok && refused.as_str().unwrap().contains("secret"), "{refused}");
    let (ok, refused) =
        call(&router, &key, "settings_update", json!({ "group": "email", "values": { "smtpPassword": "hunter2", "provider": "smtp" } }))
            .await;
    assert!(!ok, "{refused}");
    let saved = json!({ "group": "ai", "values": { "provider": "openai", "model": "gpt-test", "apiKey": "sk-saved" } });
    let (response, body) = send(&router, post_json("/api/rpc/settings.update", saved, &session)).await;
    assert_eq!(response.status(), StatusCode::OK, "{body}");
    let (ok, refused) =
        call(&router, &key, "settings_update", json!({ "group": "ai", "values": { "baseUrl": "https://evil.example/v1" } })).await;
    assert!(!ok && refused.as_str().unwrap().contains("somewhere new"), "a saved key cannot be pointed elsewhere: {refused}");
    let (ok, changed) = call(&router, &key, "settings_update", json!({ "group": "ai", "values": { "monthlyTokenBudget": 5000 } })).await;
    assert!(ok, "{changed}");
    assert_eq!(changed["values"]["monthlyTokenBudget"], 5000);
    let (ok, cleared) =
        call(&router, &key, "settings_update", json!({ "group": "ai", "values": { "apiKey": "", "baseUrl": "https://other.example/v1" } }))
            .await;
    assert!(ok, "clearing the key while moving it is fine: {cleared}");
    let (ok, refused) = call(&router, &key, "ai_models", json!({ "apiKey": "sk-typed" })).await;
    assert!(!ok && refused.as_str().unwrap().contains("signed in"), "{refused}");

    let (_, doc) = call(&router, &key, "documents_create", json!({ "title": "Big file" })).await;
    let id = doc["id"].as_str().unwrap().to_string();
    let (ok, link) = call(&router, &key, "documents_upload_url", json!({ "documentId": id, "name": "Big.pdf" })).await;
    assert!(ok, "{link}");
    let url = path_of(link["url"].as_str().unwrap());
    let put = |body: Vec<u8>| Request::put(&url).header("x-forwarded-for", IP).body(Body::from(body)).unwrap();
    let (response, uploaded) = send(&router, put(pdf_fixture())).await;
    assert_eq!(response.status(), StatusCode::OK, "{uploaded}");
    assert_eq!(uploaded["pages"], 1);
    let (response, _) = send(&router, put(pdf_fixture())).await;
    assert_eq!(response.status(), StatusCode::CONFLICT, "an upload link works once");
    let forged = format!("{}x", url);
    let (response, _) = send(&router, Request::put(&forged).header("x-forwarded-for", IP).body(Body::from(pdf_fixture())).unwrap()).await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let (_, audit) = send(&router, post_json("/api/rpc/audit.list", json!({ "action": "document.file_added" }), &session)).await;
    assert!(audit.to_string().contains("\"mcp\""), "the upload is audited as the agent's call: {audit}");

    let parties = json!({ "documentId": id, "parties": [{ "role": "signer", "name": "Anna", "delivery": "link" }] });
    let (ok, body) = call(&router, &key, "documents_set_parties", parties).await;
    assert!(ok, "{body}");
    let (ok, body) = call(&router, &key, "documents_send", json!({ "id": id })).await;
    assert!(ok, "{body}");
    let (ok, link) = call(&router, &key, "documents_download_url", json!({ "id": id, "version": "original" })).await;
    assert!(ok, "{link}");
    let (response, _) =
        send(&router, Request::get(path_of(link["url"].as_str().unwrap())).header("x-forwarded-for", IP).body(Body::empty()).unwrap())
            .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers().get(header::CONTENT_TYPE).unwrap(), "application/pdf");

    let (_, draft) = call(&router, &key, "documents_create", json!({ "title": "Later" })).await;
    let (_, link) = call(&router, &key, "documents_upload_url", json!({ "documentId": draft["id"], "name": "Later.pdf" })).await;
    let (_, keys) = send(&router, post_json("/api/rpc/api_keys.list", json!({}), &session)).await;
    send(&router, post_json("/api/rpc/api_keys.revoke", json!({ "id": keys[0]["id"] }), &session)).await;
    let late = Request::put(path_of(link["url"].as_str().unwrap())).header("x-forwarded-for", IP).body(Body::from(pdf_fixture())).unwrap();
    let (response, _) = send(&router, late).await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND, "revoking the key also stops its upload links");
}

#[tokio::test]
async fn consent_names_the_domain_and_registration_can_be_closed() {
    let Some(app) = app().await else { return };
    let router = router(AppState::new(app.clone()));
    let first = json!({ "setupCode": app.keys.setup_code(), "orgName": "Acme AB", "name": "Ada Admin", "email": "ada@example.com", "password": PASSWORD });
    send(&router, post_json("/api/rpc/setup.create_first_admin", first, &[])).await;
    let (response, _) =
        send(&router, post_json("/api/rpc/auth.login", json!({ "email": "ada@example.com", "password": PASSWORD }), &[])).await;
    let cookie = response.headers().get(header::SET_COOKIE).unwrap().to_str().unwrap().split(';').next().unwrap().to_string();
    let session = [("cookie", cookie.as_str()), ("origin", ORIGIN)];

    let long = "https://claude.ai.aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.example.co.uk/cb";
    let (_, client) = send(&router, post_json("/oauth/register", json!({ "client_name": "Claude", "redirect_uris": [long] }), &[])).await;
    let client_id = client["client_id"].as_str().unwrap();
    let verifier = "a-long-random-verifier-string-that-is-at-least-43-characters-long";
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let (response, _) = send(
        &router,
        Request::get(format!(
            "/oauth/authorize?response_type=code&client_id={client_id}&code_challenge={challenge}&code_challenge_method=S256"
        ))
        .body(Body::empty())
        .unwrap(),
    )
    .await;
    let location = response.headers().get(header::LOCATION).unwrap().to_str().unwrap().to_string();
    let request = query_param(&location, "request").unwrap();
    let (_, view) = send(&router, post_json("/api/rpc/oauth.describe", json!({ "request": request }), &session)).await;
    assert_eq!(view["redirectDomain"], "example.co.uk", "{view}");

    let (_, _) = send(
        &router,
        post_json("/api/rpc/settings.update", json!({ "group": "security", "values": { "allowClientRegistration": false } }), &session),
    )
    .await;
    let (_, metadata) = send(&router, Request::get("/.well-known/oauth-authorization-server").body(Body::empty()).unwrap()).await;
    assert!(metadata.get("registration_endpoint").is_none(), "{metadata}");
    let (response, refused) = send(&router, post_json("/oauth/register", json!({ "redirect_uris": ["https://ok.example/cb"] }), &[])).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(refused["error"], "access_denied");
    let (_, view) = send(&router, post_json("/api/rpc/oauth.describe", json!({ "request": request }), &session)).await;
    assert_eq!(view["clientName"], "Claude", "clients that registered earlier keep working");
}
