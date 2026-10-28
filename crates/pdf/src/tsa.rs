use chrono::{DateTime, Utc};
use cms::content_info::ContentInfo;
use cms::signed_data::SignedData;
use const_oid::ObjectIdentifier;
use const_oid::db::rfc5911::ID_SIGNED_DATA;
use const_oid::db::rfc5912::ID_SHA_256;
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

/// An RFC 3161 request for a time stamp over `data` (for PAdES, the signature value).
pub fn build_request(data: &[u8], nonce: &[u8; 16]) -> Vec<u8> {
    let mut nonce = nonce.to_vec();
    nonce[0] &= 0x7f;
    let request = TimeStampReq {
        version: 1,
        message_imprint: MessageImprint {
            hash_algorithm: sha256_alg(),
            hashed_message: OctetString::new(Sha256::digest(data).to_vec()).expect("32 bytes"),
        },
        nonce: Int::new(&nonce).expect("valid integer"),
        cert_req: true,
    };
    request.to_der().expect("a time stamp request encodes")
}

/// Checks the response's status, that the token covers `data`, and that it echoes our nonce.
/// Returns the token (a CMS ContentInfo) to embed as the signature-time-stamp attribute.
pub fn parse_response(response: &[u8], data: &[u8], nonce: &[u8; 16]) -> Result<Vec<u8>, TimestampError> {
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
    let fields = token_fields(&token_der, data)?;
    let mut expected = nonce.to_vec();
    expected[0] &= 0x7f;
    let expected = Int::new(&expected).map_err(|e| invalid(&e))?;
    let echoed =
        fields.iter().skip(5).find(|f| f.tag() == Tag::Integer).map(|f| f.decode_as::<Int>()).transpose().map_err(|e| invalid(&e))?;
    if echoed.as_ref() != Some(&expected) {
        return Err(invalid(&"the token does not echo our nonce"));
    }
    Ok(token_der)
}

/// The TSTInfo fields of a time stamp token, after checking that it covers `data` with SHA-256.
fn token_fields(token_der: &[u8], data: &[u8]) -> Result<Vec<Any>, TimestampError> {
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
    if imprint.hash_algorithm.oid != ID_SHA_256 || imprint.hashed_message.as_bytes() != Sha256::digest(data).as_slice() {
        return Err(invalid(&"the token covers different data"));
    }
    Ok(fields)
}

/// The time a token vouches for, if it covers `data`. The TSA's own signature is not checked here;
/// readers validate it against their trust lists.
pub fn token_time(token_der: &[u8], data: &[u8]) -> Result<DateTime<Utc>, TimestampError> {
    let fields = token_fields(token_der, data)?;
    let time = fields.get(4).ok_or_else(|| TimestampError::Invalid("TSTInfo has no time".into()))?;
    let time: GeneralizedTime = time.decode_as().map_err(|e| TimestampError::Invalid(e.to_string()))?;
    Ok(DateTime::<Utc>::from(time.to_system_time()))
}
