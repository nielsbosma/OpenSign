#![allow(clippy::unwrap_used)]

use std::sync::Arc;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use lopdf::{Document, Object, Stream, dictionary};
use opensign_core::context::{Actor, App, Credential, Ctx, Surface};
use opensign_core::documents::{AuthMethod, Delivery, FieldKind, PartyRole, mask_identity, open_identity, parties};
use opensign_core::error::ErrorKind;
use opensign_core::services::documents::{self as docs, FieldInput, PartyInput, Version};
use opensign_core::services::signing::{self, SignatureInput, sms_code_hash};
use opensign_core::settings::{SigningSettings, SmsSettings};
use opensign_core::testing::app;
use opensign_core::{Role, call, sms, worker};
use uuid::Uuid;

fn sample_pdf() -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let font = doc.add_object(dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica" });
    let content = doc.add_object(Stream::new(dictionary! {}, b"BT /F1 24 Tf 72 720 Td (Agreement) Tj ET".to_vec()));
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

fn image_png() -> Vec<u8> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, 64, 64);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(&[120; 64 * 64 * 3]).unwrap();
    writer.finish().unwrap();
    out
}

async fn sender(app: &Arc<App>) -> Ctx {
    let id = Uuid::now_v7();
    let email = format!("hardening-{id}@example.com");
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
    ctx.ip = Some("198.51.100.9".into());
    ctx
}

fn field(party_id: Uuid, kind: FieldKind, y: f64, required: bool) -> FieldInput {
    FieldInput {
        party_id: Some(party_id),
        kind,
        page: 0,
        x: 0.1,
        y,
        width: 0.3,
        height: 0.06,
        required: Some(required),
        label: None,
        value: None,
        options: vec![],
        validation: None,
        format: None,
        group_key: None,
        read_only: None,
        conditions: vec![],
        formula: None,
    }
}

/// A sent document with one party who gets SMS codes and may upload an image. Returns its id, the
/// party's id and their signing link token.
async fn sent(sender: &Ctx, delivery: Delivery, auth_to_sign: AuthMethod) -> (Uuid, Uuid, String) {
    let id =
        call::<docs::Create>(sender, serde_json::from_value(serde_json::json!({ "title": "Lease" })).unwrap()).await.unwrap().document.id;
    call::<docs::Upload>(
        sender,
        docs::UploadInput { document_id: id, name: "a.pdf".into(), content_base64: B64.encode(sample_pdf()), ..Default::default() },
    )
    .await
    .unwrap();
    let party = PartyInput {
        id: None,
        role: PartyRole::Signer,
        name: "Bo".into(),
        email: Some("bo@example.net".into()),
        phone: Some("+46701234567".into()),
        personal_number: None,
        company: None,
        sign_order: None,
        delivery,
        auth_to_view: AuthMethod::None,
        auth_to_sign,
        attachments: None,
        language: None,
        webcam_photo: None,
    };
    let saved = call::<docs::SetParties>(sender, docs::SetPartiesInput { document_id: id, parties: vec![party] }).await.unwrap();
    let party_id = saved[0].id;
    call::<docs::SetFields>(
        sender,
        docs::SetFieldsInput {
            document_id: id,
            fields: vec![field(party_id, FieldKind::Signature, 0.7, true), field(party_id, FieldKind::Image, 0.4, false)],
        },
    )
    .await
    .unwrap();
    call::<docs::Send>(sender, docs::SendInput { id, expires_at: None }).await.unwrap();
    let issued =
        call::<docs::IssueSigningLink>(sender, docs::SigningLinkInput { document_id: id, party_id, rotate: true, redirect_url: None })
            .await
            .unwrap();
    (id, party_id, issued.url.split_once('#').unwrap().1.to_string())
}

async fn known_code(app: &Arc<App>, signer: &Ctx, party: Uuid, token: &str) -> Uuid {
    call::<signing::RequestCode>(signer, signing::TokenInput { token: token.into(), session: None }).await.unwrap();
    let challenge: Uuid =
        sqlx::query_scalar("select id from sms_challenges where party_id = $1 and verified_at is null order by created_at desc limit 1")
            .bind(party)
            .fetch_one(&app.db)
            .await
            .unwrap();
    sqlx::query("update sms_challenges set code_hash = $2 where id = $1")
        .bind(challenge)
        .bind(sms_code_hash(signer, challenge, "424242"))
        .execute(&app.db)
        .await
        .unwrap();
    challenge
}

#[test]
fn identities_are_masked_to_their_last_digits() {
    assert_eq!(
        mask_identity("Anna Andersson, Swedish BankID, personal number 199001010017"),
        "Anna Andersson, Swedish BankID, personal number xxxxxxxx0017"
    );
    assert_eq!(
        mask_identity("Anna, Freja eID, personal number 19900101-0017 (TEST environment, not a real identification)"),
        "Anna, Freja eID, personal number xxxxxxxx-0017 (TEST environment, not a real identification)"
    );
    assert_eq!(mask_identity("SMS code to +46701234567"), "SMS code to +xxxxxxxx567");
    assert_eq!(mask_identity("Signed in to OpenSign as Sam <sam@example.com>"), "Signed in to OpenSign as Sam <sam@example.com>");
    let once = mask_identity("SMS code to +46701234567");
    assert_eq!(mask_identity(&once), once);
}

#[tokio::test]
async fn sms_attempts_stay_capped_when_tried_at_once() {
    let Some(app) = app().await else { return };
    let sender = sender(&app).await;
    let signer = signer_ctx(&app);
    let (_, party, token) = sent(&sender, Delivery::Link, AuthMethod::SmsPin).await;
    let challenge = known_code(&app, &signer, party, &token).await;
    sqlx::query("update sms_challenges set attempts = 4 where id = $1").bind(challenge).execute(&app.db).await.unwrap();
    let tries: Vec<_> = (0..12)
        .map(|_| {
            let (signer, token) = (signer.clone(), token.clone());
            tokio::spawn(
                async move { call::<signing::VerifyCode>(&signer, signing::VerifyCodeInput { token, code: "000000".into() }).await },
            )
        })
        .collect();
    for attempt in tries {
        assert!(attempt.await.unwrap().is_err());
    }
    let attempts: i32 =
        sqlx::query_scalar("select attempts from sms_challenges where id = $1").bind(challenge).fetch_one(&app.db).await.unwrap();
    assert_eq!(attempts, 5);
}

#[tokio::test]
async fn sms_budgets_are_spent_and_given_back() {
    let Some(app) = app().await else { return };
    let mut conn = app.db.acquire().await.unwrap();
    let now = chrono::Utc::now();
    let phone = "+46701234567";

    let link = Some(Uuid::now_v7());
    let per_link = SmsSettings { hourly_limit: 0, daily_limit: 0, share_link_hourly_limit: 1, ..SmsSettings::default() };
    let first = sms::reserve(&mut conn, &per_link, phone, link, now).await.unwrap();
    assert_eq!(sms::reserve(&mut conn, &per_link, phone, link, now).await.unwrap_err().kind, ErrorKind::TooManyRequests);
    sms::reserve(&mut conn, &per_link, phone, Some(Uuid::now_v7()), now).await.unwrap();
    sms::release(&mut conn, first).await.unwrap();
    sms::reserve(&mut conn, &per_link, phone, link, now).await.unwrap();

    let spent: i64 = sqlx::query_scalar("select count(*) from sms_log where sent_at > $1 - interval '1 hour'")
        .bind(now)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
    let hourly = SmsSettings { hourly_limit: spent as u32 + 1, daily_limit: 0, ..SmsSettings::default() };
    sms::reserve(&mut conn, &hourly, phone, None, now).await.unwrap();
    assert_eq!(sms::reserve(&mut conn, &hourly, phone, None, now).await.unwrap_err().kind, ErrorKind::TooManyRequests);

    let spent: i64 = sqlx::query_scalar("select count(*) from sms_log where sent_at > $1 - interval '1 day'")
        .bind(now)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
    let daily = SmsSettings { hourly_limit: 0, daily_limit: spent as u32, ..SmsSettings::default() };
    assert_eq!(sms::reserve(&mut conn, &daily, phone, None, now).await.unwrap_err().kind, ErrorKind::TooManyRequests);
}

#[tokio::test]
async fn codes_go_only_to_allowed_countries() {
    let Some(app) = app().await else { return };
    let sender = sender(&app).await;
    let signer = signer_ctx(&app);
    let (_, party, token) = sent(&sender, Delivery::Link, AuthMethod::SmsPin).await;
    {
        let mut conn = app.db.acquire().await.unwrap();
        let elsewhere = SmsSettings { allowed_countries: "47".into(), ..SmsSettings::default() };
        app.settings.save(&mut conn, &app.keys, &elsewhere, None).await.unwrap();
    }
    let refused = call::<signing::RequestCode>(&signer, signing::TokenInput { token: token.clone(), session: None }).await;
    {
        let mut conn = app.db.acquire().await.unwrap();
        app.settings.save(&mut conn, &app.keys, &SmsSettings::default(), None).await.unwrap();
    }
    assert_eq!(refused.err().unwrap().kind, ErrorKind::BadRequest);
    let challenges: i64 =
        sqlx::query_scalar("select count(*) from sms_challenges where party_id = $1").bind(party).fetch_one(&app.db).await.unwrap();
    assert_eq!(challenges, 0, "a refused code leaves nothing behind");
}

#[tokio::test]
async fn sealed_evidence_shows_masked_numbers_and_a_handed_out_link() {
    let Some(app) = app().await else { return };
    {
        let mut conn = app.db.acquire().await.unwrap();
        let no_tsa = SigningSettings { tsa_url: String::new(), ..SigningSettings::default() };
        app.settings.save(&mut conn, &app.keys, &no_tsa, None).await.unwrap();
    }
    let sender = sender(&app).await;
    let signer = signer_ctx(&app);
    let (id, party, token) = sent(&sender, Delivery::Link, AuthMethod::SmsPin).await;
    known_code(&app, &signer, party, &token).await;
    let session = call::<signing::VerifyCode>(&signer, signing::VerifyCodeInput { token: token.clone(), code: "424242".into() })
        .await
        .unwrap()
        .session;
    let carried: serde_json::Value = app.keys.verify_token("signer-session", &session, chrono::Utc::now().timestamp()).unwrap();
    assert!(!carried.to_string().contains("46701234567"), "the browser's token does not carry the number");

    call::<signing::Sign>(
        &signer,
        signing::SignInput {
            token,
            session: Some(session),
            values: vec![],
            signature: Some(SignatureInput { typed_name: Some("Bo".into()), image_png_base64: None }),
            consent: true,
        },
    )
    .await
    .unwrap();
    worker::drain(&app).await.unwrap();

    let mut conn = app.db.acquire().await.unwrap();
    let me = parties(&mut conn, id).await.unwrap().into_iter().find(|p| p.id == party).unwrap();
    assert_eq!(open_identity(&app.keys, &me).as_deref(), Some("SMS code to +xxxxxxxx567"));
    let stored = serde_json::to_string(me.evidence.as_ref().unwrap()).unwrap();
    assert!(!stored.contains("46701234567"), "the full number stays encrypted");
    assert!(me.evidence.as_ref().unwrap()["identityFull"].is_string());
    drop(conn);
    let reveal = || docs::PartyIdentityInput { document_id: id, party_id: party };
    assert!(call::<docs::PartyIdentity>(&sender, reveal()).await.is_err(), "the sender alone may not read the number");
    let mut admin = sender.actor().unwrap().clone();
    admin.role = Role::Admin;
    admin.permissions = Role::Admin.permissions();
    let admin = Ctx::new(app.clone(), Surface::Api).with_actor(admin);
    let shown = call::<docs::PartyIdentity>(&admin, reveal()).await.unwrap();
    assert!(shown.identity.unwrap().contains("46701234567"));
    let mut conn = app.db.acquire().await.unwrap();
    let audited: i64 =
        sqlx::query_scalar("select count(*) from audit_log where action = 'document.party_identity_read' and entity_id = $1")
            .bind(id.to_string())
            .fetch_one(&mut *conn)
            .await
            .unwrap();
    assert_eq!(audited, 1);

    let download = call::<docs::Download>(&sender, docs::DownloadInput { id, version: Some(Version::Signed) }).await.unwrap();
    let text = opensign_pdf::extract_text(&B64.decode(download.content_base64).unwrap()).unwrap().join("\n");
    assert!(text.contains("xxxxxxxx567") && !text.contains("46701234567"), "{text}");
    assert!(text.contains("handed out by"), "{text}");
}

#[tokio::test]
async fn replaced_uploads_are_destroyed_and_counted() {
    let Some(app) = app().await else { return };
    let sender = sender(&app).await;
    let signer = signer_ctx(&app);
    let (_, party, token) = sent(&sender, Delivery::Link, AuthMethod::Standard).await;
    let image: Uuid =
        sqlx::query_scalar("select id from fields where party_id = $1 and kind = 'image'").bind(party).fetch_one(&app.db).await.unwrap();

    let mut conn = app.db.acquire().await.unwrap();
    let first = signing::upload_field_file(&signer, &mut conn, &token, None, image, "a.png", image_png()).await.unwrap().file_id.unwrap();
    let (first_key,): (String,) =
        sqlx::query_as("select storage_key from files where id = $1").bind(first).fetch_one(&mut *conn).await.unwrap();
    let second = signing::upload_field_file(&signer, &mut conn, &token, None, image, "b.png", image_png()).await.unwrap().file_id.unwrap();
    assert_ne!(first, second);
    let left: i64 = sqlx::query_scalar("select count(*) from files where id = $1").bind(first).fetch_one(&mut *conn).await.unwrap();
    assert_eq!(left, 0, "the key to the replaced file is gone");
    let queued: i64 = sqlx::query_scalar("select count(*) from jobs where kind = 'delete_objects' and payload->'keys' ? $1")
        .bind(&first_key)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
    assert_eq!(queued, 1, "and the object is on its way out");

    sqlx::query("update fields set uploads = 20 where id = $1").bind(image).execute(&mut *conn).await.unwrap();
    let over = signing::upload_field_file(&signer, &mut conn, &token, None, image, "c.png", image_png()).await;
    assert_eq!(over.err().unwrap().kind, ErrorKind::TooManyRequests);
    let kept: Option<Uuid> =
        sqlx::query_scalar("select file_id from fields where id = $1").bind(image).fetch_one(&mut *conn).await.unwrap();
    assert_eq!(kept, Some(second));

    // A file an earlier version only marked goes on the next sweep, unless something still points at it.
    sqlx::query("update files set deleted_at = now() where id = $1").bind(second).execute(&mut *conn).await.unwrap();
    signing::sweep_retired_files(&mut conn).await.unwrap();
    let still: i64 = sqlx::query_scalar("select count(*) from files where id = $1").bind(second).fetch_one(&mut *conn).await.unwrap();
    assert_eq!(still, 1);
    sqlx::query("update fields set file_id = null where id = $1").bind(image).execute(&mut *conn).await.unwrap();
    signing::sweep_retired_files(&mut conn).await.unwrap();
    let gone: i64 = sqlx::query_scalar("select count(*) from files where id = $1").bind(second).fetch_one(&mut *conn).await.unwrap();
    assert_eq!(gone, 0);
}
