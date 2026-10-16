//! Resolve browser-facing public origin / CORS for Railway PR Environments.
//!
//! PR copies inherit preview `OXIDEAN_PUBLIC_ORIGIN`; if that hostname is a
//! Railway `*.up.railway.app` host that does not match
//! `RAILWAY_SERVICE_GATEWAY_URL` / `RAILWAY_PUBLIC_DOMAIN`, prefer the live
//! gateway host. Custom domains (production) are left alone.

/// Hostname from a URL or bare host string (no port / path).
pub fn origin_hostname(origin: &str) -> Option<&str> {
    let s = origin.trim().trim_end_matches('/');
    if s.is_empty() {
        return None;
    }
    let without_scheme = match s.find("://") {
        Some(i) => &s[i + 3..],
        None => s,
    };
    let hostport = without_scheme.split('/').next().unwrap_or("");
    let host = hostport.split(':').next().unwrap_or("").trim();
    if host.is_empty() {
        None
    } else {
        Some(host)
    }
}

fn is_railway_app_host(host: &str) -> bool {
    host.eq_ignore_ascii_case("up.railway.app")
        || host.to_ascii_lowercase().ends_with(".up.railway.app")
}

/// Railway gateway hostname when the process runs on Railway.
pub fn railway_gateway_host() -> Option<String> {
    for key in ["RAILWAY_SERVICE_GATEWAY_URL", "RAILWAY_PUBLIC_DOMAIN"] {
        let Ok(raw) = std::env::var(key) else {
            continue;
        };
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(h) = origin_hostname(trimmed) {
            return Some(h.to_string());
        }
    }
    None
}

fn railway_gateway_origin() -> Option<String> {
    railway_gateway_host().map(|h| format!("https://{h}"))
}

fn configured_public_origin() -> Option<String> {
    std::env::var("OXIDEAN_PUBLIC_ORIGIN")
        .ok()
        .map(|o| o.trim().trim_end_matches('/').to_string())
        .filter(|o| !o.is_empty())
}

/// Absolute origin for magic links, clone URLs, and SSO redirects.
///
/// Prefer `OXIDEAN_PUBLIC_ORIGIN` unless it is a stale Railway preview/PR host.
/// Custom domains are never replaced by the Railway gateway hostname.
pub fn resolve_public_origin() -> String {
    let configured = configured_public_origin();
    match (configured, railway_gateway_origin()) {
        (Some(cfg), Some(railway)) => {
            let cfg_host = origin_hostname(&cfg);
            let rw_host = origin_hostname(&railway);
            match (cfg_host, rw_host) {
                (Some(ch), Some(rh)) if is_railway_app_host(ch) && !ch.eq_ignore_ascii_case(rh) => {
                    railway
                }
                _ => cfg,
            }
        }
        (None, Some(railway)) => railway,
        (Some(cfg), None) => cfg,
        (None, None) => "http://localhost:8080".into(),
    }
}

/// Effective CORS allowlist string for `build_cors`.
///
/// When Railway gateway host is set and the configured list is missing or only
/// lists a stale `*.up.railway.app` host, use the resolved public origin.
pub fn resolve_cors_origins(explicit: Option<&str>) -> Option<String> {
    let list = explicit
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let Some(rw_host) = railway_gateway_host() else {
        return list;
    };
    let effective = resolve_public_origin();
    match list {
        None => Some(effective),
        Some(list) => {
            let matches = list.split(',').any(|o| {
                origin_hostname(o.trim()).is_some_and(|h| h.eq_ignore_ascii_case(&rw_host))
            });
            if matches {
                return Some(list);
            }
            // Only replace when every listed origin is a Railway app host (stale
            // PR/preview inheritance). Leave custom-domain allowlists alone.
            let only_railway = list
                .split(',')
                .all(|o| origin_hostname(o.trim()).is_some_and(is_railway_app_host));
            if only_railway {
                Some(effective)
            } else {
                Some(list)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn clear_origin_env() {
        for key in [
            "OXIDEAN_PUBLIC_ORIGIN",
            "RAILWAY_SERVICE_GATEWAY_URL",
            "RAILWAY_PUBLIC_DOMAIN",
        ] {
            // SAFETY: tests hold ENV_LOCK; only this process mutates these keys.
            unsafe { std::env::remove_var(key) };
        }
    }

    #[test]
    fn origin_hostname_parses_urls_and_bare_hosts() {
        assert_eq!(
            origin_hostname("https://gateway-pr-31.up.railway.app/"),
            Some("gateway-pr-31.up.railway.app")
        );
        assert_eq!(
            origin_hostname("gateway-pr-31.up.railway.app"),
            Some("gateway-pr-31.up.railway.app")
        );
        assert_eq!(origin_hostname("http://localhost:3000"), Some("localhost"));
    }

    #[test]
    fn prefers_railway_when_configured_host_stale() {
        let _g = ENV_LOCK.lock().unwrap();
        clear_origin_env();
        unsafe {
            std::env::set_var(
                "OXIDEAN_PUBLIC_ORIGIN",
                "https://gateway-preview-4893.up.railway.app",
            );
            std::env::set_var(
                "RAILWAY_SERVICE_GATEWAY_URL",
                "gateway-oxidean-pr-31.up.railway.app",
            );
        }
        assert_eq!(
            resolve_public_origin(),
            "https://gateway-oxidean-pr-31.up.railway.app"
        );
        clear_origin_env();
    }

    #[test]
    fn keeps_custom_domain_when_railway_host_differs() {
        let _g = ENV_LOCK.lock().unwrap();
        clear_origin_env();
        unsafe {
            std::env::set_var("OXIDEAN_PUBLIC_ORIGIN", "https://app.oxidean.dev");
            std::env::set_var("RAILWAY_PUBLIC_DOMAIN", "gateway-production.up.railway.app");
        }
        assert_eq!(resolve_public_origin(), "https://app.oxidean.dev");
        clear_origin_env();
    }

    #[test]
    fn keeps_configured_when_hosts_match() {
        let _g = ENV_LOCK.lock().unwrap();
        clear_origin_env();
        unsafe {
            std::env::set_var(
                "OXIDEAN_PUBLIC_ORIGIN",
                "https://gateway-oxidean-pr-31.up.railway.app/",
            );
            std::env::set_var(
                "RAILWAY_PUBLIC_DOMAIN",
                "gateway-oxidean-pr-31.up.railway.app",
            );
        }
        assert_eq!(
            resolve_public_origin(),
            "https://gateway-oxidean-pr-31.up.railway.app"
        );
        clear_origin_env();
    }

    #[test]
    fn cors_replaces_stale_allowlist() {
        let _g = ENV_LOCK.lock().unwrap();
        clear_origin_env();
        unsafe {
            std::env::set_var(
                "RAILWAY_SERVICE_GATEWAY_URL",
                "https://gateway-oxidean-pr-31.up.railway.app",
            );
        }
        let resolved = resolve_cors_origins(Some("https://gateway-preview-4893.up.railway.app"));
        assert_eq!(
            resolved.as_deref(),
            Some("https://gateway-oxidean-pr-31.up.railway.app")
        );
        clear_origin_env();
    }
}
