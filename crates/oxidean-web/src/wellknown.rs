//! `/.well-known/{webmcp,mcp}` discovery documents — Rust port of
//! `vite-plugins/webmcp-well-known.ts` (AGT-02). Traefik routes `/.well-known`
//! to the web service, so these land on the site origin.

use axum::http::header;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

pub const WEBMCP_PATH: &str = "/.well-known/webmcp";
pub const MCP_PATH: &str = "/.well-known/mcp";
const MCP_ENDPOINT_PATH: &str = "/api/mcp";
/// Keep in sync with `MCP_PROTOCOL_VERSIONS` in `apps/web/src/lib/webmcp.ts`
/// and the API's `mcp.rs`.
const MCP_PROTOCOL_VERSIONS: &[&str] = &["2024-11-05", "2025-03-26", "2025-06-18"];

fn mcp_document() -> serde_json::Value {
    json!({
        "mcp_version": "1.0",
        "server_name": "Oxidean",
        "endpoints": { "streamable_http": MCP_ENDPOINT_PATH },
        "transport": "streamable-http (JSON responses; no standalone SSE stream)",
        "protocol_versions": MCP_PROTOCOL_VERSIONS,
        "capabilities": { "tools": true, "resources": true, "prompts": false },
        "methods": [
            "initialize", "ping", "tools/list", "tools/call",
            "resources/list", "resources/templates/list", "resources/read",
            "prompts/list",
        ],
        "authentication": {
            "required": false,
            "anonymous": "public-readable data only; private or write tools answer isError",
            "methods": [
                "session-cookie (oxidean_session; same-origin browser agents)",
                "bearer-pat (oxidean_pat_… classic / oxidean_fg_… fine-grained)",
            ],
        },
    })
}

fn json_doc(v: serde_json::Value) -> Response {
    let mut resp = Json(v).into_response();
    resp.headers_mut()
        .insert(header::CACHE_CONTROL, "public, max-age=300".parse().unwrap());
    resp
}

pub async fn webmcp() -> Response {
    json_doc(json!({
        "schema_version": 1,
        "site": {
            "name": "Oxidean",
            "description": "Self-hostable code forge — repositories, issues, pull requests, Actions runs, packages, and search.",
        },
        "webmcp": {
            "api": "document.modelContext (navigator.modelContext fallback)",
            "registration": "each page registers the instance tools/list catalog as WebMCP tools; execute() proxies tools/call to the MCP endpoint",
            "declarative": "search_site form tool via toolname/tooldescription attributes",
        },
        "mcp": mcp_document(),
        "tools": {
            "list": format!("{MCP_ENDPOINT_PATH} tools/list"),
            "call": format!("{MCP_ENDPOINT_PATH} tools/call"),
        },
        "links": {
            "self": WEBMCP_PATH,
            "mcp_endpoint": MCP_ENDPOINT_PATH,
            "mcp_metadata": MCP_PATH,
        },
    }))
}

pub async fn mcp() -> Response {
    json_doc(mcp_document())
}
