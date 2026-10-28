//! Long-term validation of sealed PDFs. Fetches what `opensign_pdf` plans (issuer certificates,
//! OCSP responses, CRLs) through the public HTTP client, then adds a document time stamp so the
//! seal can be checked offline long after the certificates involved have expired.

use chrono::{DateTime, Utc};
use opensign_pdf::{RevocationCheck, RevocationStatus, ValidationData};
use reqwest::RequestBuilder;

use super::seal::timestamp;
use crate::context::Ctx;
use crate::error::{AppError, ErrorKind, Result};

const MAX_RESPONSE: usize = 256 * 1024;
const MAX_CRL: usize = 10 * 1024 * 1024;

/// The PAdES baseline level a sealed file reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    /// Signature only.
    BB,
    /// Signature with a time stamp.
    BT,
    /// Plus validation data.
    BLt,
    /// Plus a document time stamp over everything.
    BLta,
}

impl Level {
    pub fn as_str(self) -> &'static str {
        match self {
            Level::BB => "B-B",
            Level::BT => "B-T",
            Level::BLt => "B-LT",
            Level::BLta => "B-LTA",
        }
    }
}

pub struct Extended {
    pub pdf: Vec<u8>,
    pub level: Level,
    /// Certificates whose revocation status could not be fetched; readers will go online for them.
    pub unchecked: Vec<String>,
    /// Certificates a responder reported as revoked.
    pub revoked: Vec<String>,
}

fn unavailable(message: &str) -> AppError {
    AppError::new(ErrorKind::Unavailable, message.to_string())
}

async fn fetch(request: RequestBuilder, limit: usize) -> Result<Vec<u8>> {
    let mut response = request.send().await.map_err(|e| unavailable("The validation service is not reachable").with_cause(e))?;
    if !response.status().is_success() {
        return Err(unavailable(&format!("The validation service answered {}", response.status())));
    }
    if response.content_length().is_some_and(|len| len as usize > limit) {
        return Err(unavailable("The validation response is too large"));
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(AppError::internal)? {
        if body.len() + chunk.len() > limit {
            return Err(unavailable("The validation response is too large"));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

/// OCSP, CRL and issuer URLs come from certificates, so they get the same checks as any
/// configurable URL. Plain http is normal for them: what they serve is signed.
fn parse_url(url: &str) -> Option<reqwest::Url> {
    crate::net::check_outbound_url(url, true).ok()
}

/// Asks each OCSP responder in turn, then falls back to the CRLs.
async fn revocation(ctx: &Ctx, check: &RevocationCheck, now: DateTime<Utc>) -> Option<opensign_pdf::Revocation> {
    let request = check.ocsp_request().ok()?;
    for url in check.ocsp_urls.iter().filter_map(|u| parse_url(u)) {
        let call = ctx.app.http.public.post(url).header("content-type", "application/ocsp-request").body(request.clone());
        match fetch(call, MAX_RESPONSE).await.map(|body| check.accept_ocsp(&body, now)) {
            Ok(Ok(revocation)) => return Some(revocation),
            Ok(Err(e)) => tracing::warn!(subject = %check.subject, error = %e, "OCSP response refused"),
            Err(e) => tracing::warn!(subject = %check.subject, error = %e, "OCSP responder failed"),
        }
    }
    for url in check.crl_urls.iter().filter_map(|u| parse_url(u)) {
        match fetch(ctx.app.http.public.get(url), MAX_CRL).await.map(|body| check.accept_crl(&body, now)) {
            Ok(Ok(revocation)) => return Some(revocation),
            Ok(Err(e)) => tracing::warn!(subject = %check.subject, error = %e, "CRL refused"),
            Err(e) => tracing::warn!(subject = %check.subject, error = %e, "CRL download failed"),
        }
    }
    None
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> opensign_pdf::Result<T> + Send + 'static) -> Result<T> {
    tokio::task::spawn_blocking(f).await.map_err(AppError::internal)?.map_err(AppError::internal)
}

/// Turns a time-stamped seal into PAdES B-LT (validation data) and, with a time stamp authority,
/// B-LTA (a document time stamp over everything). Missing revocation data is reported, not fatal:
/// a reader can still fetch it while the CA's services are up.
pub async fn extend(ctx: &Ctx, pdf: Vec<u8>, tsa_url: &str) -> Result<Extended> {
    let pdf = std::sync::Arc::new(pdf);
    let mut extra: Vec<Vec<u8>> = Vec::new();
    let mut plan = {
        let pdf = pdf.clone();
        blocking(move || opensign_pdf::plan_validation(&pdf, &[])).await?
    };
    for _ in 0..2 {
        if plan.missing_issuers.is_empty() {
            break;
        }
        let before = extra.len();
        for url in plan.missing_issuers.iter().filter_map(|u| parse_url(u)) {
            if let Ok(body) = fetch(ctx.app.http.public.get(url), MAX_RESPONSE).await {
                extra.extend(opensign_pdf::parse_issuer_certificates(&body));
            }
        }
        if extra.len() == before {
            break;
        }
        let (pdf, more) = (pdf.clone(), extra.clone());
        plan = blocking(move || opensign_pdf::plan_validation(&pdf, &more)).await?;
    }

    let mut data = ValidationData { certificates: plan.certificates.clone(), ..Default::default() };
    let (mut unchecked, mut revoked) = (Vec::new(), Vec::new());
    for check in &plan.checks {
        match revocation(ctx, check, ctx.now).await {
            Some(revocation) => {
                if let RevocationStatus::Revoked(at) = revocation.status {
                    tracing::error!(subject = %check.subject, revoked_at = %at, "a certificate in the seal's chain is revoked");
                    revoked.push(check.subject.clone());
                }
                data.add(revocation);
            }
            None => unchecked.push(check.subject.clone()),
        }
    }
    let with_dss = {
        let pdf = pdf.clone();
        blocking(move || opensign_pdf::add_validation_data(&pdf, &data)).await?
    };
    if tsa_url.trim().is_empty() {
        return Ok(Extended { pdf: with_dss, level: Level::BLt, unchecked, revoked });
    }
    let prepared = blocking(move || opensign_pdf::prepare_document_timestamp(&with_dss)).await?;
    let token = timestamp(ctx, tsa_url.trim(), &prepared.digest).await?;
    let archived = opensign_pdf::finish_document_timestamp(prepared, &token).map_err(AppError::internal)?;
    Ok(Extended { pdf: archived, level: Level::BLta, unchecked, revoked })
}
