//! Workflow status badge — GitHub-compatible `badge.svg`.
//!
//! GitHub serves `/{owner}/{repo}/actions/workflows/{workflow_file}/badge.svg`
//! (`?branch=`/`?event=` filters) for README badges. The web tier proxies the
//! repo-path shape here; the same handler is also mounted under
//! `/api/repos/{owner}/{repo}/actions/workflows/...` for direct API use.
//!
//! Visibility follows the download endpoints: Read+ on the repo (public repos
//! allow anonymous badges, private repos 404 without credentials).

use axum::extract::{Path as AxumPath, Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};

use crate::app::AppState;
use crate::routes::action_downloads::{authorize_read, lookup_repo};

#[derive(serde::Deserialize)]
pub struct BadgeQuery {
    /// Limit to runs whose `head_ref` is this branch (`refs/heads/` prefix
    /// optional — GitHub accepts the bare branch name).
    branch: Option<String>,
    /// Limit to runs of this event name (`push`, `pull_request`, …).
    event: Option<String>,
}

/// Status → right-hand label + shields-style fill colour.
fn status_style(status: &str) -> (&'static str, &'static str) {
    match status {
        "success" => ("passing", "#2da44e"),
        "failure" | "startup_failure" => ("failing", "#cf222e"),
        "cancelled" => ("cancelled", "#6e7781"),
        "in_progress" | "queued" | "waiting" | "requested" => ("running", "#bf8700"),
        "skipped" => ("skipped", "#6e7781"),
        _ => ("no status", "#9f9f9f"),
    }
}

/// Minimal shields-style badge: `<label> | <status>` on a two-tone plate.
/// Widths approximate an 11px sans — the badge is decorative, not measured.
fn badge_svg(label: &str, status_text: &str, colour: &str) -> String {
    let esc = |s: &str| {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
    };
    let label = esc(label);
    let status_text = esc(status_text);
    let lw = (label.chars().count() as u32 * 7 + 10).max(20);
    let sw = (status_text.chars().count() as u32 * 7 + 10).max(24);
    let total = lw + sw;
    let lx = lw * 10 / 2;
    let sx = lw * 10 + sw * 10 / 2;
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{total}\" height=\"20\" role=\"img\" aria-label=\"{label}: {status_text}\">\
<title>{label}: {status_text}</title>\
<linearGradient id=\"s\" x2=\"0\" y2=\"100%\"><stop offset=\"0\" stop-color=\"#bbb\" stop-opacity=\".1\"/><stop offset=\"1\" stop-opacity=\".1\"/></linearGradient>\
<clipPath id=\"r\"><rect width=\"{total}\" height=\"20\" rx=\"3\" fill=\"#fff\"/></clipPath>\
<g clip-path=\"url(#r)\"><rect width=\"{lw}\" height=\"20\" fill=\"#555\"/><rect x=\"{lw}\" width=\"{sw}\" height=\"20\" fill=\"{colour}\"/><rect width=\"{total}\" height=\"20\" fill=\"url(#s)\"/></g>\
<g fill=\"#fff\" text-anchor=\"middle\" font-family=\"Verdana,Geneva,DejaVu Sans,sans-serif\" font-size=\"11\">\
<text x=\"{lx}\" y=\"15\" transform=\"scale(.1)\" textLength=\"{}\">{label}</text>\
<text x=\"{sx}\" y=\"15\" transform=\"scale(.1)\" textLength=\"{}\">{status_text}</text>\
</g></svg>",
        (lw - 10) * 10,
        (sw - 10) * 10,
    )
}

async fn badge_response(
    state: &AppState,
    owner: &str,
    repo: &str,
    workflow: &str,
    query: &BadgeQuery,
    headers: &HeaderMap,
) -> Response {
    let lookup = match lookup_repo(state, owner, repo).await {
        Ok(l) => l,
        Err(r) => return *r,
    };
    if let Err(r) = authorize_read(state, headers, &lookup, "repo.actions.getRun").await {
        return *r;
    }
    // `{workflow}` is a filename (`ci.yml`) or a `.github/workflows/`-prefixed
    // path; store lookup is against the full path.
    let name = workflow
        .strip_prefix(".github/workflows/")
        .unwrap_or(workflow);
    if name.is_empty() || name.contains('/') || name.contains("..") {
        return StatusCode::NOT_FOUND.into_response();
    }
    let workflow_path = format!(".github/workflows/{name}");
    let filter = oxidean_db::actions::ActionRunFilter {
        workflow_path: Some(workflow_path),
        event: query
            .event
            .as_deref()
            .map(str::trim)
            .filter(|e| !e.is_empty())
            .map(str::to_string),
        // `branch_ref_of` normalizes the bare name to refs/heads/*.
        branch: query
            .branch
            .as_deref()
            .map(str::trim)
            .filter(|b| !b.is_empty())
            .map(str::to_string),
        ..Default::default()
    };
    let runs = state
        .db
        .list_action_runs_for_repo(&lookup.repo.id, &filter, 1, 0)
        .await
        .unwrap_or_default();
    let (label, status_text, colour) = match runs.first() {
        Some(run) => {
            let (text, colour) = status_style(&run.status);
            (run.workflow_name.clone(), text, colour)
        }
        None => {
            // No runs yet — label falls back to the file stem like GitHub's
            // unnamed-workflow badge does.
            let stem = name
                .rsplit('.')
                .next()
                .filter(|_| name.contains('.'))
                .map(|_| &name[..name.rfind('.').unwrap_or(name.len())])
                .unwrap_or(name);
            (stem.to_string(), "no status", "#9f9f9f")
        }
    };
    let svg = badge_svg(&label, status_text, colour);
    let mut res = Response::new(axum::body::Body::from(svg));
    *res.status_mut() = StatusCode::OK;
    if let Ok(hv) = HeaderValue::from_str("image/svg+xml") {
        res.headers_mut().insert(header::CONTENT_TYPE, hv);
    }
    // Badges must reflect the latest run — GitHub ships no-store on badge.svg.
    if let Ok(hv) = HeaderValue::from_str("no-cache, no-store, must-revalidate") {
        res.headers_mut().insert(header::CACHE_CONTROL, hv);
    }
    res
}

/// `GET /{owner}/{repo}/actions/workflows/{*workflow}/badge.svg` — the
/// GitHub-shaped URL (web tier proxies it here).
pub async fn workflow_badge(
    State(state): State<AppState>,
    AxumPath((owner, repo, workflow)): AxumPath<(String, String, String)>,
    Query(query): Query<BadgeQuery>,
    headers: HeaderMap,
) -> Response {
    badge_response(&state, &owner, &repo, &workflow, &query, &headers).await
}
