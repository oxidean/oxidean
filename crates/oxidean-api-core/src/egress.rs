//! Shared outbound-egress (SSRF) policy for user-supplied destinations:
//! webhook delivery, git mirrors, and repository migration.
//!
//! `production`/`cloud` deployments require every egress target to be
//! publicly routable — literal IPs and every DNS answer alike. Other
//! environments allow private targets so Compose/dev stacks can reach
//! sibling services by name. `OXIDEAN_EGRESS_ALLOW_PRIVATE` relaxes the
//! bound on any environment (documented self-host escape hatch; never
//! set on a hosted deployment). Cloud metadata endpoints stay blocked in
//! every environment — private egress never reaches them.
//!
//! `resolve_egress_host` resolves DNS once and returns the validated
//! socket addresses so callers can pin the connection
//! (`reqwest::ClientBuilder::resolve_to_addrs` for webhooks,
//! `http.curloptResolve` for HTTP(S) git remotes), closing the TOCTOU
//! rebind window between validation and connect. Git's SSH transport
//! resolves inside OpenSSH on its own — the residual rebind bound for
//! `ssh://`/`git@` remotes is documented in CONFIGURATION.md.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::time::Duration;

use oxidean_core::net::is_public_ip;

/// Upper bound on an egress validation DNS lookup — a stall must never
/// hang a request or delivery worker.
const DNS_LOOKUP_TIMEOUT: Duration = Duration::from_secs(10);

/// `1`/`true` env-var parser shared by egress and trusted-proxy flags.
pub fn env_flag(name: &str) -> bool {
    std::env::var(name).is_ok_and(|v| v == "1" || v.eq_ignore_ascii_case("true"))
}

/// `1`/`true`/`yes` parser for `OXIDEAN_WEB_BEHIND_PROXY` — the documented
/// flag set is wider than [`env_flag`] accepts; the stricter parser stays
/// for `OXIDEAN_EGRESS_ALLOW_PRIVATE`.
pub(crate) fn proxy_env_flag(name: &str) -> bool {
    std::env::var(name).is_ok_and(|v| {
        let v = v.trim();
        v == "1" || v.eq_ignore_ascii_case("true") || v.eq_ignore_ascii_case("yes")
    })
}

/// Strip the brackets `url::Url::host_str` keeps around IPv6 literals
/// so the host parses as `IpAddr`. Domains are unaffected.
pub(crate) fn unbracket_host(host: &str) -> &str {
    host.trim_start_matches('[').trim_end_matches(']')
}

/// Cloud metadata endpoints — blocked in every environment, including
/// dev/test where private egress is otherwise allowed. Normalizes
/// IPv4-mapped IPv6 (`::ffff:169.254.169.254`) and covers the AWS IPv6
/// metadata address.
pub(crate) fn is_metadata_ip(ip: &IpAddr) -> bool {
    const V4: Ipv4Addr = Ipv4Addr::new(169, 254, 169, 254);
    const AWS_V6: Ipv6Addr = Ipv6Addr::new(0xfd00, 0x0ec2, 0, 0, 0, 0, 0, 0x0254);
    match ip {
        IpAddr::V4(v4) => *v4 == V4,
        IpAddr::V6(v6) => v6.to_ipv4_mapped() == Some(V4) || *v6 == AWS_V6,
    }
}

/// `OXIDEAN_ENV` value — mirrors how `AppState.env_name` is seeded at
/// boot, for egress checks on paths with no `RpcCtx` (background
/// workers such as the mirror sync queue).
pub fn env_name() -> String {
    std::env::var("OXIDEAN_ENV").unwrap_or_else(|_| "development".into())
}

/// Private egress targets are allowed outside hosted environments, or
/// anywhere when the operator opts in via `OXIDEAN_EGRESS_ALLOW_PRIVATE`.
pub fn private_egress_allowed(env_name: &str) -> bool {
    if env_flag("OXIDEAN_EGRESS_ALLOW_PRIVATE") {
        return true;
    }
    env_name != "production" && env_name != "cloud"
}

/// Resolve `host` for `port` and enforce the egress IP bound on every
/// answer. Returns the validated socket addresses so the caller can pin
/// the connection and skip a second (rebindable) lookup.
pub async fn resolve_egress_host(
    host: &str,
    port: u16,
    env_name: &str,
) -> Result<Vec<SocketAddr>, String> {
    if let Ok(ip) = host.parse::<IpAddr>() {
        if is_metadata_ip(&ip) {
            return Err(format!("egress target `{host}` is not allowed"));
        }
        if private_egress_allowed(env_name) || is_public_ip(&ip) {
            return Ok(vec![SocketAddr::new(ip, port)]);
        }
        return Err(format!("egress target `{host}` is not publicly routable"));
    }
    let addrs: Vec<SocketAddr> =
        match tokio::time::timeout(DNS_LOOKUP_TIMEOUT, tokio::net::lookup_host((host, port))).await
        {
            Ok(Ok(iter)) => iter.collect(),
            Ok(Err(e)) => return Err(format!("egress host `{host}` does not resolve: {e}")),
            Err(_) => return Err(format!("egress host `{host}` DNS lookup timed out")),
        };
    if addrs.is_empty() {
        return Err(format!("egress host `{host}` resolves to no addresses"));
    }
    // Metadata stays blocked even where private egress is allowed.
    if addrs.iter().any(|a| is_metadata_ip(&a.ip())) {
        return Err(format!(
            "egress host `{host}` resolves to a blocked address"
        ));
    }
    if !private_egress_allowed(env_name) {
        for addr in &addrs {
            if !is_public_ip(&addr.ip()) {
                return Err(format!(
                    "egress host `{host}` resolves to a non-public address ({})",
                    addr.ip()
                ));
            }
        }
    }
    Ok(addrs)
}

/// Extract `(host, port)` from a git remote URL accepted by
/// `oxidean_git::validate_remote_url` — `http(s)://` and `ssh://` parse
/// through `url::Url` (which also normalizes exotic literal notations),
/// SCP-style `git@host:path` through `oxidean_git::ssh_host_from_remote_url`.
/// `host_str` keeps brackets on IPv6 literals — stripped here so the host
/// parses as `IpAddr`.
pub fn git_remote_host_port(remote: &str) -> Result<(String, u16), String> {
    if let Ok(parsed) = url::Url::parse(remote) {
        let host = unbracket_host(
            parsed
                .host_str()
                .ok_or_else(|| format!("remote `{remote}` has no host"))?,
        )
        .to_string();
        let port = parsed
            .port()
            .or_else(|| match parsed.scheme() {
                "ssh" => Some(22),
                _ => parsed.port_or_known_default(),
            })
            .ok_or_else(|| format!("remote `{remote}` has no port"))?;
        return Ok((host, port));
    }
    match oxidean_git::ssh_host_from_remote_url(remote) {
        Ok(host) => Ok((host, 22)),
        Err(e) => Err(format!("remote host: {e}")),
    }
}

/// Enforce the egress bound on a git remote — used by mirror
/// upsert/sync and repository migration.
pub async fn validate_remote_egress(remote: &str, env_name: &str) -> Result<(), String> {
    let (host, port) = git_remote_host_port(remote)?;
    resolve_egress_host(&host, port, env_name).await.map(|_| ())
}

/// Egress check + DNS pin for a git remote: validates the host and every
/// DNS answer, and for HTTP(S) remotes with a domain host returns the
/// [`oxidean_git::RemoteDnsPin`] git transports must apply
/// (`http.curloptResolve`) so they cannot re-resolve to an unvalidated
/// answer between this check and connect. Literal-IP targets have no DNS
/// answer to pin; SSH remotes resolve inside OpenSSH (residual bound).
pub async fn resolve_remote_egress(
    remote: &str,
    env_name: &str,
) -> Result<Option<oxidean_git::RemoteDnsPin>, String> {
    let (host, port) = git_remote_host_port(remote)?;
    let addrs = resolve_egress_host(&host, port, env_name).await?;
    if !remote.to_ascii_lowercase().starts_with("http") || host.parse::<IpAddr>().is_ok() {
        return Ok(None);
    }
    Ok(Some(oxidean_git::RemoteDnsPin {
        host,
        port,
        addrs: addrs.iter().map(|a| a.ip()).collect(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_host_port_parses_all_schemes() {
        assert_eq!(
            git_remote_host_port("https://forge.example/owner/repo.git").unwrap(),
            ("forge.example".to_string(), 443)
        );
        assert_eq!(
            git_remote_host_port("http://10.0.0.5:8080/x.git").unwrap(),
            ("10.0.0.5".to_string(), 8080)
        );
        assert_eq!(
            git_remote_host_port("ssh://git@git.example:2222/o/r.git").unwrap(),
            ("git.example".to_string(), 2222)
        );
        assert_eq!(
            git_remote_host_port("ssh://git.example/o/r.git").unwrap(),
            ("git.example".to_string(), 22)
        );
        assert_eq!(
            git_remote_host_port("git@git.example:o/r.git").unwrap(),
            ("git.example".to_string(), 22)
        );
    }

    #[test]
    fn remote_host_port_unbrackets_ipv6_literals() {
        assert_eq!(
            git_remote_host_port("https://[::1]/x.git").unwrap(),
            ("::1".to_string(), 443)
        );
        assert_eq!(
            git_remote_host_port("https://[2606:4700:4700::1111]/o/r.git").unwrap(),
            ("2606:4700:4700::1111".to_string(), 443)
        );
    }

    #[test]
    fn remote_host_port_rejects_garbage() {
        assert!(git_remote_host_port("").is_err());
        assert!(git_remote_host_port("file:///etc/passwd").is_err());
        assert!(git_remote_host_port("/abs/path").is_err());
    }

    #[test]
    fn metadata_ip_normalized_across_families() {
        assert!(is_metadata_ip(&"169.254.169.254".parse().unwrap()));
        assert!(is_metadata_ip(&"::ffff:169.254.169.254".parse().unwrap()));
        assert!(is_metadata_ip(&"fd00:ec2::254".parse().unwrap()));
        assert!(!is_metadata_ip(&"169.254.169.253".parse().unwrap()));
        assert!(!is_metadata_ip(&"10.0.0.9".parse().unwrap()));
    }

    #[tokio::test]
    async fn private_targets_allowed_outside_hosted_envs() {
        // A literal private target resolves without DNS.
        assert!(resolve_egress_host("10.0.0.9", 443, "test").await.is_ok());
        assert!(resolve_egress_host("10.0.0.9", 443, "production")
            .await
            .is_err());
    }

    #[tokio::test]
    async fn metadata_blocked_even_with_private_egress() {
        // dev/test allows private egress — metadata stays blocked anyway.
        assert!(resolve_egress_host("169.254.169.254", 443, "test")
            .await
            .is_err());
        assert!(resolve_egress_host("::ffff:169.254.169.254", 443, "test")
            .await
            .is_err());
        // A DNS answer that resolves to metadata is blocked too (GCP
        // serves this name publicly; a DNS failure also errors, so the
        // assertion holds without external resolution).
        assert!(resolve_egress_host("metadata.google.internal", 443, "test")
            .await
            .is_err());
    }

    #[tokio::test]
    async fn hosted_env_rejects_loopback_dns_names() {
        // `localhost` always resolves loopback — the DNS path runs with
        // no external resolver needed.
        assert!(resolve_egress_host("localhost", 443, "production")
            .await
            .is_err());
    }

    #[tokio::test]
    async fn non_resolving_names_fail_closed() {
        assert!(resolve_egress_host("nonexistent.invalid.", 443, "test")
            .await
            .is_err());
    }
}
