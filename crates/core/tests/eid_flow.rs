#![allow(clippy::unwrap_used)]

use std::sync::{Arc, Mutex};

use axum::Json;
use axum::extract::State;
use axum::routing::post;
use base64::Engine;
use base64::engine::general_purpose::{STANDARD as B64, URL_SAFE_NO_PAD as B64URL};
use opensign_core::context::{Actor, App, Credential, Ctx, Surface};
use opensign_core::documents::{AuthMethod, Delivery, PartyRole, parties, signer_token};
use opensign_core::eid::bankid::TEST_BASE;
use opensign_core::eid::freja::{Freja, Outcome};
use opensign_core::eid::jws::Key;
use opensign_core::eid::tic::TEST_ISSUER;
use opensign_core::services::documents::{self as docs, PartyInput};
use opensign_core::services::eid::{self, Purpose, StartInput, StatusInput};
use opensign_core::services::signing::{self, TokenInput};
use opensign_core::settings::EidSettings;
use opensign_core::testing::app;
use opensign_core::{Role, call};
use rsa::pkcs1v15::SigningKey;
use rsa::signature::{SignatureEncoding, Signer};
use rsa::traits::PublicKeyParts;
use serde_json::{Value, json};
use sha2::Sha256;
use uuid::Uuid;

#[derive(Default)]
struct BankIdStandIn {
    requirement: Option<Value>,
    visible: Option<String>,
    collects: usize,
    person: String,
}

async fn stand_in(state: Arc<Mutex<BankIdStandIn>>) -> String {
    async fn start(State(s): State<Arc<Mutex<BankIdStandIn>>>, Json(body): Json<Value>) -> Json<Value> {
        let mut s = s.lock().unwrap();
        s.requirement = body.get("requirement").cloned();
        s.visible = body["userVisibleData"].as_str().map(|v| String::from_utf8(B64.decode(v).unwrap()).unwrap());
        s.collects = 0;
        Json(json!({ "orderRef": "order-1", "autoStartToken": "auto-1", "qrStartToken": "qr-token", "qrStartSecret": "qr-secret" }))
    }
    async fn collect(State(s): State<Arc<Mutex<BankIdStandIn>>>) -> Json<Value> {
        let mut s = s.lock().unwrap();
        s.collects += 1;
        if s.collects == 1 {
            return Json(json!({ "orderRef": "order-1", "status": "pending", "hintCode": "outstandingTransaction" }));
        }
        Json(json!({ "orderRef": "order-1", "status": "complete", "completionData": {
            "user": { "personalNumber": s.person, "name": "Anna Andersson", "givenName": "Anna", "surname": "Andersson" },
            "device": { "ipAddress": "198.51.100.7" }, "signature": "PFhNTD4=", "ocspResponse": "MIIB",
        }}))
    }
    let router = axum::Router::new()
        .route("/auth", post(start))
        .route("/sign", post(start))
        .route("/collect", post(collect))
        .route("/cancel", post(|| async { Json(json!({})) }))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    format!("http://{address}")
}

fn sample_pdf() -> Vec<u8> {
    use lopdf::{Document, Object, dictionary};
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let page = doc
        .add_object(dictionary! { "Type" => "Page", "Parent" => pages_id, "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()] });
    doc.objects.insert(pages_id, Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => vec![page.into()], "Count" => 1 }));
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog);
    let mut out = Vec::new();
    doc.save_to(&mut out).unwrap();
    out
}

async fn sender(app: &Arc<App>) -> Ctx {
    let id = Uuid::now_v7();
    let email = format!("eid-{id}@example.com");
    sqlx::query("insert into users (id, email, name, role, password_hash) values ($1, $2, 'Sara', 'sender', 'x')")
        .bind(id)
        .bind(&email)
        .execute(&app.db)
        .await
        .unwrap();
    let actor = Actor {
        user_id: id,
        email,
        name: "Sara".into(),
        role: Role::Sender,
        permissions: Role::Sender.permissions(),
        must_enable_two_factor: false,
        credential: Credential::Session,
    };
    Ctx::new(app.clone(), Surface::Api).with_actor(actor)
}

fn signer_ctx(app: &Arc<App>) -> Ctx {
    let mut ctx = Ctx::new(app.clone(), Surface::Signer);
    ctx.ip = Some("198.51.100.7".into());
    ctx
}

/// A sent document whose one signer must identify with BankID to view and to sign.
async fn sent_document(app: &Arc<App>, sender: &Ctx, personal_number: &str) -> String {
    let document = call::<docs::Create>(sender, serde_json::from_value(json!({ "title": "Lease" })).unwrap()).await.unwrap();
    let id = document.document.id;
    call::<docs::Upload>(sender, docs::UploadInput { document_id: id, name: "lease.pdf".into(), content_base64: B64.encode(sample_pdf()) })
        .await
        .unwrap();
    let anna = PartyInput {
        id: None,
        role: PartyRole::Signer,
        name: "Anna Andersson".into(),
        email: Some("anna@example.net".into()),
        phone: None,
        personal_number: Some(personal_number.into()),
        company: None,
        sign_order: None,
        delivery: Delivery::Link,
        auth_to_view: AuthMethod::SeBankid,
        auth_to_sign: AuthMethod::SeBankid,
        attachments: None,
        language: None,
        webcam_photo: None,
    };
    call::<docs::SetParties>(sender, docs::SetPartiesInput { document_id: id, parties: vec![anna] }).await.unwrap();
    call::<docs::Send>(sender, docs::SendInput { id, expires_at: None }).await.unwrap();
    let mut conn = app.db.acquire().await.unwrap();
    signer_token(&app.keys, &parties(&mut conn, id).await.unwrap()[0])
}

/// Both eID tests change the shared eID settings.
static EID_SETTINGS: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[tokio::test]
async fn bankid_identifies_the_right_person_only() {
    let Some(app) = app().await else { return };
    let _settings = EID_SETTINGS.lock().await;
    let standin = Arc::new(Mutex::new(BankIdStandIn { person: "199001010017".into(), ..Default::default() }));
    *TEST_BASE.lock().unwrap() = Some(stand_in(standin.clone()).await);
    {
        let mut conn = app.db.acquire().await.unwrap();
        let eid = EidSettings { bankid_enabled: true, ..EidSettings::default() };
        app.settings.save(&mut conn, &app.keys, &eid, None).await.unwrap();
    }
    let sender = sender(&app).await;
    let signer = signer_ctx(&app);

    let token = sent_document(&app, &sender, "900101-0017").await;
    let view = call::<signing::Open>(&signer, TokenInput { token: token.clone(), session: None }).await.unwrap();
    assert_eq!(view.needs_authentication, Some(AuthMethod::SeBankid));
    assert!(view.fields.is_empty() && view.document.page_count.is_none(), "nothing is shown before identification");

    let started = call::<eid::Start>(&signer, StartInput { token: token.clone(), session: None, purpose: Purpose::View }).await.unwrap();
    assert_eq!(started.auto_start_token.as_deref(), Some("auto-1"));
    assert!(started.qr.as_deref().unwrap().starts_with("bankid.qr-token.0."));
    assert_eq!(standin.lock().unwrap().requirement, Some(json!({ "personalNumber": "199001010017" })), "only Anna's BankID can answer");

    let status = |id| call::<eid::Status>(&signer, StatusInput { token: token.clone(), id });
    let pending = status(started.id).await.unwrap();
    assert_eq!(pending.status, "pending");
    assert_eq!(pending.message.as_deref(), Some("Start your BankID app."));
    assert!(pending.session.is_none());
    let done = status(started.id).await.unwrap();
    assert_eq!(done.status, "complete");
    let session = done.session.unwrap();
    let stored: String =
        sqlx::query_scalar("select identity::text from eid_sessions where id = $1").bind(started.id).fetch_one(&app.db).await.unwrap();
    assert!(stored.contains("Anna Andersson") && !stored.contains("199001010017"), "the personal number is stored encrypted");
    let again = status(started.id).await.unwrap();
    assert!(again.session.is_none() && again.status == "failed", "a session is handed out once");

    let other =
        call::<eid::Status>(&signer, StatusInput { token: sent_document(&app, &sender, "900101-0017").await, id: started.id }).await;
    assert!(other.is_err(), "another party's link cannot pick up this identification");

    let view = call::<signing::Open>(&signer, TokenInput { token: token.clone(), session: Some(session.clone()) }).await.unwrap();
    assert_eq!(view.needs_authentication, None);
    assert_eq!(view.sign_authentication, None, "the same eID covers signing");

    let signing =
        call::<eid::Start>(&signer, StartInput { token: token.clone(), session: Some(session), purpose: Purpose::Sign }).await.unwrap();
    let text = standin.lock().unwrap().visible.clone().unwrap();
    assert!(text.contains("Lease"), "BankID shows what is being signed: {text}");
    call::<eid::Cancel>(&signer, StatusInput { token: token.clone(), id: signing.id }).await.unwrap();
    assert_eq!(status(signing.id).await.unwrap().status, "failed");

    // Someone else's BankID is refused even if BankID itself accepted them.
    standin.lock().unwrap().person = "199001010025".into();
    let token = sent_document(&app, &sender, "900101-0017").await;
    let started = call::<eid::Start>(&signer, StartInput { token: token.clone(), session: None, purpose: Purpose::View }).await.unwrap();
    let status = |id| call::<eid::Status>(&signer, StatusInput { token: token.clone(), id });
    status(started.id).await.unwrap();
    let refused = status(started.id).await.unwrap();
    assert_eq!(refused.status, "failed");
    assert!(refused.session.is_none());
    let mismatches: i64 =
        sqlx::query_scalar("select count(*) from audit_log where action = 'party.eid_mismatch'").fetch_one(&app.db).await.unwrap();
    assert!(mismatches >= 1);
    *TEST_BASE.lock().unwrap() = None;
}

#[derive(Default)]
struct TicStandIn {
    challenge: String,
    nonce: String,
    nin: String,
    secret: Option<String>,
    exchanges: usize,
}

async fn tic_stand_in(private: rsa::RsaPrivateKey, state: Arc<Mutex<TicStandIn>>) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/acme", listener.local_addr().unwrap());
    let discovery = json!({
        "issuer": base,
        "authorization_endpoint": format!("{base}/oidc/authorize"),
        "token_endpoint": format!("{base}/oidc/token"),
        "jwks_uri": format!("{base}/oidc/jwks"),
        "token_endpoint_auth_methods_supported": ["client_secret_post"],
    });
    let public = private.to_public_key();
    let jwks = json!({ "keys": [{ "kty": "RSA", "kid": "k1", "n": B64URL.encode(public.n().to_bytes_be()), "e": B64URL.encode(public.e().to_bytes_be()) }] });
    let issuer = base.clone();
    let token = move |State(s): State<Arc<Mutex<TicStandIn>>>, axum::Form(form): axum::Form<std::collections::HashMap<String, String>>| {
        let (private, issuer) = (private.clone(), issuer.clone());
        async move {
            let mut s = s.lock().unwrap();
            s.exchanges += 1;
            s.secret = form.get("client_secret").cloned();
            let verifier = form.get("code_verifier").cloned().unwrap_or_default();
            if B64URL.encode(<Sha256 as sha2::Digest>::digest(verifier.as_bytes())) != s.challenge || form["code"] != "code-1" {
                return (axum::http::StatusCode::BAD_REQUEST, Json(json!({ "error": "invalid_grant" })));
            }
            let now = chrono::Utc::now().timestamp();
            let claims = json!({
                "iss": issuer, "aud": "opensign", "exp": now + 300, "iat": now, "nonce": s.nonce, "sub": "hashed",
                "name": "Anna Andersson", "nin": s.nin, "nin_type": "personal_number", "nin_issuing_country": "SE",
            });
            let signed =
                format!("{}.{}", B64URL.encode(json!({ "alg": "RS256", "kid": "k1" }).to_string()), B64URL.encode(claims.to_string()));
            let id_token = format!("{signed}.{}", B64URL.encode(SigningKey::<Sha256>::new(private).sign(signed.as_bytes()).to_bytes()));
            (axum::http::StatusCode::OK, Json(json!({ "token_type": "Bearer", "id_token": id_token, "access_token": "a" })))
        }
    };
    let router = axum::Router::new()
        .route("/acme/.well-known/openid-configuration", axum::routing::get(move || async move { Json(discovery) }))
        .route("/acme/oidc/jwks", axum::routing::get(move || async move { Json(jwks) }))
        .route("/acme/oidc/token", post(token))
        .with_state(state);
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    base
}

#[tokio::test]
async fn bankid_through_tic_identifies_the_right_person_only() {
    let Some(app) = app().await else { return };
    let _settings = EID_SETTINGS.lock().await;
    let standin = Arc::new(Mutex::new(TicStandIn { nin: "199001010017".into(), ..Default::default() }));
    let private = rsa::RsaPrivateKey::new(&mut rand::thread_rng(), 1024).unwrap();
    *TEST_ISSUER.lock().unwrap() = Some(tic_stand_in(private, standin.clone()).await);
    {
        let mut conn = app.db.acquire().await.unwrap();
        let eid = EidSettings {
            tic_enabled: true,
            tic_tenant: "acme".into(),
            tic_client_id: "opensign".into(),
            tic_client_secret: Some("s3cret".into()),
            ..EidSettings::default()
        };
        app.settings.save(&mut conn, &app.keys, &eid, None).await.unwrap();
    }
    let sender = sender(&app).await;
    let signer = signer_ctx(&app);

    let identify = |token: String| {
        let (signer, standin, app) = (signer.clone(), standin.clone(), app.clone());
        async move {
            let started =
                call::<eid::Start>(&signer, StartInput { token: token.clone(), session: None, purpose: Purpose::View }).await.unwrap();
            assert_eq!(started.method, AuthMethod::SeBankid);
            let url = reqwest::Url::parse(started.redirect_url.as_deref().unwrap()).unwrap();
            assert!(url.path().ends_with("/acme/oidc/authorize"));
            let query: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
            assert_eq!(query["scope"], "openid profile nin");
            assert_eq!(query["code_challenge_method"], "S256");
            {
                let mut s = standin.lock().unwrap();
                s.challenge = query["code_challenge"].clone();
                s.nonce = query["nonce"].clone();
            }
            let mut conn = app.db.acquire().await.unwrap();
            let id = eid::broker_callback(&signer, &mut conn, &query["state"], Some("code-1"), None).await.unwrap();
            assert_eq!(id, started.id);
            let replay = eid::broker_callback(&signer, &mut conn, &query["state"], Some("code-1"), None).await;
            assert!(replay.is_err(), "a state is used once");
            call::<eid::Status>(&signer, StatusInput { token, id }).await.unwrap()
        }
    };

    let token = sent_document(&app, &sender, "900101-0017").await;
    let done = identify(token.clone()).await;
    assert_eq!(done.status, "complete");
    assert_eq!(standin.lock().unwrap().secret.as_deref(), Some("s3cret"), "TIC takes the secret in the form");
    let view = call::<signing::Open>(&signer, TokenInput { token, session: done.session }).await.unwrap();
    assert_eq!(view.needs_authentication, None);

    standin.lock().unwrap().nin = "199001010025".into();
    let refused = identify(sent_document(&app, &sender, "900101-0017").await).await;
    assert_eq!(refused.status, "failed");
    assert!(refused.session.is_none());
    assert!(refused.message.unwrap().contains("someone other than"));
    *TEST_ISSUER.lock().unwrap() = None;
}

#[tokio::test]
async fn freja_results_count_only_when_freja_signed_them() {
    let private = rsa::RsaPrivateKey::new(&mut rand::thread_rng(), 1024).unwrap();
    let public = private.to_public_key();
    let keys = Key::from_jwks(
        &json!({ "keys": [{ "kty": "RSA", "n": B64URL.encode(public.n().to_bytes_be()), "e": B64URL.encode(public.e().to_bytes_be()) }] }),
    );
    let sign = |payload: &Value| {
        let signed = format!("{}.{}", B64URL.encode(json!({ "alg": "RS256" }).to_string()), B64URL.encode(payload.to_string()));
        format!("{signed}.{}", B64URL.encode(SigningKey::<Sha256>::new(private.clone()).sign(signed.as_bytes()).to_bytes()))
    };
    let attributes =
        json!({ "basicUserInfo": { "name": "Anna", "surname": "Andersson" }, "ssn": { "ssn": "199001010017", "country": "SE" } });
    let genuine = sign(&json!({ "authRef": "ref-1", "status": "APPROVED", "requestedAttributes": attributes }));
    let other_request = sign(&json!({ "authRef": "ref-2", "status": "APPROVED", "requestedAttributes": attributes }));
    let responses = Arc::new(Mutex::new(vec![
        json!({ "authRef": "ref-1", "status": "APPROVED", "details": other_request }),
        json!({ "authRef": "ref-1", "status": "APPROVED", "details": "e30.e30.c2ln", "requestedAttributes": attributes }),
        json!({ "authRef": "ref-1", "status": "APPROVED", "details": genuine }),
        json!({ "authRef": "ref-1", "status": "DELIVERED_TO_MOBILE" }),
    ]));
    let router = axum::Router::new()
        .route(
            "/authentication/1.0/getOneResult",
            post(|State(r): State<Arc<Mutex<Vec<Value>>>>| async move { Json(r.lock().unwrap().pop().unwrap()) }),
        )
        .with_state(responses);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let freja = Freja::with_client(reqwest::Client::new(), &base, keys[0].clone());

    assert!(matches!(freja.result("ref-1").await.unwrap(), Outcome::Pending));
    match freja.result("ref-1").await.unwrap() {
        Outcome::Complete { identity, .. } => {
            assert_eq!(identity.name, "Anna Andersson");
            assert_eq!(identity.personal_number.as_deref(), Some("199001010017"));
        }
        _ => panic!("expected a completed identification"),
    }
    assert!(freja.result("ref-1").await.is_err(), "a forged signature is refused");
    assert!(freja.result("ref-1").await.is_err(), "a signed result for another request is refused");
}

#[test]
fn certificate_files_from_banks_work_as_they_are() {
    for name in ["rp-modern.p12", "rp-legacy.p12"] {
        let der = std::fs::read(format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap();
        let pem = opensign_core::eid::pkcs12_to_pem(&der, "qwerty123").unwrap();
        assert!(pem.starts_with("-----BEGIN PRIVATE KEY-----") && pem.contains("-----BEGIN CERTIFICATE-----"), "{name}");
        opensign_core::net::client_with_certificate(&pem, None).unwrap();
        assert!(opensign_core::eid::pkcs12_to_pem(&der, "wrong").is_err(), "{name} needs its password");
    }
}

#[tokio::test]
#[ignore = "calls BankID's public test service"]
async fn bankid_test_environment_works_without_setup() {
    let eid = EidSettings { bankid_enabled: true, bankid_test: true, ..EidSettings::default() };
    let bankid = opensign_core::eid::bankid::BankId::from_settings(&eid).unwrap();
    let order = bankid.start("194.168.2.25", None, None).await.unwrap();
    assert!(!order.order_ref.is_empty());
    bankid.cancel(&order.order_ref).await.unwrap();
}
