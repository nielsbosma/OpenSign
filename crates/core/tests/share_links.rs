#![allow(clippy::unwrap_used)]

use std::sync::Arc;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use lopdf::{Document, Object, Stream, dictionary};
use opensign_core::context::{Actor, App, Credential, Ctx, Surface};
use opensign_core::documents::{AuthMethod, Delivery, FieldKind, PartyRole, parties};
use opensign_core::error::ErrorKind;
use opensign_core::services::documents::{self as docs, FieldInput, PartyInput};
use opensign_core::services::share_links::{self, code_hash};
use opensign_core::services::signatures;
use opensign_core::services::signing::{self, SignatureInput};
use opensign_core::testing::app;
use opensign_core::{Role, call};
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

fn signature_png() -> String {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, 40, 16);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(&[30; 40 * 16 * 4]).unwrap();
    writer.finish().unwrap();
    B64.encode(out)
}

async fn user(app: &Arc<App>, name: &str, role: Role) -> Ctx {
    let id = Uuid::now_v7();
    let email = format!("{}.{}@example.com", name.to_lowercase().replace(' ', "."), &id.simple().to_string()[24..]);
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
    Ctx::new(app.clone(), Surface::Api).with_actor(actor)
}

fn public(app: &Arc<App>) -> Ctx {
    let mut ctx = Ctx::new(app.clone(), Surface::Web);
    ctx.ip = Some("198.51.100.20".into());
    ctx
}

fn kind<T>(result: opensign_core::Result<T>) -> ErrorKind {
    match result {
        Err(e) => e.kind,
        Ok(_) => panic!("expected an error"),
    }
}

fn party(name: &str, role: PartyRole) -> PartyInput {
    PartyInput {
        id: None,
        role,
        name: name.into(),
        email: Some(format!("{}@example.net", name.to_lowercase())),
        phone: None,
        personal_number: None,
        company: None,
        sign_order: None,
        delivery: Delivery::Email,
        auth_to_view: AuthMethod::None,
        auth_to_sign: AuthMethod::Standard,
        attachments: None,
        language: None,
        webcam_photo: None,
    }
}

fn field(party_id: Option<Uuid>, kind: FieldKind, y: f64) -> FieldInput {
    FieldInput {
        party_id,
        kind,
        page: 0,
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

async fn with_file(ctx: &Ctx, title: &str, is_template: bool) -> Uuid {
    let create = serde_json::from_value::<docs::CreateInput>(serde_json::json!({ "title": title, "isTemplate": is_template })).unwrap();
    let id = call::<docs::Create>(ctx, create).await.unwrap().document.id;
    call::<docs::Upload>(
        ctx,
        docs::UploadInput { document_id: id, name: "a.pdf".into(), content_base64: B64.encode(sample_pdf()), ..Default::default() },
    )
    .await
    .unwrap();
    id
}

fn token(url: &str) -> String {
    url.split_once('#').unwrap().1.to_string()
}

#[tokio::test]
async fn saved_signature_is_kept_per_user_and_can_be_removed() {
    let Some(app) = app().await else { return };
    let ann = user(&app, "Ann Admin", Role::Sender).await;
    let bo = user(&app, "Bo Other", Role::Sender).await;
    assert!(call::<signatures::Get>(&ann, Default::default()).await.unwrap().empty);
    let empty = signatures::SetSignatureInput { typed_name: None, image_png_base64: None, initials: None };
    assert_eq!(kind(call::<signatures::Set>(&ann, empty).await), ErrorKind::BadRequest);
    let long = signatures::SetSignatureInput { typed_name: Some("Ann".into()), image_png_base64: None, initials: Some("ABCDEF".into()) };
    assert_eq!(kind(call::<signatures::Set>(&ann, long).await), ErrorKind::BadRequest);

    let drawn = signatures::SetSignatureInput {
        typed_name: Some("Ann Admin".into()),
        image_png_base64: Some(signature_png()),
        initials: Some("AA".into()),
    };
    let saved = call::<signatures::Set>(&ann, drawn).await.unwrap();
    assert!(!saved.empty && saved.image_png_base64.is_some() && saved.typed_name.is_none(), "a drawing wins over the typed name");
    assert_eq!(saved.initials.as_deref(), Some("AA"));
    assert!(call::<signatures::Get>(&bo, Default::default()).await.unwrap().empty, "each user has their own");

    let typed = signatures::SetSignatureInput { typed_name: Some("Ann A.".into()), image_png_base64: None, initials: None };
    let saved = call::<signatures::Set>(&ann, typed).await.unwrap();
    assert_eq!(saved.typed_name.as_deref(), Some("Ann A."));
    assert!(saved.image_png_base64.is_none());
    let retired: i64 = sqlx::query_scalar("select count(*) from files where kind = 'saved_signature' and deleted_at is not null")
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(retired, 1, "the replaced drawing is retired");

    call::<signatures::Remove>(&ann, Default::default()).await.unwrap();
    assert!(call::<signatures::Get>(&ann, Default::default()).await.unwrap().empty);
}

#[tokio::test]
async fn signing_myself_makes_me_the_only_signer_and_records_my_sign_in() {
    let Some(app) = app().await else { return };
    let sam = user(&app, "Sam Self", Role::Sender).await;
    let id = with_file(&sam, "My own form", false).await;
    let saved = call::<docs::SetParties>(
        &sam,
        docs::SetPartiesInput {
            document_id: id,
            parties: vec![party("Olle", PartyRole::Signer), party("Ada", PartyRole::Approver), party("Vera", PartyRole::Viewer)],
        },
    )
    .await
    .unwrap();
    let (olle, ada) = (saved[0].id, saved[1].id);
    call::<docs::SetFields>(
        &sam,
        docs::SetFieldsInput {
            document_id: id,
            fields: vec![field(Some(olle), FieldKind::Signature, 0.1), field(Some(ada), FieldKind::Text, 0.3)],
        },
    )
    .await
    .unwrap();

    let other = user(&app, "Nils Nosy", Role::Sender).await;
    let refused = call::<docs::SignMyself>(&other, docs::SignMyselfInput { id, auth_to_sign: AuthMethod::Standard }).await;
    assert!(refused.is_err());

    let started = call::<docs::SignMyself>(&sam, docs::SignMyselfInput { id, auth_to_sign: AuthMethod::Standard }).await.unwrap();
    let doc = call::<docs::Get>(&sam, docs::IdInput { id }).await.unwrap();
    assert_eq!(doc.document.status, "pending");
    let mut roles: Vec<(&str, &str)> = doc.parties.iter().map(|p| (p.name.as_str(), p.role.as_str())).collect();
    roles.sort();
    assert_eq!(roles, vec![("Sam Self", "signer"), ("Vera", "viewer")], "signers and approvers are replaced, copies kept");
    assert!(doc.fields.iter().all(|f| f.party_id == Some(started.party_id)), "their fields become mine");
    assert_eq!(kind(call::<docs::SignMyself>(&sam, docs::SignMyselfInput { id, auth_to_sign: AuthMethod::Standard }).await), ErrorKind::Conflict);

    let resumed = call::<docs::SignInPerson>(&sam, docs::InPersonInput { document_id: id, party_id: started.party_id }).await.unwrap();
    assert!(resumed.session.is_some(), "starting again on my own party keeps my sign-in as identification");
    let started = docs::SelfSigning { party_id: started.party_id, url: resumed.url, session: resumed.session.unwrap() };
    let signer = public(&app);
    let link = token(&started.url);
    let view = call::<signing::Open>(
        &signer,
        signing::OpenInput { token: link.clone(), session: Some(started.session.clone()), redirect_url: None },
    )
    .await
    .unwrap();
    assert!(view.party.is_sender && view.party.in_person && view.your_turn);
    let text = view.fields.iter().find(|f| f.kind == "text").unwrap().id;
    let out = call::<signing::Sign>(
        &signer,
        signing::SignInput {
            token: link,
            session: Some(started.session),
            values: vec![signing::FieldValue { field_id: text, value: "Yes".into() }],
            signature: Some(SignatureInput { typed_name: Some("Sam Self".into()), image_png_base64: None }),
            consent: true,
        },
    )
    .await
    .unwrap();
    assert!(out.completed);
    let me = parties(&mut app.db.acquire().await.unwrap(), id).await.unwrap().into_iter().find(|p| p.id == started.party_id).unwrap();
    let evidence = me.evidence.clone().unwrap();
    assert_eq!(evidence["account"], serde_json::json!(sam.actor.as_ref().unwrap().user_id));
    let identity = opensign_core::documents::open_identity(&app.keys, &me).unwrap();
    assert!(identity.starts_with("Signed in to OpenSign as Sam Self"), "{identity}");
}

async fn confirm_with(ctx: &Ctx, app: &Arc<App>, slug: &str, request: Uuid) -> opensign_core::Result<share_links::Started> {
    let hash = code_hash(ctx, request, "424242");
    sqlx::query("update share_link_requests set code_hash = $2 where id = $1").bind(request).bind(hash).execute(&app.db).await.unwrap();
    call::<share_links::Confirm>(ctx, share_links::ConfirmInput { slug: slug.into(), request_id: request, code: "424 242".into() }).await
}

fn ask(slug: &str, name: &str, email: &str) -> share_links::RequestCodeInput {
    share_links::RequestCodeInput { slug: slug.into(), name: name.into(), email: email.into(), phone: None, language: None }
}

#[tokio::test]
async fn share_links_confirm_the_email_before_making_a_document() {
    let Some(app) = app().await else { return };
    let manager = user(&app, "Mia Manager", Role::Manager).await;
    let template = with_file(&manager, "Membership", true).await;
    let saved = call::<docs::SetParties>(
        &manager,
        docs::SetPartiesInput { document_id: template, parties: vec![party("Member", PartyRole::Signer), party("Club", PartyRole::Signer)] },
    )
    .await
    .unwrap();
    let (member, club) = (saved[0].id, saved[1].id);
    call::<docs::SetFields>(
        &manager,
        docs::SetFieldsInput { document_id: template, fields: vec![field(Some(member), FieldKind::Signature, 0.1)] },
    )
    .await
    .unwrap();
    let set = |enabled: bool, party_id: Option<Uuid>, per_hour: Option<i32>| share_links::SetInput {
        template_id: template,
        enabled,
        party_id,
        per_hour,
        max_total: None,
        expires_at: None,
    };
    assert!(call::<share_links::Get>(&manager, share_links::TemplateInput { template_id: template }).await.unwrap().is_none());
    assert_eq!(kind(call::<share_links::Set>(&manager, set(true, None, None)).await), ErrorKind::BadRequest, "needs a party");
    let viewer = user(&app, "Vik Viewer", Role::Viewer).await;
    assert!(call::<share_links::Set>(&viewer, set(true, Some(member), None)).await.is_err());
    let link = call::<share_links::Set>(&manager, set(true, Some(member), Some(2))).await.unwrap();
    assert!(link.enabled && link.url.contains("/s/"));
    let slug = link.url.rsplit('/').next().unwrap().to_string();

    let visitor = public(&app);
    let page = call::<share_links::Open>(&visitor, share_links::SlugInput { slug: slug.clone() }).await.unwrap();
    assert_eq!(page.title, "Membership");
    assert!(!page.needs_phone);
    assert_eq!(kind(call::<share_links::Open>(&visitor, share_links::SlugInput { slug: "nope".into() }).await), ErrorKind::NotFound);
    assert_eq!(kind(call::<share_links::RequestCode>(&visitor, ask(&slug, "Lisa", "not an email")).await), ErrorKind::BadRequest);

    let request = call::<share_links::RequestCode>(&visitor, ask(&slug, "Lisa Larsson", "Lisa@Example.org")).await.unwrap();
    let wrong = share_links::ConfirmInput { slug: slug.clone(), request_id: request.request_id, code: "000000".into() };
    assert_eq!(kind(call::<share_links::Confirm>(&visitor, wrong).await), ErrorKind::BadRequest);
    let documents: i64 = sqlx::query_scalar("select count(*) from documents where share_link_id is not null").fetch_one(&app.db).await.unwrap();
    assert_eq!(documents, 0, "nothing is made before the email is confirmed");

    let started = confirm_with(&visitor, &app, &slug, request.request_id).await.unwrap();
    assert!(started.your_turn);
    let again = confirm_with(&visitor, &app, &slug, request.request_id).await;
    assert_eq!(kind(again), ErrorKind::BadRequest, "a code is used once");
    let view = call::<signing::Open>(&visitor, signing::OpenInput { token: token(&started.url), session: None, redirect_url: None })
        .await
        .unwrap();
    assert_eq!(view.party.name, "Lisa Larsson");
    assert_eq!(view.party.email.as_deref(), Some("lisa@example.org"));
    assert_eq!(view.document.status, "pending");
    let document = call::<docs::Get>(&manager, docs::IdInput { id: view.document.id }).await.unwrap();
    assert!(document.document.title.starts_with("Membership"));
    assert_eq!(document.document.owner_id, Some(manager.actor.as_ref().unwrap().user_id), "documents belong to whoever turned the link on");
    assert!(document.parties.iter().any(|p| p.name == "Club"), "fixed parties stay");
    let _ = club;

    let two = call::<share_links::RequestCode>(&visitor, ask(&slug, "Two", "two@example.org")).await.unwrap();
    let three = call::<share_links::RequestCode>(&visitor, ask(&slug, "Three", "three@example.org")).await.unwrap();
    confirm_with(&visitor, &app, &slug, two.request_id).await.unwrap();
    assert_eq!(kind(confirm_with(&visitor, &app, &slug, three.request_id).await), ErrorKind::TooManyRequests, "at most two per hour");
    let busy = call::<share_links::RequestCode>(&visitor, ask(&slug, "Four", "four@example.org")).await;
    assert_eq!(kind(busy), ErrorKind::TooManyRequests);

    let view = call::<share_links::Get>(&manager, share_links::TemplateInput { template_id: template }).await.unwrap().unwrap();
    assert_eq!(view.documents_created, 2);
    let rotated = call::<share_links::Rotate>(&manager, share_links::TemplateInput { template_id: template }).await.unwrap();
    assert_ne!(rotated.url, view.url);
    assert_eq!(kind(call::<share_links::Open>(&visitor, share_links::SlugInput { slug: slug.clone() }).await), ErrorKind::NotFound);
    let new_slug = rotated.url.rsplit('/').next().unwrap().to_string();
    call::<share_links::Set>(&manager, set(false, Some(member), Some(2))).await.unwrap();
    assert_eq!(kind(call::<share_links::Open>(&visitor, share_links::SlugInput { slug: new_slug }).await), ErrorKind::NotFound);
}

#[tokio::test]
async fn share_links_limit_codes_per_address() {
    let Some(app) = app().await else { return };
    let manager = user(&app, "Moa Manager", Role::Manager).await;
    let template = with_file(&manager, "Waiver", true).await;
    let saved = call::<docs::SetParties>(&manager, docs::SetPartiesInput { document_id: template, parties: vec![party("Guest", PartyRole::Signer)] })
        .await
        .unwrap();
    let input = share_links::SetInput {
        template_id: template,
        enabled: true,
        party_id: Some(saved[0].id),
        per_hour: Some(100),
        max_total: Some(1),
        expires_at: None,
    };
    let link = call::<share_links::Set>(&manager, input).await.unwrap();
    let slug = link.url.rsplit('/').next().unwrap().to_string();
    let visitor = public(&app);
    for _ in 0..3 {
        call::<share_links::RequestCode>(&visitor, ask(&slug, "Gus", "gus@example.org")).await.unwrap();
    }
    assert_eq!(kind(call::<share_links::RequestCode>(&visitor, ask(&slug, "Gus", "gus@example.org")).await), ErrorKind::TooManyRequests);
    let r = call::<share_links::RequestCode>(&visitor, ask(&slug, "Hed", "hed@example.org")).await.unwrap();
    confirm_with(&visitor, &app, &slug, r.request_id).await.unwrap();
    assert_eq!(kind(call::<share_links::Open>(&visitor, share_links::SlugInput { slug }).await), ErrorKind::NotFound, "the total limit is reached");
}
