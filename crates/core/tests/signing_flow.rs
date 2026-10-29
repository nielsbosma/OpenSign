#![allow(clippy::unwrap_used)]

use std::sync::Arc;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use lopdf::{Document, Object, Stream, dictionary};
use opensign_core::audit::verify_chain;
use opensign_core::context::{Actor, App, Credential, Ctx, Surface};
use opensign_core::documents::attachments::AttachmentRequestInput;
use opensign_core::documents::logic::Condition;
use opensign_core::documents::{AuthMethod, Delivery, FieldKind, PartyRole, parties, signer_token};
use opensign_core::error::ErrorKind;
use opensign_core::services::documents::{self as docs, FieldInput, PartyInput, Version};
use opensign_core::services::signing::{self, FieldValue, SignatureInput, sms_code_hash};
use opensign_core::services::{branding, bulk};
use opensign_core::settings::SigningSettings;
use opensign_core::testing::app;
use opensign_core::{Role, call, worker};
use uuid::Uuid;

fn sample_pdf(pages: usize) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let font = doc.add_object(dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica" });
    let mut kids = Vec::new();
    for i in 0..pages {
        let content = doc.add_object(Stream::new(dictionary! {}, format!("BT /F1 24 Tf 72 720 Td (Page {}) Tj ET", i + 1).into_bytes()));
        kids.push(Object::Reference(doc.add_object(dictionary! {
            "Type" => "Page", "Parent" => pages_id, "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
            "Contents" => content, "Resources" => dictionary! { "Font" => dictionary! { "F1" => font } },
        })));
    }
    doc.objects.insert(pages_id, Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => kids, "Count" => pages as i64 }));
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog);
    let mut out = Vec::new();
    doc.save_to(&mut out).unwrap();
    out
}

fn logo_png() -> Vec<u8> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, 60, 20);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(&[90; 60 * 20 * 4]).unwrap();
    writer.finish().unwrap();
    out
}

async fn user(app: &Arc<App>, name: &str, role: Role) -> Ctx {
    let id = Uuid::now_v7();
    let email = format!("{}@example.com", name.to_lowercase().replace(' ', "."));
    sqlx::query("insert into users (id, email, name, role, password_hash) values ($1, $2, $3, $4, 'x')")
        .bind(id)
        .bind(&email)
        .bind(name)
        .bind(role.as_str())
        .execute(&app.db)
        .await
        .unwrap();
    let actor = Actor {
        user_id: id,
        email,
        name: name.into(),
        role,
        permissions: role.permissions(),
        must_enable_two_factor: false,
        credential: Credential::Session,
    };
    let mut ctx = Ctx::new(app.clone(), Surface::Api).with_actor(actor);
    ctx.ip = Some("192.0.2.10".into());
    ctx
}

fn kind<T>(result: opensign_core::Result<T>) -> ErrorKind {
    match result {
        Err(e) => e.kind,
        Ok(_) => panic!("expected an error"),
    }
}

fn signer_ctx(app: &Arc<App>) -> Ctx {
    let mut ctx = Ctx::new(app.clone(), Surface::Signer);
    ctx.ip = Some("198.51.100.7".into());
    ctx.user_agent = Some("test-browser".into());
    ctx
}

fn party(name: &str, role: PartyRole, delivery: Delivery, auth_to_sign: AuthMethod) -> PartyInput {
    PartyInput {
        id: None,
        role,
        name: name.into(),
        email: Some(format!("{}@example.net", name.to_lowercase())),
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
    }
}

fn field(party_id: Option<Uuid>, kind: FieldKind, page: i32, y: f64) -> FieldInput {
    FieldInput {
        party_id,
        kind,
        page,
        x: 0.1,
        y,
        width: 0.3,
        height: 0.06,
        required: None,
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

#[tokio::test]
async fn sends_signs_seals_and_verifies() {
    let Some(app) = app().await else { return };
    // TEST_LIVE_TSA=1 uses the default time stamp authority and its OCSP responder: PAdES B-LTA.
    let live = std::env::var("TEST_LIVE_TSA").is_ok();
    if !live {
        let mut conn = app.db.acquire().await.unwrap();
        let no_tsa = SigningSettings { tsa_url: String::new(), ..SigningSettings::default() };
        app.settings.save(&mut conn, &app.keys, &no_tsa, None).await.unwrap();
    }
    let sender = user(&app, "Sara Sender", Role::Sender).await;
    let outsider = user(&app, "Olle Other", Role::Sender).await;
    let admin = user(&app, "Adam Admin", Role::Admin).await;
    let logo = call::<branding::SetLogo>(&admin, branding::SetLogoInput { content_base64: B64.encode(logo_png()) }).await.unwrap();
    assert!(logo.logo_url.is_some());
    assert_eq!(
        kind(call::<branding::SetLogo>(&sender, branding::SetLogoInput { content_base64: B64.encode(logo_png()) }).await),
        ErrorKind::Forbidden
    );

    let document = call::<docs::Create>(
        &sender,
        serde_json::from_value(serde_json::json!({ "title": "Consulting Agreement", "message": "Please sign by Friday." })).unwrap(),
    )
    .await
    .unwrap();
    let id = document.document.id;
    let update =
        |external_id: &str| serde_json::from_value::<docs::UpdateInput>(serde_json::json!({ "id": id, "externalId": external_id }));
    let tagged = call::<docs::Update>(&sender, update("CRM-1").unwrap()).await.unwrap();
    assert_eq!(tagged.document.external_id.as_deref(), Some("CRM-1"));
    let cleared = call::<docs::Update>(&sender, update("").unwrap()).await.unwrap();
    assert_eq!(cleared.document.external_id, None);

    let first = call::<docs::Upload>(
        &sender,
        docs::UploadInput { document_id: id, name: "agreement.pdf".into(), content_base64: B64.encode(sample_pdf(2)) },
    )
    .await
    .unwrap();
    assert_eq!(first.pages, 2);
    if app.env.converter_url.is_some() {
        let docx = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/agreement.docx")).unwrap();
        let converted = call::<docs::Upload>(
            &sender,
            docs::UploadInput { document_id: id, name: "appendix.docx".into(), content_base64: B64.encode(docx) },
        )
        .await
        .unwrap();
        assert_eq!(converted.file.name, "appendix.pdf");
        assert!(converted.pages >= 1);
    }
    let rejected = call::<docs::Upload>(
        &sender,
        docs::UploadInput { document_id: id, name: "invoice.pdf".into(), content_base64: B64.encode(b"MZ\x90\x00 not really a pdf") },
    )
    .await;
    assert_eq!(kind(rejected), ErrorKind::BadRequest);

    let set = call::<docs::SetParties>(
        &sender,
        docs::SetPartiesInput {
            document_id: id,
            parties: vec![
                PartyInput {
                    attachments: Some(vec![
                        AttachmentRequestInput {
                            name: "Copy of ID".into(),
                            description: "Passport or driving licence".into(),
                            required: None,
                        },
                        AttachmentRequestInput { name: "Other".into(), description: String::new(), required: Some(false) },
                    ]),
                    ..party("Anna", PartyRole::Signer, Delivery::Email, AuthMethod::Standard)
                },
                PartyInput { language: Some("ar-SA".into()), ..party("Bo", PartyRole::Signer, Delivery::Link, AuthMethod::SmsPin) },
                party("Cecilia", PartyRole::Viewer, Delivery::Email, AuthMethod::Standard),
            ],
        },
    )
    .await
    .unwrap();
    let (anna, bo, cecilia) = (set[0].id, set[1].id, set[2].id);
    assert_eq!(set[1].language.as_deref(), Some("ar"));
    assert_eq!(set[0].language, None);
    let unknown = call::<docs::SetParties>(
        &sender,
        docs::SetPartiesInput {
            document_id: id,
            parties: vec![PartyInput {
                language: Some("xx".into()),
                ..party("Anna", PartyRole::Signer, Delivery::Email, AuthMethod::Standard)
            }],
        },
    )
    .await;
    assert_eq!(kind(unknown), ErrorKind::BadRequest);

    let with_number = |id: Uuid, number: Option<&str>| PartyInput {
        id: Some(id),
        personal_number: number.map(str::to_string),
        ..party("Anna", PartyRole::Signer, Delivery::Email, AuthMethod::Standard)
    };
    let resave = |anna: PartyInput| docs::SetPartiesInput {
        document_id: id,
        parties: vec![
            anna,
            PartyInput { id: Some(bo), language: Some("ar".into()), ..party("Bo", PartyRole::Signer, Delivery::Link, AuthMethod::SmsPin) },
            PartyInput { id: Some(cecilia), ..party("Cecilia", PartyRole::Viewer, Delivery::Email, AuthMethod::Standard) },
        ],
    };
    let stored = call::<docs::SetParties>(&sender, resave(with_number(anna, Some("19121212-1212")))).await.unwrap();
    let masked = stored[0].personal_number.clone().unwrap();
    assert!(masked.ends_with("1212") && !masked.contains("1912"));
    let kept = call::<docs::SetParties>(&sender, resave(with_number(anna, None))).await.unwrap();
    assert_eq!(kept[0].personal_number.as_deref(), Some(masked.as_str()), "leaving it out keeps the stored number");
    let kept = call::<docs::SetParties>(&sender, resave(with_number(anna, Some(&masked)))).await.unwrap();
    assert_eq!(kept[0].personal_number.as_deref(), Some(masked.as_str()), "sending the masked value back keeps it");
    let cleared = call::<docs::SetParties>(&sender, resave(with_number(anna, Some("")))).await.unwrap();
    assert!(cleared[0].personal_number.is_none());

    let mut company = field(Some(bo), FieldKind::Text, 1, 0.3);
    company.label = Some("Bank account".into());
    call::<docs::SetFields>(
        &sender,
        docs::SetFieldsInput {
            document_id: id,
            fields: vec![
                field(Some(anna), FieldKind::Signature, 0, 0.7),
                field(Some(anna), FieldKind::Date, 0, 0.8),
                field(Some(bo), FieldKind::Signature, 1, 0.7),
                company,
                FieldInput { required: Some(true), ..field(Some(bo), FieldKind::Checkbox, 1, 0.5) },
            ],
        },
    )
    .await
    .unwrap();

    let not_found = call::<docs::Get>(&outsider, docs::IdInput { id }).await.unwrap_err();
    assert_eq!(not_found.kind, ErrorKind::NotFound, "other senders cannot see the document");

    let audited = |id: Uuid| {
        let db = app.db.clone();
        async move {
            sqlx::query_scalar::<_, i64>("select count(*) from audit_log where entity_id = $1")
                .bind(id.to_string())
                .fetch_one(&db)
                .await
                .unwrap()
        }
    };
    let before = audited(id).await;
    let preview = call::<docs::SigningPreview>(&sender, docs::SigningPreviewInput { document_id: id, party_id: bo }).await.unwrap();
    assert_eq!(preview.party.name, "Bo");
    assert_eq!(preview.sign_authentication, Some(AuthMethod::SmsPin));
    assert!(preview.needs_authentication.is_none() && preview.your_turn);
    assert_eq!(preview.fields.iter().filter(|f| f.party_id == Some(bo)).count(), 3);
    assert!(preview.fields.iter().all(|f| f.party_id == Some(bo) || f.party_id.is_none()), "only the party's own fields");
    assert_eq!(audited(id).await, before, "a preview is not recorded");
    let hidden = call::<docs::SigningPreview>(&outsider, docs::SigningPreviewInput { document_id: id, party_id: bo }).await;
    assert_eq!(kind(hidden), ErrorKind::NotFound);

    let sent = call::<docs::Send>(&sender, docs::SendInput { id, expires_at: None }).await.unwrap();
    assert_eq!(sent.document.document.status, "pending");
    assert_eq!(sent.links.len(), 1, "only Bo has link delivery");
    worker::drain(&app).await.unwrap();
    let notified: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("select notified_at from parties where id = $1").bind(anna).fetch_one(&app.db).await.unwrap();
    assert!(notified.is_some(), "Anna was emailed her link");
    let changed =
        call::<docs::Update>(&sender, serde_json::from_value(serde_json::json!({ "id": id, "title": "Changed after sending" })).unwrap())
            .await;
    assert_eq!(kind(changed), ErrorKind::Conflict);

    let (anna_token, bo_token) = {
        let mut conn = app.db.acquire().await.unwrap();
        let all = parties(&mut conn, id).await.unwrap();
        let find = |pid| all.iter().find(|p| p.id == pid).unwrap();
        (signer_token(&app.keys, find(anna)), signer_token(&app.keys, find(bo)))
    };
    assert!(sent.links[0].url.ends_with(&bo_token));

    let signer = signer_ctx(&app);
    let forged = format!("{}.AAAA", anna.simple());
    assert!(call::<signing::Open>(&signer, signing::OpenInput { token: forged, session: None, redirect_url: None }).await.is_err());

    let anna_view =
        call::<signing::Open>(&signer, signing::OpenInput { token: anna_token.clone(), session: None, redirect_url: None }).await.unwrap();
    assert_eq!(anna_view.document.language, "en");
    assert!(anna_view.your_turn && anna_view.needs_authentication.is_none());
    assert_eq!(anna_view.fields.len(), 2);
    let unsigned = call::<signing::Sign>(
        &signer,
        signing::SignInput { token: anna_token.clone(), session: None, values: vec![], signature: None, consent: true },
    )
    .await;
    assert_eq!(kind(unsigned), ErrorKind::BadRequest, "a signature is required");
    assert_eq!(anna_view.attachments.len(), 2, "requests survive saving the parties again without them");
    let id_request = anna_view.attachments[0].id;
    let typed = || Some(SignatureInput { typed_name: Some("Anna Andersson".into()), image_png_base64: None });
    let missing = call::<signing::Sign>(
        &signer,
        signing::SignInput { token: anna_token.clone(), session: None, values: vec![], signature: typed(), consent: true },
    )
    .await;
    assert_eq!(kind(missing), ErrorKind::BadRequest, "the required attachment is missing");
    {
        let mut tx = app.db.begin().await.unwrap();
        let bad = signing::upload_attachment(&signer, &mut tx, &anna_token, None, id_request, "id.exe", b"MZ\x90".to_vec()).await;
        assert_eq!(kind(bad), ErrorKind::BadRequest);
        let other = signing::upload_attachment(&signer, &mut tx, &bo_token, None, id_request, "id.pdf", sample_pdf(1)).await;
        assert!(other.is_err(), "Bo cannot upload to Anna's request");
        let uploaded =
            signing::upload_attachment(&signer, &mut tx, &anna_token, None, id_request, "C:\\scans\\id.pdf", sample_pdf(1)).await.unwrap();
        assert_eq!(uploaded.file_name.as_deref(), Some("id.pdf"));
        tx.commit().await.unwrap();
    }
    let signed = call::<signing::Sign>(
        &signer,
        signing::SignInput {
            token: anna_token.clone(),
            session: None,
            values: vec![],
            signature: Some(SignatureInput { typed_name: Some("Anna Andersson".into()), image_png_base64: None }),
            consent: true,
        },
    )
    .await
    .unwrap();
    assert!(!signed.completed);
    let again = call::<signing::Sign>(
        &signer,
        signing::SignInput {
            token: anna_token.clone(),
            session: None,
            values: vec![],
            signature: Some(SignatureInput { typed_name: Some("A".into()), image_png_base64: None }),
            consent: true,
        },
    )
    .await;
    assert_eq!(kind(again), ErrorKind::Conflict);

    let bo_view =
        call::<signing::Open>(&signer, signing::OpenInput { token: bo_token.clone(), session: None, redirect_url: None }).await.unwrap();
    assert_eq!(bo_view.document.language, "ar", "a party's own language wins over the document's");
    assert_eq!(bo_view.sign_authentication, Some(AuthMethod::SmsPin));
    let without_code = call::<signing::Sign>(
        &signer,
        signing::SignInput {
            token: bo_token.clone(),
            session: None,
            values: vec![],
            signature: Some(SignatureInput { typed_name: Some("Bo".into()), image_png_base64: None }),
            consent: true,
        },
    )
    .await;
    assert_eq!(kind(without_code), ErrorKind::Unauthorized);

    call::<signing::RequestCode>(&signer, signing::TokenInput { token: bo_token.clone(), session: None }).await.unwrap();
    let challenge: Uuid =
        sqlx::query_scalar("select id from sms_challenges where party_id = $1 and verified_at is null order by created_at desc limit 1")
            .bind(bo)
            .fetch_one(&app.db)
            .await
            .unwrap();
    sqlx::query("update sms_challenges set code_hash = $2 where id = $1")
        .bind(challenge)
        .bind(sms_code_hash(&signer, challenge, "424242"))
        .execute(&app.db)
        .await
        .unwrap();
    let wrong = call::<signing::VerifyCode>(&signer, signing::VerifyCodeInput { token: bo_token.clone(), code: "000000".into() }).await;
    assert!(wrong.is_err());
    let attempts: i32 =
        sqlx::query_scalar("select attempts from sms_challenges where id = $1").bind(challenge).fetch_one(&app.db).await.unwrap();
    assert_eq!(attempts, 1, "failed attempts are counted even though the call fails");
    let session = call::<signing::VerifyCode>(&signer, signing::VerifyCodeInput { token: bo_token.clone(), code: "424 242".into() })
        .await
        .unwrap()
        .session;

    let bo_fields =
        call::<signing::Open>(&signer, signing::OpenInput { token: bo_token.clone(), session: Some(session.clone()), redirect_url: None })
            .await
            .unwrap()
            .fields;
    let text_field = bo_fields.iter().find(|f| f.kind == "text").unwrap().id;
    let checkbox = bo_fields.iter().find(|f| f.kind == "checkbox").unwrap().id;
    let missing_checkbox = call::<signing::Sign>(
        &signer,
        signing::SignInput {
            token: bo_token.clone(),
            session: Some(session.clone()),
            values: vec![FieldValue { field_id: text_field, value: "SE45 5000 0000 0583 9825 7466".into() }],
            signature: Some(SignatureInput { typed_name: Some("Bo Berg".into()), image_png_base64: None }),
            consent: true,
        },
    )
    .await;
    assert_eq!(kind(missing_checkbox), ErrorKind::BadRequest);
    let done = call::<signing::Sign>(
        &signer,
        signing::SignInput {
            token: bo_token.clone(),
            session: Some(session),
            values: vec![
                FieldValue { field_id: text_field, value: "SE45 5000 0000 0583 9825 7466".into() },
                FieldValue { field_id: checkbox, value: "true".into() },
            ],
            signature: Some(SignatureInput { typed_name: Some("Bo Berg".into()), image_png_base64: None }),
            consent: true,
        },
    )
    .await
    .unwrap();
    assert!(done.completed);

    let ran = worker::drain(&app).await.unwrap();
    assert!(ran >= 2, "notification and sealing jobs ran ({ran})");
    let failed: Vec<(String, Option<String>)> =
        sqlx::query_as("select kind, last_error from jobs where done_at is null and kind <> 'sweep'").fetch_all(&app.db).await.unwrap();
    assert!(failed.is_empty(), "jobs left over: {failed:?}");
    let emails: Vec<String> =
        sqlx::query_scalar("select payload->>'to' from jobs where kind = 'notify_completed' and payload ? 'to' order by 1")
            .fetch_all(&app.db)
            .await
            .unwrap();
    assert!(emails.len() >= 2 && emails.iter().all(|to| to == "owner" || Uuid::try_parse(to).is_ok()), "one email job each: {emails:?}");

    let finished = call::<docs::Get>(&sender, docs::IdInput { id }).await.unwrap();
    assert_eq!(finished.document.status, "completed");
    assert_eq!(finished.sealed_with_timestamp, Some(false), "no time stamp authority in tests");
    assert!(finished.parties.iter().filter(|p| p.id != cecilia).all(|p| p.status == "signed"));

    let download = call::<docs::Download>(&sender, docs::DownloadInput { id, version: Some(Version::Signed) }).await.unwrap();
    let sealed = B64.decode(download.content_base64).unwrap();
    // CI checks this file with independent validators (poppler's pdfsig, qpdf).
    let embedded = Document::load_mem(&sealed).unwrap();
    let names = embedded.catalog().unwrap().get(b"Names").unwrap().as_dict().unwrap();
    let tree = names.get(b"EmbeddedFiles").unwrap().as_dict().unwrap().get(b"Names").unwrap().as_array().unwrap();
    assert_eq!(tree[0].as_str().unwrap(), b"attachment-01-id.pdf", "Anna's attachment travels inside the sealed PDF");
    let has_logo = embedded.get_pages().values().any(|page| {
        embedded
            .get_dictionary(*page)
            .and_then(|p| p.get(b"Resources"))
            .and_then(Object::as_dict)
            .and_then(|r| r.get(b"XObject"))
            .and_then(Object::as_dict)
            .is_ok_and(|x| x.has(b"Logo"))
    });
    assert!(has_logo, "the evidence page carries the organisation's logo");
    if let Ok(path) = std::env::var("SEALED_PDF_OUT") {
        std::fs::write(path, &sealed).unwrap();
    }
    let mut conn = app.db.acquire().await.unwrap();
    let verified = docs::verify_pdf(&mut conn, sealed.clone()).await.unwrap();
    assert_eq!(verified.document.map(|d| d.id), Some(id));
    assert_eq!(verified.signatures.len(), if live { 2 } else { 1 }, "{:?}", verified.signatures);
    assert!(verified.signatures.iter().all(|s| s.is_intact() && s.problems.is_empty()), "{:?}", verified.signatures);

    let mut tampered = sealed;
    let n = tampered.len();
    tampered[n / 3] ^= 0x20;
    if let Ok(result) = docs::verify_pdf(&mut conn, tampered).await {
        assert!(result.document.is_none());
        assert!(result.signatures.iter().all(|s| !s.is_intact()));
    }

    let after = call::<signing::Open>(&signer, signing::OpenInput { token: anna_token, session: None, redirect_url: None }).await.unwrap();
    assert!(after.can_download);
    let chain = verify_chain(&mut conn, &app.keys).await.unwrap();
    assert!(chain.valid, "{chain:?}");
    let actions: Vec<String> = sqlx::query_scalar("select action from audit_log where entity_id = $1 order by id")
        .bind(id.to_string())
        .fetch_all(&mut *conn)
        .await
        .unwrap();
    for expected in
        ["document.created", "document.sent", "party.notified", "party.viewed", "party.signed", "party.authenticated", "document.completed"]
    {
        assert!(actions.iter().any(|a| a == expected), "{expected} missing from {actions:?}");
    }
    let protect = sqlx::query("update documents set sealed_sha256 = 'x' where id = $1").bind(id).execute(&mut *conn).await;
    assert!(protect.is_err(), "a completed document cannot be changed");
}

#[tokio::test]
async fn cancel_revokes_links_and_decline_stops_signing() {
    let Some(app) = app().await else { return };
    let sender = user(&app, "Sam Sender", Role::Sender).await;
    let create = |title: &str| serde_json::from_value::<docs::CreateInput>(serde_json::json!({ "title": title })).unwrap();
    let mut ids = Vec::new();
    for title in ["To cancel", "To decline"] {
        let id = call::<docs::Create>(&sender, create(title)).await.unwrap().document.id;
        call::<docs::Upload>(
            &sender,
            docs::UploadInput { document_id: id, name: "a.pdf".into(), content_base64: B64.encode(sample_pdf(1)) },
        )
        .await
        .unwrap();
        call::<docs::SetParties>(
            &sender,
            docs::SetPartiesInput {
                document_id: id,
                parties: vec![
                    party("Dan", PartyRole::Signer, Delivery::Link, AuthMethod::Standard),
                    party("Eva", PartyRole::Signer, Delivery::Link, AuthMethod::Standard),
                ],
            },
        )
        .await
        .unwrap();
        call::<docs::Send>(&sender, docs::SendInput { id, expires_at: None }).await.unwrap();
        ids.push(id);
    }
    let signer = signer_ctx(&app);
    let token_for = |id: Uuid, index: usize| {
        let app = app.clone();
        async move {
            let mut conn = app.db.acquire().await.unwrap();
            signer_token(&app.keys, &parties(&mut conn, id).await.unwrap()[index])
        }
    };

    let dan = token_for(ids[0], 0).await;
    call::<signing::Open>(&signer, signing::OpenInput { token: dan.clone(), session: None, redirect_url: None }).await.unwrap();
    call::<docs::Cancel>(&sender, docs::CancelInput { id: ids[0], reason: Some("Wrong version".into()) }).await.unwrap();
    let revoked = call::<signing::Open>(&signer, signing::OpenInput { token: dan, session: None, redirect_url: None }).await;
    assert_eq!(kind(revoked), ErrorKind::NotFound, "cancel bumps the link version");

    let eva = token_for(ids[1], 1).await;
    call::<signing::Decline>(&signer, signing::DeclineInput { token: eva, session: None, reason: Some("Price is wrong".into()) })
        .await
        .unwrap();
    let dan2 = token_for(ids[1], 0).await;
    let stopped = call::<signing::Sign>(
        &signer,
        signing::SignInput {
            token: dan2,
            session: None,
            values: vec![],
            signature: Some(SignatureInput { typed_name: Some("Dan".into()), image_png_base64: None }),
            consent: true,
        },
    )
    .await;
    assert_eq!(kind(stopped), ErrorKind::Conflict);
    let status: String = sqlx::query_scalar("select status from documents where id = $1").bind(ids[1]).fetch_one(&app.db).await.unwrap();
    assert_eq!(status, "declined");
}

#[tokio::test]
async fn in_person_signing_issues_a_fresh_link_for_the_party_in_turn() {
    let Some(app) = app().await else { return };
    let sender = user(&app, "Sam Sender", Role::Sender).await;
    let create = serde_json::from_value::<docs::CreateInput>(serde_json::json!({ "title": "At the desk", "sequential": true })).unwrap();
    let id = call::<docs::Create>(&sender, create).await.unwrap().document.id;
    call::<docs::Upload>(&sender, docs::UploadInput { document_id: id, name: "a.pdf".into(), content_base64: B64.encode(sample_pdf(1)) })
        .await
        .unwrap();
    let mut second = party("Gus", PartyRole::Signer, Delivery::InPerson, AuthMethod::Standard);
    second.sign_order = Some(2);
    let saved = call::<docs::SetParties>(
        &sender,
        docs::SetPartiesInput {
            document_id: id,
            parties: vec![party("Fia", PartyRole::Signer, Delivery::InPerson, AuthMethod::Standard), second],
        },
    )
    .await
    .unwrap();
    let (fia, gus) = (saved[0].id, saved[1].id);
    call::<docs::Send>(&sender, docs::SendInput { id, expires_at: None }).await.unwrap();

    let not_yet = call::<docs::SignInPerson>(&sender, docs::InPersonInput { document_id: id, party_id: gus }).await;
    assert_eq!(kind(not_yet), ErrorKind::Conflict, "Gus signs after Fia");
    let first = call::<docs::SignInPerson>(&sender, docs::InPersonInput { document_id: id, party_id: fia }).await.unwrap();
    let again = call::<docs::SignInPerson>(&sender, docs::InPersonInput { document_id: id, party_id: fia }).await.unwrap();
    let token = |url: &str| url.split_once('#').unwrap().1.to_string();
    let signer = signer_ctx(&app);
    let stale = call::<signing::Open>(&signer, signing::OpenInput { token: token(&first.url), session: None, redirect_url: None }).await;
    assert_eq!(kind(stale), ErrorKind::NotFound, "starting again retires the earlier link");
    let view =
        call::<signing::Open>(&signer, signing::OpenInput { token: token(&again.url), session: None, redirect_url: None }).await.unwrap();
    assert!(view.party.in_person && view.your_turn);

    let outsider = user(&app, "Val Viewer", Role::Viewer).await;
    let refused = call::<docs::SignInPerson>(&outsider, docs::InPersonInput { document_id: id, party_id: fia }).await;
    assert!(refused.is_err());
}

#[tokio::test]
async fn bulk_send_creates_one_document_per_row() {
    let Some(app) = app().await else { return };
    let sender = user(&app, "Bea Bulk", Role::Manager).await;
    let create = serde_json::from_value::<docs::CreateInput>(serde_json::json!({ "title": "Lease", "isTemplate": true })).unwrap();
    let template = call::<docs::Create>(&sender, create).await.unwrap().document.id;
    call::<docs::Upload>(
        &sender,
        docs::UploadInput { document_id: template, name: "lease.pdf".into(), content_base64: B64.encode(sample_pdf(1)) },
    )
    .await
    .unwrap();
    let tenant = call::<docs::SetParties>(
        &sender,
        docs::SetPartiesInput {
            document_id: template,
            parties: vec![party("Tenant", PartyRole::Signer, Delivery::Email, AuthMethod::Standard)],
        },
    )
    .await
    .unwrap()[0]
        .id;
    let mut address = field(None, FieldKind::Text, 0, 0.2);
    address.label = Some("Address".into());
    call::<docs::SetFields>(
        &sender,
        docs::SetFieldsInput { document_id: template, fields: vec![field(Some(tenant), FieldKind::Signature, 0, 0.7), address] },
    )
    .await
    .unwrap();

    let csv = "Namn;E-post;Address;Kundnummer\nGöran Åberg;goran@example.test;Storgatan 1;K-1\nHelga Ek;not-an-email;Lillgatan 2;K-2\n";
    let file = |csv: &str| bulk::FileInput { template_id: template, file_name: "tenants.csv".into(), content_base64: B64.encode(csv) };
    let preview = call::<bulk::Preview>(&sender, bulk::PreviewInput { file: file(csv), use_ai: false }).await.unwrap();
    let targets: Vec<&str> = preview.mapping.iter().map(|m| m.target.as_str()).collect();
    assert_eq!(targets, ["party.0.name", "party.0.email", "field.Address", "externalId"]);
    assert_eq!(preview.row_count, 2);
    assert_eq!(preview.problems.len(), 1, "Helga's email is caught before anything is sent");
    assert_eq!(preview.problems[0].row, 3);

    let refused =
        call::<bulk::Start>(&sender, bulk::StartInput { file: file(csv), mapping: preview.mapping.clone(), title_pattern: None }).await;
    assert_eq!(kind(refused), ErrorKind::BadRequest);

    let fixed = csv.replace("not-an-email", "helga@example.test");
    let started = call::<bulk::Start>(
        &sender,
        bulk::StartInput { file: file(&fixed), mapping: preview.mapping, title_pattern: Some("Lease {Address}".into()) },
    )
    .await
    .unwrap();
    assert_eq!((started.total, started.status.as_str()), (2, "queued"));
    worker::drain(&app).await.unwrap();

    let done = call::<bulk::Get>(&sender, bulk::BulkIdInput { id: started.id }).await.unwrap();
    assert_eq!(
        (done.status.as_str(), done.sent, done.failed),
        ("done", 2, 0),
        "{:?}",
        done.rows.iter().map(|r| &r.error).collect::<Vec<_>>()
    );
    let first = call::<docs::Get>(&sender, docs::IdInput { id: done.rows[0].document_id.unwrap() }).await.unwrap();
    assert_eq!(first.document.title, "Lease Storgatan 1");
    assert_eq!(first.document.status, "pending");
    assert_eq!(first.document.external_id.as_deref(), Some("K-1"));
    assert_eq!(first.parties[0].email.as_deref(), Some("goran@example.test"));
    assert_eq!(first.fields.iter().find(|f| f.label == "Address").and_then(|f| f.value.as_deref()), Some("Storgatan 1"));
    let leftover: i64 = sqlx::query_scalar("select count(*) from bulk_rows where data is not null").fetch_one(&app.db).await.unwrap();
    assert_eq!(leftover, 0, "row data is dropped once a row is handled");

    let other = user(&app, "Olle Other", Role::Sender).await;
    assert!(call::<bulk::Get>(&other, bulk::BulkIdInput { id: started.id }).await.is_err());
}

#[tokio::test]
async fn retention_deletes_due_documents_but_never_held_ones() {
    use opensign_core::services::retention;
    use opensign_core::settings::RetentionSettings;
    let Some(app) = app().await else { return };
    let manager = user(&app, "Rita Retention", Role::Manager).await;
    let sender = user(&app, "Sam Sender", Role::Sender).await;
    let draft = |title: &'static str| {
        let manager = &manager;
        async move {
            let create = serde_json::from_value::<docs::CreateInput>(serde_json::json!({ "title": title })).unwrap();
            let id = call::<docs::Create>(manager, create).await.unwrap().document.id;
            call::<docs::Upload>(
                manager,
                docs::UploadInput { document_id: id, name: "a.pdf".into(), content_base64: B64.encode(sample_pdf(1)) },
            )
            .await
            .unwrap();
            sqlx::query("update documents set updated_at = now() - interval '40 days' where id = $1")
                .bind(id)
                .execute(&manager.app.db)
                .await
                .unwrap();
            id
        }
    };
    let stale = draft("Stale Draft").await;
    let held = draft("Disputed Draft").await;
    let file_of = |id: Uuid| {
        let db = app.db.clone();
        async move {
            sqlx::query_scalar::<_, Uuid>("select file_id from document_files where document_id = $1")
                .bind(id)
                .fetch_one(&db)
                .await
                .unwrap()
        }
    };
    let stale_file = file_of(stale).await;
    let stale_key: String =
        sqlx::query_scalar("select storage_key from files where id = $1").bind(stale_file).fetch_one(&app.db).await.unwrap();

    let hold = |id: Uuid, hold: bool, reason: &str| retention::LegalHoldInput { id, hold, reason: reason.into() };
    assert_eq!(kind(call::<retention::SetLegalHold>(&sender, hold(held, true, "Case 12")).await), ErrorKind::Forbidden);
    assert_eq!(kind(call::<retention::SetLegalHold>(&manager, hold(held, true, " ")).await), ErrorKind::BadRequest, "a reason is required");
    let placed = call::<retention::SetLegalHold>(&manager, hold(held, true, "Case 12")).await.unwrap();
    assert!(placed.legal_hold);
    assert_eq!(kind(call::<docs::Delete>(&manager, docs::IdInput { id: held }).await), ErrorKind::Conflict);

    let mut conn = app.db.acquire().await.unwrap();
    app.settings.save(&mut conn, &app.keys, &RetentionSettings { draft_days: 30, ..Default::default() }, None).await.unwrap();
    let preview = call::<retention::Preview>(&manager, Default::default()).await;
    assert_eq!(kind(preview), ErrorKind::Forbidden, "only admins see retention");
    let deleted = retention::run(&manager, &mut conn).await.unwrap();
    assert!(deleted >= 1);
    let exists = |id: Uuid| sqlx::query_scalar::<_, bool>("select exists (select 1 from documents where id = $1)").bind(id);
    assert!(!exists(stale).fetch_one(&mut *conn).await.unwrap(), "the stale draft is gone");
    assert!(exists(held).fetch_one(&mut *conn).await.unwrap(), "the held draft stays");
    let file_left: bool =
        sqlx::query_scalar("select exists (select 1 from files where id = $1)").bind(stale_file).fetch_one(&mut *conn).await.unwrap();
    assert!(!file_left, "its file row and wrapped key are gone");
    let cleanup: i64 =
        sqlx::query_scalar("select count(*) from jobs where kind = 'delete_objects' and payload::text like '%' || $1 || '%'")
            .bind(&stale_key)
            .fetch_one(&mut *conn)
            .await
            .unwrap();
    assert_eq!(cleanup, 1, "a job removes the encrypted body from the bucket");
    let reason: String = sqlx::query_scalar(
        "select metadata->>'reason' from audit_log where action = 'document.deleted' and entity_id = $1::text order by id desc limit 1",
    )
    .bind(stale)
    .fetch_one(&mut *conn)
    .await
    .unwrap();
    assert_eq!(reason, "retention");

    let view = call::<docs::Get>(&manager, docs::IdInput { id: held }).await.unwrap();
    assert!(view.deletes_at.is_none(), "a held document has no deletion date");
    call::<retention::SetLegalHold>(&manager, hold(held, false, "")).await.unwrap();
    let view = call::<docs::Get>(&manager, docs::IdInput { id: held }).await.unwrap();
    assert!(view.deletes_at.is_some());
    call::<docs::Delete>(&manager, docs::IdInput { id: held }).await.unwrap();
}

fn camera_png(width: u32, height: u32) -> Vec<u8> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, width, height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(&vec![120; (width * height * 3) as usize]).unwrap();
    writer.finish().unwrap();
    out
}

#[tokio::test]
async fn webcam_photo_is_required_taken_and_sealed() {
    let Some(app) = app().await else { return };
    {
        let mut conn = app.db.acquire().await.unwrap();
        let no_tsa = SigningSettings { tsa_url: String::new(), ..SigningSettings::default() };
        app.settings.save(&mut conn, &app.keys, &no_tsa, None).await.unwrap();
    }
    let sender = user(&app, "Pia Photo", Role::Sender).await;
    let create = serde_json::from_value::<docs::CreateInput>(serde_json::json!({ "title": "Show your face" })).unwrap();
    let id = call::<docs::Create>(&sender, create).await.unwrap().document.id;
    call::<docs::Upload>(&sender, docs::UploadInput { document_id: id, name: "a.pdf".into(), content_base64: B64.encode(sample_pdf(1)) })
        .await
        .unwrap();
    let anna = PartyInput { webcam_photo: Some(true), ..party("Anna", PartyRole::Signer, Delivery::Link, AuthMethod::Standard) };
    let copy = PartyInput { webcam_photo: Some(true), ..party("Cia", PartyRole::Viewer, Delivery::Link, AuthMethod::Standard) };
    let saved = call::<docs::SetParties>(&sender, docs::SetPartiesInput { document_id: id, parties: vec![anna, copy] }).await.unwrap();
    assert!(saved[0].webcam_photo && !saved[1].webcam_photo, "viewers sign nothing, so they take no photo");
    let anna_id = saved[0].id;

    let resave = PartyInput {
        id: Some(anna_id),
        attachments: Some(vec![]),
        ..party("Anna", PartyRole::Signer, Delivery::Link, AuthMethod::Standard)
    };
    call::<docs::SetParties>(&sender, docs::SetPartiesInput { document_id: id, parties: vec![resave] }).await.unwrap();
    let view = call::<docs::Get>(&sender, docs::IdInput { id }).await.unwrap();
    assert!(view.parties[0].webcam_photo, "clearing the file requests keeps the photo request");
    assert_eq!(view.attachments.iter().map(|a| a.kind.as_str()).collect::<Vec<_>>(), ["photo"]);
    call::<docs::Send>(&sender, docs::SendInput { id, expires_at: None }).await.unwrap();

    let token = {
        let mut conn = app.db.acquire().await.unwrap();
        signer_token(&app.keys, &parties(&mut conn, id).await.unwrap()[0])
    };
    let signer = signer_ctx(&app);
    let opened =
        call::<signing::Open>(&signer, signing::OpenInput { token: token.clone(), session: None, redirect_url: None }).await.unwrap();
    let request = opened.attachments.iter().find(|a| a.kind == "photo").unwrap().id;
    let sign = || {
        call::<signing::Sign>(
            &signer,
            signing::SignInput {
                token: token.clone(),
                session: None,
                values: vec![],
                signature: Some(SignatureInput { typed_name: Some("Anna".into()), image_png_base64: None }),
                consent: true,
            },
        )
    };
    assert_eq!(kind(sign().await), ErrorKind::BadRequest, "no photo yet");
    let taken = {
        let mut tx = app.db.begin().await.unwrap();
        let pdf = signing::upload_attachment(&signer, &mut tx, &token, None, request, "me.pdf", sample_pdf(1)).await;
        assert_eq!(kind(pdf), ErrorKind::BadRequest, "a photo is an image, never a PDF");
        let tiny = signing::upload_attachment(&signer, &mut tx, &token, None, request, "me.png", camera_png(8, 8)).await;
        assert_eq!(kind(tiny), ErrorKind::BadRequest);
        let taken = signing::upload_attachment(&signer, &mut tx, &token, None, request, "x.png", camera_png(320, 240)).await.unwrap();
        tx.commit().await.unwrap();
        taken
    };
    assert_eq!(taken.file_name.as_deref(), Some("webcam-photo.png"));
    let logged: String = sqlx::query_scalar(
        "select metadata->>'sha256' from audit_log where entity_id = $1 and action = 'party.photo_taken' order by id desc limit 1",
    )
    .bind(id.to_string())
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(Some(logged), taken.sha256, "the audit trail records the photo's hash");

    assert!(sign().await.unwrap().completed);
    worker::drain(&app).await.unwrap();
    let failed: Vec<(String, Option<String>)> =
        sqlx::query_as("select kind, last_error from jobs where done_at is null and kind <> 'sweep'").fetch_all(&app.db).await.unwrap();
    assert!(failed.is_empty(), "jobs left over: {failed:?}");
    let download = call::<docs::Download>(&sender, docs::DownloadInput { id, version: Some(Version::Signed) }).await.unwrap();
    let sealed = Document::load_mem(&B64.decode(download.content_base64).unwrap()).unwrap();
    let names = sealed.catalog().unwrap().get(b"Names").unwrap().as_dict().unwrap();
    let tree = names.get(b"EmbeddedFiles").unwrap().as_dict().unwrap().get(b"Names").unwrap().as_array().unwrap();
    assert_eq!(tree[0].as_str().unwrap(), b"attachment-01-webcam-photo.png", "the photo is sealed into the evidence");
}

fn message<T>(result: opensign_core::Result<T>) -> String {
    match result {
        Err(e) => e.message,
        Ok(_) => panic!("expected an error"),
    }
}

#[tokio::test]
async fn field_types_are_validated_and_stamped() {
    let Some(app) = app().await else { return };
    {
        let mut conn = app.db.acquire().await.unwrap();
        let no_tsa = SigningSettings { tsa_url: String::new(), ..SigningSettings::default() };
        app.settings.save(&mut conn, &app.keys, &no_tsa, None).await.unwrap();
    }
    let sender = user(&app, "Fia Fields", Role::Sender).await;
    let create = serde_json::from_value::<docs::CreateInput>(serde_json::json!({ "title": "Order form" })).unwrap();
    let id = call::<docs::Create>(&sender, create).await.unwrap().document.id;
    call::<docs::Upload>(&sender, docs::UploadInput { document_id: id, name: "a.pdf".into(), content_base64: B64.encode(sample_pdf(1)) })
        .await
        .unwrap();
    let saved = call::<docs::SetParties>(
        &sender,
        docs::SetPartiesInput { document_id: id, parties: vec![party("Anna", PartyRole::Signer, Delivery::Link, AuthMethod::Standard)] },
    )
    .await
    .unwrap();
    let anna = Some(saved[0].id);
    let at = |kind: FieldKind, n: i32, label: &str| FieldInput {
        label: Some(label.into()),
        width: 0.2,
        height: 0.04,
        ..field(anna, kind, 0, 0.05 + f64::from(n) * 0.055)
    };
    let validation = |v: serde_json::Value| Some(serde_json::from_value(v).unwrap());
    let fields = vec![
        FieldInput {
            format: Some("1 234,00 kr".into()),
            validation: validation(serde_json::json!({ "min": "1", "max": "100000" })),
            ..at(FieldKind::Number, 0, "Amount")
        },
        FieldInput { options: vec!["Small".into(), "Large".into()], ..at(FieldKind::Select, 1, "Size") },
        FieldInput { options: vec!["A".into(), "B".into(), "C".into()], ..at(FieldKind::Multiselect, 2, "Extras") },
        FieldInput {
            options: vec!["S".into()],
            group_key: Some("fit".into()),
            width: 0.03,
            height: 0.03,
            ..at(FieldKind::Radio, 3, "Fit")
        },
        FieldInput {
            options: vec!["L".into()],
            group_key: Some("fit".into()),
            width: 0.03,
            height: 0.03,
            ..at(FieldKind::Radio, 4, "Fit")
        },
        at(FieldKind::Phone, 5, "Phone"),
        FieldInput {
            format: Some("DD/MM/YYYY".into()),
            validation: validation(serde_json::json!({ "min": "2026-01-01" })),
            ..at(FieldKind::DateInput, 6, "Start")
        },
        FieldInput { height: 0.1, ..at(FieldKind::Image, 7, "Logo") },
        at(FieldKind::File, 9, "Certificate"),
        FieldInput {
            validation: validation(serde_json::json!({ "pattern": "[0-9]{3} ?[0-9]{2}", "message": "Write the postcode as 123 45" })),
            ..at(FieldKind::Text, 10, "Postcode")
        },
        FieldInput { value: Some("Fixed".into()), read_only: Some(true), ..at(FieldKind::Text, 11, "Terms") },
        at(FieldKind::Signature, 12, "Signature"),
    ];
    let set = |fields: Vec<FieldInput>| call::<docs::SetFields>(&sender, docs::SetFieldsInput { document_id: id, fields });
    let broken = FieldInput { validation: validation(serde_json::json!({ "pattern": "([a-z]" })), ..at(FieldKind::Text, 0, "Bad") };
    assert_eq!(kind(set(vec![broken]).await), ErrorKind::BadRequest, "a pattern that does not compile");
    assert_eq!(kind(set(vec![at(FieldKind::Select, 0, "Empty")]).await), ErrorKind::BadRequest, "a list without options");
    assert_eq!(kind(set(vec![field(None, FieldKind::Image, 0, 0.1)]).await), ErrorKind::BadRequest, "the sender uploads nothing");
    let bad_default = FieldInput { value: Some("Huge".into()), options: vec!["Small".into()], ..at(FieldKind::Select, 0, "Size") };
    assert_eq!(kind(set(vec![bad_default]).await), ErrorKind::BadRequest, "a default that is not an option");
    let view = set(fields).await.unwrap();
    let id_of = |label: &str, n: usize| view.fields.iter().filter(|f| f.label == label).nth(n).unwrap().id;
    let (amount, size, extras, fit_s, fit_l, phone, start, logo, certificate, postcode, terms) = (
        id_of("Amount", 0),
        id_of("Size", 0),
        id_of("Extras", 0),
        id_of("Fit", 0),
        id_of("Fit", 1),
        id_of("Phone", 0),
        id_of("Start", 0),
        id_of("Logo", 0),
        id_of("Certificate", 0),
        id_of("Postcode", 0),
        id_of("Terms", 0),
    );
    call::<docs::Send>(&sender, docs::SendInput { id, expires_at: None }).await.unwrap();

    let token = {
        let mut conn = app.db.acquire().await.unwrap();
        signer_token(&app.keys, &parties(&mut conn, id).await.unwrap()[0])
    };
    let signer = signer_ctx(&app);
    let good = vec![
        (amount, "1 234,5 kr"),
        (size, "Large"),
        (extras, r#"["C","A"]"#),
        (fit_s, ""),
        (fit_l, "L"),
        (phone, "+46 70 123 45 67"),
        (start, "2026-03-01"),
        (postcode, "123 45"),
        (terms, "Changed"),
    ];
    let sign = |changes: &[(Uuid, &str)]| {
        let values = good
            .iter()
            .map(|(id, v)| changes.iter().find(|(c, _)| c == id).map_or(*v, |(_, v)| *v))
            .zip(good.iter().map(|(id, _)| *id))
            .map(|(value, field_id)| FieldValue { field_id, value: value.into() })
            .collect();
        call::<signing::Sign>(
            &signer,
            signing::SignInput {
                token: token.clone(),
                session: None,
                values,
                signature: Some(SignatureInput { typed_name: Some("Anna".into()), image_png_base64: None }),
                consent: true,
            },
        )
    };
    assert!(message(sign(&[]).await).contains("Logo"), "the image is required");
    {
        let mut tx = app.db.begin().await.unwrap();
        let pdf = signing::upload_field_file(&signer, &mut tx, &token, None, logo, "logo.pdf", sample_pdf(1)).await;
        assert_eq!(kind(pdf), ErrorKind::BadRequest, "an image field takes an image");
        let wrong = signing::upload_field_file(&signer, &mut tx, &token, None, amount, "x.png", camera_png(10, 10)).await;
        assert_eq!(kind(wrong), ErrorKind::NotFound, "only image and file fields take uploads");
        let image =
            signing::upload_field_file(&signer, &mut tx, &token, None, logo, "C:\\me\\logo.png", camera_png(400, 300)).await.unwrap();
        assert_eq!(image.value.as_deref(), Some("logo.png"));
        signing::upload_field_file(&signer, &mut tx, &token, None, certificate, "cert.pdf", sample_pdf(1)).await.unwrap();
        tx.commit().await.unwrap();
    }
    assert_eq!(message(sign(&[(amount, "abc")]).await), "\u{201c}Amount\u{201d} must be a number");
    assert_eq!(message(sign(&[(amount, "0,5")]).await), "\u{201c}Amount\u{201d} must be at least 1");
    assert_eq!(message(sign(&[(postcode, "12")]).await), "Write the postcode as 123 45");
    assert_eq!(kind(sign(&[(size, "Huge")]).await), ErrorKind::BadRequest);
    assert_eq!(kind(sign(&[(phone, "070 123")]).await), ErrorKind::BadRequest);
    assert_eq!(kind(sign(&[(start, "2025-12-31")]).await), ErrorKind::BadRequest);
    let both = sign(&[(fit_s, "S")]).await;
    assert!(message(both).contains("only one"), "one radio button per group");
    assert!(sign(&[]).await.unwrap().completed);

    let stored: Vec<(String, Option<String>)> =
        sqlx::query_as("select label, value from fields where document_id = $1 order by y").bind(id).fetch_all(&app.db).await.unwrap();
    let value = |label: &str, n: usize| stored.iter().filter(|(l, _)| l == label).nth(n).unwrap().1.clone();
    assert_eq!(value("Amount", 0).as_deref(), Some("1234.5"));
    assert_eq!(value("Extras", 0).as_deref(), Some(r#"["A","C"]"#));
    assert_eq!((value("Fit", 0), value("Fit", 1).as_deref()), (None, Some("L")));
    assert_eq!(value("Phone", 0).as_deref(), Some("+46701234567"));
    assert_eq!(value("Start", 0).as_deref(), Some("2026-03-01"));
    assert_eq!(value("Terms", 0).as_deref(), Some("Fixed"), "a read-only value cannot be changed");

    worker::drain(&app).await.unwrap();
    let download = call::<docs::Download>(&sender, docs::DownloadInput { id, version: Some(Version::Signed) }).await.unwrap();
    let sealed = Document::load_mem(&B64.decode(download.content_base64).unwrap()).unwrap();
    let page = *sealed.get_pages().get(&1).unwrap();
    let content = sealed.get_page_content(page);
    let has = |text: &[u8]| content.windows(text.len()).any(|w| w == text);
    assert!(has(b"(1 234,50 kr)"), "numbers are written in their format");
    assert!(has(b"(01/03/2026)"), "dates are written in their format");
    assert!(has(b"(A, C)"));
    assert!(has(b"/OSImg"), "the image is drawn on the page");
    let names = sealed.catalog().unwrap().get(b"Names").unwrap().as_dict().unwrap();
    let tree = names.get(b"EmbeddedFiles").unwrap().as_dict().unwrap().get(b"Names").unwrap().as_array().unwrap();
    assert!(tree.iter().any(|n| n.as_str().is_ok_and(|n| n.ends_with(b"cert.pdf"))), "the file is sealed into the PDF");
}

#[tokio::test]
async fn conditions_and_formulas_are_worked_out_by_the_server() {
    let Some(app) = app().await else { return };
    {
        let mut conn = app.db.acquire().await.unwrap();
        let no_tsa = SigningSettings { tsa_url: String::new(), ..SigningSettings::default() };
        app.settings.save(&mut conn, &app.keys, &no_tsa, None).await.unwrap();
    }
    let sender = user(&app, "Cora Conditions", Role::Sender).await;
    let create = serde_json::from_value::<docs::CreateInput>(serde_json::json!({ "title": "Quote" })).unwrap();
    let id = call::<docs::Create>(&sender, create).await.unwrap().document.id;
    call::<docs::Upload>(&sender, docs::UploadInput { document_id: id, name: "a.pdf".into(), content_base64: B64.encode(sample_pdf(1)) })
        .await
        .unwrap();
    let saved = call::<docs::SetParties>(
        &sender,
        docs::SetPartiesInput { document_id: id, parties: vec![party("Anna", PartyRole::Signer, Delivery::Link, AuthMethod::Standard)] },
    )
    .await
    .unwrap();
    let anna = Some(saved[0].id);
    let at = |party: Option<Uuid>, kind: FieldKind, n: i32, label: &str| FieldInput {
        label: Some(label.into()),
        width: 0.2,
        height: 0.04,
        ..field(party, kind, 0, 0.05 + f64::from(n) * 0.06)
    };
    let when = |field: &str, op: &str, value: Option<&str>| -> Vec<Condition> {
        vec![serde_json::from_value(serde_json::json!({ "field": field, "op": op, "value": value })).unwrap()]
    };
    let fields = vec![
        FieldInput { value: Some("100".into()), ..at(None, FieldKind::Number, 0, "Rate") },
        FieldInput { formula: Some("{Rate} * 0.25".into()), ..at(None, FieldKind::Formula, 1, "VAT") },
        FieldInput { options: vec!["Private".into(), "Company".into()], ..at(anna, FieldKind::Select, 2, "Type") },
        FieldInput { conditions: when("Type", "equals", Some("Company")), ..at(anna, FieldKind::Text, 3, "Org Number") },
        at(anna, FieldKind::Number, 4, "Hours"),
        FieldInput {
            formula: Some("{Hours} * {Rate} * 1.25".into()),
            format: Some("1 234,00 kr".into()),
            ..at(anna, FieldKind::Formula, 5, "Total")
        },
        FieldInput { required: Some(false), ..at(anna, FieldKind::Checkbox, 6, "Extra") },
        FieldInput { conditions: when("Extra", "filled", None), ..at(anna, FieldKind::Text, 7, "Note") },
        at(anna, FieldKind::Signature, 8, "Signature"),
    ];
    let set = |fields: Vec<FieldInput>| call::<docs::SetFields>(&sender, docs::SetFieldsInput { document_id: id, fields });
    let unknown = FieldInput { formula: Some("{Nope} + 1".into()), ..at(anna, FieldKind::Formula, 0, "F") };
    assert!(message(set(vec![unknown]).await).contains("Nope"), "a formula names a field that does not exist");
    let looped = vec![
        FieldInput { conditions: when("B", "filled", None), ..at(anna, FieldKind::Text, 0, "A") },
        FieldInput { conditions: when("A", "filled", None), ..at(anna, FieldKind::Text, 1, "B") },
    ];
    assert_eq!(kind(set(looped).await), ErrorKind::BadRequest, "fields that depend on each other");
    let peeking = vec![
        at(anna, FieldKind::Number, 0, "Hours"),
        FieldInput { formula: Some("{Hours} * 2".into()), ..at(None, FieldKind::Formula, 1, "F") },
    ];
    assert_eq!(kind(set(peeking).await), ErrorKind::BadRequest, "a sender formula cannot wait for a party");
    let broken = FieldInput { formula: Some("{Rate} *".into()), ..at(None, FieldKind::Formula, 0, "F") };
    assert_eq!(kind(set(vec![broken]).await), ErrorKind::BadRequest, "a formula that cannot be read");

    let view = set(fields).await.unwrap();
    let id_of = |label: &str| view.fields.iter().find(|f| f.label == label).unwrap().id;
    assert_eq!(view.fields.iter().find(|f| f.label == "VAT").unwrap().value.as_deref(), Some("25"), "the sender's formula is known now");
    assert!(!view.fields.iter().find(|f| f.label == "Total").unwrap().required);
    let (kind_of, org, hours, total, extra, note) =
        (id_of("Type"), id_of("Org Number"), id_of("Hours"), id_of("Total"), id_of("Extra"), id_of("Note"));
    call::<docs::Send>(&sender, docs::SendInput { id, expires_at: None }).await.unwrap();

    let token = {
        let mut conn = app.db.acquire().await.unwrap();
        signer_token(&app.keys, &parties(&mut conn, id).await.unwrap()[0])
    };
    let signer = signer_ctx(&app);
    let sign = |values: Vec<(Uuid, &str)>| {
        call::<signing::Sign>(
            &signer,
            signing::SignInput {
                token: token.clone(),
                session: None,
                values: values.into_iter().map(|(field_id, value)| FieldValue { field_id, value: value.into() }).collect(),
                signature: Some(SignatureInput { typed_name: Some("Anna".into()), image_png_base64: None }),
                consent: true,
            },
        )
    };
    let company = vec![(kind_of, "Company"), (org, ""), (hours, "8"), (extra, "false"), (note, "")];
    assert_eq!(message(sign(company).await), "Fill in \u{201c}Org Number\u{201d}", "a shown field is required");
    let ticked = vec![(kind_of, "Private"), (org, ""), (hours, "8"), (extra, "true"), (note, "")];
    assert_eq!(message(sign(ticked).await), "Fill in \u{201c}Note\u{201d}");
    let private = vec![(kind_of, "Private"), (org, "should vanish"), (hours, "8"), (total, "1"), (extra, "false"), (note, "abc")];
    assert!(sign(private).await.unwrap().completed, "hidden fields are neither required nor checked");

    let stored: Vec<(String, Option<String>)> =
        sqlx::query_as("select label, value from fields where document_id = $1").bind(id).fetch_all(&app.db).await.unwrap();
    let value = |label: &str| stored.iter().find(|(l, _)| l == label).unwrap().1.clone();
    assert_eq!(value("Org Number"), None, "a hidden field is cleared");
    assert_eq!(value("Note"), None);
    assert_eq!(value("Total").as_deref(), Some("1000"), "the server computes the formula, whatever the signer sent");

    worker::drain(&app).await.unwrap();
    let download = call::<docs::Download>(&sender, docs::DownloadInput { id, version: Some(Version::Signed) }).await.unwrap();
    let sealed = Document::load_mem(&B64.decode(download.content_base64).unwrap()).unwrap();
    let page = *sealed.get_pages().get(&1).unwrap();
    let content = sealed.get_page_content(page);
    let has = |text: &[u8]| content.windows(text.len()).any(|w| w == text);
    assert!(has(b"(1 000,00 kr)"), "the formula is written in its format");
    assert!(has(b"(25)"));
    assert!(!has(b"(should vanish)") && !has(b"(abc)"), "hidden fields are not stamped");
}
