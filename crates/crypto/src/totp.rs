use hmac::{Hmac, Mac};
use sha1::Sha1;
use zeroize::Zeroizing;

use crate::token::{constant_time_eq, random_bytes};

const STEP_SECONDS: i64 = 30;
const DIGITS: u32 = 6;
const BASE32: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum TotpError {
    #[error("the secret is not valid base32")]
    BadSecret,
}

/// RFC 6238 TOTP (SHA-1, 6 digits, 30 s), which every authenticator app supports.
pub struct Totp {
    secret: Zeroizing<Vec<u8>>,
}

impl Totp {
    pub fn generate() -> Self {
        Self { secret: Zeroizing::new(random_bytes::<20>().to_vec()) }
    }

    pub fn from_base32(secret: &str) -> Result<Self, TotpError> {
        let mut bits: u64 = 0;
        let mut count = 0;
        let mut out = Vec::new();
        for c in secret.chars().filter(|c| !c.is_whitespace() && *c != '=') {
            let value = BASE32.iter().position(|b| *b as char == c.to_ascii_uppercase()).ok_or(TotpError::BadSecret)?;
            bits = (bits << 5) | value as u64;
            count += 5;
            if count >= 8 {
                count -= 8;
                out.push((bits >> count) as u8);
            }
        }
        if out.len() < 10 {
            return Err(TotpError::BadSecret);
        }
        Ok(Self { secret: Zeroizing::new(out) })
    }

    pub fn to_base32(&self) -> Zeroizing<String> {
        let mut out = String::new();
        let mut bits: u64 = 0;
        let mut count = 0;
        for byte in self.secret.iter() {
            bits = (bits << 8) | *byte as u64;
            count += 8;
            while count >= 5 {
                count -= 5;
                out.push(BASE32[((bits >> count) & 31) as usize] as char);
            }
        }
        if count > 0 {
            out.push(BASE32[((bits << (5 - count)) & 31) as usize] as char);
        }
        Zeroizing::new(out)
    }

    pub fn otpauth_url(&self, issuer: &str, account: &str) -> String {
        let enc = |s: &str| {
            s.bytes()
                .map(|b| match b {
                    b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => (b as char).to_string(),
                    _ => format!("%{b:02X}"),
                })
                .collect::<String>()
        };
        format!(
            "otpauth://totp/{}:{}?secret={}&issuer={}&algorithm=SHA1&digits={DIGITS}&period={STEP_SECONDS}",
            enc(issuer),
            enc(account),
            self.to_base32().as_str(),
            enc(issuer)
        )
    }

    fn code_at_step(&self, step: i64) -> String {
        let mut mac = <Hmac<Sha1> as Mac>::new_from_slice(&self.secret).expect("HMAC takes any key length");
        mac.update(&step.to_be_bytes());
        let digest = mac.finalize().into_bytes();
        let offset = (digest[19] & 0x0f) as usize;
        let value = u32::from_be_bytes([digest[offset] & 0x7f, digest[offset + 1], digest[offset + 2], digest[offset + 3]]);
        format!("{:0width$}", value % 10u32.pow(DIGITS), width = DIGITS as usize)
    }

    pub fn code_at(&self, unix_seconds: i64) -> String {
        self.code_at_step(unix_seconds.div_euclid(STEP_SECONDS))
    }

    /// Accepts the current step and one either side for clock drift. Returns the matching step,
    /// which the caller stores and refuses to accept again (`last_step`), so a code works once.
    pub fn verify(&self, code: &str, unix_seconds: i64, last_step: Option<i64>) -> Option<i64> {
        let code: String = code.chars().filter(|c| !c.is_whitespace()).collect();
        if code.len() != DIGITS as usize || !code.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        let now = unix_seconds.div_euclid(STEP_SECONDS);
        let mut matched = None;
        for step in [now - 1, now, now + 1] {
            if constant_time_eq(self.code_at_step(step).as_bytes(), code.as_bytes()) && last_step.is_none_or(|last| step > last) {
                matched = Some(step);
            }
        }
        matched
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn rfc_secret() -> Totp {
        Totp { secret: Zeroizing::new(b"12345678901234567890".to_vec()) }
    }

    #[test]
    fn matches_rfc6238_vectors() {
        let totp = rfc_secret();
        assert_eq!(totp.code_at(59), "287082");
        assert_eq!(totp.code_at(1_111_111_109), "081804");
        assert_eq!(totp.code_at(1_234_567_890), "005924");
        assert_eq!(totp.code_at(20_000_000_000), "353130");
    }

    #[test]
    fn base32_round_trips() {
        let totp = rfc_secret();
        assert_eq!(totp.to_base32().as_str(), "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ");
        let back = Totp::from_base32("gezd gnbv gy3t qojq gezd gnbv gy3t qojq").unwrap();
        assert_eq!(back.code_at(59), "287082");
        assert!(Totp::from_base32("not base32!").is_err());
    }

    #[test]
    fn codes_work_once_within_the_window() {
        let totp = Totp::generate();
        let now = 1_700_000_000;
        let code = totp.code_at(now);
        let step = totp.verify(&code, now + 20, None).unwrap();
        assert_eq!(totp.verify(&code, now + 20, Some(step)), None);
        assert_eq!(totp.verify(&code, now + 120, None), None);
        assert_eq!(totp.verify("12345", now, None), None);
    }

    #[test]
    fn builds_an_otpauth_url() {
        let url = Totp::generate().otpauth_url("OpenSign", "anna@example.com");
        assert!(url.starts_with("otpauth://totp/OpenSign:anna%40example.com?secret="));
    }
}
