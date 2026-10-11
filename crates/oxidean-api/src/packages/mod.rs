//! Multi-format package registry subsystem (Phase 20).

pub mod acl;
pub mod auth;
pub mod generic;
pub mod npm;
pub mod oci;
pub mod quota;
pub mod rpc;
pub mod store;

use axum::Router;

use crate::app::AppState;

/// Mount under `/v2`, `/npm`, and `/generic` from [`crate::app::router_with_state`].
pub fn registry_routers() -> (Router<AppState>, Router<AppState>, Router<AppState>) {
    (oci::router(), npm::router(), generic::router())
}
