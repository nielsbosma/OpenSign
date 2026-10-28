//! Long-term validation (PAdES B-LT and B-LTA). After sealing, everything a reader needs to check
//! the signatures without going online (certificates, OCSP responses, CRLs) goes into the
//! document security store, and a document time stamp then covers the whole file. Both are
//! incremental updates, so the seal's signature stays untouched. Network access belongs to the
//! caller: this module plans the requests and checks the answers.

use std::collections::HashSet;

use chrono::{DateTime, Duration, Utc};
use cms::cert::CertificateChoices;
use cms::content_info::ContentInfo;
use cms::signed_data::SignedData;
use const_oid::db::rfc5911::ID_SIGNED_DATA;
use const_oid::db::rfc5912::{ID_KP_OCSP_SIGNING, ID_SHA_1};
use const_oid::db::rfc6960::{ID_PKIX_OCSP_BASIC, ID_PKIX_OCSP_NOCHECK};
use der::asn1::{Null, OctetString};
use der::{Decode, Encode};
use lopdf::{Dictionary, Document, IncrementalDocument, Object, ObjectId, Stream, StringFormat, dictionary};
use sha1::Sha1;
use sha2::{Digest, Sha256};
use spki::AlgorithmIdentifierOwned;
use x509_cert::Certificate;
use x509_cert::crl::CertificateList;
use x509_ocsp::{BasicOcspResponse, CertId, CertStatus, OcspRequest, OcspResponse, OcspResponseStatus, Request, ResponderId, TbsRequest};

use crate::seal::{Prepared, fill_byte_range, fill_hole};
use crate::x509::{has_extended_key_usage, has_extension, is_self_signed, issued_by, verify_signature};
use crate::{PdfError, Result};

/// Room for a document time stamp token with the TSA's certificate chain.
const TIMESTAMP_BYTES: usize = 16 * 1024;
const MAX_CERTIFICATES: usize = 32;
/// Clock skew allowed between OpenSign and a responder.
const SKEW: Duration = Duration::minutes(5);
/// How old a response without a next-update time may be.
const MAX_AGE: Duration = Duration::days(7);

fn ltv_err(e: impl std::fmt::Display) -> PdfError {
    PdfError::Signing(e.to_string())
}

/// What to fetch so that every certificate the signatures depend on can be validated offline.
pub struct ValidationPlan {
    /// The DER certificates found, deduplicated: signers, time stamp authorities and their CAs.
    pub certificates: Vec<Vec<u8>>,
    pub checks: Vec<RevocationCheck>,
    /// caIssuers URLs for certificates whose issuer is in neither the file nor `extra`.
    pub missing_issuers: Vec<String>,
}

/// One certificate whose revocation status is needed, with where to ask.
pub struct RevocationCheck {
    pub subject: String,
    pub ocsp_urls: Vec<String>,
    pub crl_urls: Vec<String>,
    cert: Certificate,
    issuer: Certificate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RevocationStatus {
    Good,
    Revoked(DateTime<Utc>),
}

/// A checked OCSP response or CRL, ready for the document security store.
pub struct Revocation {
    pub status: RevocationStatus,
    ocsp: Option<Vec<u8>>,
    crl: Option<Vec<u8>>,
    certificates: Vec<Vec<u8>>,
}

#[derive(Default)]
pub struct ValidationData {
    pub certificates: Vec<Vec<u8>>,
    pub ocsps: Vec<Vec<u8>>,
    pub crls: Vec<Vec<u8>>,
}

impl ValidationData {
    pub fn add(&mut self, revocation: Revocation) {
        self.ocsps.extend(revocation.ocsp);
        self.crls.extend(revocation.crl);
        self.certificates.extend(revocation.certificates);
    }

    pub fn is_empty(&self) -> bool {
        self.certificates.is_empty() && self.ocsps.is_empty() && self.crls.is_empty()
    }
}

fn signed_data(der: &[u8]) -> Option<SignedData> {
    let info = ContentInfo::from_der(der).ok()?;
    (info.content_type == ID_SIGNED_DATA).then(|| info.content.decode_as().ok()).flatten()
}

fn certificates_of(signed: &SignedData) -> impl Iterator<Item = &Certificate> {
    signed.certificates.iter().flat_map(|set| set.0.iter()).filter_map(|c| match c {
        CertificateChoices::Certificate(cert) => Some(cert),
        _ => None,
    })
}

/// Certificates from every signature and document time stamp in the file, and from the time
/// stamp tokens inside signatures.
fn embedded_certificates(pdf: &[u8]) -> Result<Vec<Certificate>> {
    let doc = Document::load_mem(pdf)?;
    let mut out = Vec::new();
    for object in doc.objects.values() {
        let Ok(dict) = object.as_dict() else { continue };
        let kind = dict.get(b"Type").and_then(Object::as_name).unwrap_or_default();
        if kind != b"Sig" && kind != b"DocTimeStamp" {
            continue;
        }
        let Ok(Object::String(contents, _)) = dict.get(b"Contents") else { continue };
        let mut reader = der::SliceReader::new(contents).map_err(ltv_err)?;
        let Ok(info) = ContentInfo::decode(&mut reader) else { continue };
        let Some(signed) = info.to_der().ok().and_then(|d| signed_data(&d)) else { continue };
        out.extend(certificates_of(&signed).cloned());
        for signer in signed.signer_infos.0.iter() {
            for attr in signer.unsigned_attrs.iter().flat_map(|a| a.iter()) {
                if attr.oid != crate::seal::ID_AA_SIGNATURE_TIME_STAMP_TOKEN {
                    continue;
                }
                for value in attr.values.iter() {
                    if let Some(token) = value.to_der().ok().and_then(|d| signed_data(&d)) {
                        out.extend(certificates_of(&token).cloned());
                    }
                }
            }
        }
    }
    Ok(out)
}

/// Certificates served at a caIssuers URL: one DER certificate, or a PKCS#7 bundle.
pub fn parse_issuer_certificates(body: &[u8]) -> Vec<Vec<u8>> {
    if let Ok(cert) = Certificate::from_der(body) {
        return cert.to_der().into_iter().collect();
    }
    signed_data(body).map(|s| certificates_of(&s).filter_map(|c| c.to_der().ok()).collect()).unwrap_or_default()
}

/// Works out which certificates need revocation data. Trust anchors (self-signed) and OCSP
/// responders marked no-check need none.
pub fn plan(pdf: &[u8], extra: &[Vec<u8>]) -> Result<ValidationPlan> {
    let mut pool = embedded_certificates(pdf)?;
    pool.extend(extra.iter().filter_map(|der| Certificate::from_der(der).ok()));
    let mut seen = HashSet::new();
    pool.retain(|c| c.to_der().is_ok_and(|der| seen.insert(Sha256::digest(der))));
    pool.truncate(MAX_CERTIFICATES);

    let mut plan = ValidationPlan { certificates: Vec::new(), checks: Vec::new(), missing_issuers: Vec::new() };
    for cert in &pool {
        plan.certificates.push(cert.to_der().map_err(ltv_err)?);
        if is_self_signed(cert) || has_extension(cert, ID_PKIX_OCSP_NOCHECK) {
            continue;
        }
        let Some(issuer) = pool.iter().find(|i| issued_by(cert, i)) else {
            plan.missing_issuers.extend(crate::x509::ca_issuer_urls(cert));
            continue;
        };
        plan.checks.push(RevocationCheck {
            subject: cert.tbs_certificate.subject.to_string(),
            ocsp_urls: crate::x509::ocsp_urls(cert),
            crl_urls: crate::x509::crl_urls(cert),
            cert: cert.clone(),
            issuer: issuer.clone(),
        });
    }
    plan.missing_issuers.sort();
    plan.missing_issuers.dedup();
    Ok(plan)
}

fn to_utc(time: std::time::SystemTime) -> DateTime<Utc> {
    DateTime::<Utc>::from(time)
}

fn fresh(this_update: DateTime<Utc>, next_update: Option<DateTime<Utc>>, now: DateTime<Utc>) -> Result<(), String> {
    if this_update > now + SKEW {
        return Err("the response is dated in the future".into());
    }
    match next_update {
        Some(next) if next < now - SKEW => Err("the response has expired".into()),
        None if this_update < now - MAX_AGE => Err("the response is too old".into()),
        _ => Ok(()),
    }
}

impl RevocationCheck {
    fn cert_id(&self) -> Result<CertId> {
        let issuer = &self.issuer.tbs_certificate;
        Ok(CertId {
            hash_algorithm: AlgorithmIdentifierOwned { oid: ID_SHA_1, parameters: Some(Null.into()) },
            issuer_name_hash: OctetString::new(Sha1::digest(issuer.subject.to_der().map_err(ltv_err)?).to_vec()).map_err(ltv_err)?,
            issuer_key_hash: OctetString::new(Sha1::digest(issuer.subject_public_key_info.subject_public_key.raw_bytes()).to_vec())
                .map_err(ltv_err)?,
            serial_number: self.cert.tbs_certificate.serial_number.clone(),
        })
    }

    /// An OCSP request (DER) to POST as `application/ocsp-request`.
    pub fn ocsp_request(&self) -> Result<Vec<u8>> {
        let request = OcspRequest {
            tbs_request: TbsRequest {
                version: Default::default(),
                requestor_name: None,
                request_list: vec![Request { req_cert: self.cert_id()?, single_request_extensions: None }],
                request_extensions: None,
            },
            optional_signature: None,
        };
        request.to_der().map_err(ltv_err)
    }

    /// Accepts an OCSP response only if it is signed by the issuer or a responder the issuer
    /// delegated to, answers for this certificate, and is current.
    pub fn accept_ocsp(&self, body: &[u8], now: DateTime<Utc>) -> Result<Revocation, String> {
        let response = OcspResponse::from_der(body).map_err(|e| format!("the OCSP response is malformed: {e}"))?;
        if response.response_status != OcspResponseStatus::Successful {
            return Err(format!("the OCSP responder answered {:?}", response.response_status));
        }
        let bytes = response.response_bytes.ok_or("the OCSP response is empty")?;
        if bytes.response_type != ID_PKIX_OCSP_BASIC {
            return Err("the OCSP response is not a basic response".into());
        }
        let basic = BasicOcspResponse::from_der(bytes.response.as_bytes()).map_err(|e| format!("the OCSP response is malformed: {e}"))?;
        let tbs = basic.tbs_response_data.to_der().map_err(|e| e.to_string())?;
        let mut candidates = vec![&self.issuer];
        candidates
            .extend(basic.certs.iter().flatten().filter(|c| issued_by(c, &self.issuer) && has_extended_key_usage(c, ID_KP_OCSP_SIGNING)));
        let named = |c: &&Certificate| match &basic.tbs_response_data.responder_id {
            ResponderId::ByName(name) => c.tbs_certificate.subject == *name,
            ResponderId::ByKey(hash) => {
                hash.as_bytes() == Sha1::digest(c.tbs_certificate.subject_public_key_info.subject_public_key.raw_bytes()).as_slice()
            }
        };
        let signer = candidates
            .into_iter()
            .filter(named)
            .find(|c| {
                verify_signature(
                    &c.tbs_certificate.subject_public_key_info,
                    &basic.signature_algorithm.oid,
                    None,
                    &tbs,
                    basic.signature.raw_bytes(),
                )
            })
            .ok_or("the OCSP response is not signed by the issuer or its responder")?;

        let id = self.cert_id().map_err(|e| e.to_string())?;
        let single = basic
            .tbs_response_data
            .responses
            .iter()
            .find(|r| {
                r.cert_id.hash_algorithm.oid == ID_SHA_1
                    && r.cert_id.issuer_name_hash == id.issuer_name_hash
                    && r.cert_id.issuer_key_hash == id.issuer_key_hash
                    && r.cert_id.serial_number == id.serial_number
            })
            .ok_or("the OCSP response is about another certificate")?;
        fresh(to_utc(single.this_update.0.to_system_time()), single.next_update.as_ref().map(|t| to_utc(t.0.to_system_time())), now)?;
        let status = match &single.cert_status {
            CertStatus::Good(_) => RevocationStatus::Good,
            CertStatus::Revoked(info) => RevocationStatus::Revoked(to_utc(info.revocation_time.0.to_system_time())),
            CertStatus::Unknown(_) => return Err("the OCSP responder does not know the certificate".into()),
        };
        let certificates = if std::ptr::eq(signer, &self.issuer) { Vec::new() } else { signer.to_der().into_iter().collect() };
        Ok(Revocation { status, ocsp: Some(body.to_vec()), crl: None, certificates })
    }

    /// Accepts a CRL only if the issuer signed it and it is current.
    pub fn accept_crl(&self, body: &[u8], now: DateTime<Utc>) -> Result<Revocation, String> {
        let crl = CertificateList::from_der(body).map_err(|e| format!("the CRL is malformed: {e}"))?;
        let tbs = &crl.tbs_cert_list;
        if tbs.issuer != self.issuer.tbs_certificate.subject {
            return Err("the CRL is from another issuer".into());
        }
        let message = tbs.to_der().map_err(|e| e.to_string())?;
        if !verify_signature(
            &self.issuer.tbs_certificate.subject_public_key_info,
            &crl.signature_algorithm.oid,
            None,
            &message,
            crl.signature.raw_bytes(),
        ) {
            return Err("the CRL's signature does not verify".into());
        }
        fresh(to_utc(tbs.this_update.to_system_time()), tbs.next_update.map(|t| to_utc(t.to_system_time())), now)?;
        let serial = &self.cert.tbs_certificate.serial_number;
        let status = match tbs.revoked_certificates.iter().flatten().find(|r| r.serial_number == *serial) {
            Some(revoked) => RevocationStatus::Revoked(to_utc(revoked.revocation_date.to_system_time())),
            None => RevocationStatus::Good,
        };
        Ok(Revocation { status, ocsp: None, crl: Some(body.to_vec()), certificates: Vec::new() })
    }
}

fn resolve<'a>(doc: &'a Document, object: &'a Object) -> &'a Object {
    match object {
        Object::Reference(id) => doc.get_object(*id).unwrap_or(object),
        _ => object,
    }
}

fn resolved_array(doc: &Document, dict: &Dictionary, key: &[u8]) -> Vec<Object> {
    dict.get(key).ok().and_then(|o| resolve(doc, o).as_array().ok()).cloned().unwrap_or_default()
}

fn root_id(doc: &Document) -> Result<ObjectId> {
    doc.trailer.get(b"Root").and_then(Object::as_reference).map_err(Into::into)
}

/// Adds the certificates, OCSP responses and CRLs to the document security store in an
/// incremental update, skipping anything already there.
pub fn add_validation_data(pdf: &[u8], data: &ValidationData) -> Result<Vec<u8>> {
    let prev = Document::load_mem(pdf)?;
    let root = root_id(&prev)?;
    let catalog = prev.get_dictionary(root)?.clone();
    let existing = catalog.get(b"DSS").ok().and_then(|d| resolve(&prev, d).as_dict().ok()).cloned().unwrap_or_default();
    type List<'a> = (&'static [u8], Vec<Object>, &'a [Vec<u8>]);
    let mut lists: Vec<List> = vec![
        (b"Certs", resolved_array(&prev, &existing, b"Certs"), &data.certificates),
        (b"OCSPs", resolved_array(&prev, &existing, b"OCSPs"), &data.ocsps),
        (b"CRLs", resolved_array(&prev, &existing, b"CRLs"), &data.crls),
    ];
    let mut seen: HashSet<[u8; 32]> = HashSet::new();
    for (_, refs, _) in &lists {
        for r in refs {
            if let Ok(stream) = resolve(&prev, r).as_stream() {
                let content = stream.decompressed_content().unwrap_or_else(|_| stream.content.clone());
                seen.insert(Sha256::digest(&content).into());
            }
        }
    }

    let mut update = IncrementalDocument::create_from(pdf.to_vec(), prev);
    for (_, refs, items) in &mut lists {
        for item in items.iter() {
            if seen.insert(Sha256::digest(item).into()) {
                refs.push(Object::Reference(update.new_document.add_object(Stream::new(dictionary! {}, item.clone()))));
            }
        }
    }
    let mut dss = Dictionary::new();
    for (key, refs, _) in lists {
        if !refs.is_empty() {
            dss.set(key, refs);
        }
    }
    let mut catalog = catalog;
    catalog.set("DSS", dss);
    catalog
        .set("Extensions", dictionary! { "ESIC" => dictionary! { "BaseVersion" => Object::Name(b"1.7".to_vec()), "ExtensionLevel" => 5 } });
    update.new_document.set_object(root, catalog);
    let mut out = Vec::new();
    update.save_to(&mut out).map_err(|e| PdfError::Malformed(e.to_string()))?;
    Ok(out)
}

/// Adds an invisible document time stamp field in an incremental update and returns the file
/// with its hole and the digest to time-stamp (RFC 3161, SHA-256).
pub fn prepare_document_timestamp(pdf: &[u8]) -> Result<Prepared> {
    let prev = Document::load_mem(pdf)?;
    let root = root_id(&prev)?;
    let first_page = *prev.get_pages().values().next().ok_or_else(|| PdfError::Malformed("no pages".into()))?;
    let mut catalog = prev.get_dictionary(root)?.clone();
    let mut form = catalog.get(b"AcroForm").ok().and_then(|f| resolve(&prev, f).as_dict().ok()).cloned().unwrap_or_default();
    let mut fields = resolved_array(&prev, &form, b"Fields");
    let mut page = prev.get_dictionary(first_page)?.clone();
    let mut annots = resolved_array(&prev, &page, b"Annots");
    let number = fields.len() + 1;

    let mut update = IncrementalDocument::create_from(pdf.to_vec(), prev);
    let placeholder = || Object::Integer(9_999_999_999);
    let sig = update.new_document.add_object(dictionary! {
        "Type" => "DocTimeStamp",
        "Filter" => "Adobe.PPKLite",
        "SubFilter" => "ETSI.RFC3161",
        "ByteRange" => vec![Object::Integer(0), placeholder(), placeholder(), placeholder()],
        "Contents" => Object::String(vec![0u8; TIMESTAMP_BYTES], StringFormat::Hexadecimal),
    });
    let widget = update.new_document.add_object(dictionary! {
        "Type" => "Annot",
        "Subtype" => "Widget",
        "FT" => "Sig",
        "T" => Object::String(format!("OpenSign Archive Time Stamp {number}").into_bytes(), StringFormat::Literal),
        "V" => Object::Reference(sig),
        "Rect" => vec![0.into(), 0.into(), 0.into(), 0.into()],
        "F" => 132,
        "P" => Object::Reference(first_page),
    });
    annots.push(Object::Reference(widget));
    page.set("Annots", annots);
    update.new_document.set_object(first_page, page);
    fields.push(Object::Reference(widget));
    form.set("Fields", fields);
    form.set("SigFlags", 3);
    catalog.set("AcroForm", form);
    update.new_document.set_object(root, catalog);

    let mut out = Vec::with_capacity(pdf.len() + TIMESTAMP_BYTES * 2 + 2048);
    update.save_to(&mut out).map_err(|e| PdfError::Malformed(e.to_string()))?;
    fill_byte_range(out, pdf.len(), TIMESTAMP_BYTES)
}

/// Writes the time stamp token (a CMS ContentInfo) into the prepared hole.
pub fn finish_document_timestamp(prepared: Prepared, token: &[u8]) -> Result<Vec<u8>> {
    crate::tsa::token_fields(token, &prepared.digest).map_err(|e| PdfError::Timestamp(e.to_string()))?;
    fill_hole(prepared, token)
}
