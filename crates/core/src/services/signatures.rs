//! A user's own saved signature and initials, offered when they sign a document themselves.

use async_trait::async_trait;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::PgConnection;
use uuid::Uuid;

use super::signing::{clean_value, retire_file, signature_png};
use crate::audit::{Entry, audit};
use crate::context::Ctx;
use crate::error::{Result, bad_request};
use crate::service::{Access, Meta, Service};
use crate::storage::{NewFile, file_record, read_file, store_file};

#[derive(Serialize, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct SavedSignature {
    /// Nothing is saved yet.
    pub empty: bool,
    /// The name as typed, drawn in a script style when there is no image.
    pub typed_name: Option<String>,
    /// A drawn signature as PNG, base64.
    pub image_png_base64: Option<String>,
    pub initials: Option<String>,
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(sqlx::FromRow)]
struct Stored {
    signature_file_id: Option<Uuid>,
    signature_typed: Option<String>,
    signature_initials: Option<String>,
    signature_updated_at: Option<DateTime<Utc>>,
}

async fn stored(conn: &mut PgConnection, user_id: Uuid) -> Result<Stored> {
    Ok(sqlx::query_as(
        "select signature_file_id, signature_typed, signature_initials, signature_updated_at from users where id = $1 for update",
    )
    .bind(user_id)
    .fetch_one(&mut *conn)
    .await?)
}

async fn load(ctx: &Ctx, conn: &mut PgConnection, user_id: Uuid) -> Result<SavedSignature> {
    let row: Stored =
        sqlx::query_as("select signature_file_id, signature_typed, signature_initials, signature_updated_at from users where id = $1")
            .bind(user_id)
            .fetch_one(&mut *conn)
            .await?;
    let image = match row.signature_file_id {
        Some(id) => {
            let record = file_record(conn, id).await?;
            Some(B64.encode(read_file(&ctx.app.storage, ctx.keys(), &record).await?))
        }
        None => None,
    };
    Ok(SavedSignature {
        empty: image.is_none() && row.signature_typed.is_none() && row.signature_initials.is_none(),
        typed_name: row.signature_typed,
        image_png_base64: image,
        initials: row.signature_initials,
        updated_at: row.signature_updated_at,
    })
}

pub struct Get;

#[async_trait]
impl Service for Get {
    type Input = super::Empty;
    type Output = SavedSignature;
    const META: Meta = Meta::query(
        "account.signature",
        "Your saved signature and initials, offered on the signing page when you sign a document yourself.",
        Access::Authenticated,
    )
    .not_a_tool();

    async fn run(ctx: &Ctx, conn: &mut PgConnection, _: super::Empty) -> Result<SavedSignature> {
        load(ctx, conn, ctx.actor()?.user_id).await
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SetSignatureInput {
    /// The name as typed. Used when there is no image.
    pub typed_name: Option<String>,
    /// A drawn signature as PNG, base64. Replaces the typed name when given.
    pub image_png_base64: Option<String>,
    /// Up to five letters, filled into initials fields.
    pub initials: Option<String>,
}

pub struct Set;

#[async_trait]
impl Service for Set {
    type Input = SetSignatureInput;
    type Output = SavedSignature;
    const META: Meta = Meta::mutation(
        "account.set_signature",
        "Save your own signature (drawn or typed) and initials, to reuse when you sign documents yourself.",
        Access::Authenticated,
    )
    .not_a_tool();

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: SetSignatureInput) -> Result<SavedSignature> {
        let user_id = ctx.actor()?.user_id;
        let typed = input.typed_name.as_deref().map(clean_value).transpose()?.filter(|n| !n.is_empty());
        if typed.as_ref().is_some_and(|n| n.chars().count() > 100 || n.contains('\n')) {
            return Err(bad_request("The typed signature is too long"));
        }
        let initials = input.initials.as_deref().map(clean_value).transpose()?.filter(|n| !n.is_empty());
        if initials.as_ref().is_some_and(|n| n.chars().count() > 5 || n.contains('\n')) {
            return Err(bad_request("Initials are at most five letters"));
        }
        let png = input.image_png_base64.as_deref().filter(|s| !s.is_empty()).map(signature_png).transpose()?;
        if png.is_none() && typed.is_none() {
            return Err(bad_request("Draw or type your signature"));
        }
        let before = stored(conn, user_id).await?;
        let file = match &png {
            Some(body) => Some(
                store_file(
                    conn,
                    &ctx.app.storage,
                    ctx.keys(),
                    NewFile { kind: "saved_signature", name: "signature.png", content_type: "image/png", body, uploaded_by: Some(user_id) },
                )
                .await?,
            ),
            None => None,
        };
        sqlx::query(
            "update users set signature_file_id = $2, signature_typed = $3, signature_initials = $4, signature_updated_at = $5 where id = $1",
        )
        .bind(user_id)
        .bind(file.as_ref().map(|f| f.id))
        .bind(if png.is_some() { None } else { typed.as_ref() })
        .bind(&initials)
        .bind(ctx.now)
        .execute(&mut *conn)
        .await?;
        retire_file(conn, before.signature_file_id).await?;
        audit(
            ctx,
            conn,
            Entry::new("account.signature_saved", "user", user_id).metadata(
                json!({ "drawn": png.is_some(), "initials": initials.is_some(), "sha256": file.as_ref().map(|f| f.sha256.clone()) }),
            ),
        )
        .await?;
        load(ctx, conn, user_id).await
    }
}

pub struct Remove;

#[async_trait]
impl Service for Remove {
    type Input = super::Empty;
    type Output = super::Ok_;
    const META: Meta =
        Meta::mutation("account.remove_signature", "Delete your saved signature and initials.", Access::Authenticated).not_a_tool();

    async fn run(ctx: &Ctx, conn: &mut PgConnection, _: super::Empty) -> Result<super::Ok_> {
        let user_id = ctx.actor()?.user_id;
        let before = stored(conn, user_id).await?;
        if before.signature_updated_at.is_none() {
            return Ok(super::Ok_::default());
        }
        sqlx::query(
            "update users set signature_file_id = null, signature_typed = null, signature_initials = null, signature_updated_at = null where id = $1",
        )
        .bind(user_id)
        .execute(&mut *conn)
        .await?;
        retire_file(conn, before.signature_file_id).await?;
        audit(ctx, conn, Entry::new("account.signature_removed", "user", user_id)).await?;
        Ok(super::Ok_::default())
    }
}
