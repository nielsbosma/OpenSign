#![allow(clippy::unwrap_used)]

use std::sync::Arc;
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use lopdf::{Document, Object, Stream, dictionary};
use opensign_core::context::{Actor, App, Credential, Ctx, Surface};
use opensign_core::error::ErrorKind;
use opensign_core::services::documents as docs;
use opensign_core::services::{retention, templates};
use opensign_core::testing::app;
use opensign_core::{Role, call};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use uuid::Uuid;

fn sample_pdf() -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let content = doc.add_object(Stream::new(dictionary! {}, b"0 0 m 10 10 l S".to_vec()));
    let page = doc.add_object(dictionary! {
        "Type" => "Page", "Parent" => pages_id, "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()], "Contents" => content,
    });
    doc.objects.insert(pages_id, Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => vec![page.into()], "Count" => 1 }));
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

async fn draft(ctx: &Ctx, title: &str) -> Uuid {
    call::<docs::Create>(ctx, input(json!({ "title": title }))).await.unwrap().document.id
}

async fn upload(ctx: &Ctx, id: Uuid) -> Uuid {
    let upload =
        docs::UploadInput { document_id: id, name: "a.pdf".into(), content_base64: B64.encode(sample_pdf()), ..Default::default() };
    call::<docs::Upload>(ctx, upload).await.unwrap().file.id
}

async fn hold(ctx: &Ctx, id: Uuid, on: bool) {
    let reason = if on { "Case 12" } else { "" };
    call::<retention::SetLegalHold>(ctx, input(json!({ "id": id, "hold": on, "reason": reason }))).await.unwrap();
}

fn one_party(id: Uuid) -> docs::SetPartiesInput {
    input(json!({ "documentId": id, "parties": [{ "name": "Bo Berg", "email": "bo@example.test", "delivery": "link" }] }))
}

async fn audited(app: &Arc<App>, action: &str, id: Uuid) -> i64 {
    sqlx::query_scalar("select count(*) from audit_log where action = $1 and entity_id = $2")
        .bind(action)
        .bind(id.to_string())
        .fetch_one(&app.db)
        .await
        .unwrap()
}

#[tokio::test]
async fn a_legal_hold_stops_edits_and_cancel() {
    let Some(app) = app().await else { return };
    let manager = person(&app, Role::Manager).await;
    let id = draft(&manager, "Held").await;
    let file = upload(&manager, id).await;
    hold(&manager, id, true).await;

    assert_eq!(kind(call::<docs::Update>(&manager, input(json!({ "id": id, "title": "New" }))).await), ErrorKind::Conflict);
    assert_eq!(kind(call::<docs::SetParties>(&manager, one_party(id)).await), ErrorKind::Conflict);
    assert_eq!(kind(call::<docs::SetFields>(&manager, input(json!({ "documentId": id, "fields": [] }))).await), ErrorKind::Conflict);
    assert_eq!(kind(call::<docs::RemoveFile>(&manager, input(json!({ "documentId": id, "fileId": file }))).await), ErrorKind::Conflict);
    let more = docs::UploadInput { document_id: id, name: "b.pdf".into(), content_base64: B64.encode(sample_pdf()), ..Default::default() };
    assert_eq!(kind(call::<docs::Upload>(&manager, more).await), ErrorKind::Conflict);

    hold(&manager, id, false).await;
    call::<docs::SetParties>(&manager, one_party(id)).await.unwrap();
    sqlx::query("update documents set status = 'pending' where id = $1").bind(id).execute(&app.db).await.unwrap();
    hold(&manager, id, true).await;
    assert_eq!(kind(call::<docs::Cancel>(&manager, input(json!({ "id": id }))).await), ErrorKind::Conflict);
    hold(&manager, id, false).await;
    call::<docs::Cancel>(&manager, input(json!({ "id": id }))).await.unwrap();

    let template = call::<docs::Create>(&manager, input(json!({ "title": "Lease", "isTemplate": true }))).await.unwrap().document.id;
    upload(&manager, template).await;
    sqlx::query("update documents set legal_hold = true where id = $1").bind(template).execute(&app.db).await.unwrap();
    let restore = call::<templates::Restore>(&manager, input(json!({ "templateId": template, "number": 1 }))).await;
    assert_eq!(kind(restore), ErrorKind::Conflict);
}

#[tokio::test]
async fn only_someone_who_manages_all_deletes_what_has_been_signed() {
    let Some(app) = app().await else { return };
    let sender = person(&app, Role::Sender).await;
    let admin = person(&app, Role::Admin).await;
    let id = draft(&sender, "Partly signed").await;
    call::<docs::SetParties>(&sender, one_party(id)).await.unwrap();
    sqlx::query("update parties set signed_at = now() where document_id = $1").bind(id).execute(&app.db).await.unwrap();
    sqlx::query("update documents set status = 'canceled' where id = $1").bind(id).execute(&app.db).await.unwrap();

    assert_eq!(kind(call::<docs::Delete>(&sender, input(json!({ "id": id }))).await), ErrorKind::Forbidden);
    call::<docs::Delete>(&admin, input(json!({ "id": id }))).await.unwrap();

    let unsigned = draft(&sender, "Not signed").await;
    call::<docs::Delete>(&sender, input(json!({ "id": unsigned }))).await.unwrap();
}

#[tokio::test]
async fn a_draft_file_that_is_taken_out_is_gone() {
    let Some(app) = app().await else { return };
    let sender = person(&app, Role::Sender).await;
    let id = draft(&sender, "Files").await;
    let removed = upload(&sender, id).await;
    let kept = upload(&sender, id).await;
    call::<docs::RemoveFile>(&sender, input(json!({ "documentId": id, "fileId": removed }))).await.unwrap();

    let count = |file: Uuid| {
        let db = app.db.clone();
        async move { sqlx::query_scalar::<_, i64>("select count(*) from files where id = $1").bind(file).fetch_one(&db).await.unwrap() }
    };
    assert_eq!(count(removed).await, 0);
    assert_eq!(count(kept).await, 1);

    let replacement = docs::replace_file(&sender, id, kept, "c.pdf", sample_pdf()).await.unwrap();
    assert_eq!(count(kept).await, 0);
    assert_eq!(count(replacement.file.id).await, 1);
}

#[tokio::test]
async fn reading_a_sent_document_or_its_links_is_recorded() {
    let Some(app) = app().await else { return };
    let sender = person(&app, Role::Sender).await;
    let id = draft(&sender, "Sent").await;
    let file = upload(&sender, id).await;
    call::<docs::SetParties>(&sender, one_party(id)).await.unwrap();

    docs::preview_pdf(&sender, id, None).await.unwrap();
    assert_eq!(audited(&app, "document.downloaded", id).await, 0, "a draft is the sender's own working copy");
    sqlx::query("update documents set status = 'pending', source_file_id = $2 where id = $1")
        .bind(id)
        .bind(file)
        .execute(&app.db)
        .await
        .unwrap();
    docs::preview_pdf(&sender, id, None).await.unwrap();
    assert_eq!(audited(&app, "document.downloaded", id).await, 1);

    let mut conn = app.db.acquire().await.unwrap();
    let links = docs::handed_over_links(&sender, &mut conn, id).await.unwrap();
    assert_eq!(links.len(), 1);
    assert_eq!(audited(&app, "party.link_issued", id).await, 1);
}

#[tokio::test]
async fn edits_wait_for_a_send_in_progress() {
    let Some(app) = app().await else { return };
    let sender = person(&app, Role::Sender).await;
    let id = draft(&sender, "Racing").await;

    let mut sending = app.db.begin().await.unwrap();
    sqlx::query("select 1 from documents where id = $1 for update").bind(id).execute(&mut *sending).await.unwrap();
    let edit = call::<docs::SetParties>(&sender, one_party(id));
    let mut edit = Box::pin(edit);
    assert!(tokio::time::timeout(Duration::from_millis(300), &mut edit).await.is_err(), "the edit waits on the row");

    sqlx::query("update documents set status = 'pending' where id = $1").bind(id).execute(&mut *sending).await.unwrap();
    sending.commit().await.unwrap();
    assert_eq!(kind(edit.await), ErrorKind::Conflict, "and sees that it was sent");
}
