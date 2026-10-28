use chrono::{DateTime, Utc};
use cms::cert::CertificateChoices;
use cms::content_info::ContentInfo;
use cms::signed_data::{SignedData, SignerIdentifier};
use const_oid::db::rfc5911::{ID_MESSAGE_DIGEST, ID_SIGNED_DATA};
use const_oid::db::rfc5912::{ECDSA_WITH_SHA_256, ID_EC_PUBLIC_KEY, ID_SHA_256};
use der::asn1::OctetString;
use der::{Decode, Encode, SliceReader};
use lopdf::{Document, Object};
use p256::ecdsa::{DerSignature, VerifyingKey};
use serde::Serialize;
use sha2::{Digest, Sha256};
use signature::Verifier;
use x509_cert::Certificate;

use crate::Result;
use crate::seal::ID_AA_SIGNATURE_TIME_STAMP_TOKEN;

/// What a reader can say about one signature without trust lists: whether the bytes it covers are
/// intact and the cryptography checks out. Whether the signer is trusted is a separate question.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SignatureCheck {
    pub signer: Option<String>,
    pub reason: Option<String>,
    /// True when the signature covers every byte of the file except its own hole.
    pub covers_whole_file: bool,
    pub digest_matches: bool,
    /// `None` when the algorithm is not one OpenSign verifies (only ECDSA P-256 with SHA-256).
    pub signature_valid: Option<bool>,
    pub timestamp: Option<DateTime<Utc>>,
    pub problems: Vec<String>,
}

impl SignatureCheck {
    pub fn is_intact(&self) -> bool {
        self.covers_whole_file && self.digest_matches && self.signature_valid == Some(true)
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

/// Checks every signature dictionary in the file.
pub fn verify_signatures(pdf: &[u8]) -> Result<Vec<SignatureCheck>> {
    let doc = Document::load_mem(pdf)?;
    let mut checks = Vec::new();
    for object in doc.objects.values() {
        let Ok(dict) = object.as_dict() else { continue };
        if !dict.get(b"Type").and_then(Object::as_name).is_ok_and(|n| n == b"Sig") {
            continue;
        }
        checks.push(check_one(pdf, dict));
    }
    Ok(checks)
}

fn check_one(pdf: &[u8], dict: &lopdf::Dictionary) -> SignatureCheck {
    let mut check = SignatureCheck {
        signer: text(dict, b"Name"),
        reason: text(dict, b"Reason"),
        covers_whole_file: false,
        digest_matches: false,
        signature_valid: None,
        timestamp: None,
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
        check.problems.push("the file was changed after this signature".into());
    }
    let mut hasher = Sha256::new();
    hasher.update(&pdf[..first_len]);
    hasher.update(&pdf[second_start..second_start + second_len]);
    let digest: [u8; 32] = hasher.finalize().into();

    let Ok(Object::String(contents, _)) = dict.get(b"Contents") else {
        check.problems.push("the signature has no value".into());
        return check;
    };
    if let Err(problem) = check_cms(contents, &digest, &mut check) {
        check.problems.push(problem);
    }
    check
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

    let certificates: Vec<&Certificate> = signed
        .certificates
        .iter()
        .flat_map(|set| set.0.iter())
        .filter_map(|c| match c {
            CertificateChoices::Certificate(cert) => Some(cert),
            _ => None,
        })
        .collect();
    let cert = match &signer.sid {
        SignerIdentifier::IssuerAndSerialNumber(id) => {
            certificates.iter().find(|c| c.tbs_certificate.issuer == id.issuer && c.tbs_certificate.serial_number == id.serial_number)
        }
        SignerIdentifier::SubjectKeyIdentifier(_) => certificates.first(),
    }
    .ok_or("the signer's certificate is not included")?;
    check.signer = Some(cert.tbs_certificate.subject.to_string());

    let spki = &cert.tbs_certificate.subject_public_key_info;
    if signer.signature_algorithm.oid == ECDSA_WITH_SHA_256 && spki.algorithm.oid == ID_EC_PUBLIC_KEY {
        let key =
            VerifyingKey::from_sec1_bytes(spki.subject_public_key.raw_bytes()).map_err(|e| format!("the signer's key is invalid: {e}"))?;
        let sig = DerSignature::from_bytes(signer.signature.as_bytes()).map_err(|e| format!("the signature is malformed: {e}"))?;
        let signed_attrs = attrs.to_der().map_err(|e| e.to_string())?;
        let valid = key.verify(&signed_attrs, &sig).is_ok();
        check.signature_valid = Some(valid);
        if !valid {
            check.problems.push("the signature does not verify".into());
        }
    } else {
        check.problems.push("the signature algorithm is not one OpenSign verifies".into());
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
    }
    Ok(())
}
