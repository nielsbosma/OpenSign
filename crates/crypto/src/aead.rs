use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use zeroize::Zeroizing;

use crate::CryptoError;
use crate::token::random_bytes;

const MAGIC: &[u8; 4] = b"OSB1";
const HEADER_LEN: usize = 4 + 12;

/// A random key for one stored file. Only its wrapped form (see `Keys::wrap_data_key`) is persisted.
pub struct DataKey(Zeroizing<[u8; 32]>);

impl DataKey {
    pub fn generate() -> Self {
        Self(Zeroizing::new(random_bytes::<32>()))
    }

    pub(crate) fn from_slice(bytes: &[u8]) -> Option<Self> {
        let array: [u8; 32] = bytes.try_into().ok()?;
        Some(Self(Zeroizing::new(array)))
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Encrypts a file body for the bucket: `OSB1 ‖ nonce ‖ ciphertext+tag`. `aad` binds it to its
/// file id, so blobs cannot be swapped between files in the bucket.
pub fn seal_blob(key: &DataKey, aad: &[u8], plaintext: &[u8]) -> Vec<u8> {
    let nonce = random_bytes::<12>();
    let cipher = Aes256Gcm::new_from_slice(key.as_bytes()).expect("32-byte key");
    let ct = cipher
        .encrypt(Nonce::from_slice(&nonce), Payload { msg: plaintext, aad })
        .expect("AES-GCM encryption of an in-memory buffer cannot fail");
    let mut out = Vec::with_capacity(HEADER_LEN + ct.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ct);
    out
}

pub fn open_blob(key: &DataKey, aad: &[u8], sealed: &[u8]) -> Result<Vec<u8>, CryptoError> {
    if sealed.len() < HEADER_LEN + 16 || &sealed[..4] != MAGIC {
        return Err(CryptoError::Format);
    }
    let cipher = Aes256Gcm::new_from_slice(key.as_bytes()).expect("32-byte key");
    cipher
        .decrypt(Nonce::from_slice(&sealed[4..HEADER_LEN]), Payload { msg: &sealed[HEADER_LEN..], aad })
        .map_err(|_| CryptoError::Decrypt)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_binds_aad() {
        let key = DataKey::generate();
        let sealed = seal_blob(&key, b"file-1", b"%PDF-1.7 ...");
        assert_eq!(open_blob(&key, b"file-1", &sealed).unwrap(), b"%PDF-1.7 ...");
        assert!(open_blob(&key, b"file-2", &sealed).is_err());
        assert!(open_blob(&DataKey::generate(), b"file-1", &sealed).is_err());
    }

    #[test]
    fn rejects_truncated_and_foreign_blobs() {
        let key = DataKey::generate();
        let sealed = seal_blob(&key, b"a", b"data");
        assert!(open_blob(&key, b"a", &sealed[..sealed.len() - 1]).is_err());
        assert!(matches!(open_blob(&key, b"a", b"%PDF-1.7 plain file"), Err(CryptoError::Format)));
    }
}
