//! Compact JWS with RS256, the only algorithm eID providers here sign with. The algorithm is
//! fixed by the caller, never taken from the token's header.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64URL;
use rsa::pkcs1v15::{Signature, VerifyingKey};
use rsa::pkcs8::DecodePublicKey;
use rsa::signature::Verifier;
use rsa::{BigUint, RsaPublicKey};
use serde::Deserialize;
use serde_json::Value;
use sha2::Sha256;
use x509_cert::Certificate;
use x509_cert::der::{DecodePem, Encode};

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum JwsError {
    #[error("the token is malformed")]
    Malformed,
    #[error("the token is not signed with RS256")]
    Algorithm,
    #[error("no trusted key matches the token")]
    UnknownKey,
    #[error("the signature does not match")]
    Signature,
    #[error("the key is not usable: {0}")]
    Key(String),
}

#[derive(Clone)]
pub struct Key {
    pub kid: Option<String>,
    key: RsaPublicKey,
}

impl Key {
    pub fn from_certificate_pem(pem: &str) -> Result<Self, JwsError> {
        let cert = Certificate::from_pem(pem.trim().as_bytes()).map_err(|e| JwsError::Key(e.to_string()))?;
        let spki = cert.tbs_certificate.subject_public_key_info.to_der().map_err(|e| JwsError::Key(e.to_string()))?;
        let key = RsaPublicKey::from_public_key_der(&spki).map_err(|e| JwsError::Key(e.to_string()))?;
        Ok(Self { kid: None, key })
    }

    /// The RSA signing keys of a JWK set; other keys are skipped.
    pub fn from_jwks(jwks: &Value) -> Vec<Self> {
        #[derive(Deserialize)]
        struct Jwk {
            kty: String,
            kid: Option<String>,
            n: Option<String>,
            e: Option<String>,
            #[serde(rename = "use")]
            use_: Option<String>,
        }
        let keys: Vec<Jwk> = jwks.get("keys").cloned().and_then(|k| serde_json::from_value(k).ok()).unwrap_or_default();
        keys.into_iter()
            .filter(|k| k.kty == "RSA" && k.use_.as_deref().is_none_or(|u| u == "sig"))
            .filter_map(|k| {
                let n = BigUint::from_bytes_be(&B64URL.decode(k.n?).ok()?);
                let e = BigUint::from_bytes_be(&B64URL.decode(k.e?).ok()?);
                Some(Self { kid: k.kid, key: RsaPublicKey::new(n, e).ok()? })
            })
            .collect()
    }
}

/// The payload of `token` if one of `keys` signed it with RS256.
pub fn verify(token: &str, keys: &[Key]) -> Result<Value, JwsError> {
    let mut parts = token.split('.');
    let (Some(header), Some(payload), Some(signature), None) = (parts.next(), parts.next(), parts.next(), parts.next()) else {
        return Err(JwsError::Malformed);
    };
    let header: Value =
        serde_json::from_slice(&B64URL.decode(header).map_err(|_| JwsError::Malformed)?).map_err(|_| JwsError::Malformed)?;
    if header["alg"] != "RS256" || header.get("crit").is_some() {
        return Err(JwsError::Algorithm);
    }
    let kid = header["kid"].as_str();
    let signature =
        Signature::try_from(B64URL.decode(signature).map_err(|_| JwsError::Malformed)?.as_slice()).map_err(|_| JwsError::Malformed)?;
    let signed = &token[..token.len() - token.rsplit('.').next().map_or(0, str::len) - 1];
    let candidates: Vec<&Key> = keys.iter().filter(|k| kid.is_none() || k.kid.is_none() || k.kid.as_deref() == kid).collect();
    if candidates.is_empty() {
        return Err(JwsError::UnknownKey);
    }
    if !candidates.iter().any(|k| VerifyingKey::<Sha256>::new(k.key.clone()).verify(signed.as_bytes(), &signature).is_ok()) {
        return Err(JwsError::Signature);
    }
    serde_json::from_slice(&B64URL.decode(payload).map_err(|_| JwsError::Malformed)?).map_err(|_| JwsError::Malformed)
}

#[cfg(test)]
pub mod testing {
    use rsa::RsaPrivateKey;
    use rsa::pkcs1v15::SigningKey;
    use rsa::signature::{SignatureEncoding, Signer};
    use rsa::traits::PublicKeyParts;

    use super::*;

    pub fn keypair() -> (RsaPrivateKey, Key) {
        let private = RsaPrivateKey::new(&mut rand::thread_rng(), 1024).expect("key");
        let key = Key { kid: Some("k1".into()), key: private.to_public_key() };
        (private, key)
    }

    pub fn sign(private: &RsaPrivateKey, header: &Value, payload: &Value) -> String {
        let signed = format!("{}.{}", B64URL.encode(header.to_string()), B64URL.encode(payload.to_string()));
        let signature = SigningKey::<Sha256>::new(private.clone()).sign(signed.as_bytes());
        format!("{signed}.{}", B64URL.encode(signature.to_bytes()))
    }

    pub fn jwks(private: &RsaPrivateKey) -> Value {
        let public = private.to_public_key();
        serde_json::json!({ "keys": [{
            "kty": "RSA", "kid": "k1", "use": "sig",
            "n": B64URL.encode(public.n().to_bytes_be()), "e": B64URL.encode(public.e().to_bytes_be()),
        }]})
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::testing::*;
    use super::*;

    #[test]
    fn verifies_rs256_and_nothing_else() {
        let (private, key) = keypair();
        let keys = Key::from_jwks(&jwks(&private));
        assert_eq!(keys.len(), 1);
        let token = sign(&private, &json!({ "alg": "RS256", "kid": "k1" }), &json!({ "sub": "x" }));
        assert_eq!(verify(&token, &keys).expect("valid")["sub"], "x");
        assert_eq!(verify(&token, &[key]).expect("valid")["sub"], "x");

        let (other, _) = keypair();
        let forged = sign(&other, &json!({ "alg": "RS256", "kid": "k1" }), &json!({ "sub": "x" }));
        assert_eq!(verify(&forged, &keys), Err(JwsError::Signature));

        let (head, rest) = token.split_once('.').expect("parts");
        let none = format!("{}.{}", B64URL.encode(json!({ "alg": "none" }).to_string()), rest);
        assert_eq!(verify(&none, &keys), Err(JwsError::Algorithm));
        let hs = sign(&private, &json!({ "alg": "HS256", "kid": "k1" }), &json!({}));
        assert_eq!(verify(&hs, &keys), Err(JwsError::Algorithm));
        let wrong_kid = sign(&private, &json!({ "alg": "RS256", "kid": "k2" }), &json!({}));
        assert_eq!(verify(&wrong_kid, &keys), Err(JwsError::UnknownKey));

        let tampered = format!("{}.{}.{}", head, B64URL.encode(json!({ "sub": "y" }).to_string()), token.rsplit('.').next().expect("sig"));
        assert_eq!(verify(&tampered, &keys), Err(JwsError::Signature));
    }
}
