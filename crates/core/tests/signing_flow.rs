#![allow(clippy::unwrap_used)]

use std::sync::Arc;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use lopdf::{Document, Object, Stream, dictionary};
use opensign_core::audit::verify_chain;
use opensign_core::context::{Actor, App, Credential, Ctx, Surface};
use opensign_core::documents::attachments::AttachmentRequestInput;
use opensign_core::documents::{AuthMethod, Delivery, FieldKind, PartyRole, parties, signer_token};
use opensign_core::error::ErrorKind;
use opensign_core::services::documents::{self as docs, FieldInput, PartyInput, Version};
use opensign_core::services::signing::{self, FieldValue, SignatureInput, sms_code_hash};
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
    }
}

fn field(party_id: Option<Uuid>, kind: FieldKind, page: i32, y: f64) -> FieldInput {
    FieldInput { party_id, kind, page, x: 0.1, y, width: 0.3, height: 0.06, required: None, label: None, value: None }
}

#[tokio::test]
async fn sends_signs_seals_and_verifies() {
    let Some(app) = app().await else { return };
    {
        let mut conn = app.db.acquire().await.unwrap();
        let no_tsa = SigningSettings { tsa_url: String::new(), ..SigningSettings::default() };
        app.settings.save(&mut conn, &app.keys, &no_tsa, None).await.unwrap();
    }
    let sender = user(&app, "Sara Sender", Role::Sender).await;
    let outsider = user(&app, "Olle Other", Role::Sender).await;

    let document = call::<docs::Create>(
        &sender,
        serde_json::from_value(serde_json::json!({ "title": "Consulting Agreement", "message": "Please sign by Friday." })).unwrap(),
    )
    .await
    .unwrap();
    let id = document.document.id;

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
                party("Bo", PartyRole::Signer, Delivery::Link, AuthMethod::SmsPin),
                party("Cecilia", PartyRole::Viewer, Delivery::Email, AuthMethod::Standard),
            ],
        },
    )
    .await
    .unwrap();
    let (anna, bo, cecilia) = (set[0].id, set[1].id, set[2].id);

    let with_number = |id: Uuid, number: Option<&str>| PartyInput {
        id: Some(id),
        personal_number: number.map(str::to_string),
        ..party("Anna", PartyRole::Signer, Delivery::Email, AuthMethod::Standard)
    };
    let resave = |anna: PartyInput| docs::SetPartiesInput {
        document_id: id,
        parties: vec![
            anna,
            PartyInput { id: Some(bo), ..party("Bo", PartyRole::Signer, Delivery::Link, AuthMethod::SmsPin) },
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
    assert!(call::<signing::Open>(&signer, signing::TokenInput { token: forged, session: None }).await.is_err());

    let anna_view = call::<signing::Open>(&signer, signing::TokenInput { token: anna_token.clone(), session: None }).await.unwrap();
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

    let bo_view = call::<signing::Open>(&signer, signing::TokenInput { token: bo_token.clone(), session: None }).await.unwrap();
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

    let bo_fields = call::<signing::Open>(&signer, signing::TokenInput { token: bo_token.clone(), session: Some(session.clone()) })
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

    let finished = call::<docs::Get>(&sender, docs::IdInput { id }).await.unwrap();
    assert_eq!(finished.document.status, "completed");
    assert!(finished.parties.iter().filter(|p| p.id != cecilia).all(|p| p.status == "signed"));

    let download = call::<docs::Download>(&sender, docs::DownloadInput { id, version: Some(Version::Signed) }).await.unwrap();
    let sealed = B64.decode(download.content_base64).unwrap();
    // CI checks this file with independent validators (poppler's pdfsig, qpdf).
    let embedded = Document::load_mem(&sealed).unwrap();
    let names = embedded.catalog().unwrap().get(b"Names").unwrap().as_dict().unwrap();
    let tree = names.get(b"EmbeddedFiles").unwrap().as_dict().unwrap().get(b"Names").unwrap().as_array().unwrap();
    assert_eq!(tree[0].as_str().unwrap(), b"attachment-01-id.pdf", "Anna's attachment travels inside the sealed PDF");
    if let Ok(path) = std::env::var("SEALED_PDF_OUT") {
        std::fs::write(path, &sealed).unwrap();
    }
    let mut conn = app.db.acquire().await.unwrap();
    let verified = docs::verify_pdf(&mut conn, sealed.clone()).await.unwrap();
    assert_eq!(verified.document.map(|d| d.id), Some(id));
    assert_eq!(verified.signatures.len(), 1);
    assert!(verified.signatures[0].is_intact(), "{:?}", verified.signatures[0]);

    let mut tampered = sealed;
    let n = tampered.len();
    tampered[n / 3] ^= 0x20;
    if let Ok(result) = docs::verify_pdf(&mut conn, tampered).await {
        assert!(result.document.is_none());
        assert!(result.signatures.iter().all(|s| !s.is_intact()));
    }

    let after = call::<signing::Open>(&signer, signing::TokenInput { token: anna_token, session: None }).await.unwrap();
    assert!(after.can_download);
    let chain = verify_chain(&mut conn).await.unwrap();
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
    call::<signing::Open>(&signer, signing::TokenInput { token: dan.clone(), session: None }).await.unwrap();
    call::<docs::Cancel>(&sender, docs::CancelInput { id: ids[0], reason: Some("Wrong version".into()) }).await.unwrap();
    let revoked = call::<signing::Open>(&signer, signing::TokenInput { token: dan, session: None }).await;
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
    let stale = call::<signing::Open>(&signer, signing::TokenInput { token: token(&first.url), session: None }).await;
    assert_eq!(kind(stale), ErrorKind::NotFound, "starting again retires the earlier link");
    let view = call::<signing::Open>(&signer, signing::TokenInput { token: token(&again.url), session: None }).await.unwrap();
    assert!(view.party.in_person && view.your_turn);

    let outsider = user(&app, "Val Viewer", Role::Viewer).await;
    let refused = call::<docs::SignInPerson>(&outsider, docs::InPersonInput { document_id: id, party_id: fia }).await;
    assert!(refused.is_err());
}
