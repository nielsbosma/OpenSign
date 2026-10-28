//! Retention rules and legal holds. A daily job deletes documents the rules say are due, with
//! their files; a legal hold keeps a document out of reach of both the rules and manual deletion.

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::PgConnection;
use uuid::Uuid;

use super::settings;
use crate::audit::{Entry, audit};
use crate::context::Ctx;
use crate::documents::{DocumentRow, Need, load_document, purge, status};
use crate::error::{Result, bad_request, conflict};
use crate::permissions::Permission;
use crate::service::{Access, Meta, Service};
use crate::settings::RetentionSettings;

const BATCH: i64 = 100;

/// Documents the rules make due at `now`, oldest first. Templates and held documents never are.
const DUE: &str = "select id from documents
     where not legal_hold and not is_template and (
       ($1 > 0 and status = 'completed' and completed_at < $4 - make_interval(days => $1))
       or ($2 > 0 and status in ('declined', 'canceled', 'expired') and updated_at < $4 - make_interval(days => $2))
       or ($3 > 0 and status = 'draft' and updated_at < $4 - make_interval(days => $3)))
     order by updated_at";

fn days(value: u32) -> i32 {
    value.min(36_500) as i32
}

/// Deletes up to one batch of due documents. Returns how many it deleted.
pub async fn run(ctx: &Ctx, conn: &mut PgConnection) -> Result<usize> {
    let rules: RetentionSettings = settings(ctx, conn).await?;
    if rules.completed_days == 0 && rules.ended_days == 0 && rules.draft_days == 0 {
        return Ok(0);
    }
    let due: Vec<Uuid> = sqlx::query_scalar(&format!("{DUE} limit {BATCH}"))
        .bind(days(rules.completed_days))
        .bind(days(rules.ended_days))
        .bind(days(rules.draft_days))
        .bind(ctx.now)
        .fetch_all(&mut *conn)
        .await?;
    for id in &due {
        let document: DocumentRow =
            sqlx::query_as(&format!("select {} from documents d where id = $1", crate::documents::DOCUMENT_COLUMNS))
                .bind(id)
                .fetch_one(&mut *conn)
                .await?;
        purge(ctx, conn, &document, "retention").await?;
    }
    Ok(due.len())
}

#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RetentionPreview {
    /// Documents the current rules would delete at the next run.
    pub completed: i64,
    pub ended: i64,
    pub drafts: i64,
    /// Documents on legal hold, which no rule deletes.
    pub held: i64,
    pub oldest_held_since: Option<DateTime<Utc>>,
}

pub struct Preview;

#[async_trait]
impl Service for Preview {
    type Input = super::Empty;
    type Output = RetentionPreview;
    const META: Meta = Meta::query(
        "retention.preview",
        "Count the documents the retention rules would delete at the next run, by rule, and the documents on legal hold.",
        Access::Permission(Permission::SettingsManage),
    );

    async fn run(ctx: &Ctx, conn: &mut PgConnection, _input: super::Empty) -> Result<RetentionPreview> {
        let rules: RetentionSettings = settings(ctx, conn).await?;
        let mut counts = [0i64; 3];
        let rules = [
            ("completed", rules.completed_days, "completed_at"),
            ("declined,canceled,expired", rules.ended_days, "updated_at"),
            ("draft", rules.draft_days, "updated_at"),
        ];
        for (count, (statuses, after, column)) in counts.iter_mut().zip(rules) {
            *count = sqlx::query_scalar(&format!(
                "select count(*) from documents where not legal_hold and not is_template and status = any(string_to_array($1, ','))
                   and $2 > 0 and {column} < $3 - make_interval(days => $2)"
            ))
            .bind(statuses)
            .bind(days(after))
            .bind(ctx.now)
            .fetch_one(&mut *conn)
            .await?;
        }
        let (held, oldest_held_since): (i64, Option<DateTime<Utc>>) =
            sqlx::query_as("select count(*), min(legal_hold_at) from documents where legal_hold").fetch_one(&mut *conn).await?;
        Ok(RetentionPreview { completed: counts[0], ended: counts[1], drafts: counts[2], held, oldest_held_since })
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LegalHoldInput {
    pub id: Uuid,
    /// True places the hold, false releases it.
    pub hold: bool,
    /// Why the document must be kept, such as a case or matter reference. Required to place a hold.
    #[serde(default)]
    pub reason: String,
}

#[derive(Serialize, JsonSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct LegalHoldView {
    pub id: Uuid,
    pub legal_hold: bool,
    pub legal_hold_reason: Option<String>,
    pub legal_hold_at: Option<DateTime<Utc>>,
}

pub struct SetLegalHold;

#[async_trait]
impl Service for SetLegalHold {
    type Input = LegalHoldInput;
    type Output = LegalHoldView;
    const META: Meta = Meta::mutation(
        "documents.legal_hold",
        "Place a document on legal hold, so no retention rule or person can delete it, or release the hold. Placing a hold needs a reason such as a case reference.",
        Access::Permission(Permission::DocumentsLegalHold),
    );

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: LegalHoldInput) -> Result<LegalHoldView> {
        let document = load_document(ctx, conn, input.id, Need::Read).await?;
        if document.is_template {
            return Err(bad_request("Templates are never deleted by retention rules"));
        }
        let reason = input.reason.trim();
        if input.hold && reason.is_empty() {
            return Err(bad_request("Give a reason for the hold, such as a case reference"));
        }
        if reason.chars().count() > 500 {
            return Err(bad_request("Keep the reason under 500 characters"));
        }
        if input.hold == document.legal_hold && (!input.hold || document.legal_hold_reason.as_deref() == Some(reason)) {
            return Err(conflict(if input.hold { "The document is already on hold" } else { "The document is not on hold" }));
        }
        let actor = ctx.actor()?.user_id;
        let view: LegalHoldView = if input.hold {
            sqlx::query_as(
                "update documents set legal_hold = true, legal_hold_reason = $2, legal_hold_at = coalesce(legal_hold_at, $3), legal_hold_by = $4, updated_at = now()
                 where id = $1 returning id, legal_hold, legal_hold_reason, legal_hold_at",
            )
            .bind(document.id)
            .bind(reason)
            .bind(ctx.now)
            .bind(actor)
            .fetch_one(&mut *conn)
            .await?
        } else {
            sqlx::query_as(
                "update documents set legal_hold = false, legal_hold_reason = null, legal_hold_at = null, legal_hold_by = null, updated_at = now()
                 where id = $1 returning id, legal_hold, legal_hold_reason, legal_hold_at",
            )
            .bind(document.id)
            .fetch_one(&mut *conn)
            .await?
        };
        let action = if input.hold { "document.legal_hold_placed" } else { "document.legal_hold_released" };
        audit(
            ctx,
            conn,
            Entry::new(action, "document", document.id)
                .before(json!({ "legalHold": document.legal_hold, "reason": document.legal_hold_reason }))
                .after(json!({ "legalHold": input.hold, "reason": input.hold.then_some(reason) })),
        )
        .await?;
        Ok(view)
    }
}

/// When the rules will next be able to delete a document, for display. `None` when no rule applies.
pub fn deletes_at(rules: &RetentionSettings, document: &DocumentRow) -> Option<DateTime<Utc>> {
    if document.legal_hold || document.is_template {
        return None;
    }
    let (days, from) = match document.status.as_str() {
        status::COMPLETED => (rules.completed_days, document.completed_at?),
        status::DECLINED | status::CANCELED | status::EXPIRED => (rules.ended_days, document.updated_at),
        status::DRAFT => (rules.draft_days, document.updated_at),
        _ => return None,
    };
    (days > 0).then(|| from + Duration::days(i64::from(days)))
}
