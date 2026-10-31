//! Branded HTTP status pages shared by `oxidean-web` and `oxidean-api` —
//! one embedded template answers both serving tiers so a 404 (or 405/502)
//! looks the same no matter which service emits it (#207).
//!
//! `OXIDEAN_STATUS_PAGE_THEME` picks the variant per process: `default`
//! (unset) is the plain branded page, `meme` swaps the headline block for
//! the status's https://http.cat image. Variants share a single shell —
//! only the hero block differs, so copy/markup changes land once.
//!
//! HTML is produced only for clients that explicitly accept it
//! (`Accept: text/html`); RPC, git, and registry clients keep their terse
//! or structured bodies — see [`theme_errors`].

use std::sync::OnceLock;

use axum::body::Body;
use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;
use http::{header, HeaderMap, HeaderName, HeaderValue, StatusCode};

/// Env var both serving tiers read — `default` (unset) or `meme`.
pub const THEME_ENV: &str = "OXIDEAN_STATUS_PAGE_THEME";

/// Bytes of a displaced terse body worth lifting into the page's detail
/// line — anything larger is not a terse status line.
const DETAIL_CAP: usize = 4 * 1024;

/// Page variant — selects the hero block inside the shared shell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Theme {
    /// Plain branded page: status code, headline, blurb.
    #[default]
    Default,
    /// https://http.cat image for the status code.
    Meme,
}

/// Parse `OXIDEAN_STATUS_PAGE_THEME`; `None` → caller picks the fallback.
pub fn parse_theme(raw: &str) -> Option<Theme> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "default" => Some(Theme::Default),
        "meme" => Some(Theme::Meme),
        _ => None,
    }
}

/// The process theme: env resolved once (the var is a boot knob, not
/// per-request state). Unset/empty → `default`; unknown → warn + `default`.
pub fn theme() -> Theme {
    static THEME: OnceLock<Theme> = OnceLock::new();
    *THEME.get_or_init(|| {
        let Ok(raw) = std::env::var(THEME_ENV) else {
            return Theme::default();
        };
        let raw = raw.trim();
        parse_theme(raw).unwrap_or_else(|| {
            if !raw.is_empty() {
                tracing::warn!(
                    "{THEME_ENV}={raw:?} not recognized — using `default` (want `default`|`meme`)"
                );
            }
            Theme::default()
        })
    })
}

/// Does the requester want a rendered document? Browsers send `text/html`
/// in Accept; API/CLI clients (`*/*`, `application/json`) keep terse
/// machine bodies.
pub fn accepts_html(accept: Option<&str>) -> bool {
    accept.is_some_and(|v| v.to_ascii_lowercase().contains("text/html"))
}

/// Resolved color scheme for stamping: `Some(true)` dark, `Some(false)`
/// light, `None` when nothing resolved — leave the page to
/// `prefers-color-scheme`.
///
/// Follows the same signal chain `oxidean-web`'s inject uses for shells
/// (`oxidean-theme` cookie → `oxidean-color-scheme` cookie →
/// `Sec-CH-Prefers-Color-Scheme` header), except `system`/unset stays `None`
/// instead of coercing to light — a status page has no reason to fight the
/// OS. Because `/api/*` shares the browser origin, API-served error pages
/// see the same cookies and stamp the same scheme.
pub fn color_scheme(cookie: Option<&str>, sec_ch: Option<&str>) -> Option<bool> {
    if let Some(c) = cookie {
        for name in ["oxidean-theme", "oxidean-color-scheme"] {
            match cookie_value(c, name) {
                Some("dark") => return Some(true),
                Some("light") => return Some(false),
                _ => {}
            }
        }
    }
    match sec_ch?.trim().to_ascii_lowercase().as_str() {
        "dark" => Some(true),
        "light" => Some(false),
        _ => None,
    }
}

/// `name=value` lookup inside a Cookie header (no decoding — theme cookies
/// are ASCII flag values).
fn cookie_value<'a>(header: &'a str, name: &str) -> Option<&'a str> {
    for pair in header.split(';') {
        let pair = pair.trim();
        if let Some((k, v)) = pair.split_once('=') {
            if k == name {
                return Some(v.trim_matches('"'));
            }
        }
    }
    None
}

/// Render with the env-resolved theme, media-query color scheme.
pub fn page(status: StatusCode) -> String {
    render(theme(), status, None)
}

/// Render the full document for `status`. `dark`: `Some` stamps the color
/// scheme explicitly; `None` leaves the choice to `prefers-color-scheme`.
pub fn render(theme: Theme, status: StatusCode, dark: Option<bool>) -> String {
    render_detail(theme, status, dark, None)
}

/// Like [`render`], plus an optional `detail` shown under the blurb — the
/// terse body the response would otherwise carry (HTML-escaped on the way
/// in), e.g. `API unreachable` on a proxy 502.
pub fn render_detail(
    theme: Theme,
    status: StatusCode,
    dark: Option<bool>,
    detail: Option<&str>,
) -> String {
    let code = status.as_u16().to_string();
    let (title, blurb) = describe(status);
    let (class, scheme) = match dark {
        Some(true) => (" class=\"dark\"", "dark"),
        Some(false) => (" class=\"light\"", "light"),
        None => ("", "light dark"),
    };
    let hero = match theme {
        Theme::Default => DEFAULT_HERO
            .replace("__CODE__", &code)
            .replace("__TITLE__", title),
        Theme::Meme => MEME_HERO
            .replace("__CODE__", &code)
            .replace("__TITLE__", title)
            .replace("__REASON__", status.canonical_reason().unwrap_or("Error")),
    };
    let detail_html = detail
        .map(str::trim)
        .filter(|d| !d.is_empty())
        .map(|d| format!("<p class=\"detail\">{}</p>", escape_html(d)))
        .unwrap_or_default();
    TEMPLATE
        .replace("__CLASS__", class)
        .replace("__SCHEME__", scheme)
        .replace("__CODE__", &code)
        .replace("__TITLE__", title)
        .replace("__HERO__", &hero)
        .replace("__BLURB__", blurb)
        .replace("__DETAIL__", &detail_html)
        .replace("__MARK__", MARK)
        .replace("__STYLE__", STYLE)
}

/// `axum::middleware::from_fn` middleware — the shared status-page swap.
///
/// A 4xx/5xx carrying a *bare* body — no `Content-Type` (the framework's
/// default status response) or `text/plain` — is replaced by the themed
/// document, but **only** for clients that sent `Accept: text/html`.
/// Structured bodies (JSON envelopes, XML, media, HTML a handler produced)
/// pass through untouched, so RPC/REST/git/registry clients keep their
/// machine-readable errors while browsers get the branded page — including
/// on axum's default 404/405 responses and the web tier's proxy 502s.
///
/// The displaced terse body (≤4 KiB, e.g. `API unreachable`) rides along as
/// the page's detail line rather than being dropped. Other response headers
/// (`Allow` on a 405, `WWW-Authenticate` on a 401) are preserved.
pub async fn theme_errors(req: Request, next: Next) -> Response {
    let wants_html = accepts_html(
        req.headers()
            .get(header::ACCEPT)
            .and_then(|v| v.to_str().ok()),
    );
    let dark = color_scheme(
        req.headers()
            .get(header::COOKIE)
            .and_then(|v| v.to_str().ok()),
        req.headers()
            .get("sec-ch-prefers-color-scheme")
            .and_then(|v| v.to_str().ok()),
    );
    let response = next.run(req).await;
    if !wants_html || !is_error(response.status()) || !bare_body(response.headers()) {
        return response;
    }
    let (mut parts, body) = response.into_parts();
    let detail = axum::body::to_bytes(body, DETAIL_CAP)
        .await
        .ok()
        .and_then(|b| String::from_utf8(b.to_vec()).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    parts.headers.remove(header::CONTENT_LENGTH);
    parts.headers.remove(header::CONTENT_ENCODING);
    parts.headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    parts
        .headers
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    // The same URL can now answer HTML or the original payload depending on
    // Accept, and the page itself varies on the theme cookie/client hint —
    // shared caches must key on all three. Accept-CH advertises the hint so
    // the next navigation can send it.
    append_vary(&mut parts.headers, "Accept");
    append_vary(&mut parts.headers, "Cookie");
    append_vary(&mut parts.headers, "Sec-CH-Prefers-Color-Scheme");
    parts.headers.insert(
        HeaderName::from_static("accept-ch"),
        HeaderValue::from_static("Sec-CH-Prefers-Color-Scheme"),
    );
    let html = render_detail(theme(), parts.status, dark, detail.as_deref());
    Response::from_parts(parts, Body::from(html))
}

fn is_error(status: StatusCode) -> bool {
    status.is_client_error() || status.is_server_error()
}

/// Bodies eligible for the swap: absent `Content-Type` (`StatusCode`-only
/// responses — the "default status page" the issue names) or `text/plain`.
/// Structured bodies may be parsed downstream — never swap them.
fn bare_body(headers: &HeaderMap) -> bool {
    match headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
    {
        None => true,
        Some(ct) => ct.starts_with("text/plain"),
    }
}

/// Merge `name` into `Vary` (case-insensitive, dedup).
fn append_vary(headers: &mut HeaderMap, name: &str) {
    let current = headers
        .get(header::VARY)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let next = match current {
        Some(v) if v.split(',').any(|t| t.trim().eq_ignore_ascii_case(name)) => v,
        Some(v) => format!("{v}, {name}"),
        None => name.to_string(),
    };
    if let Ok(v) = HeaderValue::from_str(&next) {
        headers.insert(header::VARY, v);
    }
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// `(headline, blurb)` for a status. The codes the serving tiers actually
/// emit get bespoke copy; anything else falls back to the canonical reason
/// phrase.
fn describe(status: StatusCode) -> (&'static str, &'static str) {
    match status {
        StatusCode::BAD_REQUEST => ("Bad request", "The request could not be understood."),
        StatusCode::UNAUTHORIZED => (
            "Unauthorized",
            "You need to authenticate to view this page.",
        ),
        StatusCode::FORBIDDEN => ("Forbidden", "You do not have permission to view this page."),
        StatusCode::NOT_FOUND => (
            "Page not found",
            "The page you requested does not exist or may have moved.",
        ),
        StatusCode::METHOD_NOT_ALLOWED => (
            "Method not allowed",
            "This route only answers the methods it documents.",
        ),
        StatusCode::REQUEST_TIMEOUT => (
            "Request timed out",
            "The request took too long — try again.",
        ),
        StatusCode::PAYLOAD_TOO_LARGE => {
            ("Content too large", "The request exceeds the size limit.")
        }
        StatusCode::TOO_MANY_REQUESTS => {
            ("Too many requests", "Slow down and try again in a moment.")
        }
        StatusCode::INTERNAL_SERVER_ERROR => {
            ("Internal server error", "Something went wrong on our end.")
        }
        StatusCode::BAD_GATEWAY => (
            "Bad gateway",
            "An upstream service is unreachable — try again shortly.",
        ),
        StatusCode::SERVICE_UNAVAILABLE => (
            "Service unavailable",
            "The service is temporarily unavailable — try again in a moment.",
        ),
        StatusCode::GATEWAY_TIMEOUT => (
            "Gateway timed out",
            "The upstream service took too long to respond.",
        ),
        _ => (
            status.canonical_reason().unwrap_or("Error"),
            "An unexpected error occurred.",
        ),
    }
}

const TEMPLATE: &str = r##"<!doctype html>
<html lang="en"__CLASS__>
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="color-scheme" content="__SCHEME__">
<meta name="theme-color" content="#ffffff" media="(prefers-color-scheme: light)">
<meta name="theme-color" content="#121212" media="(prefers-color-scheme: dark)">
<link rel="icon" href="/favicon.ico" sizes="any">
<title>__CODE__ · __TITLE__ · Oxidean</title>
<style>
__STYLE__
</style>
</head>
<body>
<main>
<p class="brand">__MARK__Oxidean</p>
__HERO__
<p class="blurb">__BLURB__</p>
__DETAIL__
<p class="links"><a href="/">← Back to home</a></p>
</main>
</body>
</html>
"##;

const DEFAULT_HERO: &str = r#"<div class="code">__CODE__</div>
<h1>__TITLE__</h1>"#;

const MEME_HERO: &str = r#"<img class="cat" src="https://http.cat/__CODE__.jpg" alt="__CODE__ __REASON__ — http.cat">
<h1>__CODE__ · __TITLE__</h1>"#;

/// Inline brand mark — the faceted oxide shard from `brand/oxidean-mark.svg`,
/// copied so the page has zero asset dependencies.
const MARK: &str = r##"<svg viewBox="0 0 1024 1024" width="26" height="26" aria-hidden="true"><g stroke="rgba(0,0,0,0.22)" stroke-width="6" stroke-linejoin="round"><polygon points="512,120 324,500 512,540" fill="#ffab74"/><polygon points="512,120 700,500 512,540" fill="#f07c33"/><polygon points="324,500 512,904 512,540" fill="#c95408"/><polygon points="700,500 512,904 512,540" fill="#8f3c04"/></g><polygon points="468,236 428,398 482,352" fill="rgba(255,205,170,0.85)"/></svg>"##;

/// One stylesheet for both themes — tokens mirror `apps/web/src/styles.css`
/// (`--background`/`--foreground`/`--muted-foreground`/`--border`/`--primary`)
/// so the page reads as Oxidean, not a framework default. `html.dark` /
/// `html.light` are stamped server-side from the request's theme signal;
/// `html:not(.light)` under the media query covers unstamped renders.
const STYLE: &str = r#":root {
  --bg: #ffffff;
  --fg: #1f2328;
  --muted: #59636e;
  --border: #d0d7de;
  --primary: #bf4b0a;
  color-scheme: light;
}
html.dark {
  --bg: #121212;
  --fg: #f0f3f6;
  --muted: #9da7b3;
  --border: #3f434c;
  --primary: #ff7b4a;
  color-scheme: dark;
}
@media (prefers-color-scheme: dark) {
  html:not(.light) {
    --bg: #121212;
    --fg: #f0f3f6;
    --muted: #9da7b3;
    --border: #3f434c;
    --primary: #ff7b4a;
    color-scheme: dark;
  }
}
@media (prefers-contrast: more) {
  :root {
    --border: #8c959f;
    --muted: #424a53;
  }
  html.dark {
    --border: #6e7681;
    --muted: #c9d1d9;
  }
}
body {
  margin: 0;
  min-height: 100vh;
  display: flex;
  align-items: center;
  justify-content: center;
  background: var(--bg);
  color: var(--fg);
  font-family: "Source Sans 3 Variable", ui-sans-serif, system-ui, -apple-system, "Segoe UI", sans-serif;
  text-align: center;
}
main {
  display: flex;
  flex-direction: column;
  align-items: center;
  max-width: 34rem;
  padding: 2rem;
}
.brand {
  display: flex;
  align-items: center;
  gap: 0.55rem;
  margin: 0 0 3rem;
  font-size: 1.05rem;
  font-weight: 600;
  font-family: "Sora Variable", ui-sans-serif, system-ui, sans-serif;
}
.code {
  font-size: clamp(4.5rem, 16vw, 7rem);
  font-weight: 700;
  letter-spacing: -0.04em;
  line-height: 1;
  font-family: "Sora Variable", ui-sans-serif, system-ui, sans-serif;
}
.cat {
  display: block;
  max-width: min(100%, 26rem);
  border: 1px solid var(--border);
  border-radius: 0.75rem;
}
h1 {
  margin: 1rem 0 0;
  font-size: 1.5rem;
  font-weight: 600;
  font-family: "Sora Variable", ui-sans-serif, system-ui, sans-serif;
}
.blurb {
  margin: 0.9rem 0 0;
  max-width: 28rem;
  color: var(--muted);
  font-size: 0.95rem;
  line-height: 1.55;
}
.detail {
  margin: 0.9rem 0 0;
  max-width: 34rem;
  color: var(--muted);
  font-size: 0.82rem;
  font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  overflow-wrap: anywhere;
  white-space: pre-wrap;
}
.links {
  margin: 2.25rem 0 0;
  font-size: 0.95rem;
}
.links a {
  color: var(--primary);
  text-decoration: none;
}
.links a:hover {
  text-decoration: underline;
}"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_theme_values() {
        assert_eq!(parse_theme("default"), Some(Theme::Default));
        assert_eq!(parse_theme(" Meme "), Some(Theme::Meme));
        assert_eq!(parse_theme("MEME"), Some(Theme::Meme));
        assert_eq!(parse_theme("cats"), None);
        assert_eq!(parse_theme(""), None);
    }

    #[test]
    fn html_negotiation() {
        assert!(accepts_html(Some(
            "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8"
        )));
        assert!(accepts_html(Some("Text/Html")));
        assert!(!accepts_html(Some("application/json")));
        assert!(!accepts_html(Some("*/*")));
        assert!(!accepts_html(None));
    }

    #[test]
    fn color_scheme_chain() {
        assert_eq!(color_scheme(None, None), None);
        assert_eq!(color_scheme(Some("oxidean-theme=dark"), None), Some(true));
        assert_eq!(
            color_scheme(Some("oxidean-theme=light; oxidean-color-scheme=dark"), None),
            Some(false)
        );
        // `system` is not an explicit scheme — resolved cookie wins, else auto.
        assert_eq!(
            color_scheme(
                Some("oxidean-theme=system; oxidean-color-scheme=dark"),
                None
            ),
            Some(true)
        );
        assert_eq!(
            color_scheme(Some("oxidean-theme=system"), Some("dark")),
            Some(true)
        );
        assert_eq!(color_scheme(None, Some("dark")), Some(true));
        assert_eq!(color_scheme(None, Some("bogus")), None);
        // Explicit theme cookie beats the client hint.
        assert_eq!(
            color_scheme(Some("oxidean-theme=light"), Some("dark")),
            Some(false)
        );
    }

    #[test]
    fn default_theme_renders_branded_page() {
        let html = render(Theme::Default, StatusCode::NOT_FOUND, Some(false));
        assert!(html.contains("<title>404 · Page not found · Oxidean</title>"));
        assert!(html.contains("<div class=\"code\">404</div>"));
        assert!(html.contains("Oxidean"));
        assert!(html.contains("does not exist or may have moved"));
        assert!(!html.contains("http.cat"));
        assert!(html.contains("class=\"light\""));
    }

    #[test]
    fn dark_class_stamps_scheme() {
        let dark = render(Theme::Default, StatusCode::NOT_FOUND, Some(true));
        assert!(dark.contains("<html lang=\"en\" class=\"dark\">"));
        let auto = render(Theme::Default, StatusCode::NOT_FOUND, None);
        assert!(auto.contains("<html lang=\"en\">"));
    }

    #[test]
    fn meme_theme_uses_http_cat_image() {
        let html = render(Theme::Meme, StatusCode::BAD_GATEWAY, None);
        assert!(html.contains("https://http.cat/502.jpg"));
        assert!(html.contains("502 · Bad gateway"));
    }

    #[test]
    fn required_statuses_have_copy() {
        for status in [
            StatusCode::UNAUTHORIZED,
            StatusCode::FORBIDDEN,
            StatusCode::NOT_FOUND,
            StatusCode::INTERNAL_SERVER_ERROR,
            StatusCode::BAD_GATEWAY,
            StatusCode::SERVICE_UNAVAILABLE,
            StatusCode::GATEWAY_TIMEOUT,
        ] {
            let html = render(Theme::Default, status, None);
            assert!(html.contains(status.as_str()), "missing {status}");
        }
    }

    #[test]
    fn detail_is_escaped_and_laid_out() {
        let html = render_detail(
            Theme::Default,
            StatusCode::BAD_GATEWAY,
            None,
            Some("API unreachable <try harder>"),
        );
        assert!(html.contains("API unreachable &lt;try harder&gt;"));
        assert!(!html.contains("<try harder>"));
    }

    #[test]
    fn unlisted_status_falls_back_to_reason() {
        let html = render(Theme::Default, StatusCode::IM_A_TEAPOT, None);
        assert!(html.contains("I'm a teapot"));
        assert!(html.contains("418 · I'm a teapot · Oxidean"));
    }

    #[test]
    fn bare_body_gate() {
        let mut h = HeaderMap::new();
        assert!(bare_body(&h)); // framework default — no Content-Type
        h.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("text/plain; charset=utf-8"),
        );
        assert!(bare_body(&h));
        h.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        assert!(!bare_body(&h));
        h.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("text/html; charset=utf-8"),
        );
        assert!(!bare_body(&h));
    }

    #[test]
    fn vary_merges_without_duplicates() {
        let mut h = HeaderMap::new();
        append_vary(&mut h, "Accept");
        assert_eq!(h.get(header::VARY).unwrap(), "Accept");
        append_vary(&mut h, "Accept");
        assert_eq!(h.get(header::VARY).unwrap(), "Accept");
        append_vary(&mut h, "Cookie");
        assert_eq!(h.get(header::VARY).unwrap(), "Accept, Cookie");
    }

    /// End-to-end: an axum default 404 becomes the themed page only for
    /// `Accept: text/html`; the terse body rides along as the detail line.
    #[tokio::test]
    async fn middleware_swaps_bare_error_for_html_acceptors() {
        use axum::routing::get;
        use tower::ServiceExt;

        let app = axum::Router::new()
            .route(
                "/gone",
                get(|| async { (StatusCode::NOT_FOUND, "API unreachable") }),
            )
            .layer(axum::middleware::from_fn(theme_errors));

        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/gone")
                    .header(header::ACCEPT, "text/html")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
        assert_eq!(
            res.headers().get(header::CONTENT_TYPE).unwrap(),
            "text/html; charset=utf-8"
        );
        assert_eq!(
            res.headers().get(header::VARY).unwrap(),
            "Accept, Cookie, Sec-CH-Prefers-Color-Scheme"
        );
        let body = axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap();
        let body = String::from_utf8(body.to_vec()).unwrap();
        assert!(body.contains("Page not found"));
        assert!(body.contains("API unreachable"));
    }

    /// Machine clients keep the original terse body verbatim.
    #[tokio::test]
    async fn middleware_leaves_non_html_clients_alone() {
        use axum::routing::get;
        use tower::ServiceExt;

        let app = axum::Router::new()
            .route("/gone", get(|| async { StatusCode::NOT_FOUND }))
            .route(
                "/json",
                get(|| async {
                    (
                        StatusCode::FORBIDDEN,
                        [(header::CONTENT_TYPE, "application/json")],
                        "{\"error\":\"nope\"}",
                    )
                }),
            )
            .layer(axum::middleware::from_fn(theme_errors));

        // No Accept: text/html — bare 404 passes through untouched.
        let res = app
            .clone()
            .oneshot(Request::builder().uri("/gone").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
        let body = axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap();
        assert!(body.is_empty());

        // JSON errors are structured — never swapped, even for browsers.
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/json")
                    .header(header::ACCEPT, "text/html")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body = axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(&body[..], b"{\"error\":\"nope\"}");
    }
}
