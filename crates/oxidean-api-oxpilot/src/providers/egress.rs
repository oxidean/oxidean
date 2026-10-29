//! SSRF bound for admin-supplied `base_url` (BR3.2, NFR1.8–1.10):
//!
//! 1. `Url::parse` + `http`/`https` scheme — the WHATWG parser also
//!    normalizes every exotic IPv4 literal notation (dotted-quad, hex
//!    `0x…`, octal `0…`, bare integer, IPv4-mapped-IPv6) into
//!    `Host::Ipv4`, so literal-form coverage comes free at parse.
//! 2. `provider_id == None` → shape check only — a provider-less config
//!    stores URLs with no consumer, so the private-target bound is
//!    skipped (BR3.2).
//! 3. `provider_id == "ollama"` → shape check only — the blessed
//!    self-host exception (NFR1.10, disclosed in CONFIGURATION.md).
//! 4. Literal IPs must be public; DNS names resolve through
//!    `lookup_host` and EVERY answer must be public (zero answers ⇒
//!    reject). The validated answers pin into the provider client via
//!    `resolve_to_addrs`, so a post-validation re-resolution cannot
//!    rebind the connect to an unchecked address — the TOCTOU bound is
//!    closed for direct egress. A configured egress proxy resolves the
//!    name itself (admin-controlled infrastructure by definition), so
//!    env-proxy support stays enabled.
//!
//! Provider HTTP clients are built redirect-free
//! (`redirect(Policy::none())`) so a validated URL cannot 302 onto
//! private egress (BR4.5).
//!
//! The public-routable classifier lives in `oxidean_core::net` and is
//! shared with webhook delivery, git mirrors, and XFF trust.

use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

use oxidean_core::net::is_public_ip;
use url::Url;

/// Upper bound on the validation DNS lookup — a stall here must never
/// hang the boot rebuild or the settings write path (NFR4.11).
const DNS_LOOKUP_TIMEOUT: Duration = Duration::from_secs(10);

/// DNS answers validated at settings-write/build time, pinned into the
/// provider client so the socket connect reaches only these addresses —
/// a later re-resolution to a private target never takes effect.
/// `host` is the URL's DNS name — the `resolve_to_addrs` lookup key.
pub(crate) struct PinnedEgress {
    pub host: String,
    pub addrs: Vec<SocketAddr>,
}

/// Redirect-free provider client — connect 10 s, total 300 s
/// (NFR4.7/NFR4.9). `pinned` carries the validated DNS answers for a
/// domain-named base_url; `None` builds the same client as before
/// (literal-IP and exempted URLs have nothing to pin). Per-adapter
/// `build` calls this exactly once.
pub(crate) fn provider_http_client(pinned: Option<&PinnedEgress>) -> reqwest::Client {
    let mut builder = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(300))
        .redirect(reqwest::redirect::Policy::none());
    if let Some(p) = pinned {
        builder = builder.resolve_to_addrs(&p.host, &p.addrs);
    }
    builder
        .build()
        .expect("static reqwest builder config cannot fail")
}

/// Validate an admin/env `base_url` against the resolved provider.
/// Returns the DNS pin the provider client must connect through —
/// `Some` for a public-checked domain name, `None` for literal-IP and
/// bound-exempt URLs. `Err(String)` carries a short reason the caller
/// maps to `invalid_provider` — never echoed to non-admin surfaces.
pub(crate) async fn validate_base_url(
    provider_id: Option<&str>,
    url: &str,
) -> Result<Option<PinnedEgress>, String> {
    let parsed = Url::parse(url.trim()).map_err(|e| format!("base_url is not a valid URL: {e}"))?;
    match parsed.scheme() {
        "http" | "https" => {}
        scheme => return Err(format!("base_url scheme `{scheme}` must be http or https")),
    }
    let host = parsed
        .host()
        .ok_or_else(|| "base_url has no host".to_string())?;

    // No consumer (provider-less write) or the Ollama self-host
    // exemption — shape check done, private-target bound skipped.
    match provider_id {
        None => return Ok(None),
        Some("ollama") => return Ok(None),
        _ => {}
    }

    match host {
        url::Host::Ipv4(v4) => require_public(&IpAddr::V4(v4), url).map(|_| None),
        url::Host::Ipv6(v6) => require_public(&IpAddr::V6(v6), url).map(|_| None),
        url::Host::Domain(name) => {
            let port = parsed.port_or_known_default().unwrap_or(443);
            let name = name.to_string();
            let lookup = tokio::net::lookup_host((name.as_str(), port));
            let addrs: Vec<SocketAddr> =
                match tokio::time::timeout(DNS_LOOKUP_TIMEOUT, lookup).await {
                    Ok(Ok(iter)) => iter.collect(),
                    Ok(Err(e)) => return Err(format!("base_url host does not resolve: {e}")),
                    Err(_) => return Err("base_url DNS lookup timed out".to_string()),
                };
            if addrs.is_empty() {
                return Err("base_url host resolves to no addresses".to_string());
            }
            for addr in &addrs {
                require_public(&addr.ip(), url)?;
            }
            Ok(Some(PinnedEgress { host: name, addrs }))
        }
    }
}

fn require_public(ip: &IpAddr, url: &str) -> Result<(), String> {
    if is_public_ip(ip) {
        Ok(())
    } else {
        Err(format!(
            "base_url `{url}` targets a non-public address ({ip})"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn ok(provider: Option<&str>, url: &str) -> bool {
        validate_base_url(provider, url).await.is_ok()
    }

    #[tokio::test]
    async fn scheme_and_shape_rules() {
        assert!(!ok(None, "ftp://example.com/x").await);
        assert!(!ok(Some("openai"), "ftp://8.8.8.8").await);
        assert!(!ok(Some("openai"), "not a url").await);
        assert!(!ok(Some("openai"), "http:///pathonly").await);
        // Shape-only passes for a provider-less config even on private
        // targets (BR3.2 — no consumer).
        assert!(ok(None, "http://127.0.0.1:11434").await);
        assert!(ok(None, "http://10.1.2.3").await);
    }

    #[tokio::test]
    async fn ollama_self_host_exemption() {
        for url in [
            "http://127.0.0.1:11434",
            "http://localhost:11434",
            "http://192.168.1.50:11434",
            "http://[::1]:11434",
        ] {
            assert!(ok(Some("ollama"), url).await, "{url}");
        }
        // Exemption covers targets, not shape — bad URLs still reject.
        assert!(!ok(Some("ollama"), "ftp://127.0.0.1").await);
    }

    #[tokio::test]
    async fn literal_ipv4_forms_rejected_for_hosted_providers() {
        for url in [
            "http://127.0.0.1/",       // dotted loopback
            "http://0x7f000001/",      // hex loopback — WHATWG normalizes
            "http://0177.0.0.1/",      // octal loopback
            "http://2130706433/",      // bare-integer loopback
            "http://10.0.0.9/",        // private
            "http://172.16.5.5/",      // private
            "http://192.168.0.1/",     // private
            "http://169.254.169.254/", // link-local cloud metadata
            "http://100.64.7.7/",      // CGNAT
            "http://192.0.2.1/",       // TEST-NET-1 documentation
            "http://198.51.100.9/",    // TEST-NET-2
            "http://198.18.0.1/",      // benchmarking
            "http://0.0.0.0/",         // unspecified
            "http://224.0.0.1/",       // multicast
            "http://240.1.2.3/",       // reserved
            "http://255.255.255.255/", // broadcast
        ] {
            assert!(!ok(Some("openai"), url).await, "{url}");
            assert!(!ok(Some("anthropic"), url).await, "{url}");
        }
    }

    #[tokio::test]
    async fn literal_ipv6_forms_rejected() {
        for url in [
            "http://[::1]/",              // loopback
            "http://[::]/",               // unspecified
            "http://[fe80::1]/",          // link-local
            "http://[fd00::1]/",          // unique-local
            "http://[2001:db8::1]/",      // documentation
            "http://[::ffff:127.0.0.1]/", // v4-mapped loopback
            "http://[ff02::1]/",          // multicast
            "http://[64:ff9b::7f00:1]/",  // NAT64 embedding 127.0.0.1
            "http://[2002:7f00:1::]/",    // 6to4 embedding 127.0.0.1
            "http://[2001::7f00:1]/",     // Teredo-embedded target
            "http://[1::]/",              // 0001::-00ff:: reserved (inside ::/8)
        ] {
            assert!(!ok(Some("openai"), url).await, "{url}");
        }
    }

    #[tokio::test]
    async fn public_literals_accepted() {
        assert!(ok(Some("openai"), "https://8.8.8.8/v1").await);
        assert!(ok(Some("anthropic"), "https://1.1.1.1").await);
        assert!(ok(Some("openai"), "https://[2606:4700:4700::1111]").await);
    }

    #[tokio::test]
    async fn dns_names_resolve_and_every_answer_must_be_public() {
        // `localhost` resolves loopback — the DNS path runs (no external
        // DNS needed) and every answer fails the public bound.
        assert!(!ok(Some("openai"), "http://localhost:1234").await);
        // Same name is shape-only fine for ollama and provider-less.
        assert!(ok(Some("ollama"), "http://localhost:1234").await);
        assert!(ok(None, "http://localhost:1234").await);
        // A name that cannot resolve fails closed.
        assert!(!ok(Some("openai"), "http://nonexistent.invalid.").await);
    }
}
