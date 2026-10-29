#![allow(clippy::unwrap_used)]

use std::sync::Arc;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use lopdf::{Document, Object, Stream, dictionary};
use opensign_core::context::{Actor, App, Credential, Ctx, Surface};
use opensign_core::error::ErrorKind;
use opensign_core::services::documents as docs;
use opensign_core::services::templates;
use opensign_core::testing::app;
use opensign_core::{Role, call};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use uuid::Uuid;

fn sample_pdf(pages: usize) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let mut kids = Vec::new();
    for _ in 0..pages {
        let content = doc.add_object(Stream::new(dictionary! {}, b"0 0 m 10 10 l S".to_vec()));
        kids.push(Object::Reference(doc.add_object(dictionary! {
            "Type" => "Page", "Parent" => pages_id, "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()], "Contents" => content,
        })));
    }
    doc.objects.insert(pages_id, Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => kids, "Count" => pages as i64 }));
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog);
    let mut out = Vec::new();
    doc.save_to(&mut out).unwrap();
    out
}

async fn person(app: &Arc<App>, role: Role) -> Ctx {
    let id = Uuid::now_v7();
    let email = format!("{}@example.com", id.simple());
    sqlx::query("insert into users (id, email, name, role, password_hash) values ($1, $2, 'Someone', $3, 'x')")
        .bind(id)
        .bind(&email)
        .bind(role.as_str())
        .execute(&app.db)
        .await
        .unwrap();
    let actor = Actor {
        user_id: id,
        email,
        name: "Someone".into(),
        role,
        permissions: role.permissions(),
        must_enable_two_factor: false,
        credential: Credential::Session,
    };
    Ctx::new(app.clone(), Surface::Api).with_actor(actor)
}

fn input<T: DeserializeOwned>(value: Value) -> T {
    serde_json::from_value(value).unwrap()
}

fn kind<T>(result: opensign_core::Result<T>) -> ErrorKind {
    match result {
        Err(e) => e.kind,
        Ok(_) => panic!("expected an error"),
    }
}

async fn upload(ctx: &Ctx, id: Uuid, pages: usize) -> Uuid {
    call::<docs::Upload>(
        ctx,
        docs::UploadInput {
            document_id: id,
            name: "lease.pdf".into(),
            content_base64: B64.encode(sample_pdf(pages)),
            ..Default::default()
        },
    )
    .await
    .unwrap()
    .file
    .id
}

async fn set_fields(ctx: &Ctx, id: Uuid, fields: Value) {
    call::<docs::SetFields>(ctx, input(json!({ "documentId": id, "fields": fields }))).await.unwrap();
}

#[tokio::test]
async fn versions_record_history_and_restore_without_losing_anything() {
    let Some(app) = app().await else { return };
    let manager = person(&app, Role::Manager).await;
    let sender = person(&app, Role::Sender).await;

    let template = call::<docs::Create>(&manager, input(json!({ "title": "Lease", "isTemplate": true }))).await.unwrap().document.id;
    let first_file = upload(&manager, template, 2).await;
    let parties = call::<docs::SetParties>(
        &manager,
        input(json!({ "documentId": template, "parties": [
            { "name": "Tenant", "email": "tenant@example.com", "personalNumber": "19800101-1234" },
            { "name": "Landlord", "email": "landlord@example.com" },
        ] })),
    )
    .await
    .unwrap();
    let (tenant, landlord) = (parties[0].id, parties[1].id);
    set_fields(
        &manager,
        template,
        json!([
            { "partyId": tenant, "kind": "signature", "page": 1, "x": 0.1, "y": 0.8, "width": 0.3, "height": 0.08 },
            { "partyId": landlord, "kind": "signature", "page": 1, "x": 0.6, "y": 0.8, "width": 0.3, "height": 0.08 },
            { "partyId": tenant, "kind": "select", "label": "Plan", "options": ["Basic", "Plus"], "page": 0, "x": 0.1, "y": 0.1, "width": 0.3, "height": 0.04 },
            { "partyId": tenant, "kind": "text", "label": "Plus code", "page": 0, "x": 0.1, "y": 0.2, "width": 0.3, "height": 0.04,
              "conditions": [{ "field": "Plan", "op": "equals", "value": "Plus" }],
              "validation": { "pattern": "^[A-Z]{3}$", "message": "Three capitals" } },
        ]),
    )
    .await;

    let versions = |ctx: &Ctx| {
        let ctx = ctx.clone();
        async move { call::<templates::Versions>(&ctx, input(json!({ "templateId": template }))).await }
    };
    let none = versions(&manager).await.unwrap();
    assert!(none.versions.is_empty() && none.changed);

    let v1 = call::<templates::Save>(&manager, input(json!({ "templateId": template, "note": "First draft" }))).await.unwrap();
    assert_eq!((v1.number, v1.note.as_deref()), (1, Some("First draft")));
    assert_eq!(kind(call::<templates::Save>(&manager, input(json!({ "templateId": template }))).await), ErrorKind::Conflict);
    assert_eq!(kind(call::<templates::Save>(&sender, input(json!({ "templateId": template }))).await), ErrorKind::Forbidden);
    assert!(!versions(&manager).await.unwrap().changed);

    // Using an unchanged template names version 1; a changed one saves version 2 first.
    let doc1 = call::<docs::Create>(&sender, input(json!({ "title": "Lease A", "templateId": template }))).await.unwrap();
    assert_eq!(doc1.document.template_version, Some(1));
    assert_eq!(doc1.template_title.as_deref(), Some("Lease"));
    let plus = doc1.fields.iter().find(|f| f.label == "Plus code").unwrap();
    assert_eq!(plus.conditions, json!([{ "field": "Plan", "op": "equals", "value": "Plus" }]));
    assert_eq!(plus.validation["pattern"], "^[A-Z]{3}$");
    assert_eq!(doc1.parties[0].personal_number.as_deref(), Some("•••••••••1234"));

    call::<docs::Update>(&manager, input(json!({ "id": template, "title": "Lease 2027" }))).await.unwrap();
    set_fields(
        &manager,
        template,
        json!([{ "partyId": tenant, "kind": "signature", "page": 1, "x": 0.1, "y": 0.5, "width": 0.3, "height": 0.08 }]),
    )
    .await;
    let changes = call::<templates::Compare>(&manager, input(json!({ "templateId": template, "from": 1 }))).await.unwrap().changes;
    let summary: Vec<String> = changes.iter().map(|c| format!("{} {} {}", c.change, c.item, c.name)).collect();
    assert_eq!(
        summary,
        ["changed setting title", "removed field Plan", "removed field Plus code", "changed field signature", "removed field signature"]
    );

    let doc2 = call::<docs::Create>(&sender, input(json!({ "title": "Lease B", "templateId": template }))).await.unwrap();
    assert_eq!(doc2.document.template_version, Some(2));
    assert_eq!(doc2.fields.len(), 1);
    let listed = versions(&sender).await.unwrap();
    assert_eq!(listed.versions.iter().map(|v| (v.number, v.document_count)).collect::<Vec<_>>(), [(2, 1), (1, 1)]);
    assert!(listed.versions[0].note.is_none());

    // Restoring version 1 brings back the fields, title and personal number, as version 3.
    call::<docs::Update>(&manager, input(json!({ "id": template, "message": "Unsaved" }))).await.unwrap();
    assert_eq!(
        kind(call::<templates::Restore>(&sender, input(json!({ "templateId": template, "number": 1 }))).await),
        ErrorKind::Forbidden
    );
    let restored = call::<templates::Restore>(&manager, input(json!({ "templateId": template, "number": 1 }))).await.unwrap();
    assert_eq!(restored.document.title, "Lease");
    assert_eq!(restored.fields.len(), 4);
    assert_eq!(restored.files.iter().map(|f| f.id).collect::<Vec<_>>(), [first_file]);
    assert_eq!(restored.parties[0].personal_number.as_deref(), Some("•••••••••1234"));
    let after = versions(&manager).await.unwrap();
    assert_eq!(
        after.versions.iter().map(|v| (v.number, v.restored_from)).collect::<Vec<_>>(),
        [(4, Some(1)), (3, None), (2, None), (1, None)]
    );
    assert!(!after.changed);
    let unsaved = call::<templates::GetVersion>(&manager, input(json!({ "templateId": template, "number": 3 }))).await.unwrap();
    assert_eq!(unsaved.content.message, "Unsaved");
    let first = call::<templates::GetVersion>(&sender, input(json!({ "templateId": template, "number": 1 }))).await.unwrap();
    assert_eq!(first.documents.iter().map(|d| d.id).collect::<Vec<_>>(), [doc1.document.id]);
    assert_eq!(first.content.parties[0].personal_number.as_deref(), Some("•••••••••1234"));
    assert!(
        call::<templates::Compare>(&manager, input(json!({ "templateId": template, "from": 1, "to": 4 })))
            .await
            .unwrap()
            .changes
            .is_empty()
    );
    assert_eq!(
        kind(call::<templates::Restore>(&manager, input(json!({ "templateId": template, "number": 4 }))).await),
        ErrorKind::Conflict
    );

    // Versions of a document that is not a template do not exist.
    assert_eq!(kind(versions_of(&sender, doc1.document.id).await), ErrorKind::BadRequest);

    let stored: String = sqlx::query_scalar("select snapshot::text from template_versions where template_id = $1 and number = 1")
        .bind(template)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert!(!stored.contains("19800101"));
}

async fn versions_of(ctx: &Ctx, id: Uuid) -> opensign_core::Result<templates::VersionsOutput> {
    call::<templates::Versions>(ctx, input(json!({ "templateId": id }))).await
}

#[tokio::test]
async fn replacing_a_file_keeps_fields_and_history() {
    let Some(app) = app().await else { return };
    let manager = person(&app, Role::Manager).await;
    let template = call::<docs::Create>(&manager, input(json!({ "title": "Offer", "isTemplate": true }))).await.unwrap().document.id;
    let cover = upload(&manager, template, 1).await;
    let terms = upload(&manager, template, 3).await;
    let tenant = call::<docs::SetParties>(
        &manager,
        input(json!({ "documentId": template, "parties": [{ "name": "Buyer", "email": "b@example.com" }] })),
    )
    .await
    .unwrap()[0]
        .id;
    let box_on = |page: i32, label: &str| json!({ "partyId": tenant, "kind": "text", "label": label, "page": page, "x": 0.1, "y": 0.1, "width": 0.3, "height": 0.04 });
    set_fields(&manager, template, json!([box_on(0, "Cover"), box_on(1, "Terms 1"), box_on(3, "Terms 3")])).await;

    // The 3-page terms become 2 pages: the field on its third page moves to its last.
    let replaced = docs::replace_file(&manager, template, terms, "Terms v2.pdf", sample_pdf(2)).await.unwrap();
    assert_eq!(replaced.pages, 2);
    assert_eq!(replaced.moved.iter().map(|m| (m.label.as_str(), m.page)).collect::<Vec<_>>(), [("Terms 3", 2)]);
    let doc = call::<docs::Get>(&manager, input(json!({ "id": template }))).await.unwrap();
    assert_eq!(doc.files.iter().map(|f| f.id).collect::<Vec<_>>(), [cover, replaced.file.id]);
    assert_eq!(doc.files[1].name, "Terms v2.pdf");
    let pages: Vec<(String, i32)> = doc.fields.iter().map(|f| (f.label.clone(), f.page)).collect();
    assert_eq!(pages, [("Cover".to_string(), 0), ("Terms 1".to_string(), 1), ("Terms 3".to_string(), 2)]);

    // The state before the replace is version 1, and its PDF still opens.
    let history = versions_of(&manager, template).await.unwrap();
    assert_eq!(history.versions.len(), 1);
    assert!(history.changed);
    let (_, old) = docs::preview_pdf(&manager, template, Some(1)).await.unwrap();
    assert_eq!(opensign_pdf::page_sizes(&old).unwrap().len(), 4);
    let (_, new) = docs::preview_pdf(&manager, template, None).await.unwrap();
    assert_eq!(opensign_pdf::page_sizes(&new).unwrap().len(), 3);

    // Removing the terms file from the template does not delete it while version 1 names it.
    call::<docs::RemoveFile>(&manager, input(json!({ "documentId": template, "fileId": replaced.file.id }))).await.unwrap();
    let row = sqlx::query_as::<_, opensign_core::documents::DocumentRow>(&format!(
        "select {} from documents where id = $1",
        opensign_core::documents::DOCUMENT_COLUMNS
    ))
    .bind(template)
    .fetch_one(&app.db)
    .await
    .unwrap();
    let mut conn = app.db.acquire().await.unwrap();
    let other = call::<docs::Create>(&manager, input(json!({ "title": "Other", "isTemplate": true }))).await.unwrap().document.id;
    sqlx::query("insert into document_files (document_id, file_id, position) values ($1, $2, 1)")
        .bind(other)
        .bind(terms)
        .execute(&mut *conn)
        .await
        .unwrap();
    let other_row = sqlx::query_as::<_, opensign_core::documents::DocumentRow>(&format!(
        "select {} from documents where id = $1",
        opensign_core::documents::DOCUMENT_COLUMNS
    ))
    .bind(other)
    .fetch_one(&mut *conn)
    .await
    .unwrap();
    opensign_core::documents::purge(&manager, &mut conn, &other_row, "test").await.unwrap();
    let kept: bool =
        sqlx::query_scalar("select exists (select 1 from files where id = $1)").bind(terms).fetch_one(&mut *conn).await.unwrap();
    assert!(kept, "version 1 still names the old terms");
    opensign_core::documents::purge(&manager, &mut conn, &row, "test").await.unwrap();
    let gone: bool =
        sqlx::query_scalar("select exists (select 1 from files where id = $1)").bind(terms).fetch_one(&mut *conn).await.unwrap();
    assert!(!gone, "deleting the template deletes the files only its versions used");
}

#[tokio::test]
async fn text_tags_in_a_replacement_win_over_the_old_fields() {
    let Some(app) = app().await else { return };
    let manager = person(&app, Role::Manager).await;
    let template = call::<docs::Create>(&manager, input(json!({ "title": "Lease", "isTemplate": true }))).await.unwrap().document.id;
    let file = upload(&manager, template, 1).await;
    let landlord = call::<docs::SetParties>(
        &manager,
        input(json!({ "documentId": template, "parties": [{ "name": "Landlord", "email": "l@example.com" }] })),
    )
    .await
    .unwrap()[0]
        .id;
    set_fields(
        &manager,
        template,
        json!([{ "partyId": landlord, "kind": "text", "label": "Old", "page": 0, "x": 0.1, "y": 0.1, "width": 0.3, "height": 0.04 }]),
    )
    .await;

    let tagged = include_bytes!("../../pdf/fixtures/word-tags.pdf").to_vec();
    let out = docs::replace_file(&manager, template, file, "Lease with tags.pdf", tagged).await.unwrap();
    assert_eq!(out.fields.text_tags, 9, "{:?}", out.fields.problems);
    assert_eq!(out.fields.parties_added, ["Tenant"]);
    assert!(out.moved.is_empty());
    let doc = call::<docs::Get>(&manager, input(json!({ "id": template }))).await.unwrap();
    assert_eq!(doc.fields.len(), 9);
    assert!(doc.fields.iter().all(|f| f.label != "Old"));
    let old = call::<templates::GetVersion>(&manager, input(json!({ "templateId": template, "number": 1 }))).await.unwrap();
    assert_eq!(old.content.fields[0].label, "Old");
}
