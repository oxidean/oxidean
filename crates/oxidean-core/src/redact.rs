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
/// two further dot-separated segments. Dots are counted inside the candidate
/// token only (the secret-char run), never across the whole remaining input —
/// otherwise any `eyJ`-starting word followed by two sentence periods would
/// mask prose.
fn looks_like_jwt(s: &str) -> bool {
    if !s.starts_with("eyJ") {
        return false;
    }
    let token_len = s.find(|c: char| !is_secret_char(c)).unwrap_or(s.len());
    let token = &s[..token_len];
    token.matches('.').count() >= 2 && token.len() > 20
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

        // Token boundary — prefix matches only count at a token start, i.e.
        // the preceding char is not ASCII alphanumeric. Otherwise ordinary
        // words mask as secrets: `task-runner-service` matches `sk-` at byte
        // 2, `ringbearer <word>` matches `bearer ` at byte 4.
        let token_start = i == 0
            || !input[..i]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_ascii_alphanumeric());

        // JWT — checked before generic prefixes because `eyJ` overlaps no
        // vendor idiom but needs the three-segment shape test.
        if token_start && looks_like_jwt(rest) {
            let end = consume_secret(rest, i);
            out.push_str(MASK);
            i = end;
            continue;
        }

        // Generic OpenAI-style `sk-` — short prefix, kept separate so the
        // require-length rule still applies.
        if token_start && rest.starts_with("sk-") {
            let end = consume_secret(rest, i);
            if end - i - "sk-".len() >= MIN_SUFFIX_LEN {
                out.push_str(MASK);
                i = end;
                continue;
            }
        }

        if token_start {
            if let Some(prefix) = KEY_PREFIXES.iter().find(|p| rest.starts_with(**p)) {
                let end = consume_secret(rest, i);
                if end - i - prefix.len() >= MIN_SUFFIX_LEN {
                    out.push_str(MASK);
                    i = end;
                    continue;
                }
            }
        }

        // Bearer token idiom — `Bearer <token>` masks only the token.
        if token_start && (rest.starts_with("Bearer ") || rest.starts_with("bearer ")) {
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

/// Match openers whose token run is still in flight when they end a
/// buffer — everything [`redact`] can start a match on except the
/// bare `Bearer`/`PEM` forms, which carry a trailing non-secret
/// character and are covered by [`AWAITING_RUN`].
const STREAM_OPENERS: &[&str] = &["eyJ", "sk-"];

/// Complete openers that still await their token/body — they end in a
/// NON-secret char (space), so they never sit inside the trailing
/// secret-char run and the in-run scan cannot see them.
const AWAITING_RUN: &[&str] = &[PEM_BEGIN, "Bearer ", "bearer "];

/// Streaming variant of [`redact`] for delta-wise emission. Feeding
/// each provider delta through `redact` independently misses any
/// secret split across a chunk boundary (`"ghp_"` + `"abc123…"`,
/// `"Bear"` + `"er xyz…"`): each half passes its own pattern check.
/// `StreamRedactor` holds back the buffer's trailing in-flight
/// candidate — a partial opener, a matched opener whose token run
/// hasn't terminated, or a bare [`AWAITING_RUN`] opener awaiting its
/// token — and emits only `redact`ed text that can no longer extend
/// a match. Flush emits whatever remains, where buffer-end IS the
/// token end (an unterminated long run still masks).
#[derive(Default)]
pub struct StreamRedactor {
    /// Text held back as a possible in-flight candidate — bounded by
    /// the longest candidate span, not the stream.
    pending: String,
}

impl StreamRedactor {
    pub fn new() -> Self {
        Self::default()
    }

    /// Append `delta`; returns the redacted prefix now safe to emit —
    /// possibly empty while a candidate is still in flight.
    pub fn push(&mut self, delta: &str) -> String {
        self.pending.push_str(delta);
        let hold = hold_start(&self.pending);
        if hold == 0 {
            return String::new();
        }
        let out = redact(&self.pending[..hold]);
        self.pending.drain(..hold);
        out
    }

    /// Stream-end drain — the pending tail resolves as-is under the
    /// same rules (a still-open candidate at end-of-input masks per
    /// [`redact`]).
    pub fn flush(&mut self) -> String {
        let pending = std::mem::take(&mut self.pending);
        redact(&pending)
    }
}

/// `true` when `tail` (a suffix of secret-alphabet chars, so every
/// byte past an opener is still token-eligible) starts with a match
/// opener or IS the strict prefix of one — both keep the tail in
/// flight until the run terminates.
fn tail_is_in_flight(tail: &str) -> bool {
    STREAM_OPENERS
        .iter()
        .chain(KEY_PREFIXES)
        .chain(AWAITING_RUN)
        .any(|p| tail.starts_with(p) || p.starts_with(tail))
}

/// The earliest index where `buf[i..]` is still an in-flight secret
/// candidate; `buf.len()` means nothing is in flight.
fn hold_start(buf: &str) -> usize {
    // A bare `Bearer `/`-----BEGIN ` at end awaits its token/body —
    // the trailing run is empty, so the in-run scan never sees it.
    for opener in AWAITING_RUN {
        if buf.ends_with(opener) {
            return buf.len() - opener.len();
        }
    }
    // Only the trailing secret-char run can still extend a match —
    // everything before it is settled text.
    let run_start = buf
        .char_indices()
        .rev()
        .find(|(_, c)| !is_secret_char(*c))
        .map(|(i, c)| i + c.len_utf8())
        .unwrap_or(0);
    // `Bearer <run>` — the separated-token idiom's opener sits just
    // before the run (conservative on boundary: an embedded `bearer`
    // mid-word also holds — late by one token, never unsafe).
    for opener in AWAITING_RUN {
        if buf[..run_start].ends_with(opener) {
            return run_start - opener.len();
        }
    }
    // Inside the run, a position following a non-ALPHANUMERIC char is a
    // token start under the same rule `redact` applies — `x-sk-…`
    // matches at `s`. Run chars are all ASCII, so byte steps stay on
    // char boundaries.
    let bytes = buf.as_bytes();
    for i in run_start..buf.len() {
        let token_start = i == 0 || !bytes[i - 1].is_ascii_alphanumeric();
        if token_start && tail_is_in_flight(&buf[i..]) {
            return i;
        }
    }
    buf.len()
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

    /// Feed `chunks` through a StreamRedactor; the concatenated output
    /// must equal one-shot `redact` of the whole input.
    fn stream(chunks: &[&str]) -> String {
        let mut r = StreamRedactor::new();
        let mut out = String::new();
        for c in chunks {
            out.push_str(&r.push(c));
        }
        out.push_str(&r.flush());
        out
    }

    #[test]
    fn stream_masks_secret_split_across_deltas() {
        let secret = "ghp_0123456789abcdefghij";
        let text = format!("key {secret} tail");
        for split in ["key ghp_".len(), "key ghp_012".len(), text.len() - 1] {
            let out = stream(&[&text[..split], &text[split..]]);
            assert_eq!(out, "key *** tail", "split at {split}");
        }
        // One char at a time — the worst-case delta granularity.
        let chars: Vec<&str> = text
            .char_indices()
            .map(|(i, _)| i)
            .chain(std::iter::once(text.len()))
            .collect::<Vec<_>>()
            .windows(2)
            .map(|w| &text[w[0]..w[1]])
            .collect();
        assert_eq!(stream(&chars), "key *** tail");
    }

    #[test]
    fn stream_masks_bearer_split_inside_the_opener() {
        let text = "use Bearer abcdef0123456789 now";
        for split in ["use Bea".len(), "use Bearer ".len(), "use Bearer ab".len()] {
            let out = stream(&[&text[..split], &text[split..]]);
            assert_eq!(out, "use Bearer *** now", "split at {split}");
        }
    }

    #[test]
    fn stream_passes_prose_and_incomplete_candidates() {
        // Under the length floor the candidate resolves unmasked.
        assert_eq!(stream(&["key sk-abc", " tail"]), "key sk-abc tail");
        // Bare prose never holds.
        assert_eq!(
            stream(&["the answer is ", "forty-two"]),
            "the answer is forty-two"
        );
    }

    #[test]
    fn stream_masks_pem_split_across_deltas() {
        let pem = "h\n-----BEGIN PRIVATE KEY-----\nMIIabc\n-----END PRIVATE KEY-----\nt";
        for split in ["h\n-----BE".len(), "h\n-----BEGIN ".len(), 20] {
            let out = stream(&[&pem[..split], &pem[split..]]);
            assert_eq!(out, "h\n***\nt", "split at {split}");
        }
    }
}
