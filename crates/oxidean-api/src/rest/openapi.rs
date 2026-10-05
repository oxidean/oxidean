//! `GET /api/v1/openapi.json` + vendored Swagger UI at `/api/v1/docs`.
//!
//! The document is generated from the same [`spec::route_defs`] table that
//! mounts every REST handler, so the spec cannot drift from the served
//! surface. Swagger UI assets are bundled at build time
//! (`utoipa-swagger-ui/vendored`) — no CDN dependency for self-hosters.

use axum::Router;
use utoipa_swagger_ui::{Config, SwaggerUi, Url};

use super::spec;
use crate::app::AppState;

/// Routes for the spec document and Swagger UI (mounted inside the `/api/v1`
/// nest alongside the data routes). The spec route is registered at the
/// nest-relative `/openapi.json`; the UI's fetch URL must be the absolute
/// `/api/v1/openapi.json`, so `Config` pins it explicitly.
pub fn router() -> Router<AppState> {
    SwaggerUi::new("/docs")
        .external_url_unchecked("/openapi.json", spec::openapi_doc())
        .config(Config::new([Url::with_primary(
            "Oxidean REST API",
            "/api/v1/openapi.json",
            true,
        )]))
        .into()
}
