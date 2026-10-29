#![allow(clippy::unwrap_used)]

use std::sync::Arc;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use opensign_core::context::{Actor, App, Credential, Ctx, Surface};
use opensign_core::documents::{FieldRow, PartyView};
use opensign_core::services::detect;
use opensign_core::services::documents::{self as docs, FieldInput};
use opensign_core::testing::app;
use opensign_core::{Role, call};
use serde_json::json;
use uuid::Uuid;

const WORD_TAGS: &[u8] = include_bytes!("../../pdf/fixtures/word-tags.pdf");
const FORM: &[u8] = include_bytes!("../../pdf/fixtures/acroform.pdf");
const DETECT: &[u8] = include_bytes!("../../pdf/fixtures/word-detect.pdf");

async fn sender(app: &Arc<App>) -> Ctx {
    let id = Uuid::now_v7();
    let email = format!("{}@example.com", id.simple());
    sqlx::query("insert into users (id, email, name, role, password_hash) values ($1, $2, 'Sara Sender', $3, 'x')")
        .bind(id)
        .bind(&email)
        .bind(Role::Sender.as_str())
        .execute(&app.db)
        .await
        .unwrap();
    let actor = Actor {
        user_id: id,
        email,
        name: "Sara Sender".into(),
        role: Role::Sender,
        permissions: Role::Sender.permissions(),
        must_enable_two_factor: false,
        credential: Credential::Session,
    };
    Ctx::new(app.clone(), Surface::Api).with_actor(actor)
}

async fn draft(ctx: &Ctx, parties: serde_json::Value) -> Uuid {
    let document = call::<docs::Create>(ctx, serde_json::from_value(json!({ "title": "Lease" })).unwrap()).await.unwrap();
    let id = document.document.id;
    if parties.as_array().is_some_and(|p| !p.is_empty()) {
        call::<docs::SetParties>(ctx, serde_json::from_value(json!({ "documentId": id, "parties": parties })).unwrap()).await.unwrap();
    }
    id
}

async fn upload(ctx: &Ctx, id: Uuid, pdf: &[u8], text_tags: Option<bool>) -> docs::UploadOutput {
    call::<docs::Upload>(
        ctx,
        docs::UploadInput { document_id: id, name: "lease.pdf".into(), content_base64: B64.encode(pdf), text_tags, form_fields: None },
    )
    .await
    .unwrap()
}

async fn view(ctx: &Ctx, id: Uuid) -> (Vec<PartyView>, Vec<FieldRow>) {
    let doc = call::<docs::Get>(ctx, serde_json::from_value(json!({ "id": id })).unwrap()).await.unwrap();
    (doc.parties, doc.fields)
}

fn party_of<'a>(parties: &'a [PartyView], field: &FieldRow) -> &'a str {
    parties.iter().find(|p| Some(p.id) == field.party_id).map_or("-", |p| p.name.as_str())
}

#[tokio::test]
async fn text_tags_become_fields_and_leave_the_page() {
    let Some(app) = app().await else { return };
    let ctx = sender(&app).await;
    let id = draft(&ctx, json!([{ "name": "Landlord", "email": "lars@example.com" }])).await;
    let out = upload(&ctx, id, WORD_TAGS, None).await;
    assert_eq!(out.fields.text_tags, 9, "{:?}", out.fields.problems);
    assert_eq!(out.fields.parties_added, vec!["Tenant"]);
    assert!(out.fields.problems.iter().any(|p| p.contains("two lines")), "{:?}", out.fields.problems);

    let (parties, fields) = view(&ctx, id).await;
    let summary: Vec<(&str, &str, &str, bool)> =
        fields.iter().map(|f| (f.kind.as_str(), f.label.as_str(), party_of(&parties, f), f.required)).collect();
    assert_eq!(
        summary,
        vec![
            ("text", "Tenant Name", "Tenant", true),
            ("number", "Rent", "Landlord", false),
            ("date_input", "Move In", "Tenant", true),
            ("radio", "Plan", "Tenant", true),
            ("radio", "Plan", "Tenant", true),
            ("select", "Parking", "Tenant", true),
            ("checkbox", "", "Tenant", false),
            ("signature", "Tenant Sign", "Tenant", true),
            ("date", "Date", "Tenant", true),
        ]
    );
    assert_eq!(fields[1].format.as_deref(), Some("$1,234.00"));
    assert_eq!(fields[2].format.as_deref(), Some("DD/MM/YYYY"));
    assert_eq!(fields[3].group_key, fields[4].group_key);
    assert_eq!(fields[5].options, json!(["None", "Garage", "Street"]));
    assert!(fields.iter().all(|f| f.page == 0 && f.x >= 0.0 && f.x + f.width <= 1.0 && f.y + f.height <= 1.0));

    let (_, pdf) = docs::preview_pdf(&ctx, id, None).await.unwrap();
    let text = lopdf::Document::load_mem(&pdf).unwrap().extract_text(&[1]).unwrap();
    assert!(!text.contains("type=signature;role=Tenant") && text.contains("house rules"), "{text}");

    let kept = draft(&ctx, json!([])).await;
    let out = upload(&ctx, kept, WORD_TAGS, Some(false)).await;
    assert_eq!((out.fields.text_tags, out.fields.parties_added.len()), (0, 0));
    let (_, pdf) = docs::preview_pdf(&ctx, kept, None).await.unwrap();
    assert!(lopdf::Document::load_mem(&pdf).unwrap().extract_text(&[1]).unwrap().contains("type=signature"));
}

#[tokio::test]
async fn a_pdf_form_becomes_fields_after_earlier_pages() {
    let Some(app) = app().await else { return };
    let ctx = sender(&app).await;
    let id = draft(&ctx, json!([{ "name": "Owner" }, { "name": "Tenant" }])).await;
    upload(&ctx, id, DETECT, Some(false)).await;
    let out = upload(&ctx, id, FORM, None).await;
    assert_eq!((out.fields.form_fields, out.fields.text_tags), (7, 0), "{:?}", out.fields.problems);
    assert!(out.removed.iter().any(|r| r == "form fields"));
    let (parties, fields) = view(&ctx, id).await;
    assert_eq!(parties.len(), 2, "no party is added for a form");
    let summary: Vec<(&str, &str, &str, Option<&str>, i32)> =
        fields.iter().map(|f| (f.kind.as_str(), f.label.as_str(), party_of(&parties, f), f.value.as_deref(), f.page)).collect();
    assert_eq!(
        summary,
        vec![
            ("text", "Tenant.Name", "Tenant", Some("Anna Svensson"), 1),
            ("text", "Notes", "Owner", None, 1),
            ("checkbox", "Pets", "Owner", Some("true"), 1),
            ("radio", "Plan", "Owner", None, 1),
            ("radio", "Plan", "Owner", Some("Plus"), 1),
            ("select", "Parking", "Owner", Some("Garage"), 1),
            ("multiselect", "Extras", "Owner", Some("[\"Sauna\"]"), 1),
        ]
    );
}

#[tokio::test]
async fn suggests_fields_where_there_are_none() {
    let Some(app) = app().await else { return };
    let ctx = sender(&app).await;
    let id = draft(&ctx, json!([{ "name": "Tenant" }])).await;
    upload(&ctx, id, DETECT, Some(false)).await;
    let first = call::<detect::DetectFields>(&ctx, detect::DetectInput { document_id: id, ai: true }).await.unwrap();
    assert!(!first.ai_used && first.ai_note.as_deref().is_some_and(|n| n.contains("AI")), "{:?}", first.ai_note);
    let name = first.suggestions.iter().find(|s| s.label == "Name").expect("the underscores after Name:");
    assert_eq!((name.kind.as_str(), name.source.as_str(), name.page), ("name", "layout", 0));

    let (parties, _) = view(&ctx, id).await;
    let field: FieldInput = serde_json::from_value(json!({
        "partyId": parties[0].id, "kind": "name", "page": 0, "x": name.x, "y": name.y, "width": name.width, "height": name.height,
    }))
    .unwrap();
    call::<docs::SetFields>(&ctx, docs::SetFieldsInput { document_id: id, fields: vec![field] }).await.unwrap();
    let again = call::<detect::DetectFields>(&ctx, detect::DetectInput { document_id: id, ai: false }).await.unwrap();
    assert!(!again.suggestions.iter().any(|s| s.label == "Name"), "placed fields are not suggested again");
    assert!(again.ai_note.is_none());
}

#[tokio::test]
async fn text_tags_in_a_word_file() {
    let Some(app) = app().await else { return };
    if app.env.converter_url.is_none() {
        return;
    }
    let ctx = sender(&app).await;
    let id = draft(&ctx, json!([])).await;
    let docx = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/text-tags.docx")).unwrap();
    let out = call::<docs::Upload>(
        &ctx,
        docs::UploadInput { document_id: id, name: "lease.docx".into(), content_base64: B64.encode(docx), ..Default::default() },
    )
    .await
    .unwrap();
    assert!(out.fields.text_tags >= 9, "{:?}", out.fields.problems);
    assert_eq!(out.fields.parties_added, vec!["Tenant", "Landlord"]);
}
