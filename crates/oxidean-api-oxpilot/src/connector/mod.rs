//! U6 connector-gateway — the in-process server side for the Oxpilot
//! computer-use connector (C3 HTTP surface + C7 dispatch seam + the C1
//! owner-verb backing + C8 hook #2, one state machine).
//!
//! - [`gateway::OxpilotConnectorGateway`] is the concrete
//!   `ConnectorGateway` port impl — per-target decision locks + the
//!   claim `Notify` map + the connector-auth/mint `FailedAuthLimiter`,
//!   installed set-once beside `sessions` on `OxpilotRuntime` (so U4
//!   settings rebuilds can never drop it).
//! - [`routes`] holds the `/api/oxpilot/connector/*` handlers —
//!   verifier-Bearer auth, enablement fail-closed, no session middleware.
//! - [`hook::ConnectorDeleteHook`] is C8's second delete hook,
//!   registered after U3's turn-abort hook at shell assembly.
//!
//! Secret custody (BR7.3): raw verifier material exists only inside
//! [`OxpilotConnectorGateway::redeem_pairing`](gateway) and the one
//! `RedeemedPairing` response; pairing code material exists only inside
//! `mint_pairing` and its `PairingView`. Hashes persist; plaintext
//! never touches storage, audit, or logs.

pub mod gateway;
pub mod hook;
pub mod procs;
pub mod routes;

pub use gateway::OxpilotConnectorGateway;
pub use hook::ConnectorDeleteHook;
