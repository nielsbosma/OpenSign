use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use rand::RngCore;
use rand::rngs::OsRng;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

pub fn random_bytes<const N: usize>() -> [u8; N] {
    let mut bytes = [0u8; N];
    OsRng.fill_bytes(&mut bytes);
    bytes
}

/// 256 bits from the OS RNG, base64url. Store only `sha256_hex` of it.
pub fn random_token() -> String {
    B64.encode(random_bytes::<32>())
}

pub fn sha256_hex(data: impl AsRef<[u8]>) -> String {
    hex::encode(Sha256::digest(data.as_ref()))
}

pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && bool::from(a.ct_eq(b))
}

const RECOVERY_ALPHABET: &[u8] = b"abcdefghjkmnpqrstuvwxyz23456789";

/// Ten codes like `k7mq-x2pd-r9tw`, about 70 bits each.
pub fn recovery_codes() -> Vec<String> {
    (0..10)
        .map(|_| {
            let bytes = random_bytes::<12>();
            let chars: Vec<char> = bytes.iter().map(|b| RECOVERY_ALPHABET[(*b as usize) % RECOVERY_ALPHABET.len()] as char).collect();
            chars.chunks(4).map(|c| c.iter().collect::<String>()).collect::<Vec<_>>().join("-")
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_unique_and_long() {
        let a = random_token();
        assert_eq!(a.len(), 43);
        assert_ne!(a, random_token());
    }

    #[test]
    fn compares_in_constant_time_shape() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"abcd"));
    }

    #[test]
    fn recovery_codes_look_right() {
        let codes = recovery_codes();
        assert_eq!(codes.len(), 10);
        assert!(codes.iter().all(|c| c.len() == 14 && c.matches('-').count() == 2));
    }
}
