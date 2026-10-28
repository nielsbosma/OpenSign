use std::sync::OnceLock;

use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::{Algorithm, Argon2, Params, Version};
use unicode_normalization::UnicodeNormalization;
use zeroize::Zeroizing;

pub const MIN_PASSWORD_LEN: usize = 12;
const MAX_PASSWORD_LEN: usize = 256;
const MEMORY_KIB: u32 = 64 * 1024;
const ITERATIONS: u32 = 3;
const LANES: u32 = 1;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PasswordError {
    #[error("use at least {MIN_PASSWORD_LEN} characters")]
    TooShort,
    #[error("use at most {MAX_PASSWORD_LEN} characters")]
    TooLong,
}

fn argon2() -> Argon2<'static> {
    let params = Params::new(MEMORY_KIB, ITERATIONS, LANES, Some(32)).expect("valid Argon2 parameters");
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
}

fn normalize(password: &str) -> Zeroizing<String> {
    Zeroizing::new(password.nfkc().collect())
}

/// Argon2id, 64 MiB, 3 passes. Returns a PHC string.
pub fn hash_password(password: &str) -> Result<String, PasswordError> {
    let length = password.chars().count();
    if length < MIN_PASSWORD_LEN {
        return Err(PasswordError::TooShort);
    }
    if length > MAX_PASSWORD_LEN {
        return Err(PasswordError::TooLong);
    }
    let salt = SaltString::generate(&mut OsRng);
    Ok(argon2().hash_password(normalize(password).as_bytes(), &salt).expect("Argon2 hashing with valid parameters").to_string())
}

pub fn verify_password(stored: &str, password: &str) -> bool {
    if password.chars().count() > MAX_PASSWORD_LEN {
        return false;
    }
    let Ok(parsed) = PasswordHash::new(stored) else { return false };
    argon2().verify_password(normalize(password).as_bytes(), &parsed).is_ok()
}

/// True when the stored hash uses weaker parameters than today's, so sign-in can upgrade it.
pub fn password_needs_rehash(stored: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(stored) else { return true };
    let Ok(params) = Params::try_from(&parsed) else { return true };
    parsed.algorithm.as_str() != "argon2id" || params.m_cost() < MEMORY_KIB || params.t_cost() < ITERATIONS
}

static DUMMY: OnceLock<String> = OnceLock::new();

/// Spends as long as a real check, so an unknown email cannot be told apart by timing.
pub fn burn_password_check(password: &str) {
    let dummy = DUMMY.get_or_init(|| hash_password("not-a-real-password-0").expect("valid dummy password"));
    let _ = verify_password(dummy, password);
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn hashes_and_verifies() {
        let hash = hash_password("correct horse battery").unwrap();
        assert!(hash.starts_with("$argon2id$v=19$m=65536,t=3,p=1$"));
        assert!(verify_password(&hash, "correct horse battery"));
        assert!(!verify_password(&hash, "correct horse batterY"));
        assert!(!password_needs_rehash(&hash));
    }

    #[test]
    fn normalizes_unicode() {
        let hash = hash_password("p\u{e5}ssword-long-enough").unwrap();
        assert!(verify_password(&hash, "pa\u{30a}ssword-long-enough"));
    }

    #[test]
    fn enforces_length() {
        assert_eq!(hash_password("short"), Err(PasswordError::TooShort));
        assert_eq!(hash_password(&"x".repeat(257)), Err(PasswordError::TooLong));
        assert!(!verify_password("garbage", "whatever-password"));
    }

    #[test]
    fn flags_weak_hashes_for_rehash() {
        let weak = Argon2::new(Algorithm::Argon2id, Version::V0x13, Params::new(8 * 1024, 1, 1, None).unwrap())
            .hash_password(b"correct horse battery", &SaltString::generate(&mut OsRng))
            .unwrap()
            .to_string();
        assert!(verify_password(&weak, "correct horse battery"));
        assert!(password_needs_rehash(&weak));
    }
}
