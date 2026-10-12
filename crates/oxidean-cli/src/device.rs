//! `ox auth login --web` — RFC 8628 device authorization grant against the
//! instance's OAuth provider surface.
//!
//! `POST /oauth/device/code` → print the `user_code` + `verification_uri`,
//! open the browser when possible, then poll `POST /oauth/token` with
//! `grant_type=urn:ietf:params:oauth:grant-type:device_code` at the granted
//! interval (honoring `authorization_pending` / `slow_down` / `access_denied`
//! / `expired_token`). A successful exchange returns a classic PAT — stored by
//! the caller exactly like a pasted token.

use std::time::Duration;

use serde::Deserialize;

use crate::commands::CliError;

/// `POST /oauth/device/code` response (RFC 8628 §3.2).
#[derive(Debug, Deserialize)]
pub struct DeviceCodeResponse {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    #[serde(default)]
    pub verification_uri_complete: Option<String>,
    pub expires_in: i64,
    #[serde(default = "default_interval")]
    pub interval: i64,
}

fn default_interval() -> i64 {
    5
}

/// `POST /oauth/token` — either the token payload or an RFC 8628 error body.
#[derive(Debug, Deserialize)]
struct DeviceTokenResponse {
    #[serde(default)]
    access_token: Option<String>,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    error_description: Option<String>,
}

fn client() -> Result<reqwest::Client, CliError> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| CliError::Transport(format!("http client: {e}")))
}

/// `application/x-www-form-urlencoded` bodies — the OAuth endpoints take
/// forms (JSON also works on /oauth/token, but form is the interop shape).
fn form_encode(pairs: &[(&str, &str)]) -> String {
    pairs
        .iter()
        .map(|(k, v)| format!("{}={}", url_encode(k), url_encode(v)))
        .collect::<Vec<_>>()
        .join("&")
}

fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Request a device grant from `instance`. `scopes` are space-delimited
/// classic PAT scope names (the grant mints a PAT with them on completion).
pub async fn request(instance: &str, scopes: &str) -> Result<DeviceCodeResponse, CliError> {
    let url = format!("{}/oauth/device/code", instance.trim_end_matches('/'));
    let resp = client()?
        .post(&url)
        .header(
            reqwest::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded",
        )
        .body(form_encode(&[("scope", scopes)]))
        .send()
        .await
        .map_err(|e| CliError::Transport(format!("device code request failed: {e}")))?;
    let status = resp.status();
    let body = resp
        .text()
        .await
        .map_err(|e| CliError::Transport(format!("device code response: {e}")))?;
    if !status.is_success() {
        // Older instances (and deployments whose web proxy predates the
        // device endpoint) answer HTML 404s — translate that into an
        // actionable hint instead of dumping markup.
        if status == reqwest::StatusCode::NOT_FOUND {
            return Err(CliError::Usage(format!(
                "browser sign-in is not available on {instance} — the instance\n\
                 may be an older release. Use `ox auth login --instance {instance} --token <pat>`."
            )));
        }
        return Err(CliError::Transport(format!(
            "device code request failed ({status}): {}",
            body.chars().take(300).collect::<String>()
        )));
    }
    serde_json::from_str(&body)
        .map_err(|e| CliError::Transport(format!("malformed device code response: {e}")))
}

/// One poll step's outcome.
pub enum Poll {
    /// `authorization_pending` — keep waiting.
    Pending,
    /// `slow_down` — server bumped the interval to the returned seconds.
    SlowDown(i64),
    /// `access_denied` / `expired_token` / other — terminal.
    Failed(String),
    /// Token in hand.
    Token(String),
}

/// Poll once against `/oauth/token`.
pub async fn poll(instance: &str, device_code: &str, interval: i64) -> Result<Poll, CliError> {
    let url = format!("{}/oauth/token", instance.trim_end_matches('/'));
    let resp = client()?
        .post(&url)
        .header(
            reqwest::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded",
        )
        .body(form_encode(&[
            (
                "grant_type",
                "urn:ietf:params:oauth:grant-type:device_code",
            ),
            ("device_code", device_code),
        ]))
        .send()
        .await
        .map_err(|e| CliError::Transport(format!("device poll failed: {e}")))?;
    let status = resp.status();
    let body = resp
        .text()
        .await
        .map_err(|e| CliError::Transport(format!("device poll response: {e}")))?;
    let parsed: DeviceTokenResponse = serde_json::from_str(&body)
        .map_err(|e| CliError::Transport(format!("malformed device token response: {e}")))?;
    if let Some(token) = parsed.access_token {
        return Ok(Poll::Token(token));
    }
    Ok(match parsed.error.as_deref() {
        Some("authorization_pending") => Poll::Pending,
        Some("slow_down") => Poll::SlowDown(interval + 5),
        Some("access_denied") => Poll::Failed("sign-in was denied".into()),
        Some("expired_token") => Poll::Failed("device code expired — try again".into()),
        Some(other) => Poll::Failed(format!("device poll failed: {other}")),
        None if status.is_success() => {
            Poll::Failed("device poll returned no token".into())
        }
        None => Poll::Failed(format!("device poll failed ({status})")),
    })
}

/// Best-effort browser open — silent no-op headless (the printed URL carries
/// the flow either way).
pub fn open_browser(url: &str) {
    let spawn = |cmd: &str, arg: &str| {
        std::process::Command::new(cmd)
            .arg(arg)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .is_ok()
    };
    #[cfg(target_os = "macos")]
    let ok = spawn("open", url);
    #[cfg(target_os = "windows")]
    let ok = {
        let _ = spawn;
        std::process::Command::new("cmd")
            .args(["/c", "start", "", url])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .is_ok()
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let ok = spawn("xdg-open", url) || spawn("sensible-browser", url);
    #[cfg(not(any(unix, windows)))]
    let ok = false;
    let _ = ok;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_encode_percent_encodes_utf8_bytes() {
        assert_eq!(url_encode("abc-XYZ_09.~"), "abc-XYZ_09.~");
        assert_eq!(url_encode("a b"), "a%20b");
        assert_eq!(url_encode("a=b&c"), "a%3Db%26c");
        // Multi-byte UTF-8 encodes per byte (é = 0xC3 0xA9).
        assert_eq!(url_encode("é"), "%C3%A9");
    }

    #[test]
    fn form_encode_joins_pairs() {
        assert_eq!(
            form_encode(&[("grant_type", "urn:ietf:x"), ("device_code", "a b")]),
            "grant_type=urn%3Aietf%3Ax&device_code=a%20b"
        );
    }
}
