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
async fn a_deployment_setup_code_replaces_the_derived_one() {
    let Some(app) = app().await else { return };
    let Ok(mut inner) = Arc::try_unwrap(app) else { panic!("only reference") };
    let code = "Zq8rT2vLmX9pK4wN7bC3dF6hJ1sA";
    inner.env.setup_code = Some(code.into());
    let app = Arc::new(inner);
    let anon = web(&app);
    let input = |code: &str| setup::CompleteInput {
        setup_code: code.into(),
        org_name: "Acme AB".into(),
        name: "Ada Admin".into(),
        email: "ada@example.com".into(),
        password: PASSWORD.into(),
    };
    assert!(call::<setup::Complete>(&anon, input(&app.keys.setup_code())).await.is_err(), "the logged code no longer works");
    assert!(call::<setup::Complete>(&anon, input(&code.to_lowercase())).await.is_err(), "the code is case sensitive");
    call::<setup::Complete>(&anon, input(&format!(" {code} "))).await.unwrap();
    assert!(call::<setup::Complete>(&anon, input(code)).await.is_err(), "setup locks after the first admin");
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

    let managed = call::<users::CreateApiKey>(
        &admin,
        users::CreateApiKeyInput {
            name: "Ops".into(),
            permissions: vec![Permission::ApiKeysManage, Permission::DocumentsRead, Permission::DocumentsCreate],
            expires_in_days: None,
            allowed_ips: vec!["198.51.100.0/24".into()],
        },
    )
    .await
    .unwrap();
    let now = opensign_core::context::now();
    assert!(load_api_key(&mut conn, &managed.key, Some("203.0.113.5"), &SecuritySettings::default(), now).await.unwrap().is_none());
    let managed_actor =
        load_api_key(&mut conn, &managed.key, Some("198.51.100.20"), &SecuritySettings::default(), now).await.unwrap().unwrap();
    let via_managed = web(&app).with_actor(managed_actor);
    let update = |id, name: Option<&str>, permissions: Option<Vec<Permission>>, ips: Option<Vec<&str>>| users::UpdateApiKeyInput {
        id,
        name: name.map(Into::into),
        permissions,
        allowed_ips: ips.map(|v| v.into_iter().map(Into::into).collect()),
    };
    let narrowed = call::<users::UpdateApiKey>(
        &via_managed,
        update(
            managed.id,
            Some("Ops (read)"),
            Some(vec![Permission::ApiKeysManage, Permission::DocumentsRead]),
            Some(vec!["198.51.100.20"]),
        ),
    )
    .await
    .unwrap();
    assert_eq!(narrowed.name, "Ops (read)");
    assert_eq!(narrowed.allowed_ips, ["198.51.100.20"]);
    assert!(!narrowed.permissions.contains(&"documents.create".to_string()));
    let widen = call::<users::UpdateApiKey>(
        &via_managed,
        update(managed.id, None, Some(vec![Permission::ApiKeysManage, Permission::DocumentsCreate]), None),
    )
    .await;
    assert_eq!(kind(widen), ErrorKind::Forbidden, "a key cannot widen scopes");
    let open_up = call::<users::UpdateApiKey>(&via_managed, update(managed.id, None, None, Some(vec![]))).await;
    assert_eq!(kind(open_up), ErrorKind::Forbidden, "a key cannot drop its IP allowlist");
    let beyond = call::<users::UpdateApiKey>(&via_managed, update(created.id, None, Some(vec![Permission::UsersManage]), None)).await;
    assert_eq!(kind(beyond), ErrorKind::Forbidden, "no one grants more than they have");
    let bad_ip = call::<users::UpdateApiKey>(&admin, update(managed.id, None, None, Some(vec!["example.com"]))).await;
    assert_eq!(kind(bad_ip), ErrorKind::BadRequest);

    let widened = call::<users::UpdateApiKey>(
        &admin,
        update(managed.id, None, Some(vec![Permission::ApiKeysManage, Permission::DocumentsCreate]), Some(vec![])),
    )
    .await
    .unwrap();
    assert!(widened.allowed_ips.is_empty() && widened.permissions.contains(&"documents.create".to_string()), "signed in, you can widen");
    let still: String =
        sqlx::query_scalar("select key_hash from api_keys where id = $1").bind(managed.id).fetch_one(&mut *conn).await.unwrap();
    assert_eq!(still, sha256_hex(&managed.key), "editing never changes the secret");
    assert!(load_api_key(&mut conn, &managed.key, Some("203.0.113.5"), &SecuritySettings::default(), now).await.unwrap().is_some());

    let (before, after): (serde_json::Value, serde_json::Value) =
        sqlx::query_as("select before, after from audit_log where action = 'api_keys.update' and entity_id = $1 order by id desc limit 1")
            .bind(managed.id.to_string())
            .fetch_one(&mut *conn)
            .await
            .unwrap();
    assert_eq!(before["allowedIps"], serde_json::json!(["198.51.100.20"]));
    assert_eq!(after["allowedIps"], serde_json::json!([]));
    assert!(!after.to_string().contains(&managed.key[4..16]), "the audit trail never holds the key");

    let listed = call::<users::ListApiKeys>(&admin, Empty {}).await.unwrap();
    assert!(listed.iter().any(|k| k.id == managed.id && k.allowed_ips.is_empty()));
    call::<users::RevokeApiKey>(&admin, users::IdInput { id: managed.id }).await.unwrap();
    let revoked = call::<users::UpdateApiKey>(&admin, update(managed.id, Some("Back"), None, None)).await;
    assert_eq!(kind(revoked), ErrorKind::NotFound, "a revoked key cannot be edited");

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
    call::<auth::SetLanguage>(&sven, auth::SetLanguageInput { language: Some("ar".into()) }).await.unwrap();
    assert_eq!(call::<auth::WhoAmI>(&sven, Empty {}).await.unwrap().language.as_deref(), Some("ar"));
    assert_eq!(kind(call::<auth::SetLanguage>(&sven, auth::SetLanguageInput { language: Some("xx".into()) }).await), ErrorKind::BadRequest);
    assert_eq!(kind(call::<auth::SetLanguage>(&web(&app), auth::SetLanguageInput { language: None }).await), ErrorKind::Unauthorized);
    assert_eq!(
        kind(call::<users::Invite>(&sven, users::InviteInput { email: "x@example.com".into(), name: None, role: Role::Admin }).await),
        ErrorKind::Forbidden
    );

    for _ in 0..5 {
        let _ = call::<auth::Login>(&anon, login("sven@example.com", "wrong wrong wrong wrong")).await;
    }
    let locked = call::<auth::Login>(&anon, login("sven@example.com", PASSWORD)).await;
    assert_eq!(kind(locked), ErrorKind::TooManyRequests, "the right password is refused while locked");

    let sven_id = sven.actor.as_ref().unwrap().user_id;
    let change = |email: &str| users::ChangeEmailInput { id: sven_id, email: email.into() };
    assert_eq!(kind(call::<users::ChangeEmail>(&sven, change("sven2@example.com")).await), ErrorKind::Forbidden);
    assert_eq!(kind(call::<users::ChangeEmail>(&admin, change("ADA@example.com")).await), ErrorKind::Conflict);
    call::<users::ChangeEmail>(&admin, change("Sven2@Example.com")).await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, String>("select email from users where id = $1").bind(sven_id).fetch_one(&mut *conn).await.unwrap(),
        "sven@example.com",
        "the email changes only once the new address confirms"
    );
    let token = "y".repeat(43);
    sqlx::query("update email_change_tokens set id = $2 where user_id = $1 and used_at is null")
        .bind(sven_id)
        .bind(sha256_hex(&token))
        .execute(&mut *conn)
        .await
        .unwrap();
    assert!(call::<users::ConfirmEmailChange>(&anon, users::TokenInput { token: "z".repeat(43) }).await.is_err());
    let changed = call::<users::ConfirmEmailChange>(&anon, users::TokenInput { token: token.clone() }).await.unwrap();
    assert_eq!(changed.email, "sven2@example.com");
    assert!(call::<users::ConfirmEmailChange>(&anon, users::TokenInput { token }).await.is_err(), "a confirmation link works once");

    call::<auth::RequestPasswordReset>(&anon, auth::EmailInput { email: "nobody@example.com".into() }).await.unwrap();
    let chain = opensign_core::audit::verify_chain(&mut conn, &app.keys).await.unwrap();
    assert!(chain.valid, "{chain:?}");
}

#[tokio::test]
async fn parallel_guesses_all_count_toward_the_lockout() {
    let Some(app) = app().await else { return };
    let anon = web(&app);
    let input = setup::CompleteInput {
        setup_code: app.keys.setup_code(),
        org_name: "Acme AB".into(),
        name: "Ada Admin".into(),
        email: "ada@example.com".into(),
        password: PASSWORD.into(),
    };
    call::<setup::Complete>(&anon, input).await.unwrap();
    let guesses: Vec<_> = (0..6)
        .map(|i| {
            let anon = anon.clone();
            tokio::spawn(async move { call::<auth::Login>(&anon, login("ada@example.com", &format!("wrong password number {i}"))).await })
        })
        .collect();
    for guess in guesses {
        assert!(guess.await.unwrap().is_err());
    }
    let failures: i32 = sqlx::query_scalar("select max(failures) from login_failures").fetch_one(&app.db).await.unwrap();
    assert!(failures >= 5, "every guess before the lock counted: {failures}");
    assert_eq!(kind(call::<auth::Login>(&anon, login("ada@example.com", PASSWORD)).await), ErrorKind::TooManyRequests);
}

mod passkey {
    use base64::Engine;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64URL;
    use ciborium::Value;
    use p256::ecdsa::signature::Signer;
    use p256::ecdsa::{DerSignature, SigningKey};
    use sha2::{Digest, Sha256};

    pub struct Authenticator {
        pub key: SigningKey,
        pub id: Vec<u8>,
        pub count: u32,
        pub rp_id: String,
        pub origin: String,
    }

    fn cbor(value: &Value) -> Vec<u8> {
        let mut out = Vec::new();
        ciborium::into_writer(value, &mut out).unwrap();
        out
    }

    fn challenge(options: &serde_json::Value) -> String {
        options["challenge"].as_str().unwrap().to_string()
    }

    impl Authenticator {
        fn auth_data(&self, flags: u8, tail: &[u8]) -> Vec<u8> {
            let mut out = Sha256::digest(self.rp_id.as_bytes()).to_vec();
            out.push(flags);
            out.extend_from_slice(&self.count.to_be_bytes());
            out.extend_from_slice(tail);
            out
        }

        fn client(&self, kind: &str, options: &serde_json::Value) -> Vec<u8> {
            serde_json::to_vec(&serde_json::json!({ "type": kind, "challenge": challenge(options), "origin": self.origin })).unwrap()
        }

        /// (clientDataJSON, attestationObject), base64url.
        pub fn create(&self, options: &serde_json::Value) -> (String, String) {
            let point = self.key.verifying_key().to_encoded_point(false);
            let cose = cbor(&Value::Map(vec![
                (1.into(), 2.into()),
                (3.into(), (-7).into()),
                ((-1).into(), 1.into()),
                ((-2).into(), Value::Bytes(point.x().unwrap().to_vec())),
                ((-3).into(), Value::Bytes(point.y().unwrap().to_vec())),
            ]));
            let mut attested = vec![0u8; 16];
            attested.extend_from_slice(&(self.id.len() as u16).to_be_bytes());
            attested.extend_from_slice(&self.id);
            attested.extend_from_slice(&cose);
            let object = cbor(&Value::Map(vec![
                ("fmt".into(), "none".into()),
                ("attStmt".into(), Value::Map(vec![])),
                ("authData".into(), Value::Bytes(self.auth_data(0x45, &attested))),
            ]));
            (B64URL.encode(self.client("webauthn.create", options)), B64URL.encode(object))
        }

        /// (clientDataJSON, authenticatorData, signature), base64url.
        pub fn get(&mut self, options: &serde_json::Value) -> (String, String, String) {
            self.count += 1;
            self.assert(options)
        }

        /// Signs without moving the counter on.
        pub fn assert(&self, options: &serde_json::Value) -> (String, String, String) {
            let client = self.client("webauthn.get", options);
            let data = self.auth_data(0x05, &[]);
            let mut signed = data.clone();
            signed.extend_from_slice(&Sha256::digest(&client));
            let signature: DerSignature = self.key.sign(&signed);
            (B64URL.encode(client), B64URL.encode(data), B64URL.encode(signature.as_bytes()))
        }
    }
}

#[tokio::test]
async fn passkeys_register_and_sign_in_once_per_challenge() {
    use base64::Engine;
    use opensign_core::services::passkeys;

    let Some(app) = app().await else { return };
    let anon = web(&app);
    call::<setup::Complete>(
        &anon,
        setup::CompleteInput {
            setup_code: app.keys.setup_code(),
            org_name: "Acme AB".into(),
            name: "Ada Admin".into(),
            email: "ada@example.com".into(),
            password: PASSWORD.into(),
        },
    )
    .await
    .unwrap();
    let admin = signed_in(&app, call::<auth::Login>(&anon, login("ada@example.com", PASSWORD)).await.unwrap()).await;
    let url = url::Url::parse(&app.public_url()).unwrap();
    let mut device = passkey::Authenticator {
        key: p256::ecdsa::SigningKey::from_slice(&[9u8; 32]).unwrap(),
        id: b"credential-1".to_vec(),
        count: 0,
        rp_id: url.host_str().unwrap().to_string(),
        origin: url.origin().ascii_serialization(),
    };

    let refused = call::<passkeys::BeginRegistration>(&admin, passkeys::BeginInput { current_password: Some("wrong".into()) }).await;
    assert_eq!(kind(refused), ErrorKind::BadRequest, "adding a passkey asks for the password");
    let begin =
        call::<passkeys::BeginRegistration>(&admin, passkeys::BeginInput { current_password: Some(PASSWORD.into()) }).await.unwrap();
    let (client, attestation) = device.create(&begin.options);
    let add = |state: String| passkeys::FinishRegistrationInput {
        state,
        name: "Laptop".into(),
        client_data_json: client.clone(),
        attestation_object: attestation.clone(),
        transports: vec!["internal".into()],
    };
    call::<passkeys::FinishRegistration>(&admin, add(begin.state.clone())).await.unwrap();
    assert!(call::<passkeys::FinishRegistration>(&admin, add(begin.state)).await.is_err(), "a challenge works once");
    assert_eq!(call::<passkeys::List>(&admin, Empty {}).await.unwrap().len(), 1);

    let begin = call::<passkeys::BeginSignIn>(&anon, Empty {}).await.unwrap();
    let (client, data, signature) = device.get(&begin.options);
    let sign_in = |state: String| passkeys::SignInInput {
        state,
        credential_id: base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&device.id),
        client_data_json: client.clone(),
        authenticator_data: data.clone(),
        signature: signature.clone(),
        user_handle: None,
    };
    let session = signed_in(&app, call::<passkeys::SignIn>(&anon, sign_in(begin.state.clone())).await.unwrap()).await;
    assert_eq!(session.actor.as_ref().unwrap().email, "ada@example.com");
    assert_eq!(kind(call::<passkeys::SignIn>(&anon, sign_in(begin.state)).await), ErrorKind::Unauthorized, "no replay");

    let other = call::<passkeys::BeginSignIn>(&anon, Empty {}).await.unwrap();
    let (client, data, signature) = device.get(&begin_options_with_other_challenge(&other.options));
    let mismatched = passkeys::SignInInput {
        state: other.state,
        credential_id: base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&device.id),
        client_data_json: client,
        authenticator_data: data,
        signature,
        user_handle: None,
    };
    assert!(call::<passkeys::SignIn>(&anon, mismatched).await.is_err(), "a response for another challenge is refused");

    let id = call::<passkeys::List>(&admin, Empty {}).await.unwrap()[0].id;
    call::<passkeys::Remove>(&admin, passkeys::RemoveInput { id }).await.unwrap();
    let begin = call::<passkeys::BeginSignIn>(&anon, Empty {}).await.unwrap();
    let (client, data, signature) = device.get(&begin.options);
    let removed = passkeys::SignInInput {
        state: begin.state,
        credential_id: base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&device.id),
        client_data_json: client,
        authenticator_data: data,
        signature,
        user_handle: None,
    };
    assert_eq!(kind(call::<passkeys::SignIn>(&anon, removed).await), ErrorKind::Unauthorized);
}

fn begin_options_with_other_challenge(options: &serde_json::Value) -> serde_json::Value {
    let mut changed = options.clone();
    changed["challenge"] = serde_json::json!("c29tZXRoaW5nLWVsc2U");
    changed
}

async fn first_admin(app: &Arc<App>) -> Ctx {
    let anon = web(app);
    let input = setup::CompleteInput {
        setup_code: app.keys.setup_code(),
        org_name: "Acme AB".into(),
        name: "Ada Admin".into(),
        email: "ada@example.com".into(),
        password: PASSWORD.into(),
    };
    call::<setup::Complete>(&anon, input).await.unwrap();
    signed_in(app, call::<auth::Login>(&anon, login("ada@example.com", PASSWORD)).await.unwrap()).await
}

fn device_for(app: &Arc<App>, seed: u8) -> passkey::Authenticator {
    let url = url::Url::parse(&app.public_url()).unwrap();
    passkey::Authenticator {
        key: p256::ecdsa::SigningKey::from_slice(&[seed; 32]).unwrap(),
        id: format!("credential-{seed}").into_bytes(),
        count: 0,
        rp_id: url.host_str().unwrap().to_string(),
        origin: url.origin().ascii_serialization(),
    }
}

async fn add_passkey(ctx: &Ctx, device: &passkey::Authenticator) {
    use opensign_core::services::passkeys;
    let begin = call::<passkeys::BeginRegistration>(ctx, passkeys::BeginInput { current_password: Some(PASSWORD.into()) }).await.unwrap();
    let (client, attestation) = device.create(&begin.options);
    let finish = passkeys::FinishRegistrationInput {
        state: begin.state,
        name: "Laptop".into(),
        client_data_json: client,
        attestation_object: attestation,
        transports: vec![],
    };
    call::<passkeys::FinishRegistration>(ctx, finish).await.unwrap();
}

fn assertion(
    device: &mut passkey::Authenticator,
    state: String,
    options: &serde_json::Value,
) -> opensign_core::services::passkeys::SignInInput {
    use base64::Engine;
    let (client, data, signature) = device.get(options);
    opensign_core::services::passkeys::SignInInput {
        state,
        credential_id: base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&device.id),
        client_data_json: client,
        authenticator_data: data,
        signature,
        user_handle: None,
    }
}

fn from_address(app: &Arc<App>, ip: &str) -> Ctx {
    let mut ctx = web(app);
    ctx.ip = Some(ip.into());
    ctx
}

async fn second_admin(app: &Arc<App>, email: &str) -> uuid::Uuid {
    let id = uuid::Uuid::now_v7();
    let hash = opensign_crypto::hash_password(PASSWORD).unwrap();
    sqlx::query("insert into users (id, email, name, password_hash, role) values ($1, $2, 'Someone', $3, $4)")
        .bind(id)
        .bind(email)
        .bind(hash)
        .bind(Role::Admin.as_str())
        .execute(&app.db)
        .await
        .unwrap();
    id
}

#[tokio::test]
async fn a_password_alone_does_not_pass_an_account_with_a_passkey() {
    use opensign_core::services::passkeys;
    let Some(app) = app().await else { return };
    let admin = first_admin(&app).await;
    let mut device = device_for(&app, 11);
    add_passkey(&admin, &device).await;
    let other_admin = second_admin(&app, "bo@example.com").await;
    let bo = signed_in(&app, call::<auth::Login>(&web(&app), login("bo@example.com", PASSWORD)).await.unwrap()).await;
    assert_eq!(bo.actor.as_ref().unwrap().user_id, other_admin);
    let mut bos_device = device_for(&app, 12);
    add_passkey(&bo, &bos_device).await;

    let anon = web(&app);
    let LoginResult::PasskeyRequired { ceremony } = call::<auth::Login>(&anon, login("ada@example.com", PASSWORD)).await.unwrap() else {
        panic!("the password alone must not start a session");
    };
    assert_eq!(ceremony.options["allowCredentials"].as_array().unwrap().len(), 1);
    let foreign = assertion(&mut bos_device, ceremony.state, &ceremony.options);
    assert_eq!(kind(call::<passkeys::SignIn>(&anon, foreign).await), ErrorKind::Unauthorized, "only this account's passkey");

    let LoginResult::PasskeyRequired { ceremony } = call::<auth::Login>(&anon, login("ada@example.com", PASSWORD)).await.unwrap() else {
        panic!("expected a passkey step");
    };
    let own = assertion(&mut device, ceremony.state, &ceremony.options);
    let session = signed_in(&app, call::<passkeys::SignIn>(&anon, own).await.unwrap()).await;
    assert_eq!(session.actor.as_ref().unwrap().email, "ada@example.com");

    // An admin's reset clears the passkey: the lost device stops working and the password signs in again.
    call::<users::ResetTwoFactor>(&bo, users::IdInput { id: admin.actor.as_ref().unwrap().user_id }).await.unwrap();
    let begin = call::<passkeys::BeginSignIn>(&anon, Empty {}).await.unwrap();
    let lost = assertion(&mut device, begin.state, &begin.options);
    assert_eq!(kind(call::<passkeys::SignIn>(&anon, lost).await), ErrorKind::Unauthorized);
    assert!(matches!(call::<auth::Login>(&anon, login("ada@example.com", PASSWORD)).await.unwrap(), LoginResult::SignedIn { .. }));
}

#[tokio::test]
async fn one_address_cannot_lock_the_owner_out() {
    use opensign_core::services::passkeys;
    let Some(app) = app().await else { return };
    let admin = first_admin(&app).await;
    let mut device = device_for(&app, 13);
    add_passkey(&admin, &device).await;

    let guesser = from_address(&app, "198.51.100.1");
    for _ in 0..5 {
        assert_eq!(kind(call::<auth::Login>(&guesser, login("ada@example.com", "wrong guess")).await), ErrorKind::Unauthorized);
    }
    assert_eq!(
        kind(call::<auth::Login>(&guesser, login("ada@example.com", PASSWORD)).await),
        ErrorKind::TooManyRequests,
        "the guessing address is locked even with the right password"
    );
    let LoginResult::PasskeyRequired { .. } = call::<auth::Login>(&web(&app), login("ada@example.com", PASSWORD)).await.unwrap() else {
        panic!("the owner's own address is unaffected");
    };

    // An IPv6 customer rotating inside their /64 is still one address.
    let v6 = |host: &str| from_address(&app, &format!("2001:db8:5:5::{host}"));
    for host in ["1", "2", "3", "4", "5"] {
        assert_eq!(kind(call::<auth::Login>(&v6(host), login("nobody@example.com", "wrong guess")).await), ErrorKind::Unauthorized);
    }
    assert_eq!(
        kind(call::<auth::Login>(&v6("6"), login("nobody@example.com", "wrong guess")).await),
        ErrorKind::TooManyRequests,
        "an unknown email throttles exactly like a real one"
    );

    // Many addresses together do lock the account, and a passkey still gets through.
    for address in 2..=5 {
        let guesser = from_address(&app, &format!("198.51.100.{address}"));
        for _ in 0..5 {
            let _ = call::<auth::Login>(&guesser, login("ada@example.com", "wrong guess")).await;
        }
    }
    assert_eq!(kind(call::<auth::Login>(&web(&app), login("ada@example.com", PASSWORD)).await), ErrorKind::TooManyRequests);
    let anon = web(&app);
    let begin = call::<passkeys::BeginSignIn>(&anon, Empty {}).await.unwrap();
    let own = assertion(&mut device, begin.state, &begin.options);
    assert!(matches!(call::<passkeys::SignIn>(&anon, own).await.unwrap(), LoginResult::SignedIn { .. }));

    // A quiet day forgets it all.
    sqlx::query("update login_failures set last_failed_at = last_failed_at - interval '25 hours', locked_until = null")
        .execute(&app.db)
        .await
        .unwrap();
    assert_eq!(kind(call::<auth::Login>(&guesser, login("ada@example.com", "wrong guess")).await), ErrorKind::Unauthorized);
    let counted: i32 =
        sqlx::query_scalar("select failures from login_failures where address = '198.51.100.1'").fetch_one(&app.db).await.unwrap();
    assert_eq!(counted, 1);
}

#[tokio::test]
async fn lasting_access_needs_a_fresh_confirmation() {
    use opensign_core::services::passkeys;
    let Some(app) = app().await else { return };
    let admin = first_admin(&app).await;
    let key = || users::CreateApiKeyInput {
        name: "CRM".into(),
        permissions: vec![Permission::DocumentsCreate],
        expires_in_days: None,
        allowed_ips: vec![],
    };
    call::<users::CreateApiKey>(&admin, key()).await.expect("signing in just now counts");

    sqlx::query("update sessions set elevated_until = now() - interval '1 minute'").execute(&app.db).await.unwrap();
    let refused = call::<users::CreateApiKey>(&admin, key()).await.err().unwrap();
    assert_eq!(refused.code, Some("step_up_required"));

    let wrong = passkeys::StepUpInput { current_password: Some("wrong".into()), passkey: None };
    assert_eq!(kind(call::<passkeys::StepUp>(&admin, wrong).await), ErrorKind::BadRequest);
    let total: i32 = sqlx::query_scalar("select max(failures) from login_failures").fetch_one(&app.db).await.unwrap();
    assert_eq!(total, 1, "wrong guesses here count like those at the sign-in page");
    let nothing = passkeys::StepUpInput { current_password: None, passkey: None };
    assert!(call::<passkeys::StepUp>(&admin, nothing).await.is_err());
    assert!(call::<users::CreateApiKey>(&admin, key()).await.is_err());

    let right = passkeys::StepUpInput { current_password: Some(PASSWORD.into()), passkey: None };
    call::<passkeys::StepUp>(&admin, right).await.unwrap();
    call::<users::CreateApiKey>(&admin, key()).await.unwrap();

    let mut device = device_for(&app, 14);
    add_passkey(&admin, &device).await;
    sqlx::query("update sessions set elevated_until = null").execute(&app.db).await.unwrap();
    let options = call::<passkeys::BeginStepUp>(&admin, Empty {}).await.unwrap();
    assert!(options.has_password);
    let ceremony = options.passkey.unwrap();
    let confirm = assertion(&mut device, ceremony.state, &ceremony.options);
    call::<passkeys::StepUp>(&admin, passkeys::StepUpInput { current_password: None, passkey: Some(confirm) }).await.unwrap();
    call::<users::CreateApiKey>(&admin, key()).await.unwrap();
}

#[tokio::test]
async fn resets_end_api_keys_and_old_links() {
    let Some(app) = app().await else { return };
    let admin = first_admin(&app).await;
    let admin_id = admin.actor.as_ref().unwrap().user_id;
    let key = users::CreateApiKeyInput {
        name: "CRM".into(),
        permissions: vec![Permission::DocumentsCreate],
        expires_in_days: None,
        allowed_ips: vec![],
    };
    let created = call::<users::CreateApiKey>(&admin, key).await.unwrap();
    let live = |app: Arc<App>, key: String| async move {
        let mut conn = app.db.acquire().await.unwrap();
        load_api_key(&mut conn, &key, None, &SecuritySettings::default(), opensign_core::context::now()).await.unwrap().is_some()
    };
    assert!(live(app.clone(), created.key.clone()).await);

    for token in ["old-link", "newer-link"] {
        sqlx::query("insert into password_reset_tokens (id, user_id, expires_at) values ($1, $2, now() + interval '1 hour')")
            .bind(sha256_hex(token))
            .bind(admin_id)
            .execute(&app.db)
            .await
            .unwrap();
    }
    let anon = web(&app);
    let reset = |token: &str| auth::ResetPasswordInput { token: token.into(), password: "another long enough password 7".into() };
    call::<auth::ResetPassword>(&anon, reset("newer-link")).await.unwrap();
    assert!(!live(app.clone(), created.key).await, "a key made with a stolen session does not outlast the reset");
    assert!(call::<auth::ResetPassword>(&anon, reset("old-link")).await.is_err(), "the other link died with the change");
}

#[tokio::test]
async fn invites_end_with_the_inviter_and_admins_cannot_all_leave() {
    let Some(app) = app().await else { return };
    let admin = first_admin(&app).await;
    let bo = second_admin(&app, "bo@example.com").await;
    let bo_session = signed_in(&app, call::<auth::Login>(&web(&app), login("bo@example.com", PASSWORD)).await.unwrap()).await;
    // The invitation stands whether or not the mail goes out; this test knows no mail server.
    let _ = call::<users::Invite>(&bo_session, users::InviteInput { email: "new@example.com".into(), name: None, role: Role::Admin }).await;
    let invited = sqlx::query("update invites set token_hash = $1").bind(sha256_hex("invite-token")).execute(&app.db).await.unwrap();
    assert_eq!(invited.rows_affected(), 1);

    let demote = users::UpdateInput { id: bo, name: None, role: Some(Role::Sender), active: None };
    call::<users::Update>(&admin, demote).await.unwrap();
    let accept = users::AcceptInput { token: "invite-token".into(), name: "New".into(), password: PASSWORD.into() };
    assert!(call::<users::AcceptInvite>(&web(&app), accept).await.is_err(), "the demoted admin's invitation is gone");

    let promote = users::UpdateInput { id: bo, name: None, role: Some(Role::Admin), active: None };
    call::<users::Update>(&admin, promote).await.unwrap();
    let bo_session = signed_in(&app, call::<auth::Login>(&web(&app), login("bo@example.com", PASSWORD)).await.unwrap()).await;
    let ada = admin.actor.as_ref().unwrap().user_id;
    let (first, second) = tokio::join!(
        call::<users::Update>(&admin, users::UpdateInput { id: bo, name: None, role: Some(Role::Sender), active: None }),
        call::<users::Update>(&bo_session, users::UpdateInput { id: ada, name: None, role: Some(Role::Sender), active: None }),
    );
    assert!(first.is_ok() != second.is_ok(), "exactly one of two admins demoting each other goes through");
    let admins: i64 = sqlx::query_scalar("select count(*) from users where role = 'admin' and active").fetch_one(&app.db).await.unwrap();
    assert_eq!(admins, 1);
}

#[tokio::test]
async fn a_passkey_counter_that_stops_growing_is_refused() {
    use opensign_core::services::passkeys;
    let Some(app) = app().await else { return };
    let admin = first_admin(&app).await;
    let mut device = device_for(&app, 15);
    add_passkey(&admin, &device).await;
    let anon = web(&app);
    let answer = |device: &passkey::Authenticator, state: String, options: &serde_json::Value| {
        use base64::Engine;
        let (client, data, signature) = device.assert(options);
        passkeys::SignInInput {
            state,
            credential_id: base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&device.id),
            client_data_json: client,
            authenticator_data: data,
            signature,
            user_handle: None,
        }
    };

    let begin = call::<passkeys::BeginSignIn>(&anon, Empty {}).await.unwrap();
    let no_counter = answer(&device, begin.state, &begin.options);
    call::<passkeys::SignIn>(&anon, no_counter).await.expect("an authenticator without a counter reports 0");

    device.count = 5;
    let begin = call::<passkeys::BeginSignIn>(&anon, Empty {}).await.unwrap();
    let counting = answer(&device, begin.state, &begin.options);
    call::<passkeys::SignIn>(&anon, counting).await.unwrap();

    device.count = 0;
    let begin = call::<passkeys::BeginSignIn>(&anon, Empty {}).await.unwrap();
    let copied = answer(&device, begin.state, &begin.options);
    assert_eq!(kind(call::<passkeys::SignIn>(&anon, copied).await), ErrorKind::Unauthorized, "once counted, 0 is not higher");
}
