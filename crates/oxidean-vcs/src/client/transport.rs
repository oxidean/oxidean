//! The injected transport seam the client surface runs over (contract C8).
//!
//! `oxidean-vcs` never opens a socket: the host (`oxidean-cli`) supplies a
//! `VcsTransport` backed by its existing `Client`/`HttpClient`. Server `vcs.*`
//! failures arrive inside `RpcResponse` envelopes and pass through verbatim —
//! `TransportError` covers only failures that never produced an envelope, and
//! local `CliFault` codes never masquerade as `vcs.*` wire codes.

use oxidean_core::{AppError, RpcResponse};
use serde_json::Value;

/// One RPC/object-route call that produced no usable server answer — or an
/// object-route error envelope (`Envelope`), which keeps its `vcs.*` code so
/// `error_exit_code` still classifies `vcs.unauthenticated`/`vcs.forbidden`.
#[derive(Debug)]
pub enum TransportError {
    /// Network/connection failure — `reqwest`-style display text.
    Transport(String),
    /// HTTP status without a parseable `{error:{code,message}}` envelope.
    Http { status: u16, body: String },
    /// An object-route `{ok:false,error}` envelope — verbatim `AppError`.
    Envelope(AppError),
    /// The response was not a well-formed `RpcResponse` envelope.
    Malformed(String),
}

impl std::fmt::Display for TransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Transport(msg) => f.write_str(msg),
            Self::Http { status, body } => write!(f, "HTTP {status}: {body}"),
            Self::Envelope(e) => write!(f, "{}: {}", e.code, e.message),
            Self::Malformed(msg) => f.write_str(msg),
        }
    }
}

impl std::error::Error for TransportError {}

/// The seam every `ox vcs` operation (and the working copy) runs over.
#[async_trait::async_trait]
pub trait VcsTransport {
    /// `POST /api/rpc` — returns the server envelope including `{ok:false}`
    /// responses; `Err` only when no envelope could be produced.
    async fn call(&self, procedure: &str, input: Value) -> Result<RpcResponse, TransportError>;

    /// `GET /api/vcs/{owner}/{name}/objects/{hash}` — raw blob bytes.
    async fn object_get(
        &self,
        owner: &str,
        name: &str,
        hash: &str,
    ) -> Result<Vec<u8>, TransportError>;

    /// `POST /api/vcs/{owner}/{name}/objects` — returns the server-computed
    /// `sha256:` object id.
    async fn object_put(
        &self,
        owner: &str,
        name: &str,
        bytes: &[u8],
    ) -> Result<String, TransportError>;
}

/// A local (client-side) failure — usage, transport, filesystem, cache, or a
/// server envelope that arrived `{ok:false}`. Local kinds surface as
/// `ox: <kind>: <message>` and never carry a `vcs.*` code — those belong to
/// server envelopes, which `Server` carries verbatim for the caller's
/// error-classification path.
#[derive(Debug)]
pub enum CliFault {
    /// Malformed invocation — exit code 2 in the CLI.
    Usage(String),
    /// `VcsTransport` failure — network or unparseable object-route reply.
    Transport(String),
    /// Filesystem failure in the working copy (non-empty target, IO).
    Fs(String),
    /// Cache integrity failure (hash mismatch on a cached/fetched object).
    Cache(String),
    /// A `{ok:false}` server envelope mid-operation (e.g. `vcs.disabled`
    /// during checkout) — the command surfaces it as its own outcome.
    Server(Box<RpcResponse>),
}

impl CliFault {
    /// The `ox:`-namespaced code prefix for display (`Server` carries the
    /// envelope's own code — this accessor is for local kinds).
    pub fn code(&self) -> &str {
        match self {
            Self::Usage(_) => "ox.usage",
            Self::Transport(_) => "ox.transport",
            Self::Fs(_) => "ox.fs",
            Self::Cache(_) => "ox.cache",
            Self::Server(e) => match e.as_ref() {
                RpcResponse::Err { error, .. } => error.code.as_str(),
                RpcResponse::Ok { .. } => "ox.server",
            },
        }
    }
}

impl std::fmt::Display for CliFault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Usage(m) | Self::Transport(m) | Self::Fs(m) | Self::Cache(m) => f.write_str(m),
            Self::Server(e) => match e.as_ref() {
                RpcResponse::Err { error, .. } => {
                    write!(f, "{}: {}", error.code, error.message)
                }
                RpcResponse::Ok { .. } => f.write_str("unexpected ok envelope"),
            },
        }
    }
}

impl std::error::Error for CliFault {}

impl From<TransportError> for CliFault {
    fn from(e: TransportError) -> Self {
        match e {
            // Object-route error envelopes ride the verbatim server channel so
            // `vcs.unauthenticated`/`vcs.forbidden` still classify to exit 3.
            TransportError::Envelope(err) => Self::Server(Box::new(RpcResponse::err(err))),
            TransportError::Malformed(m) => Self::Transport(m),
            other => Self::Transport(other.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxidean_core::AppError;

    #[test]
    fn transport_error_display_variants() {
        assert_eq!(TransportError::Transport("boom".into()).to_string(), "boom");
        assert_eq!(
            TransportError::Http {
                status: 502,
                body: "bad gateway".into()
            }
            .to_string(),
            "HTTP 502: bad gateway"
        );
        let env = TransportError::Envelope(AppError::new("vcs.disabled", "vcs is off"));
        assert_eq!(env.to_string(), "vcs.disabled: vcs is off");
        assert_eq!(
            TransportError::Malformed("no envelope".into()).to_string(),
            "no envelope"
        );
    }

    #[test]
    fn cli_fault_local_codes_are_ox_namespaced() {
        assert_eq!(CliFault::Usage("x".into()).code(), "ox.usage");
        assert_eq!(CliFault::Transport("x".into()).code(), "ox.transport");
        assert_eq!(CliFault::Fs("x".into()).code(), "ox.fs");
        assert_eq!(CliFault::Cache("x".into()).code(), "ox.cache");
    }

    #[test]
    fn envelope_error_becomes_verbatim_server_fault() {
        let fault: CliFault =
            TransportError::Envelope(AppError::new("vcs.unauthenticated", "bad credentials"))
                .into();
        // The code survives verbatim — `error_exit_code` classifies it to 3.
        assert_eq!(fault.code(), "vcs.unauthenticated");
        assert_eq!(fault.to_string(), "vcs.unauthenticated: bad credentials");
        match fault {
            CliFault::Server(e) => match e.as_ref() {
                RpcResponse::Err { error, .. } => {
                    assert_eq!(error.code, "vcs.unauthenticated")
                }
                other => panic!("expected err envelope, got {other:?}"),
            },
            other => panic!("expected server fault, got {other:?}"),
        }
    }

    #[test]
    fn non_envelope_transport_errors_become_transport_faults() {
        for e in [
            TransportError::Transport("conn refused".into()),
            TransportError::Http {
                status: 500,
                body: "oops".into(),
            },
            TransportError::Malformed("garbage".into()),
        ] {
            let fault: CliFault = e.into();
            assert_eq!(fault.code(), "ox.transport");
        }
    }

    #[test]
    fn server_fault_display_echoes_envelope() {
        let ok = CliFault::Server(Box::new(RpcResponse::ok(serde_json::json!({}))));
        assert_eq!(ok.code(), "ox.server");
        assert_eq!(ok.to_string(), "unexpected ok envelope");
    }
}

/// What a completed `ox vcs` command hands back to the CLI layer — the raw
/// server envelope (`{ok:false}` envelopes included, so `error_exit_code` can
/// classify `vcs.*` codes), the human-readable rendering for `Out::Rpc`, and
/// an optional raw-bytes channel (`cat` is binary-safe).
#[derive(Debug)]
pub struct CliOutcome {
    pub envelope: RpcResponse,
    pub pretty: String,
    /// Raw stdout payload — written verbatim instead of `pretty` when set.
    pub bytes: Option<Vec<u8>>,
}
