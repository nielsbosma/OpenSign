use chrono::{DateTime, Utc};
use cms::content_info::ContentInfo;
use cms::signed_data::SignedData;
use const_oid::ObjectIdentifier;
use const_oid::db::rfc5911::ID_SIGNED_DATA;
use const_oid::db::rfc5912::{ID_KP_TIME_STAMPING, ID_SHA_256};
use der::asn1::{Any, GeneralizedTime, Int, OctetString};
use der::{Decode, Encode, Sequence, Tag, Tagged};
use sha2::{Digest, Sha256};
use spki::AlgorithmIdentifierOwned;

const ID_CT_TST_INFO: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.16.1.4");

#[derive(Debug, thiserror::Error)]
pub enum TimestampError {
    #[error("the time stamp authority refused the request (status {0}){1}")]
    Refused(i64, String),
    #[error("the time stamp response is invalid: {0}")]
    Invalid(String),
}

#[derive(Sequence)]
struct MessageImprint {
    hash_algorithm: AlgorithmIdentifierOwned,
    hashed_message: OctetString,
}

#[derive(Sequence)]
struct TimeStampReq {
    version: u8,
    message_imprint: MessageImprint,
    nonce: Int,
    cert_req: bool,
}

#[derive(Sequence)]
struct PkiStatusInfo {
    status: Int,
    #[asn1(optional = "true")]
    status_string: Option<Any>,
    #[asn1(optional = "true")]
    fail_info: Option<der::asn1::BitString>,
}

#[derive(Sequence)]
struct TimeStampResp {
    status: Any,
    #[asn1(optional = "true")]
    time_stamp_token: Option<Any>,
}

fn sha256_alg() -> AlgorithmIdentifierOwned {
    AlgorithmIdentifierOwned { oid: ID_SHA_256, parameters: None }
}

fn int_value(int: &Int) -> i64 {
    int.as_bytes().iter().fold(0i64, |acc, b| (acc << 8) | *b as i64)
}

/// An RFC 3161 request for a time stamp over a SHA-256 digest: of the signature value for a
/// signature time stamp, of the signed byte ranges for a document time stamp.
pub fn build_request(digest: &[u8; 32], nonce: &[u8; 16]) -> Vec<u8> {
    let mut nonce = nonce.to_vec();
    nonce[0] &= 0x7f;
    let request = TimeStampReq {
        version: 1,
        message_imprint: MessageImprint {
            hash_algorithm: sha256_alg(),
            hashed_message: OctetString::new(digest.to_vec()).expect("32 bytes"),
        },
        nonce: Int::new(&nonce).expect("valid integer"),
        cert_req: true,
    };
    request.to_der().expect("a time stamp request encodes")
}

/// Checks the response's status, that the token covers `digest`, and that it echoes our nonce.
/// Returns the token (a CMS ContentInfo) to embed.
pub fn parse_response(response: &[u8], digest: &[u8; 32], nonce: &[u8; 16]) -> Result<Vec<u8>, TimestampError> {
    let invalid = |e: &dyn std::fmt::Display| TimestampError::Invalid(e.to_string());
    let resp = TimeStampResp::from_der(response).map_err(|e| invalid(&e))?;
    let status_der = resp.status.to_der().map_err(|e| invalid(&e))?;
    let status = PkiStatusInfo::from_der(&status_der).map_err(|e| invalid(&e))?;
    let code = int_value(&status.status);
    if code > 1 {
        return Err(TimestampError::Refused(code, String::new()));
    }
    let token = resp.time_stamp_token.ok_or_else(|| invalid(&"no token in the response"))?;
    let token_der = token.to_der().map_err(|e| invalid(&e))?;
    let fields = token_fields(&token_der, digest)?;
    let mut expected = nonce.to_vec();
    expected[0] &= 0x7f;
    let expected = Int::new(&expected).map_err(|e| invalid(&e))?;
    let echoed =
        fields.iter().skip(5).find(|f| f.tag() == Tag::Integer).map(|f| f.decode_as::<Int>()).transpose().map_err(|e| invalid(&e))?;
    if echoed.as_ref() != Some(&expected) {
        return Err(invalid(&"the token does not echo our nonce"));
    }
    check_authority(&token_der).map_err(TimestampError::Invalid)?;
    Ok(token_der)
}

/// The TSTInfo fields of a time stamp token, after checking that it covers `digest` (SHA-256).
pub(crate) fn token_fields(token_der: &[u8], digest: &[u8]) -> Result<Vec<Any>, TimestampError> {
    let invalid = |e: &dyn std::fmt::Display| TimestampError::Invalid(e.to_string());
    let info = ContentInfo::from_der(token_der).map_err(|e| invalid(&e))?;
    if info.content_type != ID_SIGNED_DATA {
        return Err(invalid(&"the token is not signed data"));
    }
    let signed: SignedData = info.content.decode_as().map_err(|e| invalid(&e))?;
    if signed.encap_content_info.econtent_type != ID_CT_TST_INFO {
        return Err(invalid(&"the token does not contain TSTInfo"));
    }
    let content = signed.encap_content_info.econtent.ok_or_else(|| invalid(&"the token has no content"))?;
    let tst_der = OctetString::from_der(&content.to_der().map_err(|e| invalid(&e))?).map_err(|e| invalid(&e))?;
    let fields: Vec<Any> = Vec::from_der(tst_der.as_bytes()).map_err(|e| invalid(&e))?;
    let imprint = fields.get(2).ok_or_else(|| invalid(&"TSTInfo is too short"))?;
    let imprint = MessageImprint::from_der(&imprint.to_der().map_err(|e| invalid(&e))?).map_err(|e| invalid(&e))?;
    if imprint.hash_algorithm.oid != ID_SHA_256 || imprint.hashed_message.as_bytes() != digest {
        return Err(invalid(&"the token covers different data"));
    }
    Ok(fields)
}

/// The time a token vouches for, if it covers `data`. Whether the TSA is trusted is for readers
/// and their trust lists; `token_signature_valid` checks the cryptography.
pub fn token_time(token_der: &[u8], data: &[u8]) -> Result<DateTime<Utc>, TimestampError> {
    digest_time(token_der, &Sha256::digest(data))
}

pub(crate) fn digest_time(token_der: &[u8], digest: &[u8]) -> Result<DateTime<Utc>, TimestampError> {
    let fields = token_fields(token_der, digest)?;
    let time = fields.get(4).ok_or_else(|| TimestampError::Invalid("TSTInfo has no time".into()))?;
    let time: GeneralizedTime = time.decode_as().map_err(|e| TimestampError::Invalid(e.to_string()))?;
    Ok(DateTime::<Utc>::from(time.to_system_time()))
}

/// What can be checked about a token's authority without a trust list: its signature verifies, its
/// certificate is for time stamping, and that certificate was valid at the time it vouches for.
pub(crate) fn check_authority(token_der: &[u8]) -> Result<(), String> {
    if !token_signature_valid(token_der)? {
        return Err("the time stamp authority's signature does not verify".into());
    }
    let info = ContentInfo::from_der(token_der).map_err(|e| e.to_string())?;
    let signed: SignedData = info.content.decode_as().map_err(|e| e.to_string())?;
    let [signer] = signed.signer_infos.0.as_slice() else { return Err("the time stamp has more than one signer".into()) };
    let certs = crate::verify::certificates(&signed);
    let cert = crate::verify::signer_certificate(&certs, &signer.sid).ok_or("the time stamp does not include its certificate")?;
    if !crate::x509::has_extended_key_usage(cert, ID_KP_TIME_STAMPING) {
        return Err("the time stamp authority's certificate is not for time stamping".into());
    }
    let content = signed.encap_content_info.econtent.as_ref().ok_or("the time stamp has no content")?;
    let tst = OctetString::from_der(&content.to_der().map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let fields: Vec<Any> = Vec::from_der(tst.as_bytes()).map_err(|e| e.to_string())?;
    let time: GeneralizedTime = fields.get(4).ok_or("the time stamp has no time")?.decode_as().map_err(|e| e.to_string())?;
    let (time, validity) = (time.to_system_time(), &cert.tbs_certificate.validity);
    if time < validity.not_before.to_system_time() || time > validity.not_after.to_system_time() {
        return Err("the time stamp authority's certificate was not valid at that time".into());
    }
    Ok(())
}

/// A stored token's time, if it covers `data` and its authority passes `check_authority`.
pub fn verify_timestamp_token(token_der: &[u8], data: &[u8]) -> Result<DateTime<Utc>, String> {
    let time = token_time(token_der, data).map_err(|e| e.to_string())?;
    check_authority(token_der)?;
    Ok(time)
}

/// SHA-256 of the certificate named by a token's only signer.
pub(crate) fn authority_fingerprint(token_der: &[u8]) -> Option<String> {
    let info = ContentInfo::from_der(token_der).ok()?;
    let signed: SignedData = info.content.decode_as().ok()?;
    let [signer] = signed.signer_infos.0.as_slice() else { return None };
    let certs = crate::verify::certificates(&signed);
    let cert = crate::verify::signer_certificate(&certs, &signer.sid)?;
    Some(hex::encode(Sha256::digest(cert.to_der().ok()?)))
}

/// Checks the TSA's signature on a token: the signed attributes carry the digest of the TSTInfo
/// and verify with the key of the certificate the token names.
pub(crate) fn token_signature_valid(token_der: &[u8]) -> Result<bool, String> {
    let info = ContentInfo::from_der(token_der).map_err(|e| e.to_string())?;
    let signed: SignedData = info.content.decode_as().map_err(|e| e.to_string())?;
    let [signer] = signed.signer_infos.0.as_slice() else { return Err("the time stamp has more than one signer".into()) };
    let content = signed.encap_content_info.econtent.as_ref().ok_or("the time stamp has no content")?;
    let tst = OctetString::from_der(&content.to_der().map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let attrs = signer.signed_attrs.as_ref().ok_or("the time stamp has no signed attributes")?;
    let hash = crate::x509::Hash::from_oid(&signer.digest_alg.oid).ok_or("the time stamp uses an unsupported digest")?;
    let message_digest = attrs
        .iter()
        .find(|a| a.oid == const_oid::db::rfc5911::ID_MESSAGE_DIGEST)
        .and_then(|a| a.values.get(0))
        .and_then(|v| v.decode_as::<OctetString>().ok())
        .ok_or("the time stamp has no message digest")?;
    if message_digest.as_bytes() != hash.digest(tst.as_bytes()) {
        return Ok(false);
    }
    let certs = crate::verify::certificates(&signed);
    let cert = crate::verify::signer_certificate(&certs, &signer.sid).ok_or("the time stamp does not include its certificate")?;
    let message = attrs.to_der().map_err(|e| e.to_string())?;
    Ok(crate::x509::verify_signature(
        &cert.tbs_certificate.subject_public_key_info,
        &signer.signature_algorithm.oid,
        Some(&signer.digest_alg.oid),
        &message,
        signer.signature.as_bytes(),
    ))
}
