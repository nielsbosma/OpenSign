use std::collections::HashSet;

use chrono::{DateTime, Utc};
use cms::cert::CertificateChoices;
use cms::content_info::ContentInfo;
use cms::signed_data::{SignedData, SignerIdentifier};
use const_oid::db::rfc5911::{ID_MESSAGE_DIGEST, ID_SIGNED_DATA};
use const_oid::db::rfc5912::ID_SHA_256;
use der::asn1::OctetString;
use der::{Decode, Encode, SliceReader};
use lopdf::{Dictionary, Document, Object, ObjectId};
use serde::Serialize;
use sha2::{Digest, Sha256};
use x509_cert::Certificate;
use x509_cert::ext::pkix::SubjectKeyIdentifier;

use crate::Result;
use crate::seal::ID_AA_SIGNATURE_TIME_STAMP_TOKEN;

/// What a reader can say about one signature without trust lists: whether the bytes it covers are
/// intact and the cryptography checks out. Whether the signer is trusted is a separate question.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SignatureCheck {
    /// `signature`, or `documentTimestamp` for an archive time stamp over the whole file.
    pub kind: &'static str,
    pub signer: Option<String>,
    pub reason: Option<String>,
    /// True when the signature covers every byte of the file except its own hole.
    pub covers_whole_file: bool,
    /// True when everything added after the signature is validation data or document time
    /// stamps, the only later changes PAdES long-term validation makes.
    pub only_validation_data_added: bool,
    pub digest_matches: bool,
    /// False when the signature does not verify or uses an algorithm OpenSign does not check (it
    /// checks RSA of 2048 bits or more and ECDSA P-256 or P-384, with SHA-2).
    pub signature_valid: Option<bool>,
    pub timestamp: Option<DateTime<Utc>>,
    /// The overall verdict: the bytes are as signed and the cryptography checks out.
    pub intact: bool,
    pub problems: Vec<String>,
}

impl SignatureCheck {
    pub fn is_intact(&self) -> bool {
        (self.covers_whole_file || self.only_validation_data_added) && self.digest_matches && self.signature_valid == Some(true)
    }
}

fn byte_range(dict: &lopdf::Dictionary) -> Option<[usize; 4]> {
    let values = dict.get(b"ByteRange").ok()?.as_array().ok()?;
    let values: Vec<usize> = values.iter().map(|v| v.as_i64().ok().and_then(|v| usize::try_from(v).ok())).collect::<Option<_>>()?;
    values.try_into().ok()
}

fn text(dict: &lopdf::Dictionary, key: &[u8]) -> Option<String> {
    match dict.get(key).ok()? {
        Object::String(bytes, _) => Some(bytes.iter().map(|b| *b as char).collect()),
        _ => None,
    }
}

/// Checks every signature and document time stamp in the file, in the order they were added.
pub fn verify_signatures(pdf: &[u8]) -> Result<Vec<SignatureCheck>> {
    let doc = Document::load_mem(pdf)?;
    let mut found = Vec::new();
    for object in doc.objects.values() {
        let Ok(dict) = object.as_dict() else { continue };
        let kind = dict.get(b"Type").and_then(Object::as_name).unwrap_or_default();
        if kind == b"Sig" || kind == b"DocTimeStamp" {
            found.push((byte_range(dict).map(|r| r[2].saturating_add(r[3])).unwrap_or(usize::MAX), dict));
        }
    }
    found.sort_by_key(|(end, _)| *end);
    Ok(found
        .into_iter()
        .map(|(_, dict)| {
            let mut check = check_one(pdf, dict);
            check.intact = check.is_intact();
            check
        })
        .collect())
}

fn check_one(pdf: &[u8], dict: &lopdf::Dictionary) -> SignatureCheck {
    let document_timestamp = dict.get(b"Type").and_then(Object::as_name).is_ok_and(|n| n == b"DocTimeStamp");
    let mut check = SignatureCheck {
        kind: if document_timestamp { "documentTimestamp" } else { "signature" },
        signer: text(dict, b"Name"),
        reason: text(dict, b"Reason"),
        covers_whole_file: false,
        only_validation_data_added: false,
        digest_matches: false,
        signature_valid: None,
        timestamp: None,
        intact: false,
        problems: Vec::new(),
    };
    let Some([start, first_len, second_start, second_len]) = byte_range(dict) else {
        check.problems.push("the signature has no valid byte range".into());
        return check;
    };
    let second_end = second_start.checked_add(second_len);
    if start != 0 || first_len >= second_start || second_end.is_none_or(|end| end > pdf.len()) {
        check.problems.push("the byte range is out of bounds".into());
        return check;
    }
    let hole = &pdf[first_len..second_start];
    let hole_ok =
        hole.len() >= 2 && hole[0] == b'<' && hole[hole.len() - 1] == b'>' && hole[1..hole.len() - 1].iter().all(u8::is_ascii_hexdigit);
    if !hole_ok {
        check.problems.push("the excluded range is not just the signature value".into());
        return check;
    }
    check.covers_whole_file = second_end == Some(pdf.len());
    if !check.covers_whole_file {
        match later_changes(pdf, second_start + second_len) {
            Ok(()) => check.only_validation_data_added = true,
            Err(problem) => check.problems.push(format!("the file was changed after this signature: {problem}")),
        }
    }
    let mut hasher = Sha256::new();
    hasher.update(&pdf[..first_len]);
    hasher.update(&pdf[second_start..second_start + second_len]);
    let digest: [u8; 32] = hasher.finalize().into();

    let Ok(Object::String(contents, _)) = dict.get(b"Contents") else {
        check.problems.push("the signature has no value".into());
        return check;
    };
    let result =
        if document_timestamp { check_document_timestamp(contents, &digest, &mut check) } else { check_cms(contents, &digest, &mut check) };
    if let Err(problem) = result {
        check.problems.push(problem);
    }
    check
}

pub(crate) fn certificates(signed: &SignedData) -> Vec<&Certificate> {
    signed
        .certificates
        .iter()
        .flat_map(|set| set.0.iter())
        .filter_map(|c| match c {
            CertificateChoices::Certificate(cert) => Some(cert),
            _ => None,
        })
        .collect()
}

pub(crate) fn signer_certificate<'a>(certificates: &[&'a Certificate], sid: &SignerIdentifier) -> Option<&'a Certificate> {
    match sid {
        SignerIdentifier::IssuerAndSerialNumber(id) => certificates
            .iter()
            .find(|c| c.tbs_certificate.issuer == id.issuer && c.tbs_certificate.serial_number == id.serial_number)
            .copied(),
        SignerIdentifier::SubjectKeyIdentifier(ski) => certificates
            .iter()
            .find(|c| matches!(c.tbs_certificate.get::<SubjectKeyIdentifier>(), Ok(Some((_, own))) if own.0 == ski.0))
            .copied(),
    }
}

fn check_document_timestamp(contents: &[u8], digest: &[u8; 32], check: &mut SignatureCheck) -> Result<(), String> {
    let mut reader = SliceReader::new(contents).map_err(|e| e.to_string())?;
    let info = ContentInfo::decode(&mut reader).map_err(|e| format!("the time stamp is not CMS: {e}"))?;
    let token = info.to_der().map_err(|e| e.to_string())?;
    match crate::tsa::digest_time(&token, digest) {
        Ok(time) => {
            check.digest_matches = true;
            check.timestamp = Some(time);
        }
        Err(e) => check.problems.push(e.to_string()),
    }
    if let Ok(signed) = info.content.decode_as::<SignedData>()
        && let Some(signer) = signed.signer_infos.0.iter().next()
    {
        check.signer = signer_certificate(&certificates(&signed), &signer.sid).map(|c| c.tbs_certificate.subject.to_string());
    }
    let valid = crate::tsa::token_signature_valid(&token)?;
    check.signature_valid = Some(valid);
    if !valid {
        check.problems.push("the time stamp authority's signature does not verify".into());
    }
    Ok(())
}

fn resolve(doc: &Document, object: &Object) -> Object {
    match object {
        Object::Reference(id) => doc.get_object(*id).cloned().unwrap_or(Object::Null),
        other => other.clone(),
    }
}

fn resolved_array(doc: &Document, dict: &Dictionary, key: &[u8]) -> Vec<Object> {
    dict.get(key).ok().map(|o| resolve(doc, o)).and_then(|o| o.as_array().ok().cloned()).unwrap_or_default()
}

fn resolved_dict(doc: &Document, dict: &Dictionary, key: &[u8]) -> Dictionary {
    dict.get(key).ok().map(|o| resolve(doc, o)).and_then(|o| o.as_dict().ok().cloned()).unwrap_or_default()
}

fn without(dict: &Dictionary, keys: &[&[u8]]) -> Dictionary {
    let mut dict = dict.clone();
    for key in keys {
        dict.remove(key);
    }
    dict
}

/// `before` must be a prefix of `after`; returns what was appended.
fn appended(before: Vec<Object>, after: Vec<Object>) -> Result<Vec<Object>, String> {
    if after.len() < before.len() || after[..before.len()] != before[..] {
        return Err("existing form fields or annotations were changed".into());
    }
    Ok(after[before.len()..].to_vec())
}

/// An invisible signature widget whose value is a document time stamp.
fn is_timestamp_widget(doc: &Document, object: &Object) -> bool {
    let Ok(widget) = resolve(doc, object).as_dict().cloned() else { return false };
    let name = |key: &[u8]| widget.get(key).and_then(Object::as_name).ok().map(<[u8]>::to_vec);
    let invisible = widget.get(b"Rect").and_then(Object::as_array).is_ok_and(|r| {
        let v: Vec<f32> = r.iter().filter_map(|o| o.as_float().ok()).collect();
        v.len() == 4 && (v[0] == v[2] || v[1] == v[3])
    });
    let value_is_timestamp = widget
        .get(b"V")
        .ok()
        .map(|v| resolve(doc, v))
        .is_some_and(|v| v.as_dict().is_ok_and(|d| d.get(b"Type").and_then(Object::as_name).is_ok_and(|t| t == b"DocTimeStamp")));
    name(b"Subtype").as_deref() == Some(b"Widget".as_slice())
        && name(b"FT").as_deref() == Some(b"Sig".as_slice())
        && !widget.has(b"AP")
        && invisible
        && value_is_timestamp
}

/// Compares the revision a signature covers with the whole file. Allowed: a document security
/// store and the extensions entry in the catalog, and new invisible document time stamp fields.
/// Every other object must be exactly what was signed.
fn later_changes(pdf: &[u8], end: usize) -> Result<(), String> {
    let signed = Document::load_mem(&pdf[..end]).map_err(|_| "the signed revision cannot be read".to_string())?;
    let last = Document::load_mem(pdf).map_err(|_| "the file cannot be read".to_string())?;
    let root = |d: &Document| d.trailer.get(b"Root").and_then(Object::as_reference).ok();
    let root_id = root(&signed).ok_or("the signed revision has no catalog")?;
    if root(&last) != Some(root_id) || signed.trailer.get(b"Info").ok() != last.trailer.get(b"Info").ok() {
        return Err("the document catalog was replaced".into());
    }
    let pages: HashSet<ObjectId> = signed.get_pages().values().copied().collect();
    if last.get_pages().values().copied().collect::<HashSet<_>>() != pages {
        return Err("pages were added or removed".into());
    }
    let mut added = Vec::new();
    for (id, before) in &signed.objects {
        let after = last.objects.get(id).ok_or("an object was removed")?;
        if before == after {
            continue;
        }
        let (Ok(b), Ok(a)) = (before.as_dict(), after.as_dict()) else { return Err("an object was changed".into()) };
        if *id == root_id {
            let allowed: [&[u8]; 3] = [b"DSS", b"Extensions", b"AcroForm"];
            if without(b, &allowed) != without(a, &allowed) {
                return Err("the document catalog was changed".into());
            }
            let (form_b, form_a) = (resolved_dict(&signed, b, b"AcroForm"), resolved_dict(&last, a, b"AcroForm"));
            if without(&form_b, &[b"Fields", b"SigFlags"]) != without(&form_a, &[b"Fields", b"SigFlags"]) {
                return Err("the form was changed".into());
            }
            added.extend(appended(resolved_array(&signed, &form_b, b"Fields"), resolved_array(&last, &form_a, b"Fields"))?);
        } else if pages.contains(id) {
            if without(b, &[b"Annots"]) != without(a, &[b"Annots"]) {
                return Err("a page was changed".into());
            }
            added.extend(appended(resolved_array(&signed, b, b"Annots"), resolved_array(&last, a, b"Annots"))?);
        } else {
            return Err("an object was changed".into());
        }
    }
    if !added.iter().all(|o| is_timestamp_widget(&last, o)) {
        return Err("something other than a document time stamp was added".into());
    }
    Ok(())
}

fn check_cms(contents: &[u8], digest: &[u8; 32], check: &mut SignatureCheck) -> Result<(), String> {
    let mut reader = SliceReader::new(contents).map_err(|e| e.to_string())?;
    let info = ContentInfo::decode(&mut reader).map_err(|e| format!("the signature value is not CMS: {e}"))?;
    if info.content_type != ID_SIGNED_DATA {
        return Err("the signature value is not CMS signed data".into());
    }
    let signed: SignedData = info.content.decode_as().map_err(|e| e.to_string())?;
    let [signer] = signed.signer_infos.0.as_slice() else { return Err("expected exactly one signer".into()) };
    let attrs = signer.signed_attrs.as_ref().ok_or("the signature has no signed attributes")?;
    let message_digest = attrs
        .iter()
        .find(|a| a.oid == ID_MESSAGE_DIGEST)
        .and_then(|a| a.values.get(0))
        .and_then(|v| v.decode_as::<OctetString>().ok())
        .ok_or("the signature has no message digest")?;
    check.digest_matches = signer.digest_alg.oid == ID_SHA_256 && message_digest.as_bytes() == digest;
    if !check.digest_matches {
        check.problems.push("the document does not match the signed digest".into());
    }

    let certificates = certificates(&signed);
    let cert = signer_certificate(&certificates, &signer.sid).ok_or("the signer's certificate is not included")?;
    check.signer = Some(cert.tbs_certificate.subject.to_string());

    let signed_attrs = attrs.to_der().map_err(|e| e.to_string())?;
    let valid = crate::x509::verify_signature(
        &cert.tbs_certificate.subject_public_key_info,
        &signer.signature_algorithm.oid,
        Some(&signer.digest_alg.oid),
        &signed_attrs,
        signer.signature.as_bytes(),
    );
    check.signature_valid = Some(valid);
    if !valid {
        check.problems.push("the signature does not verify, or uses an algorithm OpenSign does not check".into());
    }

    if let Some(token) = signer
        .unsigned_attrs
        .iter()
        .flat_map(|a| a.iter())
        .find(|a| a.oid == ID_AA_SIGNATURE_TIME_STAMP_TOKEN)
        .and_then(|a| a.values.get(0))
    {
        let token = token.to_der().map_err(|e| e.to_string())?;
        match crate::tsa::token_time(&token, signer.signature.as_bytes()) {
            Ok(time) => check.timestamp = Some(time),
            Err(e) => check.problems.push(e.to_string()),
        }
        if crate::tsa::token_signature_valid(&token) != Ok(true) {
            check.problems.push("the time stamp authority's signature does not verify".into());
        }
    }
    Ok(())
}
