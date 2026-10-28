use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use serde::Serialize;
use serde::de::DeserializeOwned;
use sha2::Sha256;
use zeroize::Zeroizing;

use crate::CryptoError;
use crate::aead::DataKey;
use crate::token::{constant_time_eq, random_bytes};

pub const MIN_SECRET_LEN: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyPurpose {
    /// Secret settings and other small values in the database.
    Settings,
    /// Wraps the per-file data keys of documents in the bucket.
    DocumentKek,
    /// Signed, expiring tokens (OAuth consent requests, signer step-up state).
    TokenMac,
    /// Keys the audit chain, so a database-only attacker cannot recompute it.
    AuditChain,
    /// The one-time code that unlocks the setup wizard.
    Setup,
    /// Encrypts the PAdES signing key.
    SigningKey,
}

impl KeyPurpose {
    const ALL: [KeyPurpose; 6] = [
        KeyPurpose::Settings,
        KeyPurpose::DocumentKek,
        KeyPurpose::TokenMac,
        KeyPurpose::AuditChain,
        KeyPurpose::Setup,
        KeyPurpose::SigningKey,
    ];

    fn info(self) -> &'static [u8] {
        match self {
            KeyPurpose::Settings => b"opensign:settings:v1",
            KeyPurpose::DocumentKek => b"opensign:document-kek:v1",
            KeyPurpose::TokenMac => b"opensign:token-mac:v1",
            KeyPurpose::AuditChain => b"opensign:audit-chain:v1",
            KeyPurpose::Setup => b"opensign:setup:v1",
            KeyPurpose::SigningKey => b"opensign:signing-key:v1",
        }
    }
}

pub struct Keys {
    keys: [Zeroizing<[u8; 32]>; 6],
}

impl std::fmt::Debug for Keys {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Keys(••••)")
    }
}

#[derive(Serialize)]
struct SignedEnvelope<'a, T: Serialize> {
    k: &'a str,
    exp: i64,
    d: &'a T,
}

#[derive(serde::Deserialize)]
struct OpenedEnvelope<T> {
    k: String,
    exp: i64,
    d: T,
}

const CODE_ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";

impl Keys {
    pub fn derive(app_secret: &str) -> Result<Self, CryptoError> {
        if app_secret.chars().count() < MIN_SECRET_LEN {
            return Err(CryptoError::WeakSecret);
        }
        let hk = Hkdf::<Sha256>::new(Some(b"opensign"), app_secret.as_bytes());
        let keys = KeyPurpose::ALL.map(|purpose| {
            let mut key = Zeroizing::new([0u8; 32]);
            hk.expand(purpose.info(), key.as_mut()).expect("32 bytes is a valid HKDF-SHA256 output length");
            key
        });
        Ok(Self { keys })
    }

    fn key(&self, purpose: KeyPurpose) -> &[u8; 32] {
        let index = KeyPurpose::ALL.iter().position(|p| *p == purpose).expect("every purpose has a key");
        &self.keys[index]
    }

    fn cipher(&self, purpose: KeyPurpose) -> Aes256Gcm {
        Aes256Gcm::new_from_slice(self.key(purpose)).expect("32-byte key")
    }

    /// AES-256-GCM with `context` as associated data, so a ciphertext copied into another
    /// field or row does not decrypt. Output: `v1.<nonce>.<ciphertext+tag>`, base64url.
    pub fn encrypt(&self, purpose: KeyPurpose, context: &str, plaintext: &[u8]) -> String {
        let nonce = random_bytes::<12>();
        let ct = self
            .cipher(purpose)
            .encrypt(Nonce::from_slice(&nonce), Payload { msg: plaintext, aad: context.as_bytes() })
            .expect("AES-GCM encryption of an in-memory buffer cannot fail");
        format!("v1.{}.{}", B64.encode(nonce), B64.encode(ct))
    }

    pub fn decrypt(&self, purpose: KeyPurpose, context: &str, sealed: &str) -> Result<Zeroizing<Vec<u8>>, CryptoError> {
        let mut parts = sealed.split('.');
        let (Some("v1"), Some(nonce), Some(ct), None) = (parts.next(), parts.next(), parts.next(), parts.next()) else {
            return Err(CryptoError::Format);
        };
        let nonce = B64.decode(nonce).map_err(|_| CryptoError::Format)?;
        let ct = B64.decode(ct).map_err(|_| CryptoError::Format)?;
        if nonce.len() != 12 {
            return Err(CryptoError::Format);
        }
        self.cipher(purpose)
            .decrypt(Nonce::from_slice(&nonce), Payload { msg: &ct, aad: context.as_bytes() })
            .map(Zeroizing::new)
            .map_err(|_| CryptoError::Decrypt)
    }

    pub fn encrypt_str(&self, purpose: KeyPurpose, context: &str, plaintext: &str) -> String {
        self.encrypt(purpose, context, plaintext.as_bytes())
    }

    pub fn decrypt_str(&self, purpose: KeyPurpose, context: &str, sealed: &str) -> Result<Zeroizing<String>, CryptoError> {
        let bytes = self.decrypt(purpose, context, sealed)?;
        String::from_utf8(bytes.to_vec()).map(Zeroizing::new).map_err(|_| CryptoError::Decrypt)
    }

    /// Wraps a per-file data key with the document KEK, bound to the file's id.
    pub fn wrap_data_key(&self, key: &DataKey, file_id: &str) -> String {
        self.encrypt(KeyPurpose::DocumentKek, &format!("file:{file_id}"), key.as_bytes())
    }

    pub fn unwrap_data_key(&self, wrapped: &str, file_id: &str) -> Result<DataKey, CryptoError> {
        let bytes = self.decrypt(KeyPurpose::DocumentKek, &format!("file:{file_id}"), wrapped)?;
        DataKey::from_slice(&bytes).ok_or(CryptoError::Decrypt)
    }

    pub fn mac(&self, purpose: KeyPurpose, data: &[u8]) -> [u8; 32] {
        let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(self.key(purpose)).expect("HMAC takes any key length");
        mac.update(data);
        mac.finalize().into_bytes().into()
    }

    /// A tamper-proof, expiring token. `kind` is part of the MAC input, so a token issued for one
    /// purpose is refused for another.
    pub fn sign_token<T: Serialize>(&self, kind: &str, payload: &T, ttl_seconds: i64, now: i64) -> String {
        let body = serde_json::to_vec(&SignedEnvelope { k: kind, exp: now + ttl_seconds, d: payload }).expect("token payloads serialize");
        let body = B64.encode(body);
        let mac = B64.encode(self.mac(KeyPurpose::TokenMac, body.as_bytes()));
        format!("{body}.{mac}")
    }

    pub fn verify_token<T: DeserializeOwned>(&self, kind: &str, token: &str, now: i64) -> Result<T, CryptoError> {
        let (body, mac) = token.split_once('.').ok_or(CryptoError::InvalidToken)?;
        let expected = B64.encode(self.mac(KeyPurpose::TokenMac, body.as_bytes()));
        if !constant_time_eq(mac.as_bytes(), expected.as_bytes()) {
            return Err(CryptoError::InvalidToken);
        }
        let raw = B64.decode(body).map_err(|_| CryptoError::InvalidToken)?;
        let opened: OpenedEnvelope<T> = serde_json::from_slice(&raw).map_err(|_| CryptoError::InvalidToken)?;
        if opened.k != kind || opened.exp <= now {
            return Err(CryptoError::InvalidToken);
        }
        Ok(opened.d)
    }

    /// Printed in the container log on a fresh install. Derived, so every replica shows the same
    /// code and nothing is stored.
    pub fn setup_code(&self) -> String {
        let digest = self.mac(KeyPurpose::Setup, b"first-run");
        let code: String = digest[..8].iter().map(|b| CODE_ALPHABET[(*b as usize) % CODE_ALPHABET.len()] as char).collect();
        format!("{}-{}", &code[..4], &code[4..])
    }

    pub fn check_setup_code(&self, input: &str) -> bool {
        let normalized: String = input.trim().to_uppercase().chars().filter(char::is_ascii_alphanumeric).collect();
        constant_time_eq(normalized.as_bytes(), self.setup_code().replace('-', "").as_bytes())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    const SECRET: &str = "test-secret-test-secret-test-secret-0123";

    #[test]
    fn rejects_short_secrets() {
        assert!(matches!(Keys::derive("short"), Err(CryptoError::WeakSecret)));
    }

    #[test]
    fn purposes_get_different_keys() {
        let keys = Keys::derive(SECRET).unwrap();
        let all: Vec<_> = KeyPurpose::ALL.iter().map(|p| *keys.key(*p)).collect();
        for (i, a) in all.iter().enumerate() {
            for b in &all[i + 1..] {
                assert_ne!(a, b);
            }
        }
    }

    #[test]
    fn round_trips_and_binds_context() {
        let keys = Keys::derive(SECRET).unwrap();
        let sealed = keys.encrypt_str(KeyPurpose::Settings, "email.apiKey", "re_123");
        assert_eq!(&*keys.decrypt_str(KeyPurpose::Settings, "email.apiKey", &sealed).unwrap(), "re_123");
        assert!(keys.decrypt_str(KeyPurpose::Settings, "ai.apiKey", &sealed).is_err());
        assert!(keys.decrypt_str(KeyPurpose::DocumentKek, "email.apiKey", &sealed).is_err());
        let other = Keys::derive("another-secret-another-secret-another-0").unwrap();
        assert!(other.decrypt_str(KeyPurpose::Settings, "email.apiKey", &sealed).is_err());
    }

    #[test]
    fn detects_tampering() {
        let keys = Keys::derive(SECRET).unwrap();
        let sealed = keys.encrypt(KeyPurpose::Settings, "x", b"hello");
        let mut bytes = sealed.into_bytes();
        let last = bytes.len() - 2;
        bytes[last] = if bytes[last] == b'A' { b'B' } else { b'A' };
        assert!(keys.decrypt(KeyPurpose::Settings, "x", &String::from_utf8(bytes).unwrap()).is_err());
        assert!(matches!(keys.decrypt(KeyPurpose::Settings, "x", "v2.a.b"), Err(CryptoError::Format)));
    }

    #[test]
    fn same_plaintext_encrypts_differently() {
        let keys = Keys::derive(SECRET).unwrap();
        assert_ne!(keys.encrypt(KeyPurpose::Settings, "x", b"a"), keys.encrypt(KeyPurpose::Settings, "x", b"a"));
    }

    #[test]
    fn wraps_data_keys_per_file() {
        let keys = Keys::derive(SECRET).unwrap();
        let dek = DataKey::generate();
        let wrapped = keys.wrap_data_key(&dek, "file-1");
        assert_eq!(keys.unwrap_data_key(&wrapped, "file-1").unwrap().as_bytes(), dek.as_bytes());
        assert!(keys.unwrap_data_key(&wrapped, "file-2").is_err());
    }

    #[test]
    fn signed_tokens_expire_and_are_kind_bound() {
        let keys = Keys::derive(SECRET).unwrap();
        let token = keys.sign_token("consent", &serde_json::json!({ "client": "c1" }), 60, 1_000);
        let v: serde_json::Value = keys.verify_token("consent", &token, 1_030).unwrap();
        assert_eq!(v["client"], "c1");
        assert!(keys.verify_token::<serde_json::Value>("consent", &token, 1_060).is_err());
        assert!(keys.verify_token::<serde_json::Value>("signer", &token, 1_030).is_err());
        let forged = format!("{}x", token);
        assert!(keys.verify_token::<serde_json::Value>("consent", &forged, 1_030).is_err());
    }

    #[test]
    fn setup_code_is_stable_and_forgiving() {
        let keys = Keys::derive(SECRET).unwrap();
        let code = keys.setup_code();
        assert_eq!(code, Keys::derive(SECRET).unwrap().setup_code());
        assert_eq!(code.len(), 9);
        assert!(keys.check_setup_code(&code.to_lowercase().replace('-', " ")));
        assert!(!keys.check_setup_code("AAAA-AAAA"));
    }
}
