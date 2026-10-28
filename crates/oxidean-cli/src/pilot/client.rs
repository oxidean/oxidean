//! C3 wire client — four routes, flat `{"error":"<code>"}` envelope, and
//! the typed `C3Error` taxonomy (reliability-design error matrix).

use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::custody::SecretString;

/// Pair/report/heartbeat request budget; claim's is `wait + 15s`.
pub const REQ_TIMEOUT: Duration = Duration::from_secs(30);
/// Extra budget above the claimed wait window (functional-spec).
pub const CLAIM_TIMEOUT_SLACK: Duration = Duration::from_secs(15);
/// Client-side claim wait clamp (BR2.2 — server clamps to 60 as well).
pub const CLAIM_WAIT_MIN: u64 = 1;
pub const CLAIM_WAIT_MAX: u64 = 60;

pub const PAIR_PATH: &str = "/api/oxpilot/connector/pair";
pub const CLAIM_PATH: &str = "/api/oxpilot/connector/claim";
pub const REPORT_PATH: &str = "/api/oxpilot/connector/report";
pub const HEARTBEAT_PATH: &str = "/api/oxpilot/connector/heartbeat";

/// Every code the C3 surface can answer, plus `Transport` (network/
/// unparseable/unexpected-status) and `Unknown` (a code outside the
/// vocabulary — routes to internal-class backoff).
#[derive(Debug, Clone, PartialEq)]
pub enum C3Error {
    Unauthorized,
    Forbidden,
    Revoked,
    Disabled,
    NotFound,
    BadInput,
    Internal,
    RateLimited { retry_after: u64 },
    PairingExpired,
    PairingUsed,
    Transport(String),
    Unknown(String),
}

impl std::fmt::Display for C3Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unauthorized => write!(f, "unauthorized"),
            Self::Forbidden => write!(f, "forbidden"),
            Self::Revoked => write!(f, "revoked"),
            Self::Disabled => write!(f, "disabled"),
            Self::NotFound => write!(f, "not_found"),
            Self::BadInput => write!(f, "bad_input"),
            Self::Internal => write!(f, "internal"),
            Self::RateLimited { retry_after } => {
                write!(f, "rate_limited (retry after {retry_after}s)")
            }
            Self::PairingExpired => write!(f, "pairing_expired"),
            Self::PairingUsed => write!(f, "pairing_used"),
            Self::Transport(m) => write!(f, "transport: {m}"),
            Self::Unknown(c) => write!(f, "unknown error code: {c}"),
        }
    }
}

/// Map a wire code to its variant; a code outside the 10-word vocabulary
/// is `Unknown` and classifies as internal/backoff.
pub fn code_to_error(code: &str, retry_after: u64) -> C3Error {
    match code {
        "unauthorized" => C3Error::Unauthorized,
        "forbidden" => C3Error::Forbidden,
        "revoked" => C3Error::Revoked,
        "disabled" => C3Error::Disabled,
        "not_found" => C3Error::NotFound,
        "bad_input" => C3Error::BadInput,
        "internal" => C3Error::Internal,
        "rate_limited" => C3Error::RateLimited { retry_after },
        "pairing_expired" => C3Error::PairingExpired,
        "pairing_used" => C3Error::PairingUsed,
        other => C3Error::Unknown(other.to_string()),
    }
}

/// Decode the flat error envelope. The C3 surface answers
/// `{"error":"<code>"}` — NOT the standard `{ok:false,error:{…}}` RPC
/// envelope. A non-JSON or shape-mismatched body is `Transport` (never a
/// guessed code); a bare 429 without a JSON body still maps by status.
fn decode_error(status: u16, retry_after_hdr: Option<u64>, body: &[u8]) -> C3Error {
    let retry = retry_after_hdr.unwrap_or(1);
    if let Ok(v) = serde_json::from_slice::<serde_json::Value>(body) {
        if let Some(code) = v.get("error").and_then(|e| e.as_str()) {
            return code_to_error(code, retry);
        }
    }
    if status == 429 {
        return C3Error::RateLimited { retry_after: retry };
    }
    C3Error::Transport(format!("unexpected HTTP {status} from connector route"))
}

/// `Retry-After` header → seconds. Only the bare-integer form is parsed;
/// HTTP-dates and junk are `None` → caller's default.
fn retry_after_secs(h: Option<&reqwest::header::HeaderValue>) -> Option<u64> {
    h.and_then(|v| v.to_str().ok())
        .and_then(|s| s.trim().parse::<u64>().ok())
}

#[derive(Deserialize)]
pub struct RedeemedPairing {
    pub connector_id: String,
    pub target_id: String,
    pub verifier: String,
}

impl std::fmt::Debug for RedeemedPairing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RedeemedPairing")
            .field("connector_id", &self.connector_id)
            .field("target_id", &self.target_id)
            .field("verifier", &"[redacted]")
            .finish()
    }
}

#[derive(Debug, Deserialize)]
pub struct ActionClaim {
    pub action_id: String,
    pub kind: String,
    /// JSON-encoded STRING (double-encoded — the outer claim envelope then
    /// the inner per-kind args), never a raw object.
    pub args: String,
    /// The dispatching grant's id — informational for the client.
    #[serde(default)]
    pub grant_id: Option<String>,
}

#[derive(Debug)]
pub enum ClaimAnswer {
    /// 200 with a batch (possibly actions-empty + cancel-only).
    Batch {
        actions: Vec<ActionClaim>,
        cancel: Vec<String>,
        retry_after: u64,
    },
    /// 204 — no work; the `Retry-After` header is the only hint.
    Timeout { retry_after: u64 },
}

#[derive(Debug, Deserialize)]
struct ClaimBody {
    #[serde(default)]
    actions: Vec<ActionClaim>,
    #[serde(default)]
    cancel: Vec<String>,
    retry_after: u64,
}

#[derive(Debug)]
pub struct HeartbeatOutcome {
    pub revoked: bool,
    pub cancel: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
pub enum ReportStatus {
    Done,
    Failed,
}

impl ReportStatus {
    pub fn wire(self) -> &'static str {
        match self {
            Self::Done => "done",
            Self::Failed => "failed",
        }
    }
}

#[derive(Serialize)]
struct PairBody<'a> {
    pairing_code: &'a str,
    device_label: &'a str,
    /// C3 pins `requested_caps` — NOT `capabilities`.
    requested_caps: &'a [String],
}

#[derive(Serialize)]
struct ClaimBodyOut<'a> {
    target_id: &'a str,
    wait_seconds: u64,
    capabilities: &'a [String],
}

#[derive(Serialize)]
struct ReportBody<'a> {
    action_id: &'a str,
    status: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    exit_code: Option<i64>,
    output: &'a str,
}

/// Thin reqwest wrapper over the four C3 routes. `instance` is already a
/// normalized base URL (config stores full URLs; `HttpClient` precedent —
/// just trim the trailing slash).
#[derive(Clone)]
pub struct ConnectorClient {
    http: reqwest::Client,
    base: String,
    verbose: bool,
}

impl ConnectorClient {
    pub fn new(instance: &str, verbose: bool) -> Self {
        Self {
            http: reqwest::Client::new(),
            base: instance.trim_end_matches('/').to_string(),
            verbose,
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base, path)
    }

    /// `--verbose` wire log on stderr. Paths + status only; verifier and
    /// pairing_code are NEVER printed (the pair body is summarized, the
    /// bearer header is annotated "sent").
    fn log(&self, line: &str) {
        if self.verbose {
            eprintln!("ox pilot: {line}");
        }
    }

    /// POST pair — unauthenticated; `pairing_code` IS the credential.
    pub async fn pair(
        &self,
        pairing_code: &str,
        device_label: &str,
        capabilities: &[String],
    ) -> Result<RedeemedPairing, C3Error> {
        let body = PairBody {
            pairing_code,
            device_label,
            requested_caps: capabilities,
        };
        self.log(&format!(
            "> POST {PAIR_PATH} device_label={device_label:?} requested_caps={capabilities:?} pairing_code=[redacted]"
        ));
        let res = self
            .http
            .post(self.url(PAIR_PATH))
            .header("user-agent", concat!("ox/", env!("CARGO_PKG_VERSION")))
            .timeout(REQ_TIMEOUT)
            .json(&body)
            .send()
            .await
            .map_err(|e| C3Error::Transport(format!("pair request failed: {e}")))?;
        let status = res.status().as_u16();
        let retry = retry_after_secs(res.headers().get(reqwest::header::RETRY_AFTER));
        let bytes = res
            .bytes()
            .await
            .map_err(|e| C3Error::Transport(e.to_string()))?;
        if status != 200 {
            let err = decode_error(status, retry, &bytes);
            self.log(&format!("< {status} {err}"));
            return Err(err);
        }
        self.log("< 200 paired");
        serde_json::from_slice::<RedeemedPairing>(&bytes)
            .map_err(|e| C3Error::Transport(format!("bad pair response: {e}")))
    }

    /// POST claim — long-poll. `wait_seconds` clamps `[1,60]`; the request
    /// timeout is `wait + CLAIM_TIMEOUT_SLACK` (BR2.2).
    pub async fn claim(
        &self,
        verifier: &SecretString,
        target_id: &str,
        wait_seconds: u64,
        capabilities: &[String],
    ) -> Result<ClaimAnswer, C3Error> {
        let wait = wait_seconds.clamp(CLAIM_WAIT_MIN, CLAIM_WAIT_MAX);
        let body = ClaimBodyOut {
            target_id,
            wait_seconds: wait,
            capabilities,
        };
        self.log(&format!(
            "> POST {CLAIM_PATH} target_id={target_id:?} wait_seconds={wait} bearer=sent"
        ));
        let res = self
            .http
            .post(self.url(CLAIM_PATH))
            .header("user-agent", concat!("ox/", env!("CARGO_PKG_VERSION")))
            .bearer_auth(verifier.expose())
            .timeout(Duration::from_secs(wait) + CLAIM_TIMEOUT_SLACK)
            .json(&body)
            .send()
            .await
            .map_err(|e| C3Error::Transport(format!("claim request failed: {e}")))?;
        let status = res.status().as_u16();
        let retry = retry_after_secs(res.headers().get(reqwest::header::RETRY_AFTER));
        let bytes = res
            .bytes()
            .await
            .map_err(|e| C3Error::Transport(e.to_string()))?;
        match status {
            200 => {
                let b: ClaimBody = serde_json::from_slice(&bytes)
                    .map_err(|e| C3Error::Transport(format!("bad claim body: {e}")))?;
                self.log(&format!(
                    "< 200 actions={} cancel={} retry_after={}",
                    b.actions.len(),
                    b.cancel.len(),
                    b.retry_after
                ));
                Ok(ClaimAnswer::Batch {
                    actions: b.actions,
                    cancel: b.cancel,
                    retry_after: b.retry_after,
                })
            }
            204 => {
                let retry_after = retry.unwrap_or(1);
                self.log(&format!("< 204 retry_after={retry_after}"));
                Ok(ClaimAnswer::Timeout { retry_after })
            }
            _ => {
                let err = decode_error(status, retry, &bytes);
                self.log(&format!("< {status} {err}"));
                Err(err)
            }
        }
    }

    /// POST report — idempotent close-out of one action row.
    pub async fn report(
        &self,
        verifier: &SecretString,
        action_id: &str,
        status: ReportStatus,
        exit_code: Option<i64>,
        output: &str,
    ) -> Result<(), C3Error> {
        let body = ReportBody {
            action_id,
            status: status.wire(),
            exit_code,
            output,
        };
        self.log(&format!(
            "> POST {REPORT_PATH} action_id={action_id:?} status={} output={}B",
            status.wire(),
            output.len()
        ));
        let res = self
            .http
            .post(self.url(REPORT_PATH))
            .header("user-agent", concat!("ox/", env!("CARGO_PKG_VERSION")))
            .bearer_auth(verifier.expose())
            .timeout(REQ_TIMEOUT)
            .json(&body)
            .send()
            .await
            .map_err(|e| C3Error::Transport(format!("report request failed: {e}")))?;
        let status = res.status().as_u16();
        let retry = retry_after_secs(res.headers().get(reqwest::header::RETRY_AFTER));
        let bytes = res
            .bytes()
            .await
            .map_err(|e| C3Error::Transport(e.to_string()))?;
        if status != 200 {
            let err = decode_error(status, retry, &bytes);
            self.log(&format!("< {status} {err}"));
            return Err(err);
        }
        self.log("< 200 ack");
        Ok(())
    }

    /// POST heartbeat — liveness + cancel-set announce.
    pub async fn heartbeat(
        &self,
        verifier: &SecretString,
        target_id: &str,
    ) -> Result<HeartbeatOutcome, C3Error> {
        #[derive(Serialize)]
        struct Hb<'a> {
            target_id: &'a str,
        }
        self.log(&format!("> POST {HEARTBEAT_PATH} target_id={target_id:?}"));
        let res = self
            .http
            .post(self.url(HEARTBEAT_PATH))
            .header("user-agent", concat!("ox/", env!("CARGO_PKG_VERSION")))
            .bearer_auth(verifier.expose())
            .timeout(REQ_TIMEOUT)
            .json(&Hb { target_id })
            .send()
            .await
            .map_err(|e| C3Error::Transport(format!("heartbeat request failed: {e}")))?;
        let status = res.status().as_u16();
        let retry = retry_after_secs(res.headers().get(reqwest::header::RETRY_AFTER));
        let bytes = res
            .bytes()
            .await
            .map_err(|e| C3Error::Transport(e.to_string()))?;
        if status != 200 {
            let err = decode_error(status, retry, &bytes);
            self.log(&format!("< {status} {err}"));
            return Err(err);
        }
        let v: serde_json::Value = serde_json::from_slice(&bytes)
            .map_err(|e| C3Error::Transport(format!("bad heartbeat body: {e}")))?;
        let revoked = v.get("revoked").and_then(|b| b.as_bool()).unwrap_or(false);
        let cancel: Vec<String> = v
            .get("cancel")
            .and_then(|c| c.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        self.log(&format!("< 200 revoked={revoked} cancel={}", cancel.len()));
        Ok(HeartbeatOutcome { revoked, cancel })
    }
}
