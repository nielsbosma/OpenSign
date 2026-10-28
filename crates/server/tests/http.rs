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
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS, "but the guessed account is locked for a while");

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
