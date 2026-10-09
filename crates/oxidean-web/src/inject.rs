//! Per-request HTML rewriting on shell bytes — the Rust replacement for the
//! bits `__root.tsrx` stamped during SSR: `<html>` theme class, `color-scheme`
//! meta, route `<title>`, and the `oxidean:ssh-*` advertise metas the client
//! reads for clone URLs.

use axum::http::HeaderMap;

use crate::session::SessionSignal;

/// `"light"`/`"dark"` after applying the same chain the FOUC boot script +
/// `resolveThemeForSsr` use: explicit `oxidean-theme` cookie → resolved
/// `oxidean-color-scheme` cookie → `Sec-CH-Prefers-Color-Site` header → light.
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
/// advertised SSH clone host/port.
pub struct AdvertiseMeta {
    pub ssh_host: Option<String>,
    pub ssh_port: Option<u16>,
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
    let mut html_attrs = String::new();
    if theme == "dark" {
        html_attrs.push_str(" class=\"dark\"");
    }
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
        out = out.replacen(
            "<html lang=\"en\"",
            &format!("<html lang=\"en\"{html_attrs}"),
            1,
        );
    }
    if theme == "dark" {
        out = out.replacen(
            "<meta name=\"color-scheme\" content=\"light\"",
            "<meta name=\"color-scheme\" content=\"dark\"",
            1,
        );
    }

    // Route <title> — shells ship a generic "Oxidean" title for dynamic pages.
    if let Some(t) = title {
        if let (Some(a), Some(b)) = (out.find("<title>"), out.find("</title>")) {
            out = format!("{}<title>{}</title>{}", &out[..a], t, &out[b + "</title>".len()..]);
        }
    }

    // SSH advertise metas — read by `resolveSshAdvertiseHost/Port` in the SPA.
    let mut extra = String::new();
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
            &AdvertiseMeta { ssh_host: Some("git.example.com".into()), ssh_port: Some(2222) },
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
        let meta = AdvertiseMeta { ssh_host: None, ssh_port: None };
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
        let meta = AdvertiseMeta { ssh_host: None, ssh_port: None };

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
}
