//! `providers` — the U4 provider tree: closed registry + resolution map,
//! per-adapter [`LlmProvider`](oxidean_api_core::oxpilot::LlmProvider)
//! impls over raw reqwest (no provider SDK crates — BR5.3), shared
//! SSE/NDJSON decoders, key custody helpers, and the SSRF egress bound.
//!
//! Module visibility is custody-relevant: `key::decrypt_key` is
//! `pub(in crate::providers)` so only provider-construction code inside
//! this tree can open stored key material — `settings.rs` is same-crate,
//! so `pub(crate)` would NOT enforce the boundary (NFR1.4).

pub mod anthropic;
pub mod egress;
pub mod key;
pub mod ollama;
pub mod openai;
pub mod registry;
pub mod stream;

use oxidean_api_core::oxpilot::ProviderError;

/// Shared request-send: at most ONE retry and only when the failure is
/// connect-phase (DNS/TLS/refused/pool-acquire — `err.is_connect()`),
/// i.e. no request bytes reached the wire (BR4.1). Sent-but-unanswered
/// calls never retry — double-billing is worse than a failed turn.
pub(crate) async fn send_bounded(
    req: reqwest::RequestBuilder,
) -> Result<reqwest::Response, ProviderError> {
    let retry = req.try_clone();
    match req.send().await {
        Ok(resp) => Ok(resp),
        Err(e) if e.is_connect() => match retry {
            Some(second) => second.send().await.map_err(classify_transport),
            None => Err(classify_transport(e)),
        },
        Err(e) => Err(classify_transport(e)),
    }
}

/// Transport-failure → stable `ProviderError.code` — used for the
/// pre-stream send and for mid-stream transport items alike (the latter
/// surface as `ChatStreamItem::Error`).
pub(crate) fn classify_transport(e: reqwest::Error) -> ProviderError {
    let code = if e.is_timeout() {
        "provider_timeout"
    } else if e.is_connect() {
        "provider_unavailable"
    } else if e.is_redirect() {
        // redirect(Policy::none()) turns a 3xx into this — surfaced, never
        // followed (BR4.5/NFR4.9).
        "provider_redirect"
    } else {
        "provider_unavailable"
    };
    ProviderError {
        code: code.to_string(),
        // reqwest errors carry URL + kind, never header material.
        message: e.to_string(),
    }
}

/// Non-2xx response → classified `ProviderError`. The body is read once
/// (bounded by the server, still truncated for the message) because
/// context-length classification is a body-content check.
pub(crate) async fn classify_status(resp: reqwest::Response) -> ProviderError {
    let status = resp.status().as_u16();
    let body = resp.text().await.unwrap_or_default();
    classify_http(status, &body)
}

/// The shared status-code table (BR4.x): 401 `provider_unauthenticated`,
/// 403 `provider_forbidden`, 429 `provider_rate_limited`, context-length
/// failures `context_length`, 5xx `provider_unavailable`, 3xx
/// `provider_redirect` (never followed), anything else `provider_error`.
pub(crate) fn classify_http(status: u16, body: &str) -> ProviderError {
    let code = match status {
        401 => "provider_unauthenticated",
        403 => "provider_forbidden",
        429 => "provider_rate_limited",
        400..=499 if is_context_length_error(body) => "context_length",
        500..=599 => "provider_unavailable",
        300..=399 => "provider_redirect",
        _ => "provider_error",
    };
    ProviderError {
        code: code.to_string(),
        message: truncate_message(body),
    }
}

/// Context-window failures come back as ordinary 4xx bodies — the
/// classification is a content sniff across each provider's phrasing.
fn is_context_length_error(body: &str) -> bool {
    let b = body.to_ascii_lowercase();
    const MARKERS: &[&str] = &[
        "context_length_exceeded",
        "maximum context length",
        "context window",
        "prompt is too long",
        "request_too_large",
        "reduce the length",
    ];
    MARKERS.iter().any(|m| b.contains(m))
}

/// Bound the provider's error text — upstream bodies are untrusted and
/// unbounded; a terminal frame needs a short message.
fn truncate_message(body: &str) -> String {
    const MAX: usize = 300;
    let trimmed = body.trim();
    if trimmed.chars().count() <= MAX {
        trimmed.to_string()
    } else {
        trimmed.chars().take(MAX).collect::<String>() + "…"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_http_status_table() {
        assert_eq!(classify_http(401, "no").code, "provider_unauthenticated");
        assert_eq!(classify_http(403, "no").code, "provider_forbidden");
        assert_eq!(
            classify_http(429, "slow down").code,
            "provider_rate_limited"
        );
        assert_eq!(classify_http(503, "down").code, "provider_unavailable");
        assert_eq!(classify_http(302, "moved").code, "provider_redirect");
        assert_eq!(classify_http(418, "teapot").code, "provider_error");
    }

    #[test]
    fn classify_http_context_length_markers() {
        for body in [
            "context_length_exceeded",
            "This model's maximum context length is 8192",
            "the request_too_large field",
            "Prompt is too long for this model",
            "reduce the length of the messages",
        ] {
            assert_eq!(
                classify_http(400, body).code,
                "context_length",
                "body: {body}"
            );
        }
        assert_eq!(classify_http(400, "bad request").code, "provider_error");
    }

    #[test]
    fn truncate_message_bounds_body() {
        let long = "x".repeat(1000);
        let t = truncate_message(&long);
        assert!(t.chars().count() <= 301);
        assert!(t.ends_with('…'));
        assert_eq!(truncate_message("  hi  "), "hi");
    }
}
