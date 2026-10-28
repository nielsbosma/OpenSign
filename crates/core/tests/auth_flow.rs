#![allow(clippy::unwrap_used)]

use std::sync::Arc;

use opensign_core::auth::{load_api_key, load_session};
use opensign_core::context::{App, Ctx, Surface};
use opensign_core::error::ErrorKind;
use opensign_core::services::auth::{self, LoginResult};
use opensign_core::services::{Empty, setup, users};
use opensign_core::settings::SecuritySettings;
use opensign_core::testing::app;
use opensign_core::{Permission, Role, call};
use opensign_crypto::{Totp, sha256_hex};

const PASSWORD: &str = "correct horse battery staple 42";

fn kind<T>(result: opensign_core::Result<T>) -> ErrorKind {
    match result {
        Err(e) => e.kind,
        Ok(_) => panic!("expected an error"),
    }
}

fn web(app: &Arc<App>) -> Ctx {
    let mut ctx = Ctx::new(app.clone(), Surface::Web);
    ctx.ip = Some("203.0.113.5".into());
    ctx
}

fn login(email: &str, password: &str) -> auth::LoginInput {
    auth::LoginInput { email: email.into(), password: password.into() }
}

async fn signed_in(app: &Arc<App>, result: LoginResult) -> Ctx {
    let LoginResult::SignedIn { token, .. } = result else { panic!("expected a session") };
    let mut conn = app.db.acquire().await.unwrap();
    let (actor, session) =
        load_session(&mut conn, &token, &SecuritySettings::default(), opensign_core::context::now()).await.unwrap().unwrap();
    let mut ctx = web(app).with_actor(actor);
    ctx.session_id = Some(session);
    ctx
}

#[tokio::test]
async fn setup_login_lockout_two_factor_and_api_keys() {
    let Some(app) = app().await else { return };
    let anon = web(&app);
    assert!(call::<setup::Status>(&anon, Empty {}).await.unwrap().needs_setup);
    let setup_input = |code: String| setup::CompleteInput {
        setup_code: code,
        org_name: "Acme AB".into(),
        name: "Ada Admin".into(),
        email: "Ada@Example.com".into(),
        password: PASSWORD.into(),
    };
    assert!(call::<setup::Complete>(&anon, setup_input("AAAA-AAAA".into())).await.is_err(), "a wrong setup code is refused");
    call::<setup::Complete>(&anon, setup_input(app.keys.setup_code())).await.unwrap();
    assert!(call::<setup::Complete>(&anon, setup_input(app.keys.setup_code())).await.is_err(), "setup locks after the first admin");
    assert!(!call::<setup::Status>(&anon, Empty {}).await.unwrap().needs_setup);

    let unknown = call::<auth::Login>(&anon, login("nobody@example.com", PASSWORD)).await.err().unwrap();
    let wrong = call::<auth::Login>(&anon, login("ada@example.com", "not the password at all")).await.err().unwrap();
    assert_eq!(unknown.message, wrong.message, "unknown accounts and wrong passwords look the same");

    let admin = signed_in(&app, call::<auth::Login>(&anon, login(" ADA@example.com ", PASSWORD)).await.unwrap()).await;
    assert_eq!(admin.actor.as_ref().unwrap().role, Role::Admin);

    let setup = call::<auth::BeginTwoFactor>(&admin, Empty {}).await.unwrap();
    let totp = Totp::from_base32(&setup.secret).unwrap();
    let now = opensign_core::context::now().timestamp();
    assert!(call::<auth::ConfirmTwoFactor>(&admin, auth::CodeInput { code: "000000".into() }).await.is_err());
    let recovery = call::<auth::ConfirmTwoFactor>(&admin, auth::CodeInput { code: totp.code_at(now) }).await.unwrap().recovery_codes;
    assert!(recovery.len() >= 8);
    assert_eq!(
        kind(call::<auth::BeginTwoFactor>(&admin, Empty {}).await),
        ErrorKind::Conflict,
        "a stolen session must not swap the authenticator"
    );

    let LoginResult::TwoFactorRequired { challenge } = call::<auth::Login>(&anon, login("ada@example.com", PASSWORD)).await.unwrap() else {
        panic!("two-factor is required once enabled")
    };
    let replay = call::<auth::VerifyTwoFactor>(&anon, auth::TwoFactorInput { challenge: challenge.clone(), code: totp.code_at(now) }).await;
    assert_eq!(kind(replay), ErrorKind::Unauthorized, "a TOTP code works once");
    let by_recovery =
        call::<auth::VerifyTwoFactor>(&anon, auth::TwoFactorInput { challenge: challenge.clone(), code: recovery[0].clone() })
            .await
            .unwrap();
    assert!(matches!(by_recovery, LoginResult::SignedIn { .. }));
    let LoginResult::TwoFactorRequired { challenge } = call::<auth::Login>(&anon, login("ada@example.com", PASSWORD)).await.unwrap() else {
        panic!()
    };
    let reused = call::<auth::VerifyTwoFactor>(&anon, auth::TwoFactorInput { challenge, code: recovery[0].clone() }).await;
    assert_eq!(kind(reused), ErrorKind::Unauthorized, "a recovery code works once");

    let created = call::<users::CreateApiKey>(
        &admin,
        users::CreateApiKeyInput {
            name: "CRM".into(),
            permissions: vec![Permission::DocumentsCreate],
            expires_in_days: Some(30),
            allowed_ips: vec![],
        },
    )
    .await
    .unwrap();
    let mut conn = app.db.acquire().await.unwrap();
    let stored: String =
        sqlx::query_scalar("select key_hash from api_keys where id = $1").bind(created.id).fetch_one(&mut *conn).await.unwrap();
    assert_eq!(stored, sha256_hex(&created.key), "only a hash of the key is stored");
    let key_actor =
        load_api_key(&mut conn, &created.key, None, &SecuritySettings::default(), opensign_core::context::now()).await.unwrap().unwrap();
    assert!(key_actor.can(Permission::DocumentsCreate));
    assert!(!key_actor.can(Permission::UsersManage), "a key is limited to its scopes even for an admin");
    let tampered = format!("{}x", &created.key[..created.key.len() - 1]);
    assert!(load_api_key(&mut conn, &tampered, None, &SecuritySettings::default(), opensign_core::context::now()).await.unwrap().is_none());
    let via_key = web(&app).with_actor(key_actor);
    let escalate = call::<users::CreateApiKey>(
        &via_key,
        users::CreateApiKeyInput {
            name: "more".into(),
            permissions: vec![Permission::DocumentsCreate],
            expires_in_days: None,
            allowed_ips: vec![],
        },
    )
    .await;
    assert_eq!(kind(escalate), ErrorKind::Forbidden, "an API key cannot mint keys");
    call::<users::RevokeApiKey>(&admin, users::IdInput { id: created.id }).await.unwrap();
    assert!(
        load_api_key(&mut conn, &created.key, None, &SecuritySettings::default(), opensign_core::context::now()).await.unwrap().is_none()
    );

    let invite = call::<users::Invite>(
        &admin,
        users::InviteInput { email: "sven@example.com".into(), name: Some("Sven".into()), role: Role::Sender },
    )
    .await
    .unwrap();
    assert_eq!(invite.role, "sender");
    let token = "x".repeat(43);
    sqlx::query("update invites set token_hash = $2 where id = $1")
        .bind(invite.id)
        .bind(sha256_hex(&token))
        .execute(&mut *conn)
        .await
        .unwrap();
    assert!(
        call::<users::AcceptInvite>(&anon, users::AcceptInput { token: token.clone(), name: "Sven".into(), password: "short".into() })
            .await
            .is_err()
    );
    call::<users::AcceptInvite>(&anon, users::AcceptInput { token: token.clone(), name: "Sven S".into(), password: PASSWORD.into() })
        .await
        .unwrap();
    assert!(
        call::<users::AcceptInvite>(&anon, users::AcceptInput { token, name: "Again".into(), password: PASSWORD.into() }).await.is_err(),
        "an invite works once"
    );
    let sven = signed_in(&app, call::<auth::Login>(&anon, login("sven@example.com", PASSWORD)).await.unwrap()).await;
    assert_eq!(kind(call::<users::List>(&web(&app), Empty {}).await), ErrorKind::Unauthorized);
    assert!(call::<users::List>(&sven, Empty {}).await.is_ok());
    assert_eq!(
        kind(call::<users::Invite>(&sven, users::InviteInput { email: "x@example.com".into(), name: None, role: Role::Admin }).await),
        ErrorKind::Forbidden
    );

    for _ in 0..5 {
        let _ = call::<auth::Login>(&anon, login("sven@example.com", "wrong wrong wrong wrong")).await;
    }
    let locked = call::<auth::Login>(&anon, login("sven@example.com", PASSWORD)).await;
    assert_eq!(kind(locked), ErrorKind::TooManyRequests, "the right password is refused while locked");

    call::<auth::RequestPasswordReset>(&anon, auth::EmailInput { email: "nobody@example.com".into() }).await.unwrap();
    let chain = opensign_core::audit::verify_chain(&mut conn, &app.keys).await.unwrap();
    assert!(chain.valid, "{chain:?}");
}
