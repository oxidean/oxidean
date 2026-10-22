//! Recovery codes: 10 one-time codes, `xxxxx-xxxxx` shape (10 base32 chars),
//! stored as SHA-256 hashes — plaintext returned to the user exactly once.

use crate::auth::session::sha256_hex;

pub const RECOVERY_CODE_COUNT: usize = 10;

const ALPHABET: &[u8; 32] = b"0123456789abcdefghjkmnpqrstvwxyz";

fn random_char() -> char {
    let mut b = [0u8; 1];
    loop {
        rand::fill(&mut b);
        // Rejection-sample to keep the 32-char alphabet uniform (224 = 7*32).
        if b[0] < 224 {
            return ALPHABET[(b[0] % 32) as usize] as char;
        }
    }
}

/// One code: `xxxxx-xxxxx` (10 chars + separator).
fn generate_one() -> String {
    let mut code = String::with_capacity(11);
    for i in 0..10 {
        if i == 5 {
            code.push('-');
        }
        code.push(random_char());
    }
    code
}

/// A fresh set of `RECOVERY_CODE_COUNT` unique codes.
pub fn generate() -> Vec<String> {
    let mut codes = Vec::with_capacity(RECOVERY_CODE_COUNT);
    while codes.len() < RECOVERY_CODE_COUNT {
        let c = generate_one();
        if !codes.contains(&c) {
            codes.push(c);
        }
    }
    codes
}

/// Normalize user input: lowercase, strip separators/space.
pub fn normalize(code: &str) -> String {
    code.chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .collect::<String>()
        .to_ascii_lowercase()
}

/// Hash form persisted and compared (SHA-256 hex of the normalized code).
pub fn hash(code: &str) -> String {
    sha256_hex(normalize(code).as_bytes())
}

/// `xxxxx-xxxxx` shape check (recovery codes are the only 10-char method).
pub fn looks_like_recovery_code(code: &str) -> bool {
    let n = normalize(code);
    n.len() == 10 && n.chars().all(|c| c.is_ascii_alphanumeric())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_unique_shaped_codes() {
        let codes = generate();
        assert_eq!(codes.len(), RECOVERY_CODE_COUNT);
        let mut dedup = codes.clone();
        dedup.sort();
        dedup.dedup();
        assert_eq!(dedup.len(), RECOVERY_CODE_COUNT);
        for c in &codes {
            assert_eq!(c.len(), 11);
            assert_eq!(&c[5..6], "-");
            assert!(looks_like_recovery_code(c));
        }
    }

    #[test]
    fn normalize_accepts_upper_and_separators() {
        assert_eq!(normalize("ABCDE-FGHIJ"), "abcdefghij");
        assert_eq!(normalize("  abcde  fghij "), "abcdefghij");
        let c = generate_one();
        assert_eq!(hash(&format!(" {c} ")), hash(&c));
    }
}
