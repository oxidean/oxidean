//! `/api/rpc` transport — matches the generated `@oxidean/api-client` shape:
//! `POST {instance}/api/rpc` with `Oxidean-RPC-Version: 1`, a
//! `{procedure, input}` JSON body, and `Authorization: Bearer <pat>` auth.

use oxidean_core::{AppError, RpcRequest, RpcResponse, RPC_PROTOCOL_VERSION};
use serde_json::Value;

/// Version gate header (same constant the generated TS client emits).
pub const VERSION_HEADER: &str = "Oxidean-RPC-Version";

/// Failures that never produced a usable response envelope.
#[derive(Debug)]
pub enum CallError {
    /// Network-level failure, or a body that isn't the RPC envelope at all
    /// (proxy pages, wrong URL, older server).
    Transport(String),
    /// HTTP error status whose body didn't parse as an RPC envelope.
    Http { status: u16, body: String },
}

impl CallError {
    /// 401/403 without an envelope → auth exit code; anything else → api error.
    pub fn exit_code(&self) -> u8 {
        match self {
            Self::Http {
                status: 401 | 403, ..
            } => 3,
            _ => 1,
        }
    }
}

impl std::fmt::Display for CallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Transport(m) => f.write_str(m),
            Self::Http { status, body } => {
                write!(f, "HTTP {status}")?;
                if !body.is_empty() {
                    write!(f, ": {}", crate::output::truncate(body, 200))?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for CallError {}

/// Exit code for a `{ok:false}` envelope: auth failures → 3, else 1.
pub fn error_exit_code(error: &AppError) -> u8 {
    if error.code == "auth.unauthenticated" {
        3
    } else {
        1
    }
}

/// `{instance}/api/rpc` — trailing slashes stripped.
pub fn rpc_url(instance: &str) -> String {
    format!("{}/api/rpc", instance.trim_end_matches('/'))
}

/// Wire body `{procedure, input}` — field names mirror
/// `oxidean_core::RpcRequest` and the generated TS client exactly.
pub fn build_body(procedure: &str, input: Value) -> RpcRequest {
    RpcRequest {
        procedure: procedure.to_string(),
        input,
    }
}

pub fn is_ok(resp: &RpcResponse) -> bool {
    matches!(resp, RpcResponse::Ok { .. })
}

pub fn error(resp: &RpcResponse) -> Option<&AppError> {
    match resp {
        RpcResponse::Err { error, .. } => Some(error),
        RpcResponse::Ok { .. } => None,
    }
}

pub struct Client {
    http: reqwest::Client,
    url: String,
    token: Option<String>,
}

impl Client {
    /// `instance` is the normalized base URL (e.g. `https://forge.example.com`).
    pub fn new(instance: &str, token: Option<String>) -> Self {
        Self {
            http: reqwest::Client::new(),
            url: rpc_url(instance),
            token,
        }
    }

    /// POST one RPC envelope. Returns the server's `RpcResponse` — including
    /// `{ok:false}` envelopes — so `--json` can echo it verbatim. Only
    /// transport/HTTP-level failures come back as `Err(CallError)`.
    pub async fn call(&self, procedure: &str, input: Value) -> Result<RpcResponse, CallError> {
        let body = build_body(procedure, input);
        let mut req = self
            .http
            .post(&self.url)
            .header(VERSION_HEADER, RPC_PROTOCOL_VERSION.to_string())
            .header("user-agent", concat!("ox/", env!("CARGO_PKG_VERSION")))
            .json(&body);
        if let Some(token) = &self.token {
            req = req.bearer_auth(token);
        }
        let res = req
            .send()
            .await
            .map_err(|e| CallError::Transport(format!("request to {} failed: {e}", self.url)))?;
        let status = res.status().as_u16();
        let text = res
            .text()
            .await
            .map_err(|e| CallError::Transport(format!("reading response failed: {e}")))?;
        match serde_json::from_str::<RpcResponse>(&text) {
            Ok(resp) => Ok(resp),
            Err(_) => Err(CallError::Http {
                status,
                body: crate::output::truncate(&text, 200),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn body_matches_wire_envelope() {
        let body = build_body("issue.get", json!({"owner": "o", "name": "n", "number": 4}));
        let v = serde_json::to_value(&body).unwrap();
        assert_eq!(
            v,
            json!({"procedure": "issue.get", "input": {"owner": "o", "name": "n", "number": 4}})
        );
    }

    #[test]
    fn rpc_url_joins_once() {
        assert_eq!(
            rpc_url("https://f.example.com"),
            "https://f.example.com/api/rpc"
        );
        assert_eq!(
            rpc_url("https://f.example.com/"),
            "https://f.example.com/api/rpc"
        );
        assert_eq!(
            rpc_url("http://127.0.0.1:8080///"),
            "http://127.0.0.1:8080/api/rpc"
        );
    }

    #[test]
    fn exit_code_classification() {
        assert_eq!(
            CallError::Http {
                status: 401,
                body: String::new()
            }
            .exit_code(),
            3
        );
        assert_eq!(CallError::Transport("boom".into()).exit_code(), 1);
        assert_eq!(
            error_exit_code(&AppError::new("auth.unauthenticated", "nope")),
            3
        );
        assert_eq!(error_exit_code(&AppError::new("repo.not_found", "nope")), 1);
    }
}
