//! Secret-shaped content detection for transcript/log writes — distinct
//! from the runner's `mask_secrets`, which masks *known* values supplied by
//! the caller. `redact` scans text for tokens that look like credentials
//! (bearer tokens, JWTs, PEM blocks, vendor key idioms) and replaces each
//! match with `***` so persisted content never holds detected secret
//! material.
//!
//! Detection is heuristic, not exhaustive — the pattern table below is the
//! documented recall limit; novel formats pass through unmasked. Callers
//! must not rely on `redact` for values they already know are secrets
//! (mask those at the source); `redact` catches secrets embedded in
//! free-text content like pasted chat messages.

const MASK: &str = "***";
const PEM_BEGIN: &str = "-----BEGIN ";
const PEM_END: &str = "-----END ";

/// Key-idiom prefixes that flag a token as secret-shaped. Anything after
/// the prefix is consumed while it stays in the token alphabet.
const KEY_PREFIXES: &[&str] = &[
    // Oxidean PATs — never persist a usable credential.
    "oxidean_pat_",
    "oxidean_fg_",
    // Job runtime tokens.
    "oxidean_job_",
    // GitHub PATs + OAuth/installation/user-to-server tokens.
    "github_pat_",
    "ghp_",
    "gho_",
    "ghu_",
    "ghs_",
    "ghr_",
    // GitLab personal tokens.
    "glpat-",
    // Slack tokens.
    "xoxb-",
    "xoxa-",
    "xoxp-",
    "xoxr-",
    "xoxs-",
    // Stripe.
    "sk_live_",
    "rk_live_",
    // OpenAI-style keys (prefix match before the generic `sk-` rule below).
    "sk-ant-",
    // npm + PyPI tokens.
    "npm_",
    "pypi-",
    // AWS access-key id.
    "AKIA",
    // Google API key.
    "AIza",
];

/// Minimum characters after a matched prefix for a token to count — guards
/// against masking ordinary prose that happens to carry a prefix fragment
/// (e.g. the word "`sk-live`" alone).
const MIN_SUFFIX_LEN: usize = 8;

/// Token alphabet for secret material — base64/base64url plus the small
/// punctuation set real formats carry (JWT dots, Stripe dashes).
fn is_secret_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '~' | '+' | '/' | '=')
}

/// `true` when `s` starts a JWT — `eyJ` (base64 `{"`) followed by at least
/// two further dot-separated segments.
fn looks_like_jwt(s: &str) -> bool {
    if !s.starts_with("eyJ") {
        return false;
    }
    s.matches('.').count() >= 2 && s.len() > 20
}

/// Redact secret-shaped content in `input`; returns the masked string.
/// Idempotent — re-running is a no-op (`***` never matches a pattern).
pub fn redact(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // PEM block — mask header through footer inclusive.
        if input[i..].starts_with(PEM_BEGIN) {
            if let Some(end_rel) = input[i..].find(PEM_END) {
                let footer_start = i + end_rel;
                // Consume the footer line through its terminating "-----".
                let label_start = footer_start + PEM_END.len();
                let footer_end = input[label_start..]
                    .find("-----")
                    .map(|rel| label_start + rel + "-----".len())
                    .unwrap_or(label_start);
                out.push_str(MASK);
                i = footer_end.min(input.len());
                continue;
            }
            // Unterminated BEGIN — mask to end; nothing after it is safe.
            out.push_str(MASK);
            break;
        }

        let rest = &input[i..];

        // JWT — checked before generic prefixes because `eyJ` overlaps no
        // vendor idiom but needs the three-segment shape test.
        if looks_like_jwt(rest) {
            let end = consume_secret(rest, i);
            out.push_str(MASK);
            i = end;
            continue;
        }

        // Generic OpenAI-style `sk-` — short prefix, kept separate so the
        // require-length rule still applies.
        if rest.starts_with("sk-") {
            let end = consume_secret(rest, i);
            if end - i - "sk-".len() >= MIN_SUFFIX_LEN {
                out.push_str(MASK);
                i = end;
                continue;
            }
        }

        if let Some(prefix) = KEY_PREFIXES.iter().find(|p| rest.starts_with(**p)) {
            let end = consume_secret(rest, i);
            if end - i - prefix.len() >= MIN_SUFFIX_LEN {
                out.push_str(MASK);
                i = end;
                continue;
            }
        }

        // Bearer token idiom — `Bearer <token>` masks only the token.
        if rest.starts_with("Bearer ") || rest.starts_with("bearer ") {
            let token_start = i + "Bearer ".len();
            let token_rest = &input[token_start..];
            let end = consume_secret(token_rest, token_start);
            if end - token_start >= MIN_SUFFIX_LEN {
                out.push_str(&input[i..token_start]);
                out.push_str(MASK);
                i = end;
                continue;
            }
        }

        // One byte of prose. Only match prefixes at byte positions — all
        // pattern tokens are ASCII so slicing stays on char boundaries.
        let ch_len = utf8_len(bytes[i]);
        out.push_str(&input[i..i + ch_len]);
        i += ch_len;
    }
    out
}

/// Consume the secret-character run starting at `s` (which is
/// `input[start..]`); returns the absolute end index.
fn consume_secret(s: &str, start: usize) -> usize {
    let mut n = 0usize;
    for (off, c) in s.char_indices() {
        if !is_secret_char(c) {
            n = off;
            break;
        }
        n = off + c.len_utf8();
    }
    start + n
}

fn utf8_len(b: u8) -> usize {
    if b < 0x80 {
        1
    } else if b < 0xE0 {
        2
    } else if b < 0xF0 {
        3
    } else {
        4
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bearer_and_pat_tokens_redacted() {
        assert_eq!(
            redact("auth: oxidean_pat_abc123def456xyz789 tail"),
            "auth: *** tail"
        );
        assert_eq!(redact("Bearer abcdef0123456789"), "Bearer ***");
    }

    #[test]
    fn vendor_key_idioms_redacted() {
        assert_eq!(redact("key ghp_0123456789abcdefghij here"), "key *** here");
        assert_eq!(redact("AKIAIOSFODNN7EXAMPLE in text"), "*** in text");
        assert_eq!(
            redact("openai sk-abcDEF0123456789xyz done"),
            "openai *** done"
        );
    }

    #[test]
    fn jwt_redacted() {
        let jwt = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxIn0.eyJhbGciOiJIUzI1NiJ9";
        assert_eq!(redact(&format!("t={jwt}")), "t=***");
    }

    #[test]
    fn pem_block_redacted_inclusive() {
        let pem = "before\n-----BEGIN PRIVATE KEY-----\nMIIabc\n-----END PRIVATE KEY-----\nafter";
        assert_eq!(redact(pem), "before\n***\nafter");
    }

    #[test]
    fn prose_passes_through() {
        let text = "please sk-live debug the bearer problem";
        assert_eq!(redact(text), text);
    }

    #[test]
    fn redact_is_idempotent() {
        let once = redact("x oxidean_fg_0123456789abcdef");
        assert_eq!(once, "x ***");
        assert_eq!(redact(&once), once);
    }
}
