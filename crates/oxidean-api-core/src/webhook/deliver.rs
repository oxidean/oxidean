//! Outbound webhook HTTP delivery — HMAC, Hookshot headers, SSRF (D-HOOK-04/12/18).

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Instant;

use oxidean_core::net::{is_loopback_ip, is_public_ip};
use oxidean_db::{Database, WebhookDeliveryRow, WebhookRow};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::egress::{private_egress_allowed, resolve_egress_host};

/// Cloud metadata endpoint — blocked in every environment, including
/// dev/test where private egress is otherwise allowed.
const METADATA_IP: Ipv4Addr = Ipv4Addr::new(169, 254, 169, 254);

/// Default max delivery attempts (D-HOOK-14 discretion).
pub fn max_attempts() -> i64 {
    std::env::var("OXIDEAN_WEBHOOK_MAX_ATTEMPTS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(5)
}

pub fn delivery_timeout_secs() -> u64 {
    std::env::var("OXIDEAN_WEBHOOK_TIMEOUT_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10)
}

/// HMAC-SHA256 over `message` with `key` (RFC 2104) using existing `sha2`.
pub fn hmac_sha256_hex(key: &[u8], message: &[u8]) -> String {
    const BLOCK: usize = 64;
    let mut key_block = [0u8; BLOCK];
    if key.len() > BLOCK {
        let hashed = Sha256::digest(key);
        key_block[..hashed.len()].copy_from_slice(&hashed);
    } else {
        key_block[..key.len()].copy_from_slice(key);
    }
    let mut ipad = [0x36u8; BLOCK];
    let mut opad = [0x5cu8; BLOCK];
    for i in 0..BLOCK {
        ipad[i] ^= key_block[i];
        opad[i] ^= key_block[i];
    }
    let mut inner = Sha256::new();
    inner.update(ipad);
    inner.update(message);
    let inner_hash = inner.finalize();
    let mut outer = Sha256::new();
    outer.update(opad);
    outer.update(inner_hash);
    hex_encode(&outer.finalize())
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0xf) as usize] as char);
    }
    out
}

/// Strip the brackets `url::Url::host_str` keeps around IPv6 literals
/// so the host parses as `IpAddr`. Domains are unaffected.
fn unbracket_host(host: &str) -> &str {
    host.trim_start_matches('[').trim_end_matches(']')
}

/// Validate webhook URL: https required except loopback http in non-production (D-HOOK-04).
///
/// Literal-IP and host-name rules share `oxidean_core::net`: hosted
/// deployments require publicly routable targets, other environments
/// follow [`private_egress_allowed`]. The cloud metadata address is
/// blocked everywhere. Domain hosts additionally get a resolved-IP
/// check at connect time via `resolve_webhook_target`.
pub fn validate_webhook_url(url: &str, env_name: &str) -> Result<(), String> {
    let parsed = url::Url::parse(url).map_err(|_| "invalid webhook URL".to_string())?;
    let scheme = parsed.scheme();
    let host = parsed.host_str().unwrap_or("").to_ascii_lowercase();
    let bare = unbracket_host(&host);
    let allow_loopback_http = env_name != "production" && env_name != "cloud";
    match scheme {
        "https" => {}
        "http" if allow_loopback_http && is_loopback_host(&host) => {}
        "http" => {
            return Err(
                "webhook URL must use https (http only allowed for localhost in dev/test)".into(),
            )
        }
        _ => return Err("webhook URL scheme must be https".into()),
    }
    if is_blocked_host(&host) {
        return Err("webhook URL host is not allowed".into());
    }
    if let Ok(ip) = bare.parse::<IpAddr>() {
        if ip == IpAddr::V4(METADATA_IP) {
            return Err("webhook URL host is not allowed".into());
        }
        if !private_egress_allowed(env_name) && !is_public_ip(&ip) {
            return Err("webhook URL host is not allowed".into());
        }
    }
    Ok(())
}

fn is_loopback_host(host: &str) -> bool {
    host == "localhost"
        || unbracket_host(host)
            .parse::<IpAddr>()
            .is_ok_and(|ip| is_loopback_ip(&ip))
}

fn is_blocked_host(host: &str) -> bool {
    host.is_empty() || host == "metadata" || host == "metadata.google.internal"
}

/// Resolve the webhook host once and enforce the egress bound on every
/// DNS answer. The returned addresses are pinned on the reqwest client
/// so the connection cannot rebind to a different (unvalidated) answer
/// between this check and connect.
async fn resolve_webhook_target(
    url: &str,
    env_name: &str,
) -> Result<(bool, Vec<SocketAddr>), String> {
    let parsed = url::Url::parse(url).map_err(|_| "invalid webhook URL".to_string())?;
    let host = parsed.host_str().unwrap_or("");
    let port = parsed.port_or_known_default().unwrap_or(443);
    let addrs = resolve_egress_host(unbracket_host(host), port, env_name).await?;
    let is_domain = matches!(parsed.host(), Some(url::Host::Domain(_)));
    Ok((is_domain, addrs))
}

pub struct DeliveryOutcome {
    pub http_status: Option<i32>,
    pub error_message: Option<String>,
    pub duration_ms: i64,
    pub response_snippet: Option<String>,
    pub success: bool,
    pub transient: bool,
}

fn is_transient_status(status: i32) -> bool {
    status == 429 || (500..600).contains(&status)
}

fn backoff_secs(attempt_number: i64) -> i64 {
    // 2, 4, 8, 16, 32 …
    2i64.pow(attempt_number.clamp(1, 5) as u32)
}

fn next_attempt_iso(attempt_number: i64) -> String {
    let secs = backoff_secs(attempt_number);
    (chrono::Utc::now() + chrono::Duration::seconds(secs))
        .format("%Y-%m-%dT%H:%M:%SZ")
        .to_string()
}

/// POST one delivery attempt; records attempt row and updates delivery status / retry.
pub async fn deliver_once(
    db: &Database,
    hook: &WebhookRow,
    delivery: &WebhookDeliveryRow,
    env_name: &str,
) -> DeliveryOutcome {
    let body = delivery.payload_json.as_bytes();
    let sig = hmac_sha256_hex(hook.secret.as_bytes(), body);
    let signature_header = format!("sha256={sig}");
    let started = Instant::now();
    let attempt_number = delivery.attempt_count + 1;
    let max = max_attempts();

    let target = match validate_webhook_url(&hook.url, env_name) {
        Ok(()) => resolve_webhook_target(&hook.url, env_name).await,
        Err(e) => Err(e),
    };
    let (is_domain, resolved_addrs) = match target {
        Ok(t) => t,
        Err(e) => {
            return record_and_finish(
                db,
                delivery,
                attempt_number,
                max,
                DeliveryOutcome {
                    http_status: None,
                    error_message: Some(e),
                    duration_ms: started.elapsed().as_millis() as i64,
                    response_snippet: None,
                    success: false,
                    transient: false,
                },
            )
            .await;
        }
    };

    let mut builder = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(delivery_timeout_secs()))
        .redirect(reqwest::redirect::Policy::none());
    if is_domain {
        if let Some(domain) = url::Url::parse(&hook.url)
            .ok()
            .and_then(|u| u.domain().map(str::to_owned))
        {
            builder = builder.resolve_to_addrs(&domain, &resolved_addrs);
        }
    }
    let client = match builder.build() {
        Ok(c) => c,
        Err(e) => {
            return record_and_finish(
                db,
                delivery,
                attempt_number,
                max,
                DeliveryOutcome {
                    http_status: None,
                    error_message: Some(format!("http client: {e}")),
                    duration_ms: started.elapsed().as_millis() as i64,
                    response_snippet: None,
                    success: false,
                    transient: false,
                },
            )
            .await;
        }
    };

    let req = client
        .post(&hook.url)
        .header("Content-Type", "application/json")
        .header("User-Agent", "Oxidean-Hookshot/1.0")
        .header("X-GitHub-Event", &delivery.event)
        .header("X-GitHub-Delivery", &delivery.delivery_guid)
        .header("X-GitHub-Hook-ID", &hook.id)
        .header("X-Hub-Signature-256", &signature_header)
        .body(delivery.payload_json.clone());

    let outcome = match req.send().await {
        Ok(res) => {
            let status = res.status().as_u16() as i32;
            let snippet = match res.bytes().await {
                Ok(bytes) => {
                    let take = bytes.len().min(512);
                    Some(String::from_utf8_lossy(&bytes[..take]).into_owned())
                }
                Err(_) => None,
            };
            let success = (200..300).contains(&status);
            DeliveryOutcome {
                http_status: Some(status),
                error_message: if success {
                    None
                } else {
                    Some(format!("HTTP {status}"))
                },
                duration_ms: started.elapsed().as_millis() as i64,
                response_snippet: snippet,
                success,
                transient: !success && is_transient_status(status),
            }
        }
        Err(e) => {
            let msg = if e.is_timeout() {
                format!("timeout: {e}")
            } else {
                format!("request failed: {e}")
            };
            DeliveryOutcome {
                http_status: None,
                error_message: Some(msg),
                duration_ms: started.elapsed().as_millis() as i64,
                response_snippet: None,
                success: false,
                transient: true,
            }
        }
    };

    record_and_finish(db, delivery, attempt_number, max, outcome).await
}

async fn record_and_finish(
    db: &Database,
    delivery: &WebhookDeliveryRow,
    attempt_number: i64,
    max: i64,
    outcome: DeliveryOutcome,
) -> DeliveryOutcome {
    let attempt_id = Uuid::new_v4().to_string();
    let _ = db
        .insert_webhook_delivery_attempt(
            &attempt_id,
            &delivery.id,
            attempt_number,
            outcome.http_status,
            outcome.error_message.as_deref(),
            Some(outcome.duration_ms),
            outcome.response_snippet.as_deref(),
        )
        .await;

    if outcome.success {
        let _ = db
            .mark_webhook_delivery_result(&delivery.id, "success", attempt_number, None)
            .await;
    } else if outcome.transient && attempt_number < max {
        let next = next_attempt_iso(attempt_number);
        let _ = db
            .mark_webhook_delivery_result(&delivery.id, "pending", attempt_number, Some(&next))
            .await;
    } else {
        let _ = db
            .mark_webhook_delivery_result(&delivery.id, "failed", attempt_number, None)
            .await;
    }
    outcome
}

/// Fire-and-forget delivery after enqueue (D-HOOK-11).
pub fn spawn_deliver(db: Database, webhook_id: String, delivery_id: String, env_name: String) {
    tokio::spawn(async move {
        let Ok(hook) = db.get_webhook(&webhook_id).await else {
            return;
        };
        let Ok(delivery) = db.get_webhook_delivery(&delivery_id).await else {
            return;
        };
        let _ = deliver_once(&db, &hook, &delivery, &env_name).await;
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hmac_matches_known_vector() {
        let sig = hmac_sha256_hex(b"secret", b"hello");
        assert_eq!(
            sig,
            "88aab3ede8d3adf94d26ab90d3bafd4a2083070c3bcce9c014ee04a443847c0b"
        );
    }

    #[test]
    fn url_policy_https_ok() {
        assert!(validate_webhook_url("https://example.com/hook", "production").is_ok());
    }

    #[test]
    fn url_policy_http_localhost_dev() {
        assert!(validate_webhook_url("http://127.0.0.1:9/hook", "development").is_ok());
        assert!(validate_webhook_url("http://example.com/hook", "development").is_err());
        assert!(validate_webhook_url("http://127.0.0.1:9/hook", "production").is_err());
    }

    #[test]
    fn url_policy_blocks_metadata_everywhere() {
        for env in ["development", "production", "cloud"] {
            assert!(
                validate_webhook_url("https://169.254.169.254/", env).is_err(),
                "{env}"
            );
        }
    }

    #[test]
    fn url_policy_private_literals_env_gated() {
        // Dev/test allows private egress so local services can receive
        // hooks; hosted deployments reject every non-public literal.
        assert!(validate_webhook_url("https://10.0.0.1/hook", "development").is_ok());
        assert!(validate_webhook_url("https://192.168.1.1/hook", "development").is_ok());
        for env in ["production", "cloud"] {
            assert!(
                validate_webhook_url("https://10.0.0.1/hook", env).is_err(),
                "{env}"
            );
            assert!(
                validate_webhook_url("https://192.168.1.1/hook", env).is_err(),
                "{env}"
            );
            assert!(
                validate_webhook_url("https://169.254.1.1/hook", env).is_err(),
                "{env}"
            );
            assert!(
                validate_webhook_url("https://[::ffff:10.0.0.1]/hook", env).is_err(),
                "{env}"
            );
            assert!(
                validate_webhook_url("https://[fd00::1]/hook", env).is_err(),
                "{env}"
            );
            assert!(
                validate_webhook_url("https://[::1]/hook", env).is_err(),
                "{env}"
            );
        }
    }

    #[test]
    fn url_policy_blocks_metadata_names() {
        assert!(validate_webhook_url("https://metadata/hook", "production").is_err());
        assert!(
            validate_webhook_url("https://metadata.google.internal/hook", "production").is_err()
        );
    }
}
