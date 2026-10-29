#![allow(clippy::unwrap_used)]

use std::sync::Arc;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use opensign_core::context::{Actor, App, Credential, Ctx, Surface};
use opensign_core::error::ErrorKind;
use opensign_core::services::documents::{self as docs, IdInput};
use opensign_core::services::exports::{Export, ExportInput, Values};
use opensign_core::testing::app;
use opensign_core::{Role, call};
use serde_json::{Value, json};
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

async fn document(ctx: &Ctx, input: Value) -> Uuid {
    call::<docs::Create>(ctx, serde_json::from_value(input).unwrap()).await.unwrap().document.id
}

async fn party(app: &Arc<App>, document: Uuid, name: &str, order: i32) -> Uuid {
    let id = Uuid::now_v7();
    sqlx::query(
        "insert into parties (id, document_id, name, email, personal_number, sign_order, status, signed_at)
         values ($1, $2, $3, $4, 'v1.secret', $5, 'signed', now())",
    )
    .bind(id)
    .bind(document)
    .bind(name)
    .bind(format!("{}@example.com", name.to_lowercase()))
    .bind(order)
    .execute(&app.db)
    .await
    .unwrap();
    id
}

async fn field(app: &Arc<App>, document: Uuid, party: Uuid, kind: &str, label: &str, value: &str, y: f64) {
    sqlx::query("insert into fields (id, document_id, party_id, kind, page, x, y, width, height, label, value) values ($1, $2, $3, $4, 1, 0.1, $5, 0.2, 0.05, $6, $7)")
        .bind(Uuid::now_v7())
        .bind(document)
        .bind(party)
        .bind(kind)
        .bind(y)
        .bind(label)
        .bind(value)
        .execute(&app.db)
        .await
        .unwrap();
}

async fn export(ctx: &Ctx, input: Value) -> opensign_core::Result<opensign_core::services::exports::ExportOutput> {
    call::<Export>(ctx, serde_json::from_value::<ExportInput>(input).unwrap()).await
}

#[tokio::test]
async fn values_and_exports_respect_access() {
    let Some(app) = app().await else { return };
    let sara = person(&app, Role::Sender).await;
    let bo = person(&app, Role::Sender).await;
    let auditor = person(&app, Role::Auditor).await;
    let viewer = person(&app, Role::Viewer).await;
    let manager = person(&app, Role::Manager).await;

    let template = document(&manager, json!({ "title": "Lease", "isTemplate": true })).await;
    let mut leases = Vec::new();
    for (tenant, rent, pets) in [("Anna", "12500", "true"), ("Cecilia", "-3", "false")] {
        let id = document(&sara, json!({ "title": format!("Lease {tenant}"), "templateId": template, "externalId": "=1+1" })).await;
        let p = party(&app, id, tenant, 1).await;
        let landlord = party(&app, id, "Sara", 2).await;
        field(&app, id, p, "number", "Rent", rent, 0.1).await;
        field(&app, id, p, "checkbox", "Pets", pets, 0.2).await;
        field(&app, id, p, "text", "Comment", "=HYPERLINK(\"http://evil.test\")", 0.3).await;
        field(&app, id, p, "name", "Name", tenant, 0.4).await;
        field(&app, id, landlord, "name", "Name", "Sara", 0.5).await;
        field(&app, id, landlord, "signature", "Signature", "signed", 0.6).await;
        sqlx::query("update documents set status = 'completed', completed_at = now() where id = $1")
            .bind(id)
            .execute(&app.db)
            .await
            .unwrap();
        leases.push(id);
    }
    let bos = document(&bo, json!({ "title": "Bo's own" })).await;

    let values = call::<Values>(&sara, IdInput { id: leases[0] }).await.unwrap();
    assert_eq!(values.values["Rent"], json!(12500));
    assert_eq!(values.values["Pets"], json!(true));
    assert_eq!(values.values["Name (Party 1)"], json!("Anna"));
    assert_eq!(values.values["Name (Party 2)"], json!("Sara"));
    assert!(!values.values.contains_key("Signature"));
    assert!(!serde_json::to_string(&values).unwrap().contains("secret"));
    assert_eq!(kind(call::<Values>(&bo, IdInput { id: leases[0] }).await), ErrorKind::NotFound);

    let csv = export(&sara, json!({ "templateId": template })).await.unwrap();
    assert_eq!(csv.count, 2);
    assert!(csv.name.starts_with("Lease ") && csv.name.ends_with(".csv"), "{}", csv.name);
    let text = String::from_utf8(B64.decode(csv.content_base64.unwrap()).unwrap()).unwrap();
    let mut lines = text.lines();
    assert_eq!(
        lines.next().unwrap().trim_start_matches('\u{feff}'),
        "Document ID,Title,Reference,Status,Created (UTC),Sent (UTC),Completed (UTC),Party 1 Name,Party 1 Email,Party 1 Signed (UTC),Party 2 Name,Party 2 Email,Party 2 Signed (UTC),Rent,Pets,Comment,Name (Party 1),Name (Party 2)"
    );
    assert!(text.contains(",12500,true,\"'=HYPERLINK(\"\"http://evil.test\"\")\",Anna,Sara"), "{text}");
    assert!(text.contains(",-3,false,") && text.contains(",'=1+1,"), "{text}");

    let json_export = export(&sara, json!({ "format": "json", "ids": [leases[1], bos] })).await.unwrap();
    assert_eq!(json_export.count, 1, "Bo's document is not Sara's to export");
    assert_eq!(json_export.documents.unwrap()[0].values["Rent"], json!(-3));

    let xlsx = export(&auditor, json!({ "format": "xlsx", "status": "completed", "templateId": template })).await.unwrap();
    assert_eq!(xlsx.count, 2);
    assert!(B64.decode(xlsx.content_base64.unwrap()).unwrap().starts_with(b"PK"));

    assert_eq!(export(&sara, json!({ "templateId": template, "archived": true })).await.unwrap().count, 0);
    assert_eq!(export(&sara, json!({ "templateId": template, "status": "pending" })).await.unwrap().count, 0);
    assert_eq!(kind(export(&viewer, json!({})).await), ErrorKind::Forbidden);
    assert_eq!(kind(export(&sara, json!({ "bulkId": Uuid::now_v7() })).await), ErrorKind::NotFound);
    assert_eq!(kind(export(&sara, json!({ "ids": [] })).await), ErrorKind::BadRequest);

    let audited: i64 = sqlx::query_scalar(
        "select count(*) from audit_log where action = 'documents.exported' and actor_id = $1 and metadata->>'format' = 'csv'",
    )
    .bind(sara.actor.as_ref().unwrap().user_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert!(audited >= 1);
}
