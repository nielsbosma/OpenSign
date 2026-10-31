#![allow(clippy::unwrap_used)]

use axum::body::Body;
use axum::http::{Request, Response, StatusCode, header};
use http_body_util::BodyExt;
use opensign_core::testing::app;
use opensign_server::{AppState, router};
use serde_json::{Value, json};
use tower::ServiceExt;

const PASSWORD: &str = "correct horse battery staple 42";
const ORIGIN: &str = "http://localhost:3000";

struct Client {
    router: axum::Router,
    ip: &'static str,
}

impl Client {
    fn rpc(&self, name: &str) -> axum::http::request::Builder {
        Request::post(format!("/api/rpc/{name}")).header(header::CONTENT_TYPE, "application/json").header("x-forwarded-for", self.ip)
    }

    async fn send(&self, request: Request<Body>) -> (Response<Body>, Value) {
        let response = self.router.clone().oneshot(request).await.unwrap();
        let (parts, body) = response.into_parts();
        let bytes = body.collect().await.unwrap().to_bytes();
        let json = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        (Response::from_parts(parts, Body::empty()), json)
    }

    async fn call(&self, name: &str, body: Value, extra: &[(&str, &str)]) -> (Response<Body>, Value) {
        let mut builder = self.rpc(name);
        for (k, v) in extra {
            builder = builder.header(*k, *v);
        }
        self.send(builder.body(Body::from(body.to_string())).unwrap()).await
    }
}

fn cookie_of(response: &Response<Body>) -> String {
    let set = response.headers().get(header::SET_COOKIE).unwrap().to_str().unwrap();
    assert!(set.contains("HttpOnly") && set.contains("SameSite=Lax"), "{set}");
    set.split(';').next().unwrap().to_string()
}

#[tokio::test]
async fn sessions_csrf_rate_limits_and_api_keys() {
    let Some(app) = app().await else { return };
    let state = AppState::new(app.clone());
    let client = Client { router: router(state), ip: "203.0.113.7" };

    let (response, _) = client.call("account.me", json!({}), &[]).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let h = response.headers();
    assert!(h.get(header::CONTENT_SECURITY_POLICY).unwrap().to_str().unwrap().contains("frame-ancestors 'none'"));
    assert_eq!(h.get(header::X_CONTENT_TYPE_OPTIONS).unwrap(), "nosniff");
    assert_eq!(h.get(header::REFERRER_POLICY).unwrap(), "no-referrer");
    assert!(h.get("x-request-id").is_some());

    let (response, _) = client.call("no.such_call", json!({}), &[]).await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let setup = json!({ "setupCode": app.keys.setup_code(), "orgName": "Acme AB", "name": "Ada", "email": "ada@example.com", "password": PASSWORD });
    let (response, _) = client.call("setup.create_first_admin", setup, &[]).await;
    assert_eq!(response.status(), StatusCode::OK);

    let (response, body) = client.call("auth.login", json!({ "email": "ada@example.com", "password": PASSWORD }), &[]).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(body["status"], "signed_in");
    assert!(body.get("token").is_none(), "the session token only travels in the cookie");
    assert_eq!(response.headers().get(header::CACHE_CONTROL).unwrap(), "no-store");
    let cookie = cookie_of(&response);
    assert!(cookie.starts_with("opensign_session="));

    let (response, me) = client.call("account.me", json!({}), &[("cookie", &cookie), ("origin", ORIGIN)]).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(me["email"], "ada@example.com");

    for origin in ["https://evil.example", "https://localhost:3000", "null"] {
        let (response, _) = client.call("account.me", json!({}), &[("cookie", &cookie), ("origin", origin)]).await;
        assert_eq!(response.status(), StatusCode::FORBIDDEN, "a cookie write from {origin} is refused");
    }
    let (response, _) = client.call("account.me", json!({}), &[("cookie", &cookie)]).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN, "no Origin and no Sec-Fetch-Site is refused");
    let (response, _) = client.call("account.me", json!({}), &[("cookie", &cookie), ("sec-fetch-site", "cross-site")]).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let (response, _) = client.call("account.me", json!({}), &[("cookie", &cookie), ("sec-fetch-site", "same-origin")]).await;
    assert_eq!(response.status(), StatusCode::OK);

    let key_input = json!({ "name": "CI", "permissions": ["documents.create"] });
    let (response, key) = client.call("api_keys.create", key_input.clone(), &[("cookie", &cookie), ("origin", ORIGIN)]).await;
    assert_eq!(response.status(), StatusCode::OK, "{key}");
    let bearer = format!("Bearer {}", key["key"].as_str().unwrap());
    let (response, _) = client.call("documents.list", json!({}), &[("authorization", &bearer)]).await;
    assert_eq!(response.status(), StatusCode::OK, "an API key needs no Origin: it is not ambient");
    let (response, _) = client.call("api_keys.create", key_input, &[("authorization", &bearer)]).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN, "keys cannot reach app-only calls");
    let (response, _) = client.call("account.me", json!({}), &[("authorization", "Bearer osk_not_a_real_key")]).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "a bad key is refused, not ignored");

    let auth: &[(&str, &str)] = &[("cookie", &cookie), ("origin", ORIGIN)];
    let forged = json!({ "group": "email", "values": { "fromAddress": "sign@example.com", "verifiedAt": "2020-01-01T00:00:00Z" } });
    let (response, _) = client.call("settings.update", forged, auth).await;
    assert_eq!(response.status(), StatusCode::OK);
    let (_, status) = client.call("setup.status", json!({}), auth).await;
    assert_eq!(status["emailConfigured"], false, "only a test email that went out marks email as working");
    let (response, _) = client.call("settings.send_test_email", json!({}), auth).await;
    assert_eq!(response.status(), StatusCode::OK);
    let (_, status) = client.call("setup.status", json!({}), auth).await;
    assert_eq!(status["emailConfigured"], true);
    let changed = json!({ "group": "email", "values": { "fromAddress": "other@example.com" } });
    client.call("settings.update", changed, auth).await;
    let (_, status) = client.call("setup.status", json!({}), auth).await;
    assert_eq!(status["emailConfigured"], false, "a new sender needs a new test");

    let url = json!({ "group": "email", "values": { "provider": "smtp", "smtpHost": "smtp://mail.example.com" } });
    let (response, _) = client.call("settings.update", url, auth).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST, "a URL is not a host name");
    let plain = json!({ "group": "email", "values": { "provider": "smtp", "smtpHost": "mail.example.com", "smtpSecurity": "none" } });
    let (response, _) = client.call("settings.update", plain, auth).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST, "unencrypted SMTP is refused");
    let smtp = json!({ "group": "email", "values": {
        "provider": "smtp", "smtpHost": "mail.example.com", "smtpPort": 465, "smtpSecurity": "tls", "smtpUsername": "opensign", "smtpPassword": "hunter2"
    } });
    let (response, saved) = client.call("settings.update", smtp, auth).await;
    assert_eq!(response.status(), StatusCode::OK, "{saved}");
    assert_eq!(saved["values"]["smtpPassword"], true, "the password is never returned");
    assert_eq!(saved["values"]["smtpHost"], "mail.example.com");

    let (response, _) = client.call("auth.logout", json!({}), &[("cookie", &cookie), ("origin", ORIGIN)]).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert!(response.headers().get(header::SET_COOKIE).unwrap().to_str().unwrap().contains("Max-Age=0"));
    let (response, _) = client.call("account.me", json!({}), &[("cookie", &cookie), ("origin", ORIGIN)]).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "the session is gone server side too");

    let guesser = Client { router: client.router.clone(), ip: "198.51.100.9" };
    let mut statuses = Vec::new();
    for _ in 0..12 {
        let (response, _) = guesser.call("auth.login", json!({ "email": "ada@example.com", "password": "wrong" }), &[]).await;
        statuses.push(response.status());
    }
    assert_eq!(statuses.last(), Some(&StatusCode::TOO_MANY_REQUESTS));
    let (response, _) = client.call("auth.login", json!({ "email": "bo@example.com", "password": PASSWORD }), &[]).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "other addresses are not throttled");
    let (response, _) = client.call("auth.login", json!({ "email": "ada@example.com", "password": PASSWORD }), &[]).await;
    assert_eq!(response.status(), StatusCode::OK, "and one address cannot lock the owner out");

    let (response, body) = client.send(Request::get("/api/nothing-here").body(Body::empty()).unwrap()).await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(body["kind"], "not_found");
    let (response, body) = client.send(Request::get("/api/health").body(Body::empty()).unwrap()).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(body["ok"], true);
}

#[tokio::test]
async fn file_endpoints_need_access_and_serve_pdfs_safely() {
    let Some(app) = app().await else { return };
    let client = Client { router: router(AppState::new(app.clone())), ip: "203.0.113.8" };
    let setup = json!({ "setupCode": app.keys.setup_code(), "orgName": "Acme AB", "name": "Ada", "email": "ada@example.com", "password": PASSWORD });
    client.call("setup.create_first_admin", setup, &[]).await;
    let (response, _) = client.call("auth.login", json!({ "email": "ada@example.com", "password": PASSWORD }), &[]).await;
    let cookie = cookie_of(&response);
    let auth: [(&str, &str); 2] = [("cookie", &cookie), ("origin", ORIGIN)];

    let (_, doc) = client.call("documents.create", json!({ "title": "Agreement" }), &auth).await;
    let id = doc["id"].as_str().unwrap().to_string();

    let pdf = opensign_pdf_fixture();
    let boundary = "XBOUNDARYX";
    let mut multipart = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"agreement.pdf\"\r\nContent-Type: application/pdf\r\n\r\n"
    )
    .into_bytes();
    multipart.extend_from_slice(&pdf);
    multipart.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    let upload = |cookie: Option<&str>| {
        let mut b = Request::post(format!("/api/documents/{id}/files"))
            .header(header::CONTENT_TYPE, format!("multipart/form-data; boundary={boundary}"))
            .header("x-forwarded-for", "203.0.113.8")
            .header("origin", ORIGIN);
        if let Some(c) = cookie {
            b = b.header("cookie", c);
        }
        b.body(Body::from(multipart.clone())).unwrap()
    };
    let (response, _) = client.send(upload(None)).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let (response, body) = client.send(upload(Some(&cookie))).await;
    assert_eq!(response.status(), StatusCode::OK, "{body}");

    let (response, _) = client.send(Request::get(format!("/api/documents/{id}/preview")).body(Body::empty()).unwrap()).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let preview = Request::get(format!("/api/documents/{id}/preview")).header("cookie", &cookie).body(Body::empty()).unwrap();
    let (response, _) = client.send(preview).await;
    assert_eq!(response.status(), StatusCode::OK);
    let h = response.headers();
    assert_eq!(h.get(header::CONTENT_TYPE).unwrap(), "application/pdf");
    assert!(h.get(header::CONTENT_DISPOSITION).unwrap().to_str().unwrap().starts_with("attachment"));
    assert!(h.get(header::CONTENT_SECURITY_POLICY).unwrap().to_str().unwrap().starts_with("sandbox"));

    let other = uuid::Uuid::new_v4();
    let missing = Request::get(format!("/api/documents/{other}/preview")).header("cookie", &cookie).body(Body::empty()).unwrap();
    assert_eq!(client.send(missing).await.0.status(), StatusCode::NOT_FOUND);

    let signer = Request::post("/api/sign/file").header("x-forwarded-for", "203.0.113.8").body(Body::from(r#"{"token":"x.y"}"#)).unwrap();
    let (response, _) = client.send(signer).await;
    assert!(response.status().is_client_error());

    let verify = Request::post("/api/verify").header("x-forwarded-for", "203.0.113.8").body(Body::from(pdf)).unwrap();
    let (response, body) = client.send(verify).await;
    assert_eq!(response.status(), StatusCode::OK, "{body}");
    assert_eq!(body["signatures"].as_array().map(Vec::len), Some(0));
}

fn opensign_pdf_fixture() -> Vec<u8> {
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

#[tokio::test]
async fn rest_api_with_keys_idempotency_and_openapi() {
    let Some(app) = app().await else { return };
    let client = Client { router: router(AppState::new(app.clone())), ip: "203.0.113.9" };
    let setup = json!({ "setupCode": app.keys.setup_code(), "orgName": "Acme AB", "name": "Ada", "email": "ada@example.com", "password": PASSWORD });
    client.call("setup.create_first_admin", setup, &[]).await;
    let (response, _) = client.call("auth.login", json!({ "email": "ada@example.com", "password": PASSWORD }), &[]).await;
    let cookie = cookie_of(&response);
    let session: [(&str, &str); 2] = [("cookie", &cookie), ("origin", ORIGIN)];
    let (_, key) = client.call("api_keys.create", json!({ "name": "CRM", "permissions": ["documents.create"] }), &session).await;
    let bearer = format!("Bearer {}", key["key"].as_str().unwrap());
    let (_, narrow) = client.call("api_keys.create", json!({ "name": "Org only", "permissions": ["org.read"] }), &session).await;
    let narrow = format!("Bearer {}", narrow["key"].as_str().unwrap());

    let rest = |method: &str, path: &str, auth: &str, body: Option<Value>, idempotency: Option<&str>| {
        let mut b = Request::builder()
            .method(method)
            .uri(format!("/api/v1{path}"))
            .header("authorization", auth)
            .header("x-forwarded-for", "203.0.113.9");
        if let Some(k) = idempotency {
            b = b.header("idempotency-key", k);
        }
        match body {
            Some(body) => b.header(header::CONTENT_TYPE, "application/json").body(Body::from(body.to_string())).unwrap(),
            None => b.body(Body::empty()).unwrap(),
        }
    };

    let (_, manager) =
        client.call("api_keys.create", json!({ "name": "Ops", "permissions": ["api_keys.manage", "org.read"] }), &session).await;
    let manager_bearer = format!("Bearer {}", manager["key"].as_str().unwrap());
    let manager_path = format!("/api-keys/{}", manager["id"].as_str().unwrap());
    let (response, edited) = client
        .send(rest("PATCH", &manager_path, &manager_bearer, Some(json!({ "name": "Ops edited", "allowedIps": ["203.0.113.0/24"] })), None))
        .await;
    assert_eq!(response.status(), StatusCode::OK, "{edited}");
    assert_eq!(edited["allowedIps"], json!(["203.0.113.0/24"]));
    assert!(edited.get("key").is_none() && !edited.to_string().contains(manager["key"].as_str().unwrap()));
    let (response, _) = client
        .send(rest(
            "PATCH",
            &manager_path,
            &manager_bearer,
            Some(json!({ "permissions": ["api_keys.manage", "documents.read_all"] })),
            None,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN, "a key cannot widen a key");
    let (response, _) = client.send(rest("PATCH", &manager_path, &manager_bearer, Some(json!({ "allowedIps": [] })), None)).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN, "a key cannot lift an IP allowlist");
    let (response, listed) = client.send(rest("GET", "/api-keys", &manager_bearer, None, None)).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert!(listed.as_array().unwrap().iter().any(|k| k["name"] == "Ops edited"));

    let (response, created) =
        client.send(rest("POST", "/documents", &bearer, Some(json!({ "title": "NDA", "externalId": "crm-42" })), Some("create-nda"))).await;
    assert_eq!(response.status(), StatusCode::CREATED, "{created}");
    let id = created["id"].as_str().unwrap().to_string();
    let (response, again) =
        client.send(rest("POST", "/documents", &bearer, Some(json!({ "title": "NDA", "externalId": "crm-42" })), Some("create-nda"))).await;
    assert_eq!(response.status(), StatusCode::CREATED);
    assert_eq!(response.headers().get("idempotent-replayed").unwrap(), "true");
    assert_eq!(again["id"], created["id"], "a retry does not create a second document");
    let stored: String =
        sqlx::query_scalar("select response::text from idempotency_keys where key = 'create-nda'").fetch_one(&app.db).await.unwrap();
    assert!(!stored.contains("crm-42"), "stored responses are encrypted");
    let (response, _) = client.send(rest("POST", "/documents", &bearer, Some(json!({ "title": "Other" })), Some("create-nda"))).await;
    assert_eq!(response.status(), StatusCode::CONFLICT, "a reused key with another body is refused");

    let (response, list) = client.send(rest("GET", "/documents?search=crm-42&limit=5", &bearer, None, None)).await;
    assert_eq!(response.status(), StatusCode::OK, "{list}");
    assert_eq!(list.as_array().unwrap().len(), 1);
    let (response, _) = client.send(rest("GET", "/documents?limit=lots", &bearer, None, None)).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let (response, templates) = client.send(rest("GET", "/templates", &bearer, None, None)).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(templates.as_array().unwrap().len(), 0);

    let (response, doc) = client.send(rest("PATCH", &format!("/documents/{id}"), &bearer, Some(json!({ "title": "NDA v2" })), None)).await;
    assert_eq!(response.status(), StatusCode::OK, "{doc}");
    assert_eq!(doc["title"], "NDA v2");
    let parties = json!({ "parties": [{ "role": "signer", "name": "Bo Berg", "email": "bo@example.com" }] });
    let (response, body) = client.send(rest("PUT", &format!("/documents/{id}/parties"), &bearer, Some(parties), None)).await;
    assert_eq!(response.status(), StatusCode::OK, "{body}");

    let (response, _) = client.send(rest("GET", &format!("/documents/{id}"), &narrow, None, None)).await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND, "a key without document scopes sees nothing");
    let (response, _) = client.send(rest("GET", "/documents/not-a-uuid", &bearer, None, None)).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let (response, _) = client.send(rest("GET", "/documents", "Bearer nope", None, None)).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let (response, _) = client.send(rest("DELETE", &format!("/documents/{id}"), &bearer, None, None)).await;
    assert_eq!(response.status(), StatusCode::OK);

    let (response, spec) = client.send(Request::get("/api/v1/openapi.json").body(Body::empty()).unwrap()).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(spec["openapi"], "3.1.0");
    let text = spec.to_string();
    assert!(!text.contains("#/$defs/"), "every reference points into components");
    for (path, ops) in spec["paths"].as_object().unwrap() {
        for (method, op) in ops.as_object().unwrap() {
            assert!(op["operationId"].is_string(), "{method} {path}");
        }
    }
    let send = &spec["paths"]["/documents/{id}/send"]["post"];
    assert_eq!(send["parameters"][0]["name"], "id");
    let refs: Vec<String> =
        text.match_indices("#/components/schemas/").map(|(i, _)| text[i + 21..].split('"').next().unwrap().to_string()).collect();
    for r in refs {
        assert!(spec["components"]["schemas"].get(&r).is_some(), "{r} is defined");
    }
}

#[test]
fn every_rest_path_parameter_is_an_input_field() {
    use opensign_core::services::registry;
    let services: std::collections::HashMap<_, _> = registry().into_iter().map(|s| (s.meta().name, s)).collect();
    for route in opensign_server::rest::ROUTES {
        let service = services.get(route.service).unwrap_or_else(|| panic!("{} is registered", route.service));
        let schema = serde_json::to_value(service.input_schema()).unwrap();
        for param in route.path.split('/').filter_map(|s| s.strip_prefix('{').and_then(|s| s.strip_suffix('}'))) {
            assert!(schema["properties"].get(param).is_some(), "{} {} has no field {param}", route.method, route.path);
        }
        for (field, _) in route.fixed {
            assert!(schema["properties"].get(*field).is_some(), "{} has no field {field}", route.service);
        }
    }
}

#[tokio::test]
async fn only_the_signing_page_may_use_the_camera() {
    let Some(app) = app().await else { return };
    let client = Client { router: router(AppState::new(app.clone())), ip: "203.0.113.9" };
    let policy = |response: &Response<Body>| response.headers().get("permissions-policy").unwrap().to_str().unwrap().to_string();
    let (sign, _) = client.send(Request::get("/sign").body(Body::empty()).unwrap()).await;
    assert!(policy(&sign).starts_with("camera=(self)"), "{}", policy(&sign));
    for path in ["/documents", "/sign/x", "/api/health"] {
        let (other, _) = client.send(Request::get(path).body(Body::empty()).unwrap()).await;
        assert!(policy(&other).starts_with("camera=()"), "{path}: {}", policy(&other));
    }
}

#[tokio::test]
async fn only_listed_sites_may_frame_the_signing_page() {
    let Some(app) = app().await else { return };
    let client = Client { router: router(AppState::new(app.clone())), ip: "203.0.113.10" };
    let ancestors = |response: &Response<Body>| {
        let csp = response.headers().get(header::CONTENT_SECURITY_POLICY).unwrap().to_str().unwrap().to_string();
        csp.split(';').map(str::trim).find(|d| d.starts_with("frame-ancestors")).unwrap().to_string()
    };
    let get = |path: &str| Request::get(path).body(Body::empty()).unwrap();

    let (sign, _) = client.send(get("/sign")).await;
    assert_eq!(ancestors(&sign), "frame-ancestors 'none'", "off by default");
    assert_eq!(sign.headers().get(header::X_FRAME_OPTIONS).unwrap(), "DENY");

    let mut conn = app.db.acquire().await.unwrap();
    let security = opensign_core::settings::SecuritySettings {
        embed_signing: true,
        embed_origins: vec!["https://app.example.com".into()],
        ..Default::default()
    };
    app.settings.save(&mut conn, &app.keys, &security, None).await.unwrap();

    let (sign, _) = client.send(get("/sign")).await;
    assert_eq!(ancestors(&sign), "frame-ancestors https://app.example.com");
    assert!(sign.headers().get(header::X_FRAME_OPTIONS).is_none());
    for path in ["/", "/documents", "/settings/admin/security", "/sign/x", "/api/health", "/verify"] {
        let (other, _) = client.send(get(path)).await;
        assert_eq!(ancestors(&other), "frame-ancestors 'none'", "{path}");
        assert_eq!(other.headers().get(header::X_FRAME_OPTIONS).unwrap(), "DENY", "{path}");
    }

    let (script, _) = client.send(get("/embed.js")).await;
    assert_eq!(script.headers().get("cross-origin-resource-policy").unwrap(), "cross-origin");

    let off = opensign_core::settings::SecuritySettings { embed_signing: false, ..security };
    app.settings.save(&mut conn, &app.keys, &off, None).await.unwrap();
    let (sign, _) = client.send(get("/sign")).await;
    assert_eq!(ancestors(&sign), "frame-ancestors 'none'", "turning it off stops framing at once");
}

#[tokio::test]
async fn review_links_serve_text_without_indexing() {
    let Some(app) = app().await else { return };
    let client = Client { router: router(AppState::new(app.clone())), ip: "203.0.113.9" };
    let setup = json!({ "setupCode": app.keys.setup_code(), "orgName": "Acme AB", "name": "Ada", "email": "ada@example.com", "password": PASSWORD });
    client.call("setup.create_first_admin", setup, &[]).await;
    let (response, _) = client.call("auth.login", json!({ "email": "ada@example.com", "password": PASSWORD }), &[]).await;
    let cookie = cookie_of(&response);
    let auth: [(&str, &str); 2] = [("cookie", &cookie), ("origin", ORIGIN)];

    let (_, doc) = client.call("documents.create", json!({ "title": "Agreement" }), &auth).await;
    let id = doc["id"].as_str().unwrap().to_string();
    let upload = json!({ "documentId": id, "name": "agreement.pdf", "contentBase64": base64_of(&opensign_pdf_fixture()) });
    let (response, body) = client.call("documents.upload", upload, &auth).await;
    assert_eq!(response.status(), StatusCode::OK, "{body}");
    let parties = json!({ "documentId": id, "parties": [{ "role": "signer", "name": "Anna", "delivery": "link" }] });
    client.call("documents.set_parties", parties, &auth).await;
    let (_, sent) = client.call("documents.send", json!({ "id": id }), &auth).await;
    let token = sent["links"][0]["url"].as_str().unwrap().rsplit('#').next().unwrap().to_string();

    let (response, created) = client.call("signing.create_review_link", json!({ "token": token }), &[]).await;
    assert_eq!(response.status(), StatusCode::OK, "{created}");
    let url = created["url"].as_str().unwrap();
    let path = &url[url.find("/review/").unwrap()..];

    let response = client
        .router
        .clone()
        .oneshot(Request::get(path).header("x-forwarded-for", "203.0.113.9").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let h = response.headers();
    assert_eq!(h.get(header::CONTENT_TYPE).unwrap(), "text/plain; charset=utf-8");
    assert_eq!(h.get("x-robots-tag").unwrap(), "noindex, nofollow, noarchive");
    assert_eq!(h.get(header::CACHE_CONTROL).unwrap(), "private, no-store");
    assert!(h.get(header::CONTENT_SECURITY_POLICY).unwrap().to_str().unwrap().starts_with("sandbox"));
    let text = String::from_utf8(response.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap();
    assert!(text.starts_with("# Agreement") && text.contains("DOCUMENT>>>"), "{text}");

    let pdf = Request::get(format!("{path}/document.pdf")).header("x-forwarded-for", "203.0.113.9").body(Body::empty()).unwrap();
    let (response, _) = client.send(pdf).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers().get(header::CONTENT_TYPE).unwrap(), "application/pdf");

    let bogus = Request::get("/review/0190f000-0000-7000-8000-000000000000.nope")
        .header("x-forwarded-for", "203.0.113.9")
        .body(Body::empty())
        .unwrap();
    let response = client.router.clone().oneshot(bogus).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(response.headers().get(header::CONTENT_TYPE).unwrap(), "text/plain; charset=utf-8");
}

fn base64_of(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

#[tokio::test]
async fn logos_are_checked_and_served_publicly() {
    use base64::Engine;
    let Some(app) = app().await else { return };
    let client = Client { router: router(AppState::new(app.clone())), ip: "203.0.113.12" };
    let setup = json!({ "setupCode": app.keys.setup_code(), "orgName": "Acme AB", "name": "Ada", "email": "ada@example.com", "password": PASSWORD });
    client.call("setup.create_first_admin", setup, &[]).await;
    let (response, _) = client.call("auth.login", json!({ "email": "ada@example.com", "password": PASSWORD }), &[]).await;
    let cookie = cookie_of(&response);
    let auth: [(&str, &str); 2] = [("cookie", &cookie), ("origin", ORIGIN)];

    let mut image = Vec::new();
    let mut encoder = png::Encoder::new(&mut image, 40, 20);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(&[200; 40 * 20 * 4]).unwrap();
    writer.finish().unwrap();
    let encode = |bytes: &[u8]| base64::engine::general_purpose::STANDARD.encode(bytes);

    let svg = encode(br#"<svg xmlns="http://www.w3.org/2000/svg"><script>alert(1)</script></svg>"#);
    let (response, _) = client.call("branding.set_logo", json!({ "contentBase64": svg }), &auth).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let (response, _) = client.call("branding.set_logo", json!({ "contentBase64": encode(&image) }), &[]).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let (response, branding) = client.call("branding.set_logo", json!({ "contentBase64": encode(&image) }), &auth).await;
    assert_eq!(response.status(), StatusCode::OK, "{branding}");
    let url = branding["logoUrl"].as_str().unwrap().to_string();
    let (_, public) = client.call("branding.get", json!({}), &[]).await;
    assert_eq!(public["logoUrl"], url.as_str());
    assert_eq!(public["organisation"], "Acme AB");

    let fetch = |path: &str| Request::get(path).header("x-forwarded-for", "203.0.113.12").body(Body::empty()).unwrap();
    let response = client.router.clone().oneshot(fetch(&url)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let h = response.headers();
    assert_eq!(h.get(header::CONTENT_TYPE).unwrap(), "image/png");
    assert!(h.get(header::CONTENT_SECURITY_POLICY).unwrap().to_str().unwrap().starts_with("sandbox"));
    assert_eq!(h.get("x-content-type-options").unwrap(), "nosniff");
    assert_eq!(h.get("cross-origin-resource-policy").unwrap(), "cross-origin");
    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert!(body.starts_with(b"\x89PNG\r\n\x1a\n"));

    // The logo can only be changed by uploading one; another file's id is ignored, and is never served as a logo.
    let (_, doc) = client.call("documents.create", json!({ "title": "Agreement" }), &auth).await;
    let pdf = encode(&opensign_pdf_fixture());
    let (_, upload) =
        client.call("documents.upload", json!({ "documentId": doc["id"], "name": "a.pdf", "contentBase64": pdf }), &auth).await;
    let file_id = upload["file"]["id"].as_str().unwrap().to_string();
    let (response, _) = client.call("settings.update", json!({ "group": "org", "values": { "logoFileId": file_id } }), &auth).await;
    assert_eq!(response.status(), StatusCode::OK);
    let (_, after) = client.call("branding.get", json!({}), &[]).await;
    assert_eq!(after["logoUrl"], url.as_str());
    assert_eq!(client.send(fetch(&format!("/api/logo/{}.png", file_id.replace('-', "")))).await.0.status(), StatusCode::NOT_FOUND);

    let (_, replaced) = client.call("branding.set_logo", json!({ "contentBase64": encode(&image) }), &auth).await;
    assert_ne!(replaced["logoUrl"], url.as_str());
    assert_eq!(client.send(fetch(&url)).await.0.status(), StatusCode::NOT_FOUND);
    assert_eq!(client.send(fetch("/api/logo/not-a-uuid.png")).await.0.status(), StatusCode::NOT_FOUND);

    let (_, removed) = client.call("branding.remove_logo", json!({}), &auth).await;
    assert!(removed["logoUrl"].is_null());
}

#[tokio::test]
async fn bodies_are_refused_before_they_are_read() {
    let Some(app) = app().await else { return };
    let client = Client { router: router(AppState::new(app.clone())), ip: "203.0.113.20" };
    let setup = json!({ "setupCode": app.keys.setup_code(), "orgName": "Acme AB", "name": "Ada", "email": "ada@example.com", "password": PASSWORD });
    client.call("setup.create_first_admin", setup, &[]).await;

    let big = vec![b'['; 3 * 1024 * 1024];
    let send_big = |name: &str| client.rpc(name).body(Body::from(big.clone())).unwrap();
    let (response, _) = client.send(send_big("documents.list")).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "no credentials: the size does not matter");
    let (response, _) = client.send(send_big("documents.upload")).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let (response, _) = client.send(send_big("setup.status")).await;
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE, "a public call takes two megabytes at most");

    let upload = Request::post(format!("/api/documents/{}/files", uuid::Uuid::nil()))
        .header(header::CONTENT_TYPE, "multipart/form-data; boundary=x")
        .header("x-forwarded-for", client.ip)
        .body(Body::from("--x--"))
        .unwrap();
    let (response, _) = client.send(upload).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn public_calls_refuse_other_sites() {
    let Some(app) = app().await else { return };
    let client = Client { router: router(AppState::new(app.clone())), ip: "203.0.113.21" };
    let setup = json!({ "setupCode": app.keys.setup_code(), "orgName": "Acme AB", "name": "Ada", "email": "ada@example.com", "password": PASSWORD });
    client.call("setup.create_first_admin", setup, &[]).await;
    let login = json!({ "email": "ada@example.com", "password": PASSWORD });

    let form = Request::post("/api/rpc/auth.login")
        .header(header::CONTENT_TYPE, "text/plain")
        .header("x-forwarded-for", client.ip)
        .body(Body::from(login.to_string()))
        .unwrap();
    let (response, _) = client.send(form).await;
    assert_eq!(response.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
    let untyped = Request::post("/api/rpc/auth.login").header("x-forwarded-for", client.ip).body(Body::from(login.to_string())).unwrap();
    let (response, _) = client.send(untyped).await;
    assert_eq!(response.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
    let (response, _) = client.call("auth.login", login.clone(), &[("sec-fetch-site", "cross-site")]).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let (response, _) = client.call("auth.login", login, &[("sec-fetch-site", "same-origin")]).await;
    assert_eq!(response.status(), StatusCode::OK);
    let empty = Request::post("/api/rpc/setup.status").header("x-forwarded-for", client.ip).body(Body::empty()).unwrap();
    let (response, _) = client.send(empty).await;
    assert_eq!(response.status(), StatusCode::OK, "no body needs no content type");
}

#[tokio::test]
async fn file_routes_ask_for_the_required_two_factor() {
    let Some(app) = app().await else { return };
    let client = Client { router: router(AppState::new(app.clone())), ip: "203.0.113.22" };
    let setup = json!({ "setupCode": app.keys.setup_code(), "orgName": "Acme AB", "name": "Ada", "email": "ada@example.com", "password": PASSWORD });
    client.call("setup.create_first_admin", setup, &[]).await;
    let (response, _) = client.call("auth.login", json!({ "email": "ada@example.com", "password": PASSWORD }), &[]).await;
    let cookie = cookie_of(&response);
    let auth: [(&str, &str); 2] = [("cookie", &cookie), ("origin", ORIGIN)];
    let (_, doc) = client.call("documents.create", json!({ "title": "Agreement" }), &auth).await;
    let id = doc["id"].as_str().unwrap().to_string();
    let (response, _) = client.call("settings.update", json!({ "group": "security", "values": { "enforceTwoFactor": true } }), &auth).await;
    assert_eq!(response.status(), StatusCode::OK);

    for path in
        [format!("/api/documents/{id}/file"), format!("/api/documents/{id}/preview"), format!("/api/documents/{id}/attachments/{id}")]
    {
        let get = Request::get(&path).header("cookie", &cookie).header("x-forwarded-for", client.ip).body(Body::empty()).unwrap();
        let (response, body) = client.send(get).await;
        assert_eq!(response.status(), StatusCode::FORBIDDEN, "{path}: {body}");
    }
    let upload = Request::post(format!("/api/documents/{id}/files"))
        .header("cookie", &cookie)
        .header("origin", ORIGIN)
        .header(header::CONTENT_TYPE, "multipart/form-data; boundary=x")
        .header("x-forwarded-for", client.ip)
        .body(Body::from("--x--"))
        .unwrap();
    let (response, _) = client.send(upload).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[test]
fn every_large_body_names_a_real_service() {
    let names: Vec<&str> = opensign_core::services::registry().into_iter().map(|s| s.meta().name).collect();
    for name in ["documents.upload", "documents.replace_file", "documents.verify", "bulk.preview", "bulk.start", "branding.set_logo"] {
        assert!(names.contains(&name), "{name}");
        assert!(opensign_server::rpc::body_limit(name) > opensign_server::rpc::body_limit("documents.list"), "{name}");
    }
}
