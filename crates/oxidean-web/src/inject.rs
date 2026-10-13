//! Per-request HTML rewriting on shell bytes — the Rust replacement for the
//! bits `__root.tsrx` stamped during SSR: `<html>` theme class, `color-scheme`
//! meta, route `<title>`, and the `oxidean:ssh-*` advertise metas the client
//! reads for clone URLs.

use axum::http::HeaderMap;

use crate::session::SessionSignal;

/// `"light"`/`"dark"` after applying the same chain the FOUC boot script
/// mirrors into cookies: explicit `oxidean-theme` cookie → resolved
/// `oxidean-color-scheme` cookie → `Sec-CH-Prefers-Color-Scheme` header → light.
pub fn resolve_theme(cookie: Option<&str>, sec_ch: Option<&str>) -> &'static str {
    if let Some(c) = cookie {
        if let Some(m) = find_cookie(c, "oxidean-theme") {
            if m == "light" || m == "dark" {
                return if m == "dark" { "dark" } else { "light" };
            }
        }
        if let Some(m) = find_cookie(c, "oxidean-color-scheme") {
            if m == "light" || m == "dark" {
                return if m == "dark" { "dark" } else { "light" };
            }
        }
    }
    if let Some(ch) = sec_ch {
        let ch = ch.trim().to_ascii_lowercase();
        if ch == "dark" || ch == "light" {
            return if ch == "dark" { "dark" } else { "light" };
        }
    }
    "light"
}

/// `name=value` lookup inside a Cookie header (no decoding — our cookies are
/// all ASCII flag values).
pub(crate) fn find_cookie<'a>(header: &'a str, name: &str) -> Option<&'a str> {
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

/// Anonymous-gate input: the `oxidean_signed_in=1` presence flag OR a real
/// `oxidean_session` cookie. The hint is client-set and can be absent on
/// sessions minted before it existed (see `syncSessionPresenceHint`) — the
/// HttpOnly session cookie is the stronger signal and visible to the server.
/// Neither is authoritative for ACL — that stays in the API + client shell.
pub fn signed_in(cookie: Option<&str>) -> bool {
    let Some(c) = cookie else { return false };
    if find_cookie(c, "oxidean_signed_in").is_some_and(|v| v == "1") {
        return true;
    }
    find_cookie(c, "oxidean_session").is_some_and(|v| !v.is_empty())
}

/// Extra `<head>` meta the serving tier knows and the client cannot derive:
/// advertised SSH clone host/port and the canonical public origin.
#[derive(Default)]
pub struct AdvertiseMeta {
    pub ssh_host: Option<String>,
    pub ssh_port: Option<u16>,
    /// `OXIDEAN_PUBLIC_ORIGIN` — operator-set canonical origin. None falls
    /// back to per-request derivation from Host / X-Forwarded-* (the latter
    /// only when `behind_proxy`).
    pub public_origin: Option<String>,
    /// `OXIDEAN_WEB_BEHIND_PROXY` — a sanitizing edge is in front, so
    /// X-Forwarded-* headers may be trusted for origin derivation.
    pub behind_proxy: bool,
}

/// Build-time origin placeholder. Astro prerenders shells before the deploy
/// origin exists, so `resolvePublicOriginClient()` emits this literal into
/// `dist/` — the serving tier rewrites it per request. Never carries a port:
/// `http://localhost:8080` in docs/example text must survive untouched.
const BUILD_ORIGIN_PLACEHOLDER: &str = "http://localhost";

/// Public origin for this request: the configured `OXIDEAN_PUBLIC_ORIGIN`
/// wins; otherwise derive from the request — `X-Forwarded-Host`/`Proto` only
/// behind a declared edge (`behind_proxy`), raw `Host` always. HTTP scheme
/// unless the edge says otherwise (this tier terminates no TLS itself).
fn request_origin(headers: &HeaderMap, behind_proxy: bool) -> Option<String> {
    let hdr = |name: &str| {
        headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(str::trim)
            .filter(|s| !s.is_empty())
    };
    let fwd_host = behind_proxy.then(|| hdr("x-forwarded-host")).flatten();
    let host = fwd_host.or_else(|| hdr("host"))?;
    let proto = behind_proxy
        .then(|| hdr("x-forwarded-proto"))
        .flatten()
        .filter(|p| *p == "https" || *p == "http")
        .unwrap_or("http");
    Some(format!("{proto}://{host}"))
}

/// Replace every bare `http://localhost` placeholder with the resolved origin.
/// `http://localhost:PORT` occurrences are left alone — those are legitimate
/// example text, not the placeholder.
fn rewrite_build_origin(html: &mut String, origin: &str) {
    if origin == BUILD_ORIGIN_PLACEHOLDER {
        return;
    }
    let mut from = 0;
    while let Some(rel) = html[from..].find(BUILD_ORIGIN_PLACEHOLDER) {
        let at = from + rel;
        let end = at + BUILD_ORIGIN_PLACEHOLDER.len();
        if html.as_bytes().get(end) == Some(&b':') {
            from = end;
            continue;
        }
        html.replace_range(at..end, origin);
        from = at + origin.len();
    }
}

/// Apply the per-request stamps to a shell's HTML.
///
/// `signal` is the resolved session state (`session::resolve_signal`): `Valid`
/// stamps unconditionally, `Invalid`/`Absent` never stamp, and `Unknown` falls
/// back to cookie presence so an API outage degrades to yesterday's behavior.
pub fn inject(
    html: &str,
    headers: &HeaderMap,
    title: Option<&str>,
    meta: &AdvertiseMeta,
    signal: SessionSignal,
) -> String {
    let cookie = headers
        .get(axum::http::header::COOKIE)
        .and_then(|v| v.to_str().ok());
    let sec_ch = headers
        .get("sec-ch-prefers-color-scheme")
        .and_then(|v| v.to_str().ok());
    let theme = resolve_theme(cookie, sec_ch);

    let mut out = html.to_string();

    // <html lang="en"> → <html lang="en" class="dark" data-oxidean-session="1">:
    // the theme class the FOUC boot script keys on, plus a signed-in stamp so
    // pending islands can pre-select their skeleton instead of flashing the
    // anonymous landing before hydration resolves `auth.me`.
    // A pre-existing `class` on <html> must be merged, not duplicated — HTML
    // takes the first attribute and drops repeats.
    if theme == "dark" {
        stamp_class(&mut out, "dark");
    }
    let mut html_attrs = String::new();
    let stamped = match signal {
        SessionSignal::Valid => true,
        SessionSignal::Unknown => signed_in(cookie),
        SessionSignal::Absent | SessionSignal::Invalid => false,
    };
    if stamped {
        html_attrs.push_str(" data-oxidean-session=\"1\"");
    }
    // Readable companion written by the client once `auth.provider_config`
    // resolves — lets the anon header skeleton match the resolved cluster's
    // one- vs two-button geometry on the next shell. Absent/`0` = closed.
    if cookie.is_some_and(|c| find_cookie(c, "oxidean_allow_signup") == Some("1")) {
        html_attrs.push_str(" data-oxidean-signup=\"1\"");
    }
    if !html_attrs.is_empty() {
        if let Some(end) = html_tag_open_end(&out) {
            out.insert_str(end, &html_attrs);
        }
    }
    if theme == "dark" {
        set_meta_content(&mut out, "color-scheme", "dark");
    }

    // Route <title> — shells ship a generic "Oxidean" title for dynamic pages.
    if let Some(t) = title {
        if let (Some(a), Some(b)) = (out.find("<title>"), out.find("</title>")) {
            out = format!("{}<title>{}</title>{}", &out[..a], t, &out[b + "</title>".len()..]);
        }
    }

    // Public origin — canonical env first, request-derived otherwise. The meta
    // is read by `resolvePublicOriginClient`; the rewrite fixes shells that
    // baked the build-time placeholder into SSR'd markup.
    let origin = meta
        .public_origin
        .clone()
        .or_else(|| request_origin(headers, meta.behind_proxy));
    if let Some(o) = &origin {
        rewrite_build_origin(&mut out, o);
    }

    // SSH advertise metas — read by `resolveSshAdvertiseHost/Port` in the SPA.
    let mut extra = String::new();
    if let Some(o) = &origin {
        extra.push_str(&format!(
            "<meta name=\"oxidean:public-origin\" content=\"{}\">",
            escape_attr(o)
        ));
    }
    if let Some(h) = &meta.ssh_host {
        extra.push_str(&format!(
            "<meta name=\"oxidean:ssh-host\" content=\"{}\">",
            escape_attr(h)
        ));
    }
    if let Some(p) = meta.ssh_port {
        extra.push_str(&format!("<meta name=\"oxidean:ssh-port\" content=\"{p}\">"));
    }
    if !extra.is_empty() {
        if let Some(pos) = out.rfind("</head>") {
            out.insert_str(pos, &extra);
        }
    }
    out
}

fn escape_attr(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
}

/// Index just before the `>` closing the opening `<html …>` tag — attribute
/// order-insensitive so `<html class="x" lang="en">` still stamps. None when
/// the document has no `<html>` open tag (never true for our shells).
fn html_tag_open_end(s: &str) -> Option<usize> {
    let start = s.find("<html")?;
    let next = s.as_bytes().get(start + "<html".len())?;
    // `<htmlx` / `<htmlfoo` is not the tag.
    if !matches!(next, b' ' | b'\t' | b'\r' | b'\n' | b'>' | b'/') {
        return None;
    }
    s[start..].find('>').map(|i| start + i)
}

/// Add `class` to the `<html>` tag — merges into an existing `class="…"`
/// (duplicated attributes are dropped by parsers, so a second `class` would
/// be silently ignored).
fn stamp_class(html: &mut String, class: &str) {
    let Some(end) = html_tag_open_end(html) else {
        return;
    };
    let start = html.find("<html").unwrap();
    if let Some(cs) = html[start..end].find("class=\"") {
        let val_start = start + cs + "class=\"".len();
        if let Some(val_len) = html[val_start..end].find('"') {
            let cur = &html[val_start..val_start + val_len];
            if !cur.split_whitespace().any(|c| c == class) {
                html.insert_str(val_start + val_len, &format!(" {class}"));
            }
            return;
        }
    }
    html.insert_str(end, &format!(" class=\"{class}\""));
}

/// Rewrite `content="…"` on the `<meta name="{name}">` tag. Attribute order
/// inside the tag is free — only the tag carrying the name is touched.
fn set_meta_content(html: &mut String, name: &str, value: &str) {
    let needle = format!("name=\"{name}\"");
    let mut from = 0;
    while let Some(rel) = html[from..].find("<meta") {
        let tag_start = from + rel;
        let Some(tag_len) = html[tag_start..].find('>') else {
            return;
        };
        let tag_end = tag_start + tag_len;
        if html[tag_start..tag_end].contains(&needle) {
            let Some(val_rel) = html[tag_start..tag_end].find("content=\"") else {
                return;
            };
            let val_start = tag_start + val_rel + "content=\"".len();
            let Some(val_len) = html[val_start..tag_end].find('"') else {
                return;
            };
            html.replace_range(val_start..val_start + val_len, value);
            return;
        }
        from = tag_end;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn theme_chain() {
        assert_eq!(resolve_theme(None, None), "light");
        assert_eq!(
            resolve_theme(Some("oxidean-theme=dark"), None),
            "dark"
        );
        // resolved cookie beats CH header
        assert_eq!(
            resolve_theme(Some("oxidean-color-scheme=light"), Some("dark")),
            "light"
        );
        assert_eq!(resolve_theme(None, Some("dark")), "dark");
        assert_eq!(
            resolve_theme(Some("oxidean-theme=system; oxidean-color-scheme=dark"), None),
            "dark"
        );
    }

    #[test]
    fn presence() {
        assert!(signed_in(Some("a=b; oxidean_signed_in=1; c=d")));
        // Authoritative session cookie alone also passes the gate — the hint
        // is a client-set companion, not the credential.
        assert!(signed_in(Some("oxidean_session=abc123")));
        assert!(!signed_in(Some("oxidean_signed_in=0")));
        assert!(!signed_in(Some("oxidean_session=")));
        assert!(!signed_in(None));
    }

    #[test]
    fn stamps() {
        let html = r#"<html lang="en"><head><meta name="color-scheme" content="light"><title>Oxidean</title></head>"#;
        let mut h = HeaderMap::new();
        h.insert("cookie", HeaderValue::from_static("oxidean-theme=dark"));
        let out = inject(
            html,
            &h,
            Some("Issues · a/b · Oxidean"),
            &AdvertiseMeta {
                ssh_host: Some("git.example.com".into()),
                ssh_port: Some(2222),
                ..Default::default()
            },
            SessionSignal::Absent,
        );
        assert!(out.contains("<html lang=\"en\" class=\"dark\""));
        assert!(out.contains("content=\"dark\""));
        assert!(out.contains("<title>Issues · a/b · Oxidean</title>"));
        assert!(out.contains("oxidean:ssh-host\" content=\"git.example.com\""));
    }

    #[test]
    fn session_stamp() {
        let html = r#"<html lang="en"><head><title>Oxidean</title></head>"#;
        let meta = AdvertiseMeta::default();
        let mut h = HeaderMap::new();
        h.insert("cookie", HeaderValue::from_static("oxidean_session=abc"));

        // Upstream-validated → stamp regardless of what presence thinks.
        let out = inject(html, &h, None, &meta, SessionSignal::Valid);
        assert!(out.contains("data-oxidean-session=\"1\""));

        // Definitively invalid or absent → never stamp (hint included).
        let out = inject(html, &h, None, &meta, SessionSignal::Invalid);
        assert!(!out.contains("data-oxidean-session"));
        let mut hint_only = HeaderMap::new();
        hint_only.insert("cookie", HeaderValue::from_static("oxidean_signed_in=1"));
        let out = inject(html, &hint_only, None, &meta, SessionSignal::Absent);
        assert!(!out.contains("data-oxidean-session"));

        // Unknown (API unreachable/unconfigured) → presence fallback.
        let out = inject(html, &h, None, &meta, SessionSignal::Unknown);
        assert!(out.contains("data-oxidean-session=\"1\""));
        let out = inject(html, &HeaderMap::new(), None, &meta, SessionSignal::Unknown);
        assert!(!out.contains("data-oxidean-session"));
    }

    #[test]
    fn signup_stamp() {
        let html = r#"<html lang="en"><head><title>Oxidean</title></head>"#;
        let meta = AdvertiseMeta::default();

        // Open registration per the companion cookie → stamped.
        let mut open = HeaderMap::new();
        open.insert(
            "cookie",
            HeaderValue::from_static("oxidean_allow_signup=1"),
        );
        let out = inject(html, &open, None, &meta, SessionSignal::Absent);
        assert!(out.contains("data-oxidean-signup=\"1\""));

        // Closed (`=0`) or never-resolved (absent) → no stamp.
        let mut closed = HeaderMap::new();
        closed.insert(
            "cookie",
            HeaderValue::from_static("oxidean_allow_signup=0"),
        );
        let out = inject(html, &closed, None, &meta, SessionSignal::Absent);
        assert!(!out.contains("data-oxidean-signup"));
        let out = inject(html, &HeaderMap::new(), None, &meta, SessionSignal::Absent);
        assert!(!out.contains("data-oxidean-signup"));
    }

    /// Attribute order/position must not matter — a future layout emitting
    /// `<html class="x" lang="en">` or `<html>` bare still gets stamped.
    #[test]
    fn stamps_survive_attr_reorder() {
        let meta = AdvertiseMeta::default();
        let mut h = HeaderMap::new();
        h.insert(
            "cookie",
            HeaderValue::from_static("oxidean-theme=dark; oxidean_session=abc"),
        );
        for html in [
            r#"<html class="a" lang="en"><head><meta content="light" name="color-scheme"></head>"#,
            r#"<html><head><meta name="color-scheme" content="light" media="x"></head>"#,
        ] {
            let out = inject(html, &h, None, &meta, SessionSignal::Unknown);
            assert!(
                out.contains("class=\"dark\"") || out.contains("class=\"a dark\""),
                "dark class must be stamped into {html}: {out}"
            );
            assert!(out.contains("data-oxidean-session=\"1\""), "{out}");
            assert!(out.contains("content=\"dark\""), "{out}");
        }
        // `<htmlx>` is not the html tag — must not be stamped.
        let out = inject(
            r#"<htmlx><meta name="color-scheme" content="light"></htmlx>"#,
            &h,
            None,
            &meta,
            SessionSignal::Unknown,
        );
        assert!(out.contains("<htmlx>") && !out.contains("data-oxidean-session"));
    }

    /// Configured `OXIDEAN_PUBLIC_ORIGIN` wins over request headers and both
    /// injects the meta and rewrites the build-time `http://localhost`
    /// placeholder everywhere it appears.
    #[test]
    fn public_origin_meta_and_placeholder_rewrite() {
        let html = concat!(
            r#"<html><head><title>Oxidean</title></head><body>"#,
            r#"<input value="curl -fsSL http://localhost/cli/install.sh | sh">"#,
            r#"<code>http://localhost/api/mcp</code>"#,
            r#"<p>dev setup: OXIDEAN_API_ORIGIN=http://localhost:8080</p>"#,
            r#"</body></html>"#
        );
        let meta = AdvertiseMeta {
            public_origin: Some("https://git.example.com".into()),
            ..Default::default()
        };
        let mut h = HeaderMap::new();
        h.insert("host", HeaderValue::from_static("internal.lan:3000"));

        let out = inject(html, &h, None, &meta, SessionSignal::Absent);

        assert!(out.contains("oxidean:public-origin\" content=\"https://git.example.com\""));
        // Placeholder rewritten in attribute and text contexts alike.
        assert!(out.contains("value=\"curl -fsSL https://git.example.com/cli/install.sh | sh\""));
        assert!(out.contains("<code>https://git.example.com/api/mcp</code>"));
        // Port-bearing localhost is real example text — untouched.
        assert!(out.contains("http://localhost:8080"));
        assert!(!out.contains("http://localhost/"));
    }

    /// No configured origin → derive per request: raw `Host` always,
    /// X-Forwarded-* only when a sanitizing edge is declared.
    #[test]
    fn public_origin_request_fallback() {
        let html = r#"<html><head></head><body>http://localhost/api/mcp</body></html>"#;
        let meta = AdvertiseMeta::default();
        let mut h = HeaderMap::new();
        h.insert("host", HeaderValue::from_static("forge.internal:3000"));

        let out = inject(html, &h, None, &meta, SessionSignal::Absent);
        assert!(out.contains("oxidean:public-origin\" content=\"http://forge.internal:3000\""));
        assert!(out.contains("<body>http://forge.internal:3000/api/mcp</body>"));

        // Direct (not behind a declared edge): client-sent X-Forwarded-* are
        // ignored — Host alone derives the origin.
        h.insert("x-forwarded-host", HeaderValue::from_static("evil.example"));
        h.insert("x-forwarded-proto", HeaderValue::from_static("https"));
        let out = inject(html, &h, None, &meta, SessionSignal::Absent);
        assert!(out.contains("oxidean:public-origin\" content=\"http://forge.internal:3000\""));

        // Behind a sanitizing edge the forwarded pair wins.
        let meta = AdvertiseMeta {
            behind_proxy: true,
            ..Default::default()
        };
        let out = inject(html, &h, None, &meta, SessionSignal::Absent);
        assert!(out.contains("oxidean:public-origin\" content=\"https://evil.example\""));
    }

    /// An origin equal to the placeholder (Compose default) is a no-op rewrite.
    #[test]
    fn public_origin_localhost_noop() {
        let html = r#"<html><head></head><body>http://localhost/api/mcp</body></html>"#;
        let meta = AdvertiseMeta {
            public_origin: Some("http://localhost".into()),
            ..Default::default()
        };
        let out = inject(html, &HeaderMap::new(), None, &meta, SessionSignal::Absent);
        assert!(out.contains("oxidean:public-origin\" content=\"http://localhost\""));
        assert!(out.contains("<body>http://localhost/api/mcp</body>"));
    }
}
