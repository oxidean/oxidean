//! Instance PAT expiration policy (env defaults + admin overrides).
//!
//! Resolution order per field mirrors `instance_mcp_settings` /
//! `instance_lfs_settings`: a non-NULL `instance_pat_settings` column wins,
//! otherwise the `OXIDEAN_PAT_*` env var, otherwise the built-in default.

use chrono::{DateTime, Duration, Utc};
use oxidean_core::AppError;
use oxidean_db::Database;

/// Built-in fallback for `OXIDEAN_PAT_DEFAULT_TTL_DAYS` (90 days).
pub const DEFAULT_TTL_DAYS: u32 = 90;
/// Built-in fallback for `OXIDEAN_PAT_MAX_TTL_DAYS` (365 days).
pub const MAX_TTL_DAYS: u32 = 365;
/// Built-in fallback for `OXIDEAN_PAT_ALLOW_NO_EXPIRY` (disabled).
pub const ALLOW_NO_EXPIRY: bool = false;

/// Effective policy after env defaults and stored admin overrides.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PatPolicy {
    pub default_ttl_days: u32,
    pub max_ttl_days: u32,
    pub allow_never_expire: bool,
}

fn env_u32(name: &str, default: u32) -> u32 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.trim().parse::<u32>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(default)
}

fn env_bool(name: &str, default: bool) -> bool {
    match std::env::var(name).ok().map(|v| v.trim().to_lowercase()) {
        Some(v) if matches!(v.as_str(), "1" | "true" | "yes" | "on") => true,
        Some(v) if matches!(v.as_str(), "0" | "false" | "no" | "off") => false,
        _ => default,
    }
}

/// Policy from env alone (used as the base layer before DB overrides).
pub fn env_policy() -> PatPolicy {
    PatPolicy {
        default_ttl_days: env_u32("OXIDEAN_PAT_DEFAULT_TTL_DAYS", DEFAULT_TTL_DAYS),
        max_ttl_days: env_u32("OXIDEAN_PAT_MAX_TTL_DAYS", MAX_TTL_DAYS),
        allow_never_expire: env_bool("OXIDEAN_PAT_ALLOW_NO_EXPIRY", ALLOW_NO_EXPIRY),
    }
}

/// Env + stored overrides → effective policy. Falls back to env-only when the
/// settings row/table is unavailable (pre-migration or db-less contexts).
pub async fn effective_policy(db: &Database) -> PatPolicy {
    let mut p = env_policy();
    if let Ok(row) = db.get_pat_settings().await {
        if let Some(v) = row.default_ttl_days {
            if v > 0 {
                p.default_ttl_days = v as u32;
            }
        }
        if let Some(v) = row.max_ttl_days {
            if v > 0 {
                p.max_ttl_days = v as u32;
            }
        }
        if let Some(v) = row.allow_never_expire {
            p.allow_never_expire = v;
        }
    }
    // default must never exceed max — clamp so an admin cannot lock creates.
    p.default_ttl_days = p.default_ttl_days.min(p.max_ttl_days);
    p
}

/// Resolve a create request's `expires_at` to the stored value under policy.
///
/// - `None`/blank → default TTL from now.
/// - `"never"` → `None` (no expiry) when policy allows, else `rpc.bad_input`.
/// - RFC3339 → must be future and within `max_ttl_days` of now.
///
/// Returns the canonical RFC3339 string to persist (`None` = never expires).
pub fn resolve_expires_at(
    policy: &PatPolicy,
    expires_at: Option<&str>,
) -> Result<Option<String>, AppError> {
    let Some(raw) = expires_at.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(Some(
            (Utc::now() + Duration::days(i64::from(policy.default_ttl_days)))
                .to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        ));
    };
    if raw.eq_ignore_ascii_case("never") {
        if !policy.allow_never_expire {
            return Err(AppError::new(
                "pat.expiry_forbidden",
                "this instance requires personal access tokens to expire",
            ));
        }
        return Ok(None);
    }
    let dt = DateTime::parse_from_rfc3339(raw)
        .map_err(|_| {
            AppError::new(
                "rpc.bad_input",
                "expires_at must be a valid RFC3339 timestamp or \"never\"",
            )
        })?
        .with_timezone(&Utc);
    let now = Utc::now();
    if dt <= now {
        return Err(AppError::new(
            "rpc.bad_input",
            "expires_at must be in the future",
        ));
    }
    if dt > now + Duration::days(i64::from(policy.max_ttl_days)) {
        return Err(AppError::new(
            "pat.expiry_forbidden",
            format!(
                "expires_at must be within {} days on this instance",
                policy.max_ttl_days
            ),
        ));
    }
    Ok(Some(
        dt.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
    ))
}

/// Seconds until `expires_at` (0 when `None`) — for the OAuth device exchange's
/// `expires_in` response field.
pub fn secs_until(expires_at: Option<&str>) -> i64 {
    expires_at
        .and_then(|raw| DateTime::parse_from_rfc3339(raw).ok())
        .map(|dt| (dt.with_timezone(&Utc) - Utc::now()).num_seconds().max(0))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy() -> PatPolicy {
        PatPolicy {
            default_ttl_days: 90,
            max_ttl_days: 365,
            allow_never_expire: false,
        }
    }

    #[test]
    fn omitted_expiry_uses_default_ttl() {
        let resolved = resolve_expires_at(&policy(), None).unwrap().unwrap();
        let dt = DateTime::parse_from_rfc3339(&resolved).unwrap();
        let delta = dt.with_timezone(&Utc) - Utc::now();
        assert!(delta.num_days() >= 89 && delta.num_days() <= 90);
    }

    #[test]
    fn never_requires_policy_permission() {
        assert!(resolve_expires_at(&policy(), Some("never")).is_err());
        let mut p = policy();
        p.allow_never_expire = true;
        assert_eq!(resolve_expires_at(&p, Some("never")).unwrap(), None);
        assert_eq!(resolve_expires_at(&p, Some(" NEVER ")).unwrap(), None);
    }

    #[test]
    fn explicit_expiry_must_be_future_and_capped() {
        let p = policy();
        let past = (Utc::now() - Duration::days(1)).to_rfc3339();
        assert!(resolve_expires_at(&p, Some(&past)).is_err());
        let ok = (Utc::now() + Duration::days(30)).to_rfc3339();
        assert!(resolve_expires_at(&p, Some(&ok)).is_ok());
        let too_far = (Utc::now() + Duration::days(400)).to_rfc3339();
        assert!(resolve_expires_at(&p, Some(&too_far)).is_err());
        assert!(resolve_expires_at(&p, Some("not-a-date")).is_err());
    }
}
