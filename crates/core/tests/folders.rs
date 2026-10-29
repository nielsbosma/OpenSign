#![allow(clippy::unwrap_used)]

use std::sync::Arc;

use opensign_core::context::{Actor, App, Credential, Ctx, Surface};
use opensign_core::error::ErrorKind;
use opensign_core::services::documents as docs;
use opensign_core::services::folders::{self, Scope};
use opensign_core::testing::app;
use opensign_core::{Role, call};
use serde_json::json;
use uuid::Uuid;

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

fn kind<T>(result: opensign_core::Result<T>) -> ErrorKind {
    match result {
        Err(e) => e.kind,
        Ok(_) => panic!("expected an error"),
    }
}

async fn folder(ctx: &Ctx, scope: Scope, name: &str, parent_id: Option<Uuid>) -> opensign_core::Result<folders::FolderView> {
    call::<folders::Create>(ctx, folders::CreateInput { scope, name: name.into(), parent_id }).await
}

async fn document(ctx: &Ctx, input: serde_json::Value) -> Uuid {
    call::<docs::Create>(ctx, serde_json::from_value(input).unwrap()).await.unwrap().document.id
}

async fn list(ctx: &Ctx, input: serde_json::Value) -> Vec<Uuid> {
    call::<docs::List>(ctx, serde_json::from_value(input).unwrap()).await.unwrap().into_iter().map(|d| d.id).collect()
}

#[tokio::test]
async fn folders_nest_move_and_delete_without_losing_anything() {
    let Some(app) = app().await else { return };
    let sara = person(&app, Role::Sender).await;
    let bo = person(&app, Role::Sender).await;
    let manager = person(&app, Role::Manager).await;
    let viewer = person(&app, Role::Viewer).await;

    let clients = folder(&sara, Scope::Documents, "Clients", None).await.unwrap();
    let acme = folder(&sara, Scope::Documents, "Acme", Some(clients.id)).await.unwrap();
    let y2026 = folder(&sara, Scope::Documents, "2026", Some(acme.id)).await.unwrap();
    assert_eq!(kind(folder(&sara, Scope::Documents, "Too deep", Some(y2026.id)).await), ErrorKind::BadRequest);
    assert_eq!(kind(folder(&sara, Scope::Documents, "acme", Some(clients.id)).await), ErrorKind::Conflict);
    assert_eq!(kind(folder(&sara, Scope::Documents, "  ", None).await), ErrorKind::BadRequest);
    assert_eq!(kind(folder(&viewer, Scope::Documents, "Mine", None).await), ErrorKind::Forbidden);
    assert_eq!(kind(folder(&sara, Scope::Templates, "Leases", None).await), ErrorKind::Forbidden);
    let leases = folder(&manager, Scope::Templates, "Leases", None).await.unwrap();
    assert_eq!(kind(folder(&manager, Scope::Templates, "Mixed", Some(clients.id)).await), ErrorKind::BadRequest);

    let rename = |ctx: &Ctx, id: Uuid, name: &str, parent_id: Option<Uuid>| {
        let ctx = ctx.clone();
        let name = name.to_string();
        async move { call::<folders::Update>(&ctx, folders::UpdateInput { id, name, parent_id }).await }
    };
    assert_eq!(kind(rename(&bo, acme.id, "Acme AB", Some(clients.id)).await), ErrorKind::Forbidden);
    assert_eq!(kind(rename(&sara, clients.id, "Clients", Some(y2026.id)).await), ErrorKind::BadRequest);
    let other = folder(&sara, Scope::Documents, "Other", None).await.unwrap();
    assert_eq!(kind(rename(&sara, clients.id, "Clients", Some(other.id)).await), ErrorKind::BadRequest, "would be four deep");
    assert_eq!(rename(&manager, acme.id, "Acme AB", Some(clients.id)).await.unwrap().name, "Acme AB");

    let contract = document(&sara, json!({ "title": "Contract", "folderId": acme.id })).await;
    let loose = document(&sara, json!({ "title": "Loose" })).await;
    let bos = document(&bo, json!({ "title": "Bo's" })).await;
    assert_eq!(
        kind(call::<docs::Create>(&sara, serde_json::from_value(json!({ "title": "x", "folderId": leases.id })).unwrap()).await),
        ErrorKind::BadRequest
    );

    assert_eq!(list(&sara, json!({ "folderId": acme.id })).await, vec![contract]);
    assert_eq!(list(&sara, json!({})).await.len(), 2);

    let move_to = |ctx: &Ctx, ids: Vec<Uuid>, folder_id: Option<Uuid>| {
        let ctx = ctx.clone();
        async move { call::<folders::Move>(&ctx, folders::MoveInput { ids, folder_id }).await }
    };
    assert_eq!(kind(move_to(&sara, vec![bos], Some(acme.id)).await), ErrorKind::NotFound, "a folder never grants access");
    move_to(&bo, vec![bos], Some(acme.id)).await.unwrap();
    assert_eq!(list(&sara, json!({ "folderId": acme.id })).await, vec![contract], "Bo's document stays hidden from Sara");
    let in_acme = call::<folders::List>(&sara, folders::ListInput { scope: Scope::Documents }).await.unwrap();
    assert_eq!(in_acme.iter().find(|f| f.id == acme.id).unwrap().item_count, 1);
    assert_eq!(kind(move_to(&sara, vec![loose], Some(leases.id)).await), ErrorKind::BadRequest);
    move_to(&sara, vec![loose, contract], Some(y2026.id)).await.unwrap();

    call::<folders::Delete>(&sara, folders::IdInput { id: acme.id }).await.unwrap();
    let left = call::<folders::List>(&manager, folders::ListInput { scope: Scope::Documents }).await.unwrap();
    assert!(left.iter().all(|f| f.id != acme.id));
    assert_eq!(left.iter().find(|f| f.id == y2026.id).unwrap().parent_id, Some(clients.id));
    assert_eq!(list(&manager, json!({ "folderId": clients.id })).await, vec![bos]);
    assert_eq!(list(&manager, json!({ "folderId": y2026.id })).await.len(), 2);
}

#[tokio::test]
async fn only_finished_documents_and_templates_are_archived() {
    let Some(app) = app().await else { return };
    let sara = person(&app, Role::Sender).await;
    let manager = person(&app, Role::Manager).await;

    let draft = document(&sara, json!({ "title": "Draft" })).await;
    let done = document(&sara, json!({ "title": "Done" })).await;
    sqlx::query("update documents set status = 'completed', completed_at = now() where id = $1").bind(done).execute(&app.db).await.unwrap();
    let archive = |ctx: &Ctx, ids: Vec<Uuid>, archived: bool| {
        let ctx = ctx.clone();
        async move { call::<folders::Archive>(&ctx, folders::ArchiveInput { ids, archived }).await }
    };
    assert_eq!(kind(archive(&sara, vec![draft, done], true).await), ErrorKind::Conflict);
    assert_eq!(archive(&sara, vec![done], true).await.unwrap().changed, 1);
    assert_eq!(list(&sara, json!({})).await, vec![draft]);
    assert_eq!(list(&sara, json!({ "archived": true })).await, vec![done]);
    let counts = call::<docs::Counts>(&sara, serde_json::from_value(json!({})).unwrap()).await.unwrap();
    assert_eq!((counts.all, counts.archived), (1, 1));
    assert!(!counts.by_status.contains_key("completed"));

    let template = document(&manager, json!({ "title": "Lease", "isTemplate": true })).await;
    assert_eq!(kind(archive(&sara, vec![template], true).await), ErrorKind::Forbidden);
    archive(&manager, vec![template], true).await.unwrap();
    assert!(list(&sara, json!({ "templates": true })).await.is_empty());
    let use_it = serde_json::from_value(json!({ "title": "From archived", "templateId": template })).unwrap();
    assert_eq!(kind(call::<docs::Create>(&sara, use_it).await), ErrorKind::Conflict);

    archive(&manager, vec![template], false).await.unwrap();
    document(&sara, json!({ "title": "From restored", "templateId": template })).await;
    let actions: Vec<String> = sqlx::query_scalar("select action from audit_log where entity_id = $1 order by id")
        .bind(template.to_string())
        .fetch_all(&app.db)
        .await
        .unwrap();
    assert!(actions.contains(&"document.archived".to_string()) && actions.contains(&"document.unarchived".to_string()));
}
