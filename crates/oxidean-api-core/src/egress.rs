//! Shared outbound-egress (SSRF) policy for user-supplied destinations:
//! webhook delivery, git mirrors, and repository migration.
//!
//! `production`/`cloud` deployments require every egress target to be
//! publicly routable — literal IPs and every DNS answer alike. Other
//! environments allow private targets so Compose/dev stacks can reach
//! sibling services by name. `OXIDEAN_EGRESS_ALLOW_PRIVATE` relaxes the
//! bound on any environment (documented self-host escape hatch; never
//! set on a hosted deployment).
//!
//! `resolve_egress_host` resolves DNS once and returns the validated
//! socket addresses so HTTP callers can pin the connection
//! (`reqwest::ClientBuilder::resolve_to_addrs`), closing the TOCTOU
//! rebind window between validation and connect. Git transports shell
//! out to `git`, which re-resolves on its own — the create/update/sync
//! checks still reject statically-private targets, and the residual
//! rebind bound is documented in CONFIGURATION.md.

use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

use oxidean_core::net::is_public_ip;

/// Upper bound on an egress validation DNS lookup — a stall must never
/// hang a request or delivery worker.
const DNS_LOOKUP_TIMEOUT: Duration = Duration::from_secs(10);

/// `1`/`true` env-var parser shared by egress and trusted-proxy flags.
pub fn env_flag(name: &str) -> bool {
    std::env::var(name).is_ok_and(|v| v == "1" || v.eq_ignore_ascii_case("true"))
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
        if private_egress_allowed(env_name) || is_public_ip(&ip) {
            return Ok(vec![SocketAddr::new(ip, port)]);
        }
        return Err(format!("egress target `{host}` is not publicly routable"));
    }
    let addrs: Vec<SocketAddr> =
        match tokio::time::timeout(DNS_LOOKUP_TIMEOUT, tokio::net::lookup_host((host, port))).await {
            Ok(Ok(iter)) => iter.collect(),
            Ok(Err(e)) => return Err(format!("egress host `{host}` does not resolve: {e}")),
            Err(_) => return Err(format!("egress host `{host}` DNS lookup timed out")),
        };
    if addrs.is_empty() {
        return Err(format!("egress host `{host}` resolves to no addresses"));
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
pub fn git_remote_host_port(remote: &str) -> Result<(String, u16), String> {
    if let Ok(parsed) = url::Url::parse(remote) {
        let host = parsed
            .host_str()
            .ok_or_else(|| format!("remote `{remote}` has no host"))?
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
    fn remote_host_port_rejects_garbage() {
        assert!(git_remote_host_port("").is_err());
        assert!(git_remote_host_port("file:///etc/passwd").is_err());
        assert!(git_remote_host_port("/abs/path").is_err());
    }

    #[tokio::test]
    async fn private_targets_allowed_outside_hosted_envs() {
        // A literal private target resolves without DNS.
        assert!(
            resolve_egress_host("10.0.0.9", 443, "test")
                .await
                .is_ok()
        );
        assert!(
            resolve_egress_host("10.0.0.9", 443, "production")
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn hosted_env_rejects_loopback_dns_names() {
        // `localhost` always resolves loopback — the DNS path runs with
        // no external resolver needed.
        assert!(
            resolve_egress_host("localhost", 443, "production")
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn non_resolving_names_fail_closed() {
        assert!(
            resolve_egress_host("nonexistent.invalid.", 443, "test")
                .await
                .is_err()
        );
    }
}
