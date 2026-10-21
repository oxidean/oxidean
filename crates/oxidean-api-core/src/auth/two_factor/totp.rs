//! RFC 6238 TOTP (HMAC-SHA1, 6 digits, 30s step) over the established `hmac` +
//! `sha1` crates — no new crypto implementation, only the standard HOTP
//! dynamic-truncation formula.

use data_encoding::BASE32_NOPAD;
use hmac::{Hmac, KeyInit, Mac};
use sha1::Sha1;

pub const STEP_SECS: u64 = 30;
pub const DIGITS: u32 = 6;
const SECRET_BYTES: usize = 20;
/// One step of clock skew accepted in either direction (RFC 6238 §5.2 guidance).
const VERIFY_WINDOW: i64 = 1;

/// CSPRNG 160-bit secret → 32-char RFC 4648 base32 (authenticator-app format).
pub fn generate_secret_b32() -> String {
    let mut bytes = [0u8; SECRET_BYTES];
    rand::fill(&mut bytes);
    BASE32_NOPAD.encode(&bytes)
}

/// Manual-entry form: uppercase base32 grouped in fours (`ABCD EF12 …`).
pub fn group_secret(secret_b32: &str) -> String {
    secret_b32
        .chars()
        .collect::<Vec<_>>()
        .chunks(4)
        .map(|c| c.iter().collect::<String>())
        .collect::<Vec<_>>()
        .join(" ")
}

fn decode_secret(secret_b32: &str) -> Option<Vec<u8>> {
    let cleaned: String = secret_b32
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .collect::<String>()
        .to_ascii_uppercase();
    BASE32_NOPAD.decode(cleaned.as_bytes()).ok()
}

/// HOTP (RFC 4226): HMAC-SHA1 → dynamic truncation → `DIGITS`-wide code.
fn hotp(secret: &[u8], counter: u64) -> Option<u32> {
    let mut mac = <Hmac<Sha1> as KeyInit>::new_from_slice(secret).ok()?;
    mac.update(&counter.to_be_bytes());
    let h = mac.finalize().into_bytes();
    let offset = (h[h.len() - 1] & 0x0f) as usize;
    let bin =
        u32::from_be_bytes([h[offset], h[offset + 1], h[offset + 2], h[offset + 3]]) & 0x7fff_ffff;
    Some(bin % 10u32.pow(DIGITS))
}

/// Expected code at a Unix time (tests + internal verification).
pub fn code_at_unix(secret_b32: &str, unix_secs: u64) -> Option<String> {
    let secret = decode_secret(secret_b32)?;
    let code = hotp(&secret, unix_secs / STEP_SECS)?;
    Some(format!("{code:0width$}", width = DIGITS as usize))
}

/// Verify a user-entered code against the secret at `unix_secs ±1 step`.
/// Codes are compared after normalization (digits only).
pub fn verify_code(secret_b32: &str, code: &str, unix_secs: u64) -> bool {
    let trimmed: String = code.chars().filter(|c| !c.is_whitespace()).collect();
    if trimmed.len() != DIGITS as usize || !trimmed.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    let step = (unix_secs / STEP_SECS) as i64;
    for skew in -VERIFY_WINDOW..=VERIFY_WINDOW {
        let counter = step.saturating_add(skew);
        if counter < 0 {
            continue;
        }
        if code_at_unix(secret_b32, (counter as u64) * STEP_SECS).as_deref() == Some(&trimmed) {
            return true;
        }
    }
    false
}

/// `otpauth://` URI for QR + manual-entry flows.
/// `otpauth://totp/{issuer}:{account}?secret=..&issuer=..&algorithm=SHA1&digits=6&period=30`
pub fn otpauth_uri(issuer: &str, account: &str, secret_b32: &str) -> String {
    let enc = |s: &str| -> String { url::form_urlencoded::byte_serialize(s.as_bytes()).collect() };
    format!(
        "otpauth://totp/{}:{}?secret={}&issuer={}&algorithm=SHA1&digits={}&period={}",
        enc(issuer),
        enc(account),
        secret_b32,
        enc(issuer),
        DIGITS,
        STEP_SECS,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC 6238 Appendix B SHA-1 test vector: ASCII "12345678901234567890".
    #[test]
    fn rfc6238_sha1_vector() {
        let secret_b32 = BASE32_NOPAD.encode(b"12345678901234567890");
        assert_eq!(secret_b32, "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ");
        // t=59 → counter 1 → 94287082 → 287082 (8-digit) → 6-digit 287082.
        assert_eq!(code_at_unix(&secret_b32, 59).as_deref(), Some("287082"));
        // t=1111111109 → 07081804 → 081804.
        assert_eq!(
            code_at_unix(&secret_b32, 1111111109).as_deref(),
            Some("081804")
        );
        // t=1234567890 → 00592489 wait 89005924 → 005924 → check 6-digit = 005924.
        assert_eq!(
            code_at_unix(&secret_b32, 1234567890).as_deref(),
            Some("005924")
        );
    }

    #[test]
    fn verify_accepts_current_step() {
        let secret = generate_secret_b32();
        let now = 1_700_000_000u64;
        let code = code_at_unix(&secret, now).unwrap();
        assert!(verify_code(&secret, &code, now));
        assert!(verify_code(&secret, &code, now + STEP_SECS)); // next step still valid
        assert!(!verify_code(&secret, "000000", now));
        assert!(!verify_code(&secret, "12345", now)); // wrong length
    }

    #[test]
    fn secret_format_and_grouping() {
        let s = generate_secret_b32();
        assert_eq!(s.len(), 32);
        assert!(s.chars().all(|c| c.is_ascii_alphanumeric()));
        let g = group_secret(&s);
        assert!(g.contains(' '));
        let cleaned: String = g.chars().filter(|c| !c.is_whitespace()).collect();
        assert_eq!(cleaned, s);
    }

    #[test]
    fn uri_encodes_issuer_and_account() {
        let uri = otpauth_uri("Oxidean localhost", "ada@x.io", "ABC234");
        assert!(uri.starts_with("otpauth://totp/"));
        assert!(uri.contains("secret=ABC234"));
        assert!(uri.contains("issuer=Oxidean+localhost"));
        assert!(uri.contains("digits=6"));
        assert!(uri.contains("period=30"));
    }
}
