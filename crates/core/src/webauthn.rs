//! Just enough WebAuthn for passkeys: no attestation trust, user verification always required,
//! ES256, EdDSA and RS256 keys. Pure Rust, so there is no OpenSSL underneath.

use std::io::Cursor;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64URL;
use ciborium::Value;
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::error::{AppError, ErrorKind, Result};

const FLAG_UP: u8 = 0x01;
const FLAG_UV: u8 = 0x04;
const FLAG_AT: u8 = 0x40;

pub const ES256: i64 = -7;
pub const EDDSA: i64 = -8;
pub const RS256: i64 = -257;

fn refused(message: &str) -> AppError {
    AppError::new(ErrorKind::BadRequest, message)
}

pub fn decode(value: &str) -> Result<Vec<u8>> {
    B64URL.decode(value.trim_end_matches('=')).map_err(|_| refused("The passkey response is not valid"))
}

pub fn encode(bytes: &[u8]) -> String {
    B64URL.encode(bytes)
}

/// Who we are to the authenticator: the host of APP_URL and its origin.
#[derive(Debug, Clone)]
pub struct RelyingParty {
    pub id: String,
    pub origin: String,
}

impl RelyingParty {
    pub fn from_url(app_url: &str) -> Result<Self> {
        let url = url::Url::parse(app_url).map_err(AppError::internal)?;
        let id = url.host_str().ok_or_else(|| AppError::internal(anyhow::anyhow!("APP_URL has no host")))?.to_string();
        Ok(Self { id, origin: url.origin().ascii_serialization() })
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClientData {
    #[serde(rename = "type")]
    kind: String,
    challenge: String,
    origin: String,
    #[serde(default)]
    cross_origin: bool,
}

fn check_client_data(raw: &[u8], kind: &str, challenge: &[u8], rp: &RelyingParty) -> Result<()> {
    let data: ClientData = serde_json::from_slice(raw).map_err(|_| refused("The passkey response is not valid"))?;
    let challenge_ok = decode(&data.challenge).is_ok_and(|c| opensign_crypto::constant_time_eq(&c, challenge));
    if data.kind != kind || !challenge_ok || data.cross_origin {
        return Err(refused("The passkey response does not belong to this sign-in"));
    }
    if data.origin != rp.origin {
        return Err(refused("The passkey was used on another site"));
    }
    Ok(())
}

struct AuthData<'a> {
    flags: u8,
    sign_count: u32,
    rest: &'a [u8],
}

fn auth_data<'a>(raw: &'a [u8], rp: &RelyingParty) -> Result<AuthData<'a>> {
    if raw.len() < 37 {
        return Err(refused("The passkey response is not valid"));
    }
    if !opensign_crypto::constant_time_eq(&raw[..32], &Sha256::digest(rp.id.as_bytes())) {
        return Err(refused("The passkey belongs to another site"));
    }
    let flags = raw[32];
    if flags & FLAG_UP == 0 || flags & FLAG_UV == 0 {
        return Err(refused("The passkey did not verify you. Use your device's PIN, fingerprint or face."));
    }
    Ok(AuthData { flags, sign_count: u32::from_be_bytes([raw[33], raw[34], raw[35], raw[36]]), rest: &raw[37..] })
}

/// A credential to store after registration.
#[derive(Debug)]
pub struct NewCredential {
    pub id: Vec<u8>,
    /// The COSE key exactly as the authenticator sent it.
    pub public_key: Vec<u8>,
    pub sign_count: u32,
}

/// Checks a `navigator.credentials.create()` response.
pub fn verify_registration(
    rp: &RelyingParty,
    challenge: &[u8],
    client_data_json: &[u8],
    attestation_object: &[u8],
) -> Result<NewCredential> {
    check_client_data(client_data_json, "webauthn.create", challenge, rp)?;
    let object: Value = ciborium::from_reader(attestation_object).map_err(|_| refused("The passkey response is not valid"))?;
    let raw = map_get(&object, "authData").and_then(Value::as_bytes).ok_or_else(|| refused("The passkey response is not valid"))?;
    let data = auth_data(raw, rp)?;
    if data.flags & FLAG_AT == 0 || data.rest.len() < 18 {
        return Err(refused("The passkey response has no key"));
    }
    let id_len = u16::from_be_bytes([data.rest[16], data.rest[17]]) as usize;
    let key_start = 18 + id_len;
    if id_len == 0 || id_len > 1023 || data.rest.len() <= key_start {
        return Err(refused("The passkey response is not valid"));
    }
    let id = data.rest[18..key_start].to_vec();
    let mut cursor = Cursor::new(&data.rest[key_start..]);
    let key: Value = ciborium::from_reader(&mut cursor).map_err(|_| refused("The passkey's key is not valid"))?;
    let public_key = data.rest[key_start..key_start + cursor.position() as usize].to_vec();
    PublicKey::from_cose(&key)?;
    Ok(NewCredential { id, public_key, sign_count: data.sign_count })
}

/// Checks a `navigator.credentials.get()` response against a stored key. Returns the new sign count.
pub fn verify_assertion(
    rp: &RelyingParty,
    challenge: &[u8],
    stored_key: &[u8],
    stored_count: u32,
    client_data_json: &[u8],
    authenticator_data: &[u8],
    signature: &[u8],
) -> Result<u32> {
    check_client_data(client_data_json, "webauthn.get", challenge, rp)?;
    let data = auth_data(authenticator_data, rp)?;
    let key: Value = ciborium::from_reader(stored_key).map_err(AppError::internal)?;
    let mut signed = authenticator_data.to_vec();
    signed.extend_from_slice(&Sha256::digest(client_data_json));
    if !PublicKey::from_cose(&key)?.verify(&signed, signature) {
        return Err(AppError::new(ErrorKind::Unauthorized, "The passkey could not be verified"));
    }
    // Authenticators without a counter always report 0; once a counter has been seen it must grow.
    if (stored_count > 0 || data.sign_count > 0) && data.sign_count <= stored_count {
        return Err(AppError::new(ErrorKind::Unauthorized, "This passkey may have been copied. Remove it and add it again."));
    }
    Ok(data.sign_count)
}

fn map_get<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    value.as_map()?.iter().find(|(k, _)| k.as_text() == Some(key)).map(|(_, v)| v)
}

fn cose_get(value: &Value, label: i64) -> Option<&Value> {
    value.as_map()?.iter().find(|(k, _)| k.as_integer().and_then(|i| i64::try_from(i).ok()) == Some(label)).map(|(_, v)| v)
}

enum PublicKey {
    Es256(p256::ecdsa::VerifyingKey),
    EdDsa(ed25519_dalek::VerifyingKey),
    Rs256(rsa::pkcs1v15::VerifyingKey<sha2::Sha256>),
}

impl PublicKey {
    fn from_cose(key: &Value) -> Result<Self> {
        let invalid = || refused("This kind of passkey is not supported");
        let int = |label| cose_get(key, label).and_then(Value::as_integer).and_then(|i| i64::try_from(i).ok());
        let bytes = |label| cose_get(key, label).and_then(Value::as_bytes).ok_or_else(invalid);
        match (int(1), int(3)) {
            (Some(2), Some(ES256)) if int(-1) == Some(1) => {
                let (x, y) = (bytes(-2)?, bytes(-3)?);
                if x.len() != 32 || y.len() != 32 {
                    return Err(invalid());
                }
                let mut point = vec![4u8];
                point.extend_from_slice(x);
                point.extend_from_slice(y);
                Ok(Self::Es256(p256::ecdsa::VerifyingKey::from_sec1_bytes(&point).map_err(|_| invalid())?))
            }
            (Some(1), Some(EDDSA)) if int(-1) == Some(6) => {
                let x: [u8; 32] = bytes(-2)?.as_slice().try_into().map_err(|_| invalid())?;
                Ok(Self::EdDsa(ed25519_dalek::VerifyingKey::from_bytes(&x).map_err(|_| invalid())?))
            }
            (Some(3), Some(RS256)) => {
                let n = rsa::BigUint::from_bytes_be(bytes(-1)?);
                let e = rsa::BigUint::from_bytes_be(bytes(-2)?);
                let key = rsa::RsaPublicKey::new(n, e).map_err(|_| invalid())?;
                if rsa::traits::PublicKeyParts::size(&key) < 256 {
                    return Err(invalid());
                }
                Ok(Self::Rs256(rsa::pkcs1v15::VerifyingKey::new(key)))
            }
            _ => Err(invalid()),
        }
    }

    fn verify(&self, message: &[u8], signature: &[u8]) -> bool {
        use p256::ecdsa::signature::Verifier;
        match self {
            Self::Es256(key) => p256::ecdsa::DerSignature::try_from(signature).is_ok_and(|s| key.verify(message, &s).is_ok()),
            Self::EdDsa(key) => ed25519_dalek::Signature::from_slice(signature).is_ok_and(|s| key.verify_strict(message, &s).is_ok()),
            Self::Rs256(key) => rsa::pkcs1v15::Signature::try_from(signature).is_ok_and(|s| key.verify(message, &s).is_ok()),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use p256::ecdsa::signature::Signer;
    use p256::ecdsa::{DerSignature, SigningKey};

    use super::*;

    fn rp() -> RelyingParty {
        RelyingParty::from_url("https://sign.example.test").unwrap()
    }

    fn cbor(value: &Value) -> Vec<u8> {
        let mut out = Vec::new();
        ciborium::into_writer(value, &mut out).unwrap();
        out
    }

    fn cose(key: &SigningKey) -> Vec<u8> {
        let point = key.verifying_key().to_encoded_point(false);
        cbor(&Value::Map(vec![
            (1.into(), 2.into()),
            (3.into(), ES256.into()),
            ((-1).into(), 1.into()),
            ((-2).into(), Value::Bytes(point.x().unwrap().to_vec())),
            ((-3).into(), Value::Bytes(point.y().unwrap().to_vec())),
        ]))
    }

    fn client_data(kind: &str, challenge: &[u8], origin: &str) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({ "type": kind, "challenge": encode(challenge), "origin": origin })).unwrap()
    }

    fn auth(rp_id: &str, flags: u8, count: u32, tail: &[u8]) -> Vec<u8> {
        let mut out = Sha256::digest(rp_id.as_bytes()).to_vec();
        out.push(flags);
        out.extend_from_slice(&count.to_be_bytes());
        out.extend_from_slice(tail);
        out
    }

    #[test]
    fn registers_and_signs_in_with_an_es256_passkey() {
        let rp = rp();
        let key = SigningKey::from_slice(&[7u8; 32]).unwrap();
        let cose_key = cose(&key);
        let mut attested = vec![0u8; 16];
        attested.extend_from_slice(&3u16.to_be_bytes());
        attested.extend_from_slice(b"abc");
        attested.extend_from_slice(&cose_key);
        let object = cbor(&Value::Map(vec![
            ("fmt".into(), "none".into()),
            ("attStmt".into(), Value::Map(vec![])),
            ("authData".into(), Value::Bytes(auth(&rp.id, FLAG_UP | FLAG_UV | FLAG_AT, 0, &attested))),
        ]));
        let created = verify_registration(&rp, b"reg", &client_data("webauthn.create", b"reg", &rp.origin), &object).unwrap();
        assert_eq!(created.id, b"abc");
        assert_eq!(created.public_key, cose_key);

        let wrong_origin = verify_registration(&rp, b"reg", &client_data("webauthn.create", b"reg", "https://evil.test"), &object);
        assert!(wrong_origin.is_err());
        let wrong_challenge = verify_registration(&rp, b"other", &client_data("webauthn.create", b"reg", &rp.origin), &object);
        assert!(wrong_challenge.is_err());

        let data = auth(&rp.id, FLAG_UP | FLAG_UV, 5, &[]);
        let client = client_data("webauthn.get", b"login", &rp.origin);
        let mut signed = data.clone();
        signed.extend_from_slice(&Sha256::digest(&client));
        let signature: DerSignature = key.sign(&signed);
        assert_eq!(verify_assertion(&rp, b"login", &created.public_key, 0, &client, &data, signature.as_bytes()).unwrap(), 5);
        assert!(
            verify_assertion(&rp, b"login", &created.public_key, 5, &client, &data, signature.as_bytes()).is_err(),
            "a counter that did not move"
        );
        let mut forged = signed.clone();
        forged[0] ^= 1;
        let bad: DerSignature = key.sign(&forged);
        assert!(verify_assertion(&rp, b"login", &created.public_key, 0, &client, &data, bad.as_bytes()).is_err());

        let unverified = auth(&rp.id, FLAG_UP, 6, &[]);
        let mut signed = unverified.clone();
        signed.extend_from_slice(&Sha256::digest(&client));
        let signature: DerSignature = key.sign(&signed);
        assert!(
            verify_assertion(&rp, b"login", &created.public_key, 0, &client, &unverified, signature.as_bytes()).is_err(),
            "user verification is required"
        );
        let other_site = auth("evil.test", FLAG_UP | FLAG_UV, 6, &[]);
        assert!(verify_assertion(&rp, b"login", &created.public_key, 0, &client, &other_site, signature.as_bytes()).is_err());
    }
}
