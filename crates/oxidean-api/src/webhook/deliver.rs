//! Outbound webhook HTTP delivery — HMAC, Hookshot headers, SSRF (D-HOOK-04/12/18).

use std::net::IpAddr;
use std::time::Instant;

use oxidean_db::{Database, WebhookDeliveryRow, WebhookRow};
use sha2::{Digest, Sha256};
use uuid::Uuid;

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

/// Validate webhook URL: https required except loopback http in non-production (D-HOOK-04).
pub fn validate_webhook_url(url: &str, env_name: &str) -> Result<(), String> {
    let parsed = url::Url::parse(url).map_err(|_| "invalid webhook URL".to_string())?;
    let scheme = parsed.scheme();
    let host = parsed.host_str().unwrap_or("").to_ascii_lowercase();
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
    // Literal IP hosts
    if let Ok(ip) = host.parse::<IpAddr>() {
        if is_blocked_ip(ip) && !(allow_loopback_http && ip.is_loopback()) {
            return Err("webhook URL host is not allowed".into());
        }
    }
    Ok(())
}

fn is_loopback_host(host: &str) -> bool {
    host == "localhost" || host == "127.0.0.1" || host == "::1" || host == "[::1]"
}

fn is_blocked_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_unspecified()
                || v4.octets()[0] == 169 && v4.octets()[1] == 254
        }
        IpAddr::V6(v6) => v6.is_loopback() || v6.is_unique_local() || v6.is_unspecified(),
    }
}

fn is_blocked_host(host: &str) -> bool {
    if host.is_empty() {
        return true;
    }
    let blocked = [
        "169.254.169.254",
        "metadata.google.internal",
        "metadata",
        "0.0.0.0",
    ];
    if blocked.contains(&host) {
        return true;
    }
    if host.starts_with("169.254.") || host.starts_with("10.") {
        return true;
    }
    if let Some(rest) = host.strip_prefix("192.168.") {
        if !rest.is_empty() {
            return true;
        }
    }
    if let Some(rest) = host.strip_prefix("172.") {
        if let Some((octet, _)) = rest.split_once('.') {
            if let Ok(n) = octet.parse::<u8>() {
                if (16..=31).contains(&n) {
                    return true;
                }
            }
        }
    }
    false
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

    if let Err(e) = validate_webhook_url(&hook.url, env_name) {
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

    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(delivery_timeout_secs()))
        .redirect(reqwest::redirect::Policy::none())
        .build()
    {
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
    fn url_policy_blocks_metadata() {
        assert!(validate_webhook_url("https://169.254.169.254/", "development").is_err());
        assert!(validate_webhook_url("https://10.0.0.1/hook", "development").is_err());
        assert!(validate_webhook_url("https://192.168.1.1/hook", "development").is_err());
    }
}
