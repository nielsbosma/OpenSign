use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use cms::cert::CertificateChoices;
use cms::content_info::ContentInfo;
use cms::signed_data::{SignedData, SignerIdentifier};
use const_oid::db::rfc5911::{ID_MESSAGE_DIGEST, ID_SIGNED_DATA};
use const_oid::db::rfc5912::ID_SHA_256;
use der::asn1::OctetString;
use der::{Decode, Encode, SliceReader};
use lopdf::{Dictionary, Document, Object, ObjectId};
use serde::{Deserialize, Deserializer, Serialize};
use sha2::{Digest, Sha256};
use x509_cert::Certificate;
use x509_cert::ext::pkix::SubjectKeyIdentifier;

use crate::load::load;
use crate::revisions::appended_objects;
use crate::seal::ID_AA_SIGNATURE_TIME_STAMP_TOKEN;
use crate::{PdfError, Result};

const MAX_SIGNATURES: usize = 32;

/// Spelled out, serde's derive would borrow the value from its input, which never lives that long.
type StaticStr = &'static str;

/// What a reader can say about one signature without trust lists: whether the bytes it covers are
/// intact and the cryptography checks out. Whether the signer is trusted is a separate question.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignatureCheck {
    /// `signature`, or `documentTimestamp` for an archive time stamp over the whole file.
    #[serde(deserialize_with = "known_kind")]
    pub kind: StaticStr,
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
    /// SHA-256 of the signer's certificate, when the signature verifies with it.
    pub signer_certificate_sha256: Option<String>,
    /// True when the signer is one of this server's seal identities. Anyone can make a signature
    /// that is intact; only this says who made it. Filled in by the caller, who has the identities.
    #[serde(default)]
    pub trusted: bool,
    pub problems: Vec<String>,
}

fn known_kind<'de, D: Deserializer<'de>>(deserializer: D) -> std::result::Result<&'static str, D::Error> {
    match String::deserialize(deserializer)?.as_str() {
        "signature" => Ok("signature"),
        "documentTimestamp" => Ok("documentTimestamp"),
        _ => Err(serde::de::Error::custom("unknown signature kind")),
    }
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
    let doc = load(pdf)?;
    let mut found = Vec::new();
    for object in doc.objects.values() {
        let Ok(dict) = object.as_dict() else { continue };
        let kind = dict.get(b"Type").and_then(Object::as_name).unwrap_or_default();
        if kind == b"Sig" || kind == b"DocTimeStamp" {
            found.push((byte_range(dict).map(|r| r[2].saturating_add(r[3])).unwrap_or(usize::MAX), dict));
        }
    }
    if found.len() > MAX_SIGNATURES {
        return Err(PdfError::Rejected("The PDF has too many signatures".into()));
    }
    found.sort_by_key(|(end, _)| *end);
    let mut changes = HashMap::new();
    Ok(found
        .into_iter()
        .map(|(_, dict)| {
            let mut check = check_one(pdf, &doc, dict, &mut changes);
            check.intact = check.is_intact();
            check
        })
        .collect())
}

fn check_one(pdf: &[u8], doc: &Document, dict: &lopdf::Dictionary, changes: &mut HashMap<usize, Result<(), String>>) -> SignatureCheck {
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
        signer_certificate_sha256: None,
        trusted: false,
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
    let Ok(Object::String(contents, _)) = dict.get(b"Contents") else {
        check.problems.push("the signature has no value".into());
        return check;
    };
    // Otherwise the bytes the range leaves out and the value checked below could be two different things.
    if !hole_ok || hex::decode(&hole[1..hole.len() - 1]).ok().as_ref() != Some(contents) {
        check.problems.push("the excluded range is not just the signature value".into());
        return check;
    }
    check.covers_whole_file = second_end == Some(pdf.len());
    if !check.covers_whole_file {
        let end = second_start + second_len;
        match changes.entry(end).or_insert_with(|| later_changes(pdf, end, doc)) {
            Ok(()) => check.only_validation_data_added = true,
            Err(problem) => check.problems.push(format!("the file was changed after this signature: {problem}")),
        }
    }
    let mut hasher = Sha256::new();
    hasher.update(&pdf[..first_len]);
    hasher.update(&pdf[second_start..second_start + second_len]);
    let digest: [u8; 32] = hasher.finalize().into();

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
    let time = match crate::tsa::digest_time(&token, digest) {
        Ok(time) => {
            check.digest_matches = true;
            Some(time)
        }
        Err(e) => {
            check.problems.push(e.to_string());
            None
        }
    };
    if let Ok(signed) = info.content.decode_as::<SignedData>()
        && let Some(signer) = signed.signer_infos.0.iter().next()
    {
        check.signer = signer_certificate(&certificates(&signed), &signer.sid).map(|c| c.tbs_certificate.subject.to_string());
    }
    match crate::tsa::check_authority(&token) {
        Ok(()) => {
            check.signature_valid = Some(true);
            check.timestamp = time;
        }
        Err(problem) => {
            check.signature_valid = Some(false);
            check.problems.push(problem);
        }
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

/// `dict` has these keys and no others.
fn has_exactly(dict: &Dictionary, keys: &[&[u8]]) -> bool {
    dict.len() == keys.len() && keys.iter().all(|key| dict.has(key))
}

fn name_is(dict: &Dictionary, key: &[u8], value: &[u8]) -> bool {
    dict.get(key).and_then(Object::as_name).is_ok_and(|n| n == value)
}

/// The invisible field OpenSign adds for a document time stamp, key for key: anything more on it
/// (an appearance, an action) would be a change a reader can see or trigger. Returns the objects it is made of.
fn timestamp_widget(doc: &Document, object: &Object, pages: &HashSet<ObjectId>) -> Option<[ObjectId; 2]> {
    let id = object.as_reference().ok()?;
    let widget = doc.get_dictionary(id).ok()?;
    if !has_exactly(widget, &[b"Type", b"Subtype", b"FT", b"T", b"V", b"Rect", b"F", b"P"])
        || !name_is(widget, b"Type", b"Annot")
        || !name_is(widget, b"Subtype", b"Widget")
        || !name_is(widget, b"FT", b"Sig")
        || !matches!(widget.get(b"T"), Ok(Object::String(..)))
        || widget.get(b"F").and_then(Object::as_i64).ok()? != 132
        || !widget.get(b"P").and_then(Object::as_reference).is_ok_and(|p| pages.contains(&p))
    {
        return None;
    }
    let rect: Vec<f32> = widget.get(b"Rect").and_then(Object::as_array).ok()?.iter().filter_map(|o| o.as_float().ok()).collect();
    if rect != [0.0, 0.0, 0.0, 0.0] {
        return None;
    }
    let value = widget.get(b"V").and_then(Object::as_reference).ok()?;
    let sig = doc.get_dictionary(value).ok()?;
    let plain = has_exactly(sig, &[b"Type", b"Filter", b"SubFilter", b"ByteRange", b"Contents"])
        && name_is(sig, b"Type", b"DocTimeStamp")
        && name_is(sig, b"Filter", b"Adobe.PPKLite")
        && name_is(sig, b"SubFilter", b"ETSI.RFC3161")
        && byte_range(sig).is_some()
        && matches!(sig.get(b"Contents"), Ok(Object::String(..)));
    plain.then_some([id, value])
}

/// The document security store as OpenSign writes it: lists of plain streams and nothing else.
/// Adds its objects to `allowed`.
fn check_security_store(doc: &Document, catalog: &Dictionary, allowed: &mut HashSet<ObjectId>) -> Result<(), String> {
    let refused = || "the document security store holds something other than validation data".to_string();
    let Ok(store) = catalog.get(b"DSS") else { return Ok(()) };
    let store = match store {
        Object::Reference(id) => {
            allowed.insert(*id);
            doc.get_dictionary(*id).map_err(|_| refused())?
        }
        other => other.as_dict().map_err(|_| refused())?,
    };
    for (key, list) in store.iter() {
        if !matches!(key.as_slice(), b"Certs" | b"OCSPs" | b"CRLs") {
            return Err(refused());
        }
        for item in list.as_array().map_err(|_| refused())? {
            let id = item.as_reference().map_err(|_| refused())?;
            let stream = doc.get_object(id).and_then(Object::as_stream).map_err(|_| refused())?;
            if stream.dict.iter().any(|(key, _)| !matches!(key.as_slice(), b"Length" | b"Filter")) {
                return Err(refused());
            }
            allowed.insert(id);
        }
    }
    Ok(())
}

fn check_extensions(catalog: &Dictionary) -> Result<(), String> {
    let Ok(extensions) = catalog.get(b"Extensions") else { return Ok(()) };
    let plain =
        extensions.as_dict().ok().filter(|e| has_exactly(e, &[b"ESIC"])).and_then(|e| e.get(b"ESIC").ok()).and_then(|e| e.as_dict().ok());
    if plain.is_some_and(|esic| has_exactly(esic, &[b"BaseVersion", b"ExtensionLevel"])) {
        Ok(())
    } else {
        Err("the document's extensions were changed".into())
    }
}

/// Numbers the signed revision refers to but does not define. A later update defining one would
/// change what the signed objects mean without changing any of them.
fn undefined_references(object: &Object, defined: &Document, found: &mut HashSet<ObjectId>, depth: usize) {
    if depth > 64 {
        return;
    }
    match object {
        Object::Reference(id) if !defined.objects.contains_key(id) => {
            found.insert(*id);
        }
        Object::Array(items) => items.iter().for_each(|o| undefined_references(o, defined, found, depth + 1)),
        Object::Dictionary(dict) => dict.iter().for_each(|(_, o)| undefined_references(o, defined, found, depth + 1)),
        Object::Stream(stream) => stream.dict.iter().for_each(|(_, o)| undefined_references(o, defined, found, depth + 1)),
        _ => {}
    }
}

/// Compares the revision a signature covers with the whole file. Allowed: a document security
/// store and the extensions entry in the catalog, and new invisible document time stamp fields,
/// each exactly as OpenSign writes them. Every other object must be exactly what was signed, and
/// nothing else may be added, not even something no signed object seems to use.
fn later_changes(pdf: &[u8], end: usize, last: &Document) -> Result<(), String> {
    let written = appended_objects(pdf, end)?;
    let signed = load(&pdf[..end]).map_err(|_| "the signed revision cannot be read".to_string())?;
    if signed.xref_recovery.is_some() || last.xref_recovery.is_some() {
        return Err("the file's cross-reference table is damaged".into());
    }
    let root = |d: &Document| d.trailer.get(b"Root").and_then(Object::as_reference).ok();
    let root_id = root(&signed).ok_or("the signed revision has no catalog")?;
    if root(last) != Some(root_id) || signed.trailer.get(b"Info").ok() != last.trailer.get(b"Info").ok() {
        return Err("the document catalog was replaced".into());
    }
    let pages: HashSet<ObjectId> = signed.get_pages().values().copied().collect();
    if last.get_pages().values().copied().collect::<HashSet<_>>() != pages {
        return Err("pages were added or removed".into());
    }
    let mut added = Vec::new();
    let mut allowed = HashSet::new();
    for (id, before) in &signed.objects {
        let after = last.objects.get(id).ok_or("an object was removed")?;
        if before == after {
            continue;
        }
        let (Ok(b), Ok(a)) = (before.as_dict(), after.as_dict()) else { return Err("an object was changed".into()) };
        if *id == root_id {
            let changeable: [&[u8]; 3] = [b"DSS", b"Extensions", b"AcroForm"];
            if without(b, &changeable) != without(a, &changeable) {
                return Err("the document catalog was changed".into());
            }
            check_security_store(last, a, &mut allowed)?;
            check_extensions(a)?;
            let (form_b, form_a) = (resolved_dict(&signed, b, b"AcroForm"), resolved_dict(last, a, b"AcroForm"));
            if without(&form_b, &[b"Fields", b"SigFlags"]) != without(&form_a, &[b"Fields", b"SigFlags"]) {
                return Err("the form was changed".into());
            }
            added.extend(appended(resolved_array(&signed, &form_b, b"Fields"), resolved_array(last, &form_a, b"Fields"))?);
        } else if pages.contains(id) {
            if without(b, &[b"Annots"]) != without(a, &[b"Annots"]) {
                return Err("a page was changed".into());
            }
            added.extend(appended(resolved_array(&signed, b, b"Annots"), resolved_array(last, a, b"Annots"))?);
        } else {
            return Err("an object was changed".into());
        }
    }
    for object in &added {
        let parts = timestamp_widget(last, object, &pages).ok_or("something other than a document time stamp was added")?;
        allowed.extend(parts);
    }
    allowed.extend(written.xref_streams.iter().map(|number| (*number, 0)));
    let new: Vec<ObjectId> = last.objects.keys().filter(|id| !signed.objects.contains_key(id)).copied().collect();
    if new.iter().any(|id| !allowed.contains(id)) {
        return Err("something other than validation data or a document time stamp was added".into());
    }
    let numbers: HashSet<u32> = last.objects.keys().map(|(number, _)| *number).collect();
    if written.objects.iter().any(|number| !numbers.contains(number)) {
        return Err("an update holds an object that cannot be read".into());
    }
    if !new.is_empty() {
        let mut undefined = HashSet::new();
        signed.objects.values().for_each(|o| undefined_references(o, &signed, &mut undefined, 0));
        if new.iter().any(|id| undefined.contains(id)) {
            return Err("an update defines an object the signed revision refers to".into());
        }
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
    if valid {
        check.signer_certificate_sha256 = Some(hex::encode(Sha256::digest(cert.to_der().map_err(|e| e.to_string())?)));
    } else {
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
        // A time only counts once its token has been checked: this one is also shown to people.
        match (crate::tsa::token_time(&token, signer.signature.as_bytes()), crate::tsa::check_authority(&token)) {
            (Ok(time), Ok(())) => check.timestamp = Some(time),
            (Err(e), _) => check.problems.push(e.to_string()),
            (_, Err(problem)) => check.problems.push(problem),
        }
    }
    Ok(())
}
