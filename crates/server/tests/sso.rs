#![allow(clippy::unwrap_used)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::Json;
use axum::body::Body;
use axum::extract::State;
use axum::http::{Request, Response, StatusCode, header};
use axum::routing::{get, post};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64URL;
use http_body_util::BodyExt;
use opensign_core::settings::SsoSettings;
use opensign_core::testing::app;
use opensign_server::{AppState, router};
use rsa::pkcs1v15::SigningKey;
use rsa::signature::{SignatureEncoding, Signer};
use rsa::traits::PublicKeyParts;
use serde_json::{Value, json};
use sha2::Sha256;
use tower::ServiceExt;

type Nonce = Arc<Mutex<String>>;

async fn provider(nonce: Nonce) -> String {
    let private = rsa::RsaPrivateKey::new(&mut rand::thread_rng(), 1024).unwrap();
    let public = private.to_public_key();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let issuer = format!("http://{}", listener.local_addr().unwrap());
    let discovery = json!({
        "issuer": issuer,
        "authorization_endpoint": format!("{issuer}/authorize"),
        "token_endpoint": format!("{issuer}/token"),
        "jwks_uri": format!("{issuer}/jwks"),
    });
    let jwks = json!({ "keys": [{ "kty": "RSA", "kid": "k1", "n": B64URL.encode(public.n().to_bytes_be()), "e": B64URL.encode(public.e().to_bytes_be()) }] });
    let iss = issuer.clone();
    let token = move |State(nonce): State<Nonce>, _: axum::Form<HashMap<String, String>>| {
        let (private, iss) = (private.clone(), iss.clone());
        async move {
            let now = chrono::Utc::now().timestamp();
            let claims = json!({
                "iss": iss, "aud": "opensign", "exp": now + 300, "iat": now, "nonce": *nonce.lock().unwrap(),
                "sub": "subject-1", "email": "anna@example.com", "email_verified": true, "name": "Anna", "amr": ["pwd", "otp"],
            });
            let signed =
                format!("{}.{}", B64URL.encode(json!({ "alg": "RS256", "kid": "k1" }).to_string()), B64URL.encode(claims.to_string()));
            let id_token = format!("{signed}.{}", B64URL.encode(SigningKey::<Sha256>::new(private).sign(signed.as_bytes()).to_bytes()));
            Json(json!({ "id_token": id_token }))
        }
    };
    let app = axum::Router::new()
        .route("/.well-known/openid-configuration", get(move || async move { Json(discovery) }))
        .route("/jwks", get(move || async move { Json(jwks) }))
        .route("/token", post(token))
        .with_state(nonce);
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    issuer
}

async fn send(app: &axum::Router, request: Request<Body>) -> (Response<Body>, Value) {
    let response = app.clone().oneshot(request).await.unwrap();
    let (parts, body) = response.into_parts();
    let bytes = body.collect().await.unwrap().to_bytes();
    (Response::from_parts(parts, Body::empty()), serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

fn cookies(response: &Response<Body>) -> Vec<String> {
    response.headers().get_all(header::SET_COOKIE).iter().map(|c| c.to_str().unwrap().to_string()).collect()
}

fn rpc(name: &str) -> axum::http::request::Builder {
    Request::post(format!("/api/rpc/{name}")).header(header::CONTENT_TYPE, "application/json")
}

fn location(response: &Response<Body>) -> &str {
    response.headers().get(header::LOCATION).unwrap().to_str().unwrap()
}

#[tokio::test]
async fn sign_in_over_http() {
    let Some(core) = app().await else { return };
    let nonce = Nonce::default();
    let issuer = provider(nonce.clone()).await;
    sqlx::query("insert into users (id, email, name, role) values ($1, 'admin@example.com', 'Admin', 'admin')")
        .bind(uuid::Uuid::now_v7())
        .execute(&core.db)
        .await
        .unwrap();
    let sso = SsoSettings {
        enabled: true,
        issuer,
        client_id: "opensign".into(),
        client_secret: Some("s3cret".into()),
        default_role: "sender".into(),
        ..SsoSettings::default()
    };
    core.settings.save(&mut core.db.acquire().await.unwrap(), &core.keys, &sso, None).await.unwrap();
    let app = router(AppState::new(core.clone()));

    let (response, status) = send(&app, rpc("auth.sso_status").body(Body::from("{}")).unwrap()).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(status["enabled"], true);
    assert!(status.get("issuer").is_none() && status.get("clientId").is_none());

    let cross_site = rpc("auth.sso_start").header("sec-fetch-site", "cross-site").body(Body::from("{}")).unwrap();
    assert_eq!(send(&app, cross_site).await.0.status(), StatusCode::FORBIDDEN, "no other site can start a sign-in");

    let (response, started) = send(&app, rpc("auth.sso_start").body(Body::from("{}")).unwrap()).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert!(started.get("browserBinding").is_none(), "the binding only travels in the cookie");
    let set = &cookies(&response)[0];
    assert!(
        set.starts_with("opensign_sso=") && set.contains("HttpOnly") && set.contains("SameSite=Lax") && set.contains("Max-Age=600"),
        "{set}"
    );
    let cookie = set.split(';').next().unwrap().to_string();
    let url = url::Url::parse(started["redirectUrl"].as_str().unwrap()).unwrap();
    let query: HashMap<String, String> = url.query_pairs().into_owned().collect();
    *nonce.lock().unwrap() = query["nonce"].clone();

    let back = |state: &str, cookie: Option<&str>| {
        let mut request = Request::get(format!("/api/auth/sso/callback?state={state}&code=code-1"));
        if let Some(cookie) = cookie {
            request = request.header(header::COOKIE, cookie);
        }
        request.body(Body::empty()).unwrap()
    };

    let (response, _) = send(&app, back("made-up", Some(&cookie))).await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(location(&response), "/login?sso=expired");
    assert!(!cookies(&response).iter().any(|c| c.starts_with("opensign_session=")));

    // A link sent by someone who started the sign-in themselves does not sign the visitor in.
    let (response, _) = send(&app, back(&query["state"], None)).await;
    assert_eq!(location(&response), "/login?sso=other_browser");
    assert!(!cookies(&response).iter().any(|c| c.starts_with("opensign_session=")));

    let (_, started) = send(&app, rpc("auth.sso_start").body(Body::from("{}")).unwrap()).await;
    let url = url::Url::parse(started["redirectUrl"].as_str().unwrap()).unwrap();
    let query: HashMap<String, String> = url.query_pairs().into_owned().collect();
    *nonce.lock().unwrap() = query["nonce"].clone();
    let (start, _) = send(&app, rpc("auth.sso_start").body(Body::from("{}")).unwrap()).await;
    let other_cookie = cookies(&start)[0].split(';').next().unwrap().to_string();
    let (response, _) = send(&app, back(&query["state"], Some(&other_cookie))).await;
    assert_eq!(location(&response), "/login?sso=other_browser", "another browser's cookie");

    let (start, started) = send(&app, rpc("auth.sso_start").body(Body::from("{}")).unwrap()).await;
    let cookie = cookies(&start)[0].split(';').next().unwrap().to_string();
    let url = url::Url::parse(started["redirectUrl"].as_str().unwrap()).unwrap();
    let query: HashMap<String, String> = url.query_pairs().into_owned().collect();
    *nonce.lock().unwrap() = query["nonce"].clone();
    let (response, _) = send(&app, back(&query["state"], Some(&cookie))).await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(location(&response), "/");
    assert_eq!(response.headers().get(header::CACHE_CONTROL).unwrap(), "no-store");
    let set = cookies(&response);
    let session = set.iter().find(|c| c.starts_with("opensign_session=")).unwrap();
    assert!(session.contains("HttpOnly") && session.contains("SameSite=Lax"));
    assert!(set.iter().any(|c| c.starts_with("opensign_sso=;") && c.contains("Max-Age=0")), "the binding is cleared");

    let session = session.split(';').next().unwrap();
    let me =
        rpc("account.me").header(header::COOKIE, session).header(header::ORIGIN, "http://localhost:3000").body(Body::from("{}")).unwrap();
    let (response, me) = send(&app, me).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(me["email"], "anna@example.com");
    assert_eq!(me["sso"], true);
    assert_eq!(me["role"], "sender");

    // A session from single sign-on cannot call for a credential, even with a confirmation it does not have.
    let key = rpc("api_keys.create")
        .header(header::COOKIE, session)
        .header(header::ORIGIN, "http://localhost:3000")
        .body(Body::from(json!({ "name": "k", "permissions": ["documents.read"] }).to_string()))
        .unwrap();
    let (response, body) = send(&app, key).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(body["code"], "sso_account");
}
