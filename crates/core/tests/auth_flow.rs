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
    let failures: i32 =
        sqlx::query_scalar("select failed_login_count from users where email = 'ada@example.com'").fetch_one(&app.db).await.unwrap();
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
