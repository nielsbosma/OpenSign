use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use chrono::{DateTime, Utc};
use opensign_crypto::{KeyPurpose, random_bytes, sha256_hex};
use opensign_pdf::{
    EvidenceAttachment, EvidenceEvent, EvidenceParty, EvidenceReport, PdfError, SealIdentity, SealOptions, Stamp, StampContent,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::PgConnection;
use uuid::Uuid;

use super::{AuthMethod, DOCUMENT_COLUMNS, DocumentRow, FieldKind, PartyRow, fields, notify, parties, status};
use crate::audit::{Entry, audit};
use crate::context::Ctx;
use crate::error::{AppError, ErrorKind, Result};
use crate::jobs::enqueue;
use crate::services::settings;
use crate::settings::{OrgSettings, SigningSettings};
use crate::storage::{NewFile, file_record, read_file, store_file};

/// After this many failed attempts the document is sealed without a time stamp rather than left
/// waiting on an unreachable time stamp authority.
const TIMESTAMP_ATTEMPTS: i32 = 4;

fn seal_context(id: Uuid) -> String {
    format!("seal:{id}:private_key")
}

fn pdf_err(e: PdfError) -> AppError {
    AppError::internal(e)
}

#[derive(sqlx::FromRow)]
struct IdentityRow {
    id: Uuid,
    private_key: String,
    chain: Vec<String>,
}

pub async fn store_identity(ctx: &Ctx, conn: &mut PgConnection, identity: &SealIdentity) -> Result<Uuid> {
    let id = Uuid::now_v7();
    let (key, chain) = identity.to_parts().map_err(pdf_err)?;
    let cert = &identity.certificate().tbs_certificate;
    let not_after: DateTime<Utc> = DateTime::from(cert.validity.not_after.to_system_time());
    sqlx::query("update seal_identities set active = false where active").execute(&mut *conn).await?;
    sqlx::query("insert into seal_identities (id, private_key, chain, subject, not_after, created_by) values ($1, $2, $3, $4, $5, $6)")
        .bind(id)
        .bind(ctx.keys().encrypt(KeyPurpose::SigningKey, &seal_context(id), &key))
        .bind(chain.iter().map(|c| B64.encode(c)).collect::<Vec<_>>())
        .bind(cert.subject.to_string())
        .bind(not_after)
        .bind(ctx.actor.as_ref().map(|a| a.user_id))
        .execute(&mut *conn)
        .await?;
    audit(ctx, conn, Entry::new("seal.created", "seal", id).after(json!({ "subject": cert.subject.to_string(), "notAfter": not_after })))
        .await?;
    Ok(id)
}

/// The active seal, created on first use as a self-signed certificate in the organisation's name.
pub async fn active_identity(ctx: &Ctx, conn: &mut PgConnection) -> Result<SealIdentity> {
    let row: Option<IdentityRow> =
        sqlx::query_as("select id, private_key, chain from seal_identities where active").fetch_optional(&mut *conn).await?;
    let row = match row {
        Some(row) => row,
        None => {
            let org: OrgSettings = settings(ctx, conn).await?;
            let signing: SigningSettings = settings(ctx, conn).await?;
            let name = if signing.seal_name.trim().is_empty() { org.name.clone() } else { signing.seal_name.clone() };
            let identity = opensign_pdf::generate_seal_identity(&name, &org.country, 10).map_err(pdf_err)?;
            store_identity(ctx, conn, &identity).await?;
            return Ok(identity);
        }
    };
    let key = ctx.keys().decrypt(KeyPurpose::SigningKey, &seal_context(row.id), &row.private_key).map_err(AppError::internal)?;
    let chain: Vec<Vec<u8>> = row.chain.iter().map(|c| B64.decode(c)).collect::<Result<_, _>>().map_err(AppError::internal)?;
    SealIdentity::from_parts(&key, &chain).map_err(pdf_err)
}

/// An RFC 3161 time stamp token over a SHA-256 digest.
pub(crate) async fn timestamp(ctx: &Ctx, url: &str, digest: &[u8; 32]) -> Result<Vec<u8>> {
    let parsed = crate::net::check_outbound_url(url, true)
        .map_err(|e| AppError::new(ErrorKind::BadRequest, format!("The time stamp URL is not usable: {e}")))?;
    let nonce = random_bytes::<16>();
    let response = ctx
        .app
        .http
        .public
        .post(parsed)
        .header("content-type", "application/timestamp-query")
        .body(opensign_pdf::build_timestamp_request(digest, &nonce))
        .send()
        .await
        .map_err(|e| AppError::new(ErrorKind::Unavailable, "The time stamp authority is not reachable").with_cause(e))?;
    if !response.status().is_success() {
        return Err(AppError::new(ErrorKind::Unavailable, format!("The time stamp authority answered {}", response.status())));
    }
    let mut response = response;
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(AppError::internal)? {
        if body.len() + chunk.len() > 64 * 1024 {
            return Err(AppError::new(ErrorKind::Unavailable, "The time stamp response is too large"));
        }
        body.extend_from_slice(&chunk);
    }
    opensign_pdf::parse_timestamp_response(&body, digest, &nonce).map_err(|e| AppError::new(ErrorKind::Unavailable, e.to_string()))
}

fn initials(name: &str) -> String {
    name.split_whitespace().filter_map(|w| w.chars().next()).flat_map(char::to_uppercase).take(3).collect()
}

fn auth_label(party: &PartyRow) -> String {
    let evidence = party.evidence.as_ref();
    let method = evidence.and_then(|e| e["auth"].as_str()).and_then(AuthMethod::parse).unwrap_or(party.auth_to_sign());
    if method == AuthMethod::Standard && evidence.is_some_and(|e| e["account"].is_string()) {
        return "Signed-in OpenSign account".into();
    }
    method.label().to_string()
}

fn fmt_time(t: Option<DateTime<Utc>>) -> String {
    t.map(|t| t.format("%Y-%m-%d %H:%M:%S UTC").to_string()).unwrap_or_default()
}

async fn build_stamps(ctx: &Ctx, conn: &mut PgConnection, document: &DocumentRow, parties: &[PartyRow]) -> Result<Vec<Stamp>> {
    let mut stamps = Vec::new();
    let all = fields(conn, document.id).await?;
    let outcomes = super::logic::evaluate_rows(&all, None);
    for (field, outcome) in all.into_iter().zip(outcomes) {
        if !outcome.visible {
            continue;
        }
        let party = parties.iter().find(|p| Some(p.id) == field.party_id);
        let kind = FieldKind::parse(&field.kind).unwrap_or(FieldKind::Text);
        let content = match (kind, party) {
            (FieldKind::Signature, Some(p)) => {
                let signature = p.signature.clone().unwrap_or(Value::Null);
                let png = match signature["imageFileId"].as_str().and_then(|id| Uuid::try_parse(id).ok()) {
                    Some(id) => Some(read_file(&ctx.app.storage, ctx.keys(), &file_record(conn, id).await?).await?),
                    None => None,
                };
                let name = signature["typedName"].as_str().filter(|n| !n.trim().is_empty()).unwrap_or(&p.name).to_string();
                let caption = format!(
                    "{} \u{b7} {}",
                    p.signed_at.map(|t| t.format("%Y-%m-%d %H:%M UTC").to_string()).unwrap_or_default(),
                    auth_label(p)
                );
                StampContent::Signature { png, name, caption }
            }
            (FieldKind::Initials, Some(p)) => {
                StampContent::Text(field.value.clone().filter(|v| !v.is_empty()).unwrap_or_else(|| initials(&p.name)))
            }
            (FieldKind::Checkbox, _) => StampContent::Check(field.value.as_deref() == Some("true")),
            (FieldKind::Radio, _) => StampContent::Radio(field.value.is_some()),
            (FieldKind::Image, _) => match field.file_id {
                Some(id) => StampContent::Image(read_file(&ctx.app.storage, ctx.keys(), &file_record(conn, id).await?).await?),
                None => continue,
            },
            _ => match field.value.as_deref().filter(|v| !v.is_empty()) {
                Some(value) => StampContent::Text(super::values::display(kind, field.format.as_deref(), value)),
                None => continue,
            },
        };
        if matches!(content, StampContent::Signature { .. }) && party.is_some_and(|p| p.signed_at.is_none()) {
            continue;
        }
        stamps.push(Stamp { page: field.page.max(0) as usize, x: field.x, y: field.y, width: field.width, height: field.height, content });
    }
    Ok(stamps)
}

async fn evidence_events(conn: &mut PgConnection, document_id: Uuid) -> Result<Vec<EvidenceEvent>> {
    let rows: Vec<(DateTime<Utc>, String, String, Value)> = sqlx::query_as(
        "select occurred_at, action, actor_label, metadata from audit_log where entity_type = 'document' and entity_id = $1 order by id",
    )
    .bind(document_id.to_string())
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows
        .into_iter()
        .filter_map(|(at, action, actor, metadata)| {
            let name = metadata["name"].as_str().unwrap_or(&actor).to_string();
            let ip = metadata["ip"].as_str().map(|ip| format!(" from {ip}")).unwrap_or_default();
            let text = match action.as_str() {
                "document.created" => format!("Created by {actor}"),
                "document.sent" => format!("Sent for signing by {actor}"),
                "party.notified" => format!("Invitation sent to {name} by {}", metadata["channel"].as_str().unwrap_or("email")),
                "party.reminded" => format!("Reminder sent to {name}"),
                "party.viewed" => format!("Opened by {name}{ip}"),
                "party.in_person_started" => format!("In-person signing for {name} started by {actor} on their device"),
                "party.authenticated" => format!(
                    "{name} identified with {}",
                    metadata["method"].as_str().and_then(AuthMethod::parse).map(AuthMethod::label).unwrap_or("a code")
                ),
                "party.signed" => format!("Signed by {name}{ip}"),
                "party.approved" => format!("Approved by {name}{ip}"),
                "party.photo_taken" => {
                    format!("{name} took a webcam photo (SHA-256 {})", metadata["sha256"].as_str().unwrap_or(""))
                }
                "party.field_file_uploaded" => {
                    format!(
                        "{name} uploaded {} to \u{201c}{}\u{201d}",
                        metadata["file"].as_str().unwrap_or("a file"),
                        metadata["label"].as_str().filter(|l| !l.is_empty()).unwrap_or("a field")
                    )
                }
                "party.attachment_uploaded" => {
                    format!(
                        "{name} uploaded {} ({})",
                        metadata["attachment"].as_str().unwrap_or("a file"),
                        metadata["file"].as_str().unwrap_or("")
                    )
                }
                _ => return None,
            };
            Some(EvidenceEvent { at: fmt_time(Some(at)), text })
        })
        .collect())
}

async fn evidence_attachments(
    ctx: &Ctx,
    conn: &mut PgConnection,
    document: &DocumentRow,
    parties: &[PartyRow],
) -> Result<Vec<EvidenceAttachment>> {
    let mut out = Vec::new();
    for request in super::attachments::for_document(conn, document.id).await? {
        let Some(file_id) = request.file_id else { continue };
        let record = file_record(conn, file_id).await?;
        let content = read_file(&ctx.app.storage, ctx.keys(), &record).await?;
        out.push(EvidenceAttachment {
            party: parties.iter().find(|p| p.id == request.party_id).map(|p| p.name.clone()).unwrap_or_default(),
            name: request.name,
            file_name: record.name,
            sha256: record.sha256,
            content_type: record.content_type,
            content,
        });
    }
    for field in fields(conn, document.id).await? {
        let (Some(file_id), Some(FieldKind::File)) = (field.file_id, FieldKind::parse(&field.kind)) else { continue };
        let record = file_record(conn, file_id).await?;
        let content = read_file(&ctx.app.storage, ctx.keys(), &record).await?;
        out.push(EvidenceAttachment {
            party: parties.iter().find(|p| Some(p.id) == field.party_id).map(|p| p.name.clone()).unwrap_or_default(),
            name: if field.label.is_empty() { "File".into() } else { field.label },
            file_name: record.name,
            sha256: record.sha256,
            content_type: record.content_type,
            content,
        });
    }
    Ok(out)
}

/// Stamps every value and signature, appends the evidence pages, and seals the result with the
/// organisation's seal and a time stamp. Idempotent: a document that is no longer pending is left
/// alone.
pub async fn seal_document(ctx: &Ctx, conn: &mut PgConnection, document_id: Uuid, attempt: i32) -> Result<()> {
    let document: Option<DocumentRow> = sqlx::query_as(&format!("select {DOCUMENT_COLUMNS} from documents d where id = $1"))
        .bind(document_id)
        .fetch_optional(&mut *conn)
        .await?;
    let Some(document) = document.filter(|d| d.status == status::PENDING) else { return Ok(()) };
    let parties = parties(conn, document.id).await?;
    if parties.iter().any(|p| p.must_act() && p.signed_at.is_none()) {
        return Ok(());
    }
    let source_id =
        document.source_file_id.ok_or_else(|| AppError::internal(anyhow::anyhow!("document {} has no source file", document.id)))?;
    let source = read_file(&ctx.app.storage, ctx.keys(), &file_record(conn, source_id).await?).await?;
    let stamps = build_stamps(ctx, conn, &document, &parties).await?;
    let stamped =
        tokio::task::spawn_blocking(move || opensign_pdf::stamp(&source, &stamps)).await.map_err(AppError::internal)?.map_err(pdf_err)?;

    let org: OrgSettings = settings(ctx, conn).await?;
    let signing: SigningSettings = settings(ctx, conn).await?;
    let audit_head: Option<String> =
        sqlx::query_scalar("select encode(hash, 'hex') from audit_log order by id desc limit 1").fetch_optional(&mut *conn).await?;
    let report = EvidenceReport {
        document_id: document.id.to_string(),
        title: document.title.clone(),
        organisation: if org.name.is_empty() { "OpenSign".into() } else { org.name.clone() },
        original_sha256: document.original_sha256.clone().unwrap_or_default(),
        signed_sha256: sha256_hex(&stamped),
        completed_at: fmt_time(Some(ctx.now)),
        verify_url: format!("{}/verify", ctx.app.public_url()),
        parties: parties
            .iter()
            .map(|p| EvidenceParty {
                name: p.name.clone(),
                email: p.email.clone(),
                role: match p.role() {
                    super::PartyRole::Signer => "Signer".into(),
                    super::PartyRole::Approver => "Approver".into(),
                    super::PartyRole::Viewer => "Receives a copy".into(),
                },
                authentication: auth_label(p),
                signed_at: p.signed_at.map(|t| fmt_time(Some(t))),
                ip: p.evidence.as_ref().and_then(|e| e["ip"].as_str()).map(str::to_string),
                identity: super::open_identity(ctx.keys(), p),
            })
            .collect(),
        events: evidence_events(conn, document.id).await?,
        attachments: evidence_attachments(ctx, conn, &document, &parties).await?,
        audit_head,
        logo_png: crate::services::branding::logo_png(ctx, conn).await,
    };
    let identity = active_identity(ctx, conn).await?;
    let options = SealOptions {
        name: identity.certificate().tbs_certificate.subject.to_string(),
        reason: if signing.reason.trim().is_empty() { "Signed with OpenSign".into() } else { signing.reason.clone() },
        location: org.country.clone(),
        signing_time: ctx.now,
    };
    let (prepared, identity) = tokio::task::spawn_blocking(move || {
        let with_evidence = opensign_pdf::append_evidence(&stamped, &report)?;
        opensign_pdf::prepare(&with_evidence, &options).map(|p| (p, identity))
    })
    .await
    .map_err(AppError::internal)?
    .map_err(pdf_err)?;
    let signed = opensign_pdf::sign(&identity, &prepared.digest).map_err(pdf_err)?;
    let token = if signing.tsa_url.trim().is_empty() {
        None
    } else {
        match timestamp(ctx, signing.tsa_url.trim(), &Sha256::digest(&signed.signature).into()).await {
            Ok(token) => Some(token),
            Err(e) if attempt < TIMESTAMP_ATTEMPTS => return Err(e),
            Err(e) => {
                tracing::warn!(document = %document.id, error = %e, "sealing without a time stamp");
                None
            }
        }
    };
    let timestamped = token.is_some();
    let sealed = opensign_pdf::finish(prepared, &identity, signed, token).map_err(pdf_err)?;
    let (sealed, level, unchecked, revoked) = if timestamped {
        match super::ltv::extend(ctx, sealed.clone(), &signing.tsa_url).await {
            Ok(extended) => {
                if !extended.revoked.is_empty() {
                    tracing::warn!(document = %document.id, certificates = ?extended.revoked, "a responder reported a certificate as revoked");
                }
                (extended.pdf, extended.level, extended.unchecked, extended.revoked)
            }
            Err(e) => {
                tracing::warn!(document = %document.id, error = %e, "sealed without long-term validation data");
                (sealed, super::ltv::Level::BT, Vec::new(), Vec::new())
            }
        }
    } else {
        (sealed, super::ltv::Level::BB, Vec::new(), Vec::new())
    };

    let record = store_file(
        conn,
        &ctx.app.storage,
        ctx.keys(),
        NewFile {
            kind: "sealed",
            name: &format!("{}.pdf", notify::sanitize_filename(&document.title)),
            content_type: "application/pdf",
            body: &sealed,
            uploaded_by: None,
        },
    )
    .await?;
    let updated = sqlx::query("update documents set status = 'completed', sealed_file_id = $2, sealed_sha256 = $3, completed_at = $4, updated_at = now() where id = $1 and status = 'pending'")
        .bind(document.id)
        .bind(record.id)
        .bind(&record.sha256)
        .bind(ctx.now)
        .execute(&mut *conn)
        .await?;
    if updated.rows_affected() == 0 {
        return Ok(());
    }
    audit(
        ctx,
        conn,
        Entry::new("document.completed", "document", document.id)
            .metadata(json!({ "sealedSha256": record.sha256, "timestamped": timestamped, "padesLevel": level.as_str(), "revocationUnchecked": unchecked, "revoked": revoked })),
    )
    .await?;
    enqueue(conn, "notify_completed", json!({ "document": document.id }), None, Some(&format!("completed:{}", document.id))).await?;
    notify::emit(
        conn,
        "document.completed",
        json!({
            "documentId": document.id,
            "title": document.title,
            "externalId": document.external_id,
            "sealedSha256": record.sha256,
            "valuesUrl": crate::services::exports::values_url(ctx, document.id),
        }),
    )
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initials_from_names() {
        assert_eq!(initials("anna maria ångström"), "AMÅ");
        assert_eq!(initials("Bo"), "B");
    }
}
