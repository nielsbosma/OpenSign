//! Every key OpenSign uses is derived from `APP_SECRET` with HKDF, one key per purpose, so a leak
//! of one derived key does not expose the others and one secret is enough to configure.

mod aead;
mod keys;
mod password;
mod token;
mod totp;

pub use aead::{DataKey, open_blob, seal_blob};
pub use keys::{KeyPurpose, Keys, MIN_SECRET_LEN};
pub use password::{MIN_PASSWORD_LEN, PasswordError, burn_password_check, hash_password, password_needs_rehash, verify_password};
pub use token::{constant_time_eq, random_bytes, random_token, recovery_codes, sha256_hex};
pub use totp::{Totp, TotpError};

#[derive(Debug, thiserror::Error)]
pub enum CryptoError {
    #[error("APP_SECRET must be at least {MIN_SECRET_LEN} characters")]
    WeakSecret,
    #[error("unsupported ciphertext format")]
    Format,
    #[error("decryption failed")]
    Decrypt,
    #[error("the token is invalid or has expired")]
    InvalidToken,
}
