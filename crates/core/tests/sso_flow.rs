#![allow(clippy::unwrap_used)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::Json;
use axum::extract::State;
use axum::routing::{get, post};
use base64::Engine;
use base64::engine::general_purpose::{STANDARD as B64, URL_SAFE_NO_PAD as B64URL};
use opensign_core::auth::{create_session, load_api_key, load_session, user_by_email};
use opensign_core::context::{Actor, App, Credential, Ctx, Surface};
use opensign_core::documents::{AuthMethod, Delivery, PartyRole, parties, signer_token};
use opensign_core::services::documents::{self as docs, PartyInput};
use opensign_core::services::{Empty, admin, auth as auth_svc, oauth, passkeys, signing, sso, users};
use opensign_core::settings::{SecuritySettings, SsoSettings};
use opensign_core::testing::{app, production_app};
use opensign_core::{Role, call};
use rsa::pkcs1v15::SigningKey;
use rsa::signature::{SignatureEncoding, Signer};
use rsa::traits::PublicKeyParts;
use serde_json::{Value, json};
use sha2::Sha256;
use uuid::Uuid;

const PASSWORD: &str = "correct horse battery staple 42";

#[derive(Default)]
struct Pending {
    challenge: String,
    nonce: String,
    code_used: bool,
}

/// A stand-in for an identity provider. Each test says what the next token claims.
struct Idp {
    issuer: String,
    claims: Mutex<Value>,
    pending: Mutex<Pending>,
    signing_key: Mutex<rsa::RsaPrivateKey>,
    jwks_fetches: Mutex<usize>,
}

fn jwks_of(private: &rsa::RsaPrivateKey) -> Value {
    let public = private.to_public_key();
    json!({ "keys": [{ "kty": "RSA", "kid": "k1", "use": "sig", "n": B64URL.encode(public.n().to_bytes_be()), "e": B64URL.encode(public.e().to_bytes_be()) }] })
}

fn new_key() -> rsa::RsaPrivateKey {
    rsa::RsaPrivateKey::new(&mut rand::thread_rng(), 1024).unwrap()
}

async fn idp() -> Arc<Idp> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let issuer = format!("http://{}", listener.local_addr().unwrap());
    let state = Arc::new(Idp {
        issuer: issuer.clone(),
        claims: Mutex::new(good_claims("subject-1", "anna@example.com")),
        pending: Mutex::default(),
        signing_key: Mutex::new(new_key()),
        jwks_fetches: Mutex::new(0),
    });
    let discovery = json!({
        "issuer": issuer,
        "authorization_endpoint": format!("{issuer}/authorize"),
        "token_endpoint": format!("{issuer}/token"),
        "jwks_uri": format!("{issuer}/jwks"),
    });
    async fn jwks(State(idp): State<Arc<Idp>>) -> Json<Value> {
        *idp.jwks_fetches.lock().unwrap() += 1;
        Json(jwks_of(&idp.signing_key.lock().unwrap()))
    }
    async fn token(
        State(idp): State<Arc<Idp>>,
        axum::Form(form): axum::Form<HashMap<String, String>>,
    ) -> (axum::http::StatusCode, Json<Value>) {
        let mut pending = idp.pending.lock().unwrap();
        let verifier = form.get("code_verifier").cloned().unwrap_or_default();
        let right = B64URL.encode(<Sha256 as sha2::Digest>::digest(verifier.as_bytes())) == pending.challenge;
        if !right || form["code"] != "code-1" || pending.code_used {
            return (axum::http::StatusCode::BAD_REQUEST, Json(json!({ "error": "invalid_grant" })));
        }
        pending.code_used = true;
        let now = chrono::Utc::now().timestamp();
        let mut claims = json!({ "iss": idp.issuer, "aud": "opensign", "exp": now + 300, "iat": now, "nonce": pending.nonce });
        for (k, v) in idp.claims.lock().unwrap().as_object().unwrap() {
            claims[k] = v.clone();
        }
        let signed = format!("{}.{}", B64URL.encode(json!({ "alg": "RS256", "kid": "k1" }).to_string()), B64URL.encode(claims.to_string()));
        let key = idp.signing_key.lock().unwrap().clone();
        let id_token = format!("{signed}.{}", B64URL.encode(SigningKey::<Sha256>::new(key).sign(signed.as_bytes()).to_bytes()));
        (axum::http::StatusCode::OK, Json(json!({ "token_type": "Bearer", "id_token": id_token })))
    }
    let router = axum::Router::new()
        .route("/.well-known/openid-configuration", get(move || async move { Json(discovery) }))
        .route("/jwks", get(jwks))
        .route("/token", post(token))
        .with_state(state.clone());
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    state
}

fn good_claims(sub: &str, email: &str) -> Value {
    json!({ "sub": sub, "email": email, "email_verified": true, "name": "Anna Andersson", "amr": ["pwd", "otp"] })
}

async fn configure(app: &Arc<App>, idp: &Idp, change: impl FnOnce(&mut SsoSettings)) {
    let mut sso = SsoSettings {
        enabled: true,
        issuer: idp.issuer.clone(),
        client_id: "opensign".into(),
        client_secret: Some("s3cret".into()),
        default_role: "sender".into(),
        ..SsoSettings::default()
    };
    change(&mut sso);
    let mut conn = app.db.acquire().await.unwrap();
    app.settings.save(&mut conn, &app.keys, &sso, None).await.unwrap();
}

fn anonymous(app: &Arc<App>) -> Ctx {
    let mut ctx = Ctx::new(app.clone(), Surface::Web);
    ctx.ip = Some("198.51.100.20".into());
    ctx
}

async fn add_user(app: &Arc<App>, email: &str, role: &str) -> Uuid {
    let id = Uuid::now_v7();
    let hash = opensign_core::auth::new_password_hash(PASSWORD).await.unwrap();
    sqlx::query("insert into users (id, email, name, role, password_hash) values ($1, $2, 'Someone', $3, $4)")
        .bind(id)
        .bind(email)
        .bind(role)
        .bind(hash)
        .execute(&app.db)
        .await
        .unwrap();
    id
}

/// Setup is done: the install has its first admin.
async fn first_admin(app: &Arc<App>) -> Ctx {
    let email = "admin@example.com";
    let id = add_user(app, email, "admin").await;
    let actor = Actor {
        user_id: id,
        email: email.into(),
        name: "Admin".into(),
        role: Role::Admin,
        permissions: Role::Admin.permissions(),
        must_enable_two_factor: false,
        credential: Credential::Session,
    };
    anonymous(app).with_actor(actor)
}

struct Started {
    state: String,
    browser: String,
    query: HashMap<String, String>,
}

async fn start(app: &Arc<App>, idp: &Idp) -> Started {
    let started = call::<sso::Start>(&anonymous(app), Empty::default()).await.unwrap();
    let url = reqwest::Url::parse(&started.redirect_url).unwrap();
    let query: HashMap<String, String> = url.query_pairs().into_owned().collect();
    *idp.pending.lock().unwrap() = Pending { challenge: query["code_challenge"].clone(), nonce: query["nonce"].clone(), code_used: false };
    Started { state: query["state"].clone(), browser: started.browser_binding.unwrap(), query }
}

async fn sign_in(app: &Arc<App>, idp: &Idp) -> Result<sso::SsoSession, &'static str> {
    let s = start(app, idp).await;
    sso::callback(&anonymous(app), &s.state, Some(&s.browser), Some("code-1"), None).await
}

async fn session_ctx(app: &Arc<App>, session: &sso::SsoSession) -> Ctx {
    let mut conn = app.db.acquire().await.unwrap();
    let security: SecuritySettings = app.settings.get(&mut conn, &app.keys).await.unwrap();
    let (actor, id) = load_session(&mut conn, &session.token, &security, chrono::Utc::now()).await.unwrap().unwrap();
    let mut ctx = anonymous(app).with_actor(actor);
    ctx.session_id = Some(id);
    ctx
}

async fn user(app: &Arc<App>, email: &str) -> opensign_core::auth::UserRow {
    let mut conn = app.db.acquire().await.unwrap();
    user_by_email(&mut conn, email).await.unwrap().unwrap()
}

async fn refusals(app: &Arc<App>, reason: &str) -> i64 {
    sqlx::query_scalar("select count(*) from audit_log where action = 'auth.sso_refused' and metadata->>'reason' = $1")
        .bind(reason)
        .fetch_one(&app.db)
        .await
        .unwrap()
}

#[tokio::test]
async fn the_flow_asks_for_what_it_needs_and_creates_a_plain_account() {
    let Some(app) = app().await else { return };
    let idp = idp().await;
    first_admin(&app).await;
    let status = call::<sso::Status>(&anonymous(&app), Empty::default()).await.unwrap();
    assert!(!status.enabled);
    configure(&app, &idp, |s| s.button_label = "Sign In with Acme".into()).await;
    let status = call::<sso::Status>(&anonymous(&app), Empty::default()).await.unwrap();
    assert!(status.enabled);
    assert_eq!(status.button_label, "Sign In with Acme");

    let s = start(&app, &idp).await;
    assert_eq!(s.query["scope"], "openid email profile");
    assert_eq!(s.query["code_challenge_method"], "S256");
    assert_eq!(s.query["redirect_uri"], "http://localhost:3000/api/auth/sso/callback");
    assert!(!s.query.contains_key("prompt"), "someone signed in at the provider is not asked again");
    let stored: String = sqlx::query_scalar("select secrets from sso_logins limit 1").fetch_one(&app.db).await.unwrap();
    assert!(!stored.contains(&s.query["nonce"]), "the nonce and verifier are stored encrypted");

    let session = sso::callback(&anonymous(&app), &s.state, Some(&s.browser), Some("code-1"), None).await.unwrap();
    let anna = user(&app, "anna@example.com").await;
    assert!(anna.sso && anna.password_hash.is_none());
    assert_eq!(anna.role, "sender");
    assert_eq!(anna.name, "Anna Andersson");
    assert!(anna.last_login_at.is_some());

    let ctx = session_ctx(&app, &session).await;
    let me = call::<auth_svc::WhoAmI>(&ctx, Empty::default()).await.unwrap();
    assert!(me.sso);
    assert_eq!(me.email, "anna@example.com");
    let again = sign_in(&app, &idp).await.unwrap();
    assert_ne!(again.token, session.token);
    let accounts: i64 = sqlx::query_scalar("select count(*) from users").fetch_one(&app.db).await.unwrap();
    assert_eq!(accounts, 2, "the same identity is the same account");
}

#[tokio::test]
async fn a_token_that_does_not_check_out_signs_nobody_in() {
    let Some(app) = app().await else { return };
    let idp = idp().await;
    first_admin(&app).await;
    configure(&app, &idp, |_| {}).await;

    let ctx = anonymous(&app);
    let s = start(&app, &idp).await;
    assert_eq!(sso::callback(&ctx, "made-up", Some(&s.browser), Some("code-1"), None).await.unwrap_err(), "expired", "wrong state");
    assert_eq!(sso::callback(&ctx, "", None, None, None).await.unwrap_err(), "expired");
    let logged: i64 =
        sqlx::query_scalar("select count(*) from audit_log where action = 'auth.sso_refused'").fetch_one(&app.db).await.unwrap();
    assert_eq!(logged, 0, "a guess cannot fill the append-only log");

    assert_eq!(sso::callback(&ctx, &s.state, Some("another-browser"), Some("code-1"), None).await.unwrap_err(), "other_browser");
    assert_eq!(refusals(&app, "other_browser").await, 1);
    assert_eq!(sso::callback(&ctx, &s.state, Some(&s.browser), Some("code-1"), None).await.unwrap_err(), "expired", "a state is used once");

    let s = start(&app, &idp).await;
    assert_eq!(sso::callback(&ctx, &s.state, None, Some("code-1"), None).await.unwrap_err(), "other_browser", "no cookie at all");

    let s = start(&app, &idp).await;
    assert_eq!(sso::callback(&ctx, &s.state, Some(&s.browser), None, Some("access_denied")).await.unwrap_err(), "denied");
    let s = start(&app, &idp).await;
    assert_eq!(sso::callback(&ctx, &s.state, Some(&s.browser), None, None).await.unwrap_err(), "failed", "no code");

    let s = start(&app, &idp).await;
    idp.pending.lock().unwrap().code_used = true;
    assert_eq!(sso::callback(&ctx, &s.state, Some(&s.browser), Some("code-1"), None).await.unwrap_err(), "failed", "a used code");

    sqlx::query("update sso_logins set expires_at = now() - interval '1 minute'").execute(&app.db).await.unwrap();
    let s = start(&app, &idp).await;
    sqlx::query("update sso_logins set expires_at = now() - interval '1 minute'").execute(&app.db).await.unwrap();
    assert_eq!(sso::callback(&ctx, &s.state, Some(&s.browser), Some("code-1"), None).await.unwrap_err(), "expired");

    let now = chrono::Utc::now().timestamp();
    for (field, value) in [
        ("nonce", json!("someone-elses")),
        ("aud", json!("another-client")),
        ("iss", json!("https://evil.example")),
        ("exp", json!(now - 600)),
        ("sub", Value::Null),
    ] {
        let mut claims = good_claims("subject-1", "anna@example.com");
        claims[field] = value;
        *idp.claims.lock().unwrap() = claims;
        assert_eq!(sign_in(&app, &idp).await.unwrap_err(), "failed", "{field}");
    }
    let accounts: i64 = sqlx::query_scalar("select count(*) from users where sso").fetch_one(&app.db).await.unwrap();
    assert_eq!(accounts, 0);
}

#[tokio::test]
async fn keys_are_cached_and_a_forged_token_does_not_refetch_them() {
    let Some(app) = app().await else { return };
    let idp = idp().await;
    first_admin(&app).await;
    configure(&app, &idp, |_| {}).await;
    sign_in(&app, &idp).await.unwrap();
    sign_in(&app, &idp).await.unwrap();
    assert_eq!(*idp.jwks_fetches.lock().unwrap(), 1, "the provider's keys are not fetched for every sign-in");

    // Within the first minute a new key is not looked for, so someone cannot make us ask with every forged token.
    *idp.signing_key.lock().unwrap() = new_key();
    assert_eq!(sign_in(&app, &idp).await.unwrap_err(), "failed");
    assert_eq!(*idp.jwks_fetches.lock().unwrap(), 1);
}

#[tokio::test]
async fn refused_people_and_accounts() {
    let Some(app) = app().await else { return };
    let idp = idp().await;
    first_admin(&app).await;
    configure(&app, &idp, |_| {}).await;
    let set = |claims: Value| *idp.claims.lock().unwrap() = claims;

    let mut unverified = good_claims("s-unverified", "new@example.com");
    unverified["email_verified"] = json!(false);
    set(unverified);
    assert_eq!(sign_in(&app, &idp).await.unwrap_err(), "email_unverified");
    let mut silent = good_claims("s-unverified", "new@example.com");
    silent.as_object_mut().unwrap().remove("email_verified");
    set(silent);
    assert_eq!(sign_in(&app, &idp).await.unwrap_err(), "email_unverified", "no claim is not a yes");

    let mut no_mfa = good_claims("s-nomfa", "new@example.com");
    no_mfa["amr"] = json!(["pwd"]);
    set(no_mfa.clone());
    assert_eq!(sign_in(&app, &idp).await.unwrap_err(), "mfa_required");
    assert_eq!(refusals(&app, "mfa_required").await, 1);
    let nobody: i64 =
        sqlx::query_scalar("select count(*) from users where lower(email) = 'new@example.com'").fetch_one(&app.db).await.unwrap();
    assert_eq!(nobody, 0);

    // Admins are never matched by email, so the provider cannot take one over.
    set(good_claims("s-admin", "admin@example.com"));
    assert_eq!(sign_in(&app, &idp).await.unwrap_err(), "admin_account");
    let admin = user(&app, "admin@example.com").await;
    assert!(!admin.sso && admin.password_hash.is_some());

    add_user(&app, "gone@example.com", "sender").await;
    sqlx::query("update users set active = false where email = 'gone@example.com'").execute(&app.db).await.unwrap();
    set(good_claims("s-gone", "gone@example.com"));
    assert_eq!(sign_in(&app, &idp).await.unwrap_err(), "inactive");
    assert!(!user(&app, "gone@example.com").await.sso, "nothing changes on an account that is refused");

    // A deactivated person with an identity stays out.
    set(good_claims("s-out", "out@example.com"));
    sign_in(&app, &idp).await.unwrap();
    sqlx::query("update users set active = false where email = 'out@example.com'").execute(&app.db).await.unwrap();
    assert_eq!(sign_in(&app, &idp).await.unwrap_err(), "inactive");

    // Email: follows the identity, and never moves onto someone else's account.
    set(good_claims("s-move", "before@example.com"));
    sign_in(&app, &idp).await.unwrap();
    let id = user(&app, "before@example.com").await.id;
    set(good_claims("s-move", "after@example.com"));
    sign_in(&app, &idp).await.unwrap();
    assert_eq!(user(&app, "after@example.com").await.id, id, "a changed email keeps the account");
    set(good_claims("s-move", "admin@example.com"));
    assert_eq!(sign_in(&app, &idp).await.unwrap_err(), "email_conflict");
    assert_eq!(user(&app, "after@example.com").await.id, id);
    assert_eq!(user(&app, "admin@example.com").await.role, "admin");
    // The address now belongs to an account with another subject: someone new who reuses it gets nothing.
    set(good_claims("s-reused", "after@example.com"));
    assert_eq!(sign_in(&app, &idp).await.unwrap_err(), "email_conflict");
}

#[tokio::test]
async fn linking_takes_away_what_signs_in_on_its_own() {
    let Some(app) = app().await else { return };
    let idp = idp().await;
    first_admin(&app).await;
    configure(&app, &idp, |_| {}).await;
    let id = add_user(&app, "anna@example.com", "manager").await;
    sqlx::query("update users set totp_secret = 'x', totp_enabled_at = now(), has_passkey = true where id = $1")
        .bind(id)
        .execute(&app.db)
        .await
        .unwrap();
    sqlx::query("insert into passkeys (id, user_id, credential_id, public_key, name) values ($1, $2, '\\x01', '\\x02', 'Phone')")
        .bind(Uuid::now_v7())
        .bind(id)
        .execute(&app.db)
        .await
        .unwrap();
    sqlx::query("insert into recovery_codes (id, user_id, code_hash) values ($1, $2, 'h')")
        .bind(Uuid::now_v7())
        .bind(id)
        .execute(&app.db)
        .await
        .unwrap();
    sqlx::query("insert into password_reset_tokens (id, user_id, expires_at) values ($1, $2, now() + interval '1 hour')")
        .bind(opensign_crypto::sha256_hex("reset-token"))
        .bind(id)
        .execute(&app.db)
        .await
        .unwrap();
    let key = "osk_abc123_secretsecret";
    sqlx::query(
        "insert into api_keys (id, name, prefix, key_hash, user_id, permissions) values ($1, 'k', 'abc123', $2, $3, '{documents.read}')",
    )
    .bind(Uuid::now_v7())
    .bind(opensign_crypto::sha256_hex(key))
    .bind(id)
    .execute(&app.db)
    .await
    .unwrap();
    let old = {
        let mut conn = app.db.acquire().await.unwrap();
        create_session(&mut conn, id, &SecuritySettings::default(), None, None, chrono::Utc::now()).await.unwrap()
    };

    sign_in(&app, &idp).await.unwrap();
    let anna = user(&app, "anna@example.com").await;
    assert_eq!(anna.id, id, "an existing account with the verified email is linked");
    assert_eq!(anna.role, "manager", "and keeps its role");
    assert!(anna.sso && anna.password_hash.is_none() && anna.totp_secret.is_none() && anna.totp_enabled_at.is_none() && !anna.has_passkey);
    let count = |table: &'static str| {
        let db = app.db.clone();
        async move {
            sqlx::query_scalar::<_, i64>(&format!("select count(*) from {table} where user_id = $1")).bind(id).fetch_one(&db).await.unwrap()
        }
    };
    assert_eq!(count("passkeys").await, 0);
    assert_eq!(count("recovery_codes").await, 0);
    let linked: i64 = sqlx::query_scalar("select count(*) from audit_log where action = 'auth.sso_linked' and entity_id = $1")
        .bind(id.to_string())
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(linked, 1);

    let mut conn = app.db.acquire().await.unwrap();
    let security = SecuritySettings::default();
    assert!(load_api_key(&mut conn, key, None, &security, chrono::Utc::now()).await.unwrap().is_none(), "API keys are revoked");
    assert!(load_session(&mut conn, &old.token, &security, chrono::Utc::now()).await.unwrap().is_none(), "older sessions end");
    drop(conn);

    // A reset link sent before the link no longer gives a password.
    let input = serde_json::from_value(json!({ "token": "reset-token", "password": PASSWORD })).unwrap();
    assert!(call::<auth_svc::ResetPassword>(&anonymous(&app), input).await.is_err());
    assert!(user(&app, "anna@example.com").await.password_hash.is_none());
}

#[tokio::test]
async fn a_single_sign_on_account_cannot_get_a_lasting_credential() {
    let Some(app) = app().await else { return };
    let idp = idp().await;
    let admin_ctx = first_admin(&app).await;
    configure(&app, &idp, |_| {}).await;
    let session = sign_in(&app, &idp).await.unwrap();
    let ctx = session_ctx(&app, &session).await;
    let code = |e: opensign_core::AppError| e.code;

    let password = call::<auth_svc::ChangePassword>(
        &ctx,
        auth_svc::ChangePasswordInput { current_password: "guess".into(), new_password: PASSWORD.into() },
    )
    .await;
    assert_eq!(code(password.unwrap_err()), Some("sso_account"));
    assert_eq!(code(call::<auth_svc::BeginTwoFactor>(&ctx, Empty::default()).await.err().unwrap()), Some("sso_account"));
    assert_eq!(
        code(call::<auth_svc::ConfirmTwoFactor>(&ctx, auth_svc::CodeInput { code: "123456".into() }).await.err().unwrap()),
        Some("sso_account")
    );
    assert_eq!(
        code(call::<passkeys::BeginRegistration>(&ctx, passkeys::BeginInput { current_password: None }).await.err().unwrap()),
        Some("sso_account")
    );
    assert_eq!(code(call::<passkeys::BeginStepUp>(&ctx, Empty::default()).await.err().unwrap()), Some("sso_account"));
    for _ in 0..8 {
        let stepped = call::<passkeys::StepUp>(&ctx, passkeys::StepUpInput { current_password: Some("guess".into()), passkey: None }).await;
        assert_eq!(code(stepped.err().unwrap()), Some("sso_account"));
    }
    let failures: i64 = sqlx::query_scalar("select count(*) from login_failures").fetch_one(&app.db).await.unwrap();
    assert_eq!(failures, 0, "being refused a step-up does not count towards a lockout");
    let elevated: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar("select elevated_until from sessions where id = $1")
        .bind(ctx.session_id.as_deref().unwrap())
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert!(elevated.is_none());

    let key = call::<users::CreateApiKey>(
        &ctx,
        users::CreateApiKeyInput {
            name: "k".into(),
            permissions: vec![opensign_core::Permission::DocumentsRead],
            expires_in_days: None,
            allowed_ips: vec![],
        },
    )
    .await;
    assert_eq!(code(key.err().unwrap()), Some("sso_account"));
    let mcp = call::<oauth::Approve>(&ctx, oauth::RequestInput { request: "whatever".into() }).await;
    assert_eq!(code(mcp.err().unwrap()), Some("sso_account"));

    // Forgot password says the usual thing and sends no link.
    call::<auth_svc::RequestPasswordReset>(&anonymous(&app), auth_svc::EmailInput { email: "anna@example.com".into() }).await.unwrap();
    let tokens: i64 = sqlx::query_scalar("select count(*) from password_reset_tokens").fetch_one(&app.db).await.unwrap();
    assert_eq!(tokens, 0);

    // A password sign-in fails like a wrong one.
    let login =
        call::<auth_svc::Login>(&anonymous(&app), auth_svc::LoginInput { email: "anna@example.com".into(), password: PASSWORD.into() })
            .await;
    assert!(login.is_err());

    // The provider owns the email; an admin cannot redirect it.
    let id = user(&app, "anna@example.com").await.id;
    let change = call::<users::ChangeEmail>(&admin_ctx, users::ChangeEmailInput { id, email: "other@example.com".into() }).await;
    assert_eq!(code(change.err().unwrap()), Some("sso_account"));
    // Other admin actions still work on the account.
    call::<users::Update>(&admin_ctx, users::UpdateInput { id, name: None, role: Some(Role::Viewer), active: None }).await.unwrap();
    assert_eq!(user(&app, "anna@example.com").await.role, "viewer");
    assert!(sign_in(&app, &idp).await.is_ok(), "the role stays what OpenSign says");
    assert_eq!(user(&app, "anna@example.com").await.role, "viewer");
}

#[tokio::test]
async fn only_a_proven_second_factor_meets_the_requirement() {
    let Some(app) = app().await else { return };
    let idp = idp().await;
    first_admin(&app).await;
    configure(&app, &idp, |s| s.require_provider_mfa = false).await;
    let mut conn = app.db.acquire().await.unwrap();
    let enforced = SecuritySettings { enforce_two_factor: true, ..SecuritySettings::default() };
    let mut without = good_claims("s1", "anna@example.com");
    without["amr"] = json!(["pwd"]);
    *idp.claims.lock().unwrap() = without.clone();

    // With the provider's check off, and two-factor not required here, a sign-in without a factor works.
    let weak = sign_in(&app, &idp).await.unwrap();
    let now = chrono::Utc::now();
    let (off, _) = load_session(&mut conn, &weak.token, &SecuritySettings::default(), now).await.unwrap().unwrap();
    assert!(!off.must_enable_two_factor);
    // An admin requires two-factor later: that session no longer counts.
    let (on, _) = load_session(&mut conn, &weak.token, &enforced, now).await.unwrap().unwrap();
    assert!(on.must_enable_two_factor);

    *idp.claims.lock().unwrap() = good_claims("s1", "anna@example.com");
    let strong = sign_in(&app, &idp).await.unwrap();
    let (proven, _) = load_session(&mut conn, &strong.token, &enforced, now).await.unwrap().unwrap();
    assert!(!proven.must_enable_two_factor);

    // Required here, the provider's check cannot be turned off around it.
    app.settings.save(&mut conn, &app.keys, &enforced, None).await.unwrap();
    *idp.claims.lock().unwrap() = without;
    assert_eq!(sign_in(&app, &idp).await.unwrap_err(), "mfa_required");
}

#[tokio::test]
async fn passwords_for_admins_only() {
    let Some(app) = app().await else { return };
    let idp = idp().await;
    let admin_ctx = first_admin(&app).await;
    add_user(&app, "pat@example.com", "sender").await;
    let login = |email: &'static str| {
        let ctx = anonymous(&app);
        async move { call::<auth_svc::Login>(&ctx, auth_svc::LoginInput { email: email.into(), password: PASSWORD.into() }).await }
    };
    let invite = |role: Role| {
        let ctx = admin_ctx.clone();
        async move { call::<users::Invite>(&ctx, users::InviteInput { email: "invited@example.com".into(), name: None, role }).await }
    };
    configure(&app, &idp, |s| s.enabled = false).await;
    assert!(login("pat@example.com").await.is_ok());

    configure(&app, &idp, |s| s.password_sign_in = "adminsOnly".into()).await;
    assert_eq!(login("pat@example.com").await.err().unwrap().code, Some("sso_required"));
    let wrong = call::<auth_svc::Login>(
        &anonymous(&app),
        auth_svc::LoginInput { email: "pat@example.com".into(), password: "wrong wrong wrong".into() },
    )
    .await;
    assert_ne!(wrong.err().unwrap().code, Some("sso_required"), "only someone who knows the password learns this");
    assert!(login("admin@example.com").await.is_ok(), "the emergency account still works");
    assert!(invite(Role::Sender).await.is_err());

    // Turned off again, nobody is locked out.
    configure(&app, &idp, |s| {
        s.password_sign_in = "adminsOnly".into();
        s.enabled = false;
    })
    .await;
    assert!(login("pat@example.com").await.is_ok());
}

#[tokio::test]
async fn nothing_before_the_first_admin_and_never_an_admin_role() {
    let Some(app) = app().await else { return };
    let idp = idp().await;
    configure(&app, &idp, |_| {}).await;
    assert!(!call::<sso::Status>(&anonymous(&app), Empty::default()).await.unwrap().enabled);
    assert!(call::<sso::Start>(&anonymous(&app), Empty::default()).await.is_err(), "an account would also close setup for good");

    let admin_ctx = first_admin(&app).await;
    // A pinned value skips validation, so sign-in itself holds the line.
    configure(&app, &idp, |s| s.default_role = "admin".into()).await;
    assert_eq!(sign_in(&app, &idp).await.unwrap_err(), "disabled");

    let change = |values: Value| call::<admin::UpdateSettings>(&admin_ctx, admin::UpdateInput { group: admin::Group::Sso, values });
    assert!(change(json!({ "defaultRole": "admin" })).await.is_err());
    assert!(change(json!({ "defaultRole": "emperor" })).await.is_err());
    assert!(change(json!({ "passwordSignIn": "nobody", "defaultRole": "viewer" })).await.is_err());
    configure(&app, &idp, |s| {
        s.enabled = false;
        s.client_secret = Some(String::new());
    })
    .await;
    assert!(change(json!({ "enabled": true, "defaultRole": "viewer" })).await.is_err(), "needs a client secret");
    let saved = change(json!({ "enabled": true, "defaultRole": "viewer", "clientSecret": "s3cret" })).await.unwrap();
    assert_eq!(saved.values["clientSecret"], true, "a secret only shows whether it is set");

    let mut agent = admin_ctx.clone();
    agent.surface = Surface::Mcp;
    agent.actor.as_mut().unwrap().credential = Credential::OAuth;
    let refused =
        call::<admin::UpdateSettings>(&agent, admin::UpdateInput { group: admin::Group::Sso, values: json!({ "defaultRole": "manager" }) })
            .await;
    assert!(refused.is_err(), "agents do not decide who may sign in");
    let mut assistant = admin_ctx.clone();
    assistant.surface = Surface::Ai;
    assert!(!assistant.in_app());
}

#[tokio::test]
async fn a_real_install_needs_a_public_provider() {
    let Some(app) = production_app().await else { return };
    let idp = idp().await;
    first_admin(&app).await;
    configure(&app, &idp, |_| {}).await;
    let started = call::<sso::Start>(&anonymous(&app), Empty::default()).await;
    assert!(started.is_err(), "the pinned or stored address of a local provider is refused");
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

#[tokio::test]
async fn signers_are_not_affected() {
    let Some(app) = app().await else { return };
    let idp = idp().await;
    first_admin(&app).await;
    configure(&app, &idp, |s| s.password_sign_in = "adminsOnly".into()).await;
    let session = sign_in(&app, &idp).await.unwrap();
    let sender = session_ctx(&app, &session).await;

    let document = call::<docs::Create>(&sender, serde_json::from_value(json!({ "title": "Lease" })).unwrap()).await.unwrap();
    let id = document.document.id;
    call::<docs::Upload>(
        &sender,
        docs::UploadInput { document_id: id, name: "lease.pdf".into(), content_base64: B64.encode(sample_pdf()), ..Default::default() },
    )
    .await
    .unwrap();
    let party = PartyInput {
        id: None,
        role: PartyRole::Signer,
        name: "Outside Signer".into(),
        email: Some("signer@example.net".into()),
        phone: None,
        personal_number: None,
        company: None,
        sign_order: None,
        delivery: Delivery::Link,
        auth_to_view: AuthMethod::None,
        auth_to_sign: AuthMethod::Standard,
        attachments: None,
        language: None,
        webcam_photo: None,
    };
    call::<docs::SetParties>(&sender, docs::SetPartiesInput { document_id: id, parties: vec![party] }).await.unwrap();
    call::<docs::Send>(&sender, docs::SendInput { id, expires_at: None }).await.unwrap();
    let token = {
        let mut conn = app.db.acquire().await.unwrap();
        signer_token(&app.keys, &parties(&mut conn, id).await.unwrap()[0])
    };
    let signer = Ctx::new(app.clone(), Surface::Signer);
    let view = call::<signing::Open>(&signer, signing::OpenInput { token, session: None, redirect_url: None }).await.unwrap();
    assert_eq!(view.needs_authentication, None, "a link opens with no account and no provider");
}
