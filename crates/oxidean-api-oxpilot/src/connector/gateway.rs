//! `OxpilotConnectorGateway` — the concrete `ConnectorGateway` port
//! (U6). ONE state machine over `Database` + the C5 `SessionService`
//! seam: every mutating decision serializes on a per-target
//! `tokio::Mutex` (process-local, lazy-built); the claim long-poll
//! parks on a per-target `Notify` armed BEFORE the first dequeue
//! (NFR3.2's lost-wake pin); the connector-auth IP bucket and the
//! pairing-mint user bucket share one in-process `FailedAuthLimiter`.
//!
//! Audit rows are post-commit, fire-and-forget (`audit::record_with`)
//! and carry ids/kind/bounded redacted summaries — NEVER codes,
//! verifiers, hashes, raw args, env values, or unredacted output
//! (BR7.2/FR5.4).

use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, SecondsFormat, Utc};
use oxidean_api_core::audit;
use oxidean_api_core::auth::session::{bytes_to_hex, sha256_hex};
use oxidean_api_core::oxpilot::{
    denied, denied_reason, ActionView, ClaimAction, ClaimOutcome, ConnectorGateway,
    ConnectorIdentity, GrantView, HeartbeatOutcome, PairingView, RedeemedPairing, SessionService,
    TargetView, ACTION_ARGS_MAX, ACTION_GRANT_TTL_SECS, CLAIM_DEQUEUE_MAX, CLAIM_WAIT_MAX_SECS,
    CONNECTOR_CAPABILITIES, CONNECT_GRANT_TTL_SECS, HEARTBEAT_LIVENESS_WINDOW_SECS,
    PAIRING_CODE_TTL_SECS, RESULT_MAX, SUMMARY_MAX,
};
use oxidean_api_core::pat::rate_limit::FailedAuthLimiter;
use oxidean_api_core::rpc::ClientMeta;
use oxidean_core::{redact, AppError};
use oxidean_db::{
    Database, NewOxpilotAction, NewOxpilotGrant, NewOxpilotPairing, NewOxpilotTarget,
    OxpilotActionRow, OxpilotGrantRow, OxpilotTargetRow,
};
use serde_json::{json, Value};
use tokio::sync::{Mutex, Notify};
use uuid::Uuid;

/// BR1.1/BR1.2 free-text label bound — `target_name` / `device_label`
/// share one display column.
const TARGET_NAME_MAX: usize = 128;
/// Pairing-code CSPRNG size — 32 B = 256 bits ≥ BR1.1's 160-bit floor.
const PAIRING_CODE_BYTES: usize = 32;
/// Verifier CSPRNG size — 32 B = 256 bits, the BR1.2 floor.
const VERIFIER_BYTES: usize = 32;
/// `retry_after` hint on claim answers — the connector re-polls
/// immediately; the long-poll IS the push channel (BR5.5).
const CLAIM_RETRY_AFTER_SECS: u32 = 0;

fn now_rfc() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn after_secs(secs: i64) -> String {
    (Utc::now() + chrono::Duration::seconds(secs)).to_rfc3339_opts(SecondsFormat::Secs, true)
}

/// `expires_at <= now` — the store emits uniform
/// `YYYY-MM-DDTHH:MM:SSZ` strings, so lexicographic IS chronological.
fn past_due(ts: &str, now: &str) -> bool {
    ts <= now
}

/// BR5.4 — `last_seen_at` inside the 90 s window means the target is
/// still alive.
fn liveness_fresh(last_seen_at: Option<&str>) -> bool {
    let Some(ts) = last_seen_at else {
        return false;
    };
    let Ok(seen) = DateTime::parse_from_rfc3339(ts) else {
        return false;
    };
    Utc::now()
        .signed_duration_since(seen.with_timezone(&Utc))
        .num_seconds()
        < HEARTBEAT_LIVENESS_WINDOW_SECS
}

/// Byte-bounded HEAD truncation at a char boundary with an ellipsis
/// marker — `args`/`args_summary` keep the leading context (the
/// runtime.rs `truncate_marker` precedent).
fn head_bounded(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut end = max.saturating_sub('…'.len_utf8());
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &s[..end])
}

/// Byte-bounded TAIL truncation — `result_summary` keeps the ≤2 KiB
/// tail where the outcome lands (BR5.2's "≤ 2 KiB tail").
fn tail_bounded(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut start = s.len().saturating_sub(max.saturating_sub('…'.len_utf8()));
    while start < s.len() && !s.is_char_boundary(start) {
        start += 1;
    }
    format!("…{}", &s[start..])
}

fn db_err(e: String) -> AppError {
    if e == "database not configured" {
        AppError::new(
            "db.not_configured",
            "no database configured for this instance",
        )
    } else {
        tracing::error!(error = %e, "oxpilot connector gateway db error");
        AppError::new("oxpilot.internal", "connector gateway operation failed")
    }
}

fn bad_input(msg: impl Into<String>) -> AppError {
    AppError::new("bad_input", msg.into())
}

fn not_found() -> AppError {
    AppError::new("not_found", "not found")
}

fn unauthorized() -> AppError {
    AppError::new("unauthorized", "unauthorized")
}

fn grant_not_pending() -> AppError {
    AppError::new("grant_not_pending", "grant is not pending")
}

fn rate_limited(retry: Duration) -> AppError {
    AppError::new("auth.rate_limited", "too many attempts; try again later")
        .with_data(json!({ "retry_after": retry.as_secs().max(1) }))
}

fn forbidden(msg: impl Into<String>) -> AppError {
    AppError::new("forbidden", msg.into())
}

/// CSPRNG token material — hex of N random bytes. Only ever lives in
/// this function's caller until the response leaves; the hash alone
/// persists (BR7.3).
fn rand_hex<const N: usize>() -> String {
    let mut buf = [0u8; N];
    rand::fill(&mut buf);
    bytes_to_hex(&buf)
}

/// A post-commit audit event deferred until the per-target lock is
/// released — the decision paths queue these so the critical section
/// never awaits an audit write (NFR3 ordering).
struct DeferredAudit {
    user_id: Option<String>,
    event: &'static str,
    target: Option<(&'static str, String)>,
    detail: Value,
}

/// The concrete connector gateway — `db` for all connector state,
/// `sessions` for BR4.1(1)'s ownership check, `locks`/`notifies` the
/// per-target decision/wake maps, `limiter` the shared IP+user buckets.
pub struct OxpilotConnectorGateway {
    db: Database,
    sessions: Arc<dyn SessionService>,
    locks: StdMutex<HashMap<String, Arc<Mutex<()>>>>,
    notifies: StdMutex<HashMap<String, Arc<Notify>>>,
    limiter: StdMutex<FailedAuthLimiter>,
}

impl OxpilotConnectorGateway {
    pub fn new(db: Database, sessions: Arc<dyn SessionService>) -> Self {
        OxpilotConnectorGateway {
            db,
            sessions,
            locks: StdMutex::new(HashMap::new()),
            notifies: StdMutex::new(HashMap::new()),
            limiter: StdMutex::new(FailedAuthLimiter::new()),
        }
    }

    /// The per-target decision lock — dispatch, respond, lifecycle,
    /// dequeue, and the BR3.2 re-mint all serialize on it (BR4.1's
    /// "the same lock serializes"). The map's own std Mutex is never
    /// held across `.await`.
    fn lock_for(&self, target_id: &str) -> Arc<Mutex<()>> {
        self.locks
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .entry(target_id.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }

    fn notifier(&self, target_id: &str) -> Arc<Notify> {
        self.notifies
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .entry(target_id.to_string())
            .or_insert_with(|| Arc::new(Notify::new()))
            .clone()
    }

    /// Wake every parked claim on the target — fired post-commit on
    /// any transition that can change a claim answer: queued insert,
    /// cancel-set change, lifecycle kill (NFR3.x wake sources).
    fn notify_waiters(&self, target_id: &str) {
        if let Some(n) = self
            .notifies
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(target_id)
        {
            n.notify_waiters();
        }
    }

    fn limiter(&self) -> std::sync::MutexGuard<'_, FailedAuthLimiter> {
        self.limiter.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Post-commit audit — the actor id is the durable join key; the
    /// seam never sees a username, so the column stays empty rather
    /// than fabricated.
    async fn audit(
        &self,
        client: &ClientMeta,
        user_id: Option<&str>,
        event: &str,
        target: Option<(&str, &str)>,
        detail: Value,
    ) {
        audit::record_with(
            &self.db,
            client,
            user_id.map(|id| (id, "")),
            event,
            target,
            Some(detail.to_string()),
        )
        .await;
    }

    /// `TargetView` projection — `status` is derived at read (BR5.4):
    /// `revoked` lifecycle → `revoked`; `connected` + inside the
    /// liveness window → `online`; every other shape → `offline`.
    fn target_view(t: &OxpilotTargetRow) -> TargetView {
        let status = match t.status.as_str() {
            "revoked" => "revoked",
            "connected" if liveness_fresh(t.last_seen_at.as_deref()) => "online",
            _ => "offline",
        };
        TargetView {
            id: t.id.clone(),
            label: t.name.clone(),
            status: status.to_string(),
            lifecycle: t.status.clone(),
            pairing_id: t.pairing_id.clone(),
            granted_caps: serde_json::from_str(&t.caps).unwrap_or_default(),
        }
    }

    /// `GrantView` projection — `action_once`/`action_session` collapse
    /// to `action` on the wire (C1's two-member enum); `capability` is
    /// the target's caps summary on connect grants and the
    /// `action_kind` on action grants. `action_args` NEVER projects —
    /// the consent card renders `args_summary` only (NFR1.15/BR7.4).
    fn grant_view(g: &OxpilotGrantRow, target_caps: Option<&str>) -> GrantView {
        let (kind, capability) = if g.kind == "connect" {
            let caps: Vec<String> = target_caps
                .and_then(|c| serde_json::from_str(c).ok())
                .unwrap_or_default();
            ("connect".to_string(), caps.join(","))
        } else {
            (
                "action".to_string(),
                g.action_kind.clone().unwrap_or_default(),
            )
        };
        GrantView {
            id: g.id.clone(),
            kind,
            capability,
            args_summary: g.action_summary.clone(),
            session_id: g.session_id.clone(),
            state: g.state.clone(),
            expires_at: g.expires_at.clone(),
        }
    }

    /// Emit queued audits — callers invoke AFTER `drop(_g)` (the
    /// established post-lock convention).
    async fn emit_deferred(&self, client: &ClientMeta, audits: Vec<DeferredAudit>) {
        for a in audits {
            self.audit(
                client,
                a.user_id.as_deref(),
                a.event,
                a.target.as_ref().map(|(k, id)| (*k, id.as_str())),
                a.detail,
            )
            .await;
        }
    }

    /// BR3.1 lazy expiry on one observed grant — stamps `expired` on a
    /// past-due `pending` row (CAS) and queues the kind-branched audit
    /// event for post-lock emission. Returns `true` when the row was
    /// (or already is) expired.
    async fn expire_if_past_due(
        &self,
        g: &OxpilotGrantRow,
        now: &str,
        audits: &mut Vec<DeferredAudit>,
    ) -> Result<bool, AppError> {
        if g.state != "pending" || !past_due(&g.expires_at, now) {
            return Ok(false);
        }
        if self
            .db
            .expire_oxpilot_grant(&g.id, now)
            .await
            .map_err(db_err)?
            == 1
        {
            audits.push(DeferredAudit {
                user_id: Some(g.user_id.clone()),
                event: if g.kind == "connect" {
                    "oxpilot.connect_grant_expired"
                } else {
                    "oxpilot.action_grant_expired"
                },
                target: Some(("oxpilot_grant", g.id.clone())),
                detail: json!({ "grant_id": g.id, "target_id": g.target_id }),
            });
        }
        Ok(true)
    }

    /// BR3.2 — the re-consent guarantee: an owner read of a
    /// `pending_consent` target lazily mints a fresh `pending` connect
    /// grant when none is live. Past-due pendings expire first so the
    /// check under the per-target lock is truly insert-if-none-live.
    async fn ensure_live_connect_grant(
        &self,
        client: &ClientMeta,
        user_id: &str,
        t: &OxpilotTargetRow,
    ) -> Result<(), AppError> {
        let lock = self.lock_for(&t.id);
        let _g = lock.lock().await;
        // Re-verify under the held lock — the caller's `t` was read
        // before lock acquisition; a disconnect/revoke that landed in
        // between must not mint a connect grant on a dead target
        // (BR6.3's "dead targets refuse everything").
        let Some(live) = self.db.find_oxpilot_target(&t.id).await.map_err(db_err)? else {
            return Ok(());
        };
        if live.status != "pending_consent" {
            return Ok(());
        }
        let now = now_rfc();
        let mut audits = Vec::new();
        let res = async {
            let grants = self
                .db
                .list_oxpilot_grants_for_user(user_id, Some(&t.id))
                .await
                .map_err(db_err)?;
            for g in &grants {
                if g.kind == "connect" {
                    self.expire_if_past_due(g, &now, &mut audits).await?;
                }
            }
            if self
                .db
                .pending_oxpilot_connect_grant(&t.id, &now)
                .await
                .map_err(db_err)?
                .is_none()
            {
                let g = self
                    .db
                    .insert_oxpilot_grant(&NewOxpilotGrant {
                        id: &Uuid::new_v4().to_string(),
                        target_id: &t.id,
                        user_id,
                        session_id: None,
                        kind: "connect",
                        action_kind: None,
                        action_args: None,
                        action_summary: None,
                        expires_at: &after_secs(CONNECT_GRANT_TTL_SECS),
                    })
                    .await
                    .map_err(db_err)?;
                audits.push(DeferredAudit {
                    user_id: Some(user_id.to_string()),
                    event: "oxpilot.connect_grant_pending",
                    target: Some(("oxpilot_grant", g.id.clone())),
                    detail: json!({ "grant_id": g.id, "target_id": t.id }),
                });
            }
            Ok(())
        }
        .await;
        drop(_g);
        self.emit_deferred(client, audits).await;
        res
    }

    /// A pending action-grant mint (BR3.3) — the dedupe/insert arm of
    /// `dispatch_action`; kind `action` unflavored until respond.
    async fn mint_action_grant(
        &self,
        user_id: &str,
        target_id: &str,
        session_id: &str,
        kind: &str,
        stored_args: &str,
        stored_summary: &str,
    ) -> Result<OxpilotGrantRow, AppError> {
        self.db
            .insert_oxpilot_grant(&NewOxpilotGrant {
                id: &Uuid::new_v4().to_string(),
                target_id,
                user_id,
                session_id: Some(session_id),
                kind: "action",
                action_kind: Some(kind),
                action_args: Some(stored_args),
                action_summary: Some(stored_summary),
                expires_at: &after_secs(ACTION_GRANT_TTL_SECS),
            })
            .await
            .map_err(db_err)
    }

    /// The BR5.1 dequeue — runs with the per-target lock HELD: FIFO
    /// candidate scan capped at `CLAIM_DEQUEUE_MAX`; per row (a) a
    /// dead grant flips the action `cancelled` (audit queued for
    /// post-lock emit) and skips, (b) a caps miss skips only — the row
    /// stays `queued`.
    async fn dequeue(
        &self,
        identity: &ConnectorIdentity,
        claim_caps: &[String],
        audits: &mut Vec<DeferredAudit>,
    ) -> Result<Vec<ClaimAction>, AppError> {
        let now = now_rfc();
        let cands = self
            .db
            .queued_oxpilot_action_candidates(&identity.target_id, CLAIM_DEQUEUE_MAX)
            .await
            .map_err(db_err)?;
        let target_caps: Vec<String> = serde_json::from_str(&identity.caps).unwrap_or_default();
        let mut batch = Vec::new();
        for c in cands {
            if c.grant_state != "allowed" || past_due(&c.grant_expires_at, &now) {
                if self
                    .db
                    .cancel_queued_oxpilot_action(&c.action.id, &now)
                    .await
                    .map_err(db_err)?
                    == 1
                {
                    audits.push(DeferredAudit {
                        user_id: Some(identity.user_id.clone()),
                        event: "oxpilot.action_cancelled",
                        target: Some(("oxpilot_action", c.action.id.clone())),
                        detail: json!({
                            "action_id": c.action.id,
                            "target_id": identity.target_id,
                            "reason": "grant_expired",
                        }),
                    });
                }
                continue;
            }
            if !claim_caps.iter().any(|k| k == &c.action.kind)
                || !target_caps.iter().any(|k| k == &c.action.kind)
            {
                continue;
            }
            if self
                .db
                .claim_oxpilot_action(&c.action.id, &now)
                .await
                .map_err(db_err)?
                == 1
            {
                batch.push(ClaimAction {
                    action_id: c.action.id.clone(),
                    kind: c.action.kind.clone(),
                    args: c.action.args.clone(),
                    grant_id: c.action.grant_id.clone(),
                });
            }
        }
        Ok(batch)
    }

    /// Post-commit audit fan-out for a lifecycle outcome — every
    /// cancelled grant/action row owes its own event (BR7.2).
    async fn audit_lifecycle_outcome(
        &self,
        client: &ClientMeta,
        user_id: &str,
        target_id: &str,
        outcome: &oxidean_db::OxpilotTargetLifecycleOutcome,
        grant_event: &str,
        reason: &str,
    ) {
        for g in &outcome.grants {
            self.audit(
                client,
                Some(user_id),
                grant_event,
                Some(("oxpilot_grant", &g.id)),
                json!({
                    "grant_id": g.id,
                    "target_id": target_id,
                    "kind": g.kind,
                    "reason": reason,
                }),
            )
            .await;
        }
        for a in &outcome.cancelled_actions {
            self.audit(
                client,
                Some(user_id),
                "oxpilot.action_cancelled",
                Some(("oxpilot_action", &a.id)),
                json!({
                    "action_id": a.id,
                    "target_id": target_id,
                    "kind": a.kind,
                    "reason": reason,
                }),
            )
            .await;
        }
    }
}

#[async_trait]
impl ConnectorGateway for OxpilotConnectorGateway {
    // ---- U5 dispatch (C7) ----

    async fn dispatch_action(
        &self,
        client: &ClientMeta,
        user_id: &str,
        session_id: &str,
        target_id: &str,
        kind: &str,
        args: &str,
    ) -> Result<OxpilotActionRow, AppError> {
        let lock = self.lock_for(target_id);
        let _g = lock.lock().await;
        // (1) session ownership — `not_found`/`forbidden` pass through
        // as their own codes (BR4.4).
        self.sessions.get_session(user_id, session_id).await?;
        // (2) target lifecycle + liveness — absent and cross-user fold
        // into `not_connected` identically (no existence leak).
        let t = self
            .db
            .find_oxpilot_target(target_id)
            .await
            .map_err(db_err)?;
        let Some(t) = t.filter(|t| t.user_id == user_id) else {
            return Err(denied(denied_reason::NOT_CONNECTED, None));
        };
        if t.status != "connected" {
            return Err(denied(denied_reason::NOT_CONNECTED, None));
        }
        if !liveness_fresh(t.last_seen_at.as_deref()) {
            return Err(denied(denied_reason::OFFLINE, None));
        }
        // (3) capability bound.
        let caps: Vec<String> = serde_json::from_str(&t.caps).unwrap_or_default();
        if !caps.iter().any(|c| c == kind) {
            return Err(denied(denied_reason::CAPABILITY, None));
        }
        // The stored form IS the dedupe key AND the Action payload —
        // redact+bound ONCE at the ingest boundary (NFR5.4/BR7.4).
        let stored_args = head_bounded(&redact(args), ACTION_ARGS_MAX);
        let stored_summary = head_bounded(&stored_args, SUMMARY_MAX);
        let now = now_rfc();
        // (4) live `action_session` grant — the only shape that passes.
        if let Some(g) = self
            .db
            .live_oxpilot_session_grant(target_id, session_id, &now)
            .await
            .map_err(db_err)?
        {
            let action = self
                .db
                .insert_oxpilot_action(&NewOxpilotAction {
                    id: &Uuid::new_v4().to_string(),
                    target_id,
                    user_id,
                    session_id,
                    grant_id: &g.id,
                    kind,
                    args: &stored_args,
                    args_summary: &stored_summary,
                })
                .await
                .map_err(db_err)?;
            drop(_g);
            self.notify_waiters(target_id);
            self.audit(
                client,
                Some(user_id),
                "oxpilot.action_dispatched",
                Some(("oxpilot_action", &action.id)),
                json!({
                    "action_id": action.id,
                    "target_id": target_id,
                    "session_id": session_id,
                    "kind": kind,
                    "grant_id": g.id,
                    "args_summary": stored_summary,
                }),
            )
            .await;
            return Ok(action);
        }
        // (5) pending dedupe on the STORED bounded text (BR4.5) —
        // identical args join the existing card; distinct mint a new
        // one. A past-due match expires first: a consent card must
        // never receive an un-approvable grant_id (BR3.1).
        let mut audits = Vec::new();
        let mint = async {
            match self
                .db
                .pending_oxpilot_action_grant(target_id, session_id, kind, &stored_args)
                .await
                .map_err(db_err)?
            {
                Some(g) if !past_due(&g.expires_at, &now) => Ok((g, false)),
                Some(old) => {
                    self.expire_if_past_due(&old, &now, &mut audits).await?;
                    Ok((
                        self.mint_action_grant(
                            user_id,
                            target_id,
                            session_id,
                            kind,
                            &stored_args,
                            &stored_summary,
                        )
                        .await?,
                        true,
                    ))
                }
                None => Ok((
                    self.mint_action_grant(
                        user_id,
                        target_id,
                        session_id,
                        kind,
                        &stored_args,
                        &stored_summary,
                    )
                    .await?,
                    true,
                )),
            }
        }
        .await;
        if let Ok((g, true)) = &mint {
            audits.push(DeferredAudit {
                user_id: Some(user_id.to_string()),
                event: "oxpilot.action_grant_pending",
                target: Some(("oxpilot_grant", g.id.clone())),
                detail: json!({
                    "grant_id": g.id,
                    "target_id": target_id,
                    "session_id": session_id,
                    "kind": kind,
                    "args_summary": stored_summary,
                }),
            });
        }
        drop(_g);
        self.emit_deferred(client, audits).await;
        let (g, _) = mint?;
        Err(denied(denied_reason::CONSENT_REQUIRED, Some(&g.id)))
    }

    async fn action_status(
        &self,
        _client: &ClientMeta,
        user_id: &str,
        action_id: &str,
    ) -> Result<ActionView, AppError> {
        let a = self
            .db
            .find_oxpilot_action(action_id)
            .await
            .map_err(db_err)?;
        let Some(a) = a.filter(|a| a.user_id == user_id) else {
            return Err(not_found());
        };
        Ok(ActionView {
            action_id: a.id,
            kind: a.kind,
            state: a.state,
            created_at: a.created_at,
            args_summary: a.args_summary,
            result_summary: a.result_summary,
        })
    }

    // ---- owner verbs (C1 procs) ----

    async fn mint_pairing(
        &self,
        client: &ClientMeta,
        user_id: &str,
        target_name: &str,
        caps: &[String],
    ) -> Result<PairingView, AppError> {
        // BR1.1 — closed-enum bound (FR5.3): dedupe then require a
        // nonempty subset — a minted pairing can never carry a GUI cap.
        let mut granted: Vec<String> = Vec::new();
        for c in caps {
            let c = c.trim();
            if !CONNECTOR_CAPABILITIES.contains(&c) {
                return Err(bad_input(
                    "caps must be a subset of {exec_shell, fs_read, fs_write}",
                ));
            }
            if !granted.iter().any(|g| g == c) {
                granted.push(c.to_string());
            }
        }
        if granted.is_empty() {
            return Err(bad_input(
                "caps must name at least one connector capability",
            ));
        }
        let name = target_name.trim();
        if name.is_empty() || name.chars().count() > TARGET_NAME_MAX {
            return Err(bad_input(format!(
                "target_name is required and must be at most {TARGET_NAME_MAX} characters"
            )));
        }
        // BR1.5 — per-user mint limiter: check-then-record under ONE
        // guard (two concurrent mints cannot interleave a call past
        // the bucket); every call records (a check with nothing
        // recorded never trips).
        {
            let mut lim = self.limiter();
            if let Err(retry) = lim.check_user(user_id) {
                return Err(rate_limited(retry));
            }
            lim.record_user(user_id);
        }
        let code = rand_hex::<PAIRING_CODE_BYTES>();
        let code_hash = sha256_hex(code.as_bytes());
        let row = self
            .db
            .insert_oxpilot_pairing(&NewOxpilotPairing {
                id: &Uuid::new_v4().to_string(),
                user_id,
                code_hash: &code_hash,
                target_name: name,
                requested_caps: &serde_json::to_string(&granted).unwrap_or_else(|_| "[]".into()),
                expires_at: &after_secs(PAIRING_CODE_TTL_SECS),
            })
            .await
            .map_err(db_err)?;
        self.audit(
            client,
            Some(user_id),
            "oxpilot.pairing_created",
            Some(("oxpilot_pairing", &row.id)),
            json!({ "pairing_id": row.id, "target_name": name, "caps": granted }),
        )
        .await;
        // The ONE response that ever carries plaintext code material
        // (BR1.1/BR7.3) — shown once, stored hashed.
        Ok(PairingView {
            pairing_id: row.id.clone(),
            pairing_code: code,
            expires_at: row.expires_at,
        })
    }

    async fn list_targets(
        &self,
        client: &ClientMeta,
        user_id: &str,
    ) -> Result<Vec<TargetView>, AppError> {
        let rows = self
            .db
            .list_oxpilot_targets_for_user(user_id)
            .await
            .map_err(db_err)?;
        let mut views = Vec::with_capacity(rows.len());
        for t in &rows {
            if t.status == "pending_consent" {
                self.ensure_live_connect_grant(client, user_id, t).await?;
            }
            views.push(Self::target_view(t));
        }
        Ok(views)
    }

    async fn get_target(
        &self,
        client: &ClientMeta,
        user_id: &str,
        target_id: &str,
    ) -> Result<TargetView, AppError> {
        let t = self
            .db
            .find_oxpilot_target(target_id)
            .await
            .map_err(db_err)?;
        let Some(t) = t.filter(|t| t.user_id == user_id) else {
            return Err(not_found());
        };
        if t.status == "pending_consent" {
            self.ensure_live_connect_grant(client, user_id, &t).await?;
        }
        Ok(Self::target_view(&t))
    }

    async fn disconnect_target(
        &self,
        client: &ClientMeta,
        user_id: &str,
        target_id: &str,
    ) -> Result<(), AppError> {
        let t = self
            .db
            .find_oxpilot_target(target_id)
            .await
            .map_err(db_err)?;
        let Some(t) = t.filter(|t| t.user_id == user_id) else {
            return Err(not_found());
        };
        let lock = self.lock_for(&t.id);
        let _g = lock.lock().await;
        // BR6.1 — verifier tombstone + pending/allowed grants and EVERY
        // non-terminal action directly `cancelled` in one tx (the dead
        // channel makes `cancel_requested` undeliverable). `None` ⇒
        // already-terminal ⇒ idempotent `Ok` (BR6.7).
        let outcome = self
            .db
            .disconnect_oxpilot_target_tx(&t.id, &now_rfc())
            .await
            .map_err(db_err)?;
        drop(_g);
        if let Some(o) = outcome {
            self.audit(
                client,
                Some(user_id),
                "oxpilot.target_disconnected",
                Some(("oxpilot_target", &t.id)),
                json!({ "target_id": t.id }),
            )
            .await;
            self.audit_lifecycle_outcome(
                client,
                user_id,
                &t.id,
                &o,
                "oxpilot.grant_cancelled",
                "target_disconnected",
            )
            .await;
            self.notify_waiters(&t.id);
        }
        Ok(())
    }

    async fn revoke_target(
        &self,
        client: &ClientMeta,
        user_id: &str,
        target_id: &str,
    ) -> Result<(), AppError> {
        let t = self
            .db
            .find_oxpilot_target(target_id)
            .await
            .map_err(db_err)?;
        let Some(t) = t.filter(|t| t.user_id == user_id) else {
            return Err(not_found());
        };
        let lock = self.lock_for(&t.id);
        let _g = lock.lock().await;
        // BR6.2 — verifier PRESERVED (heartbeat still authenticates the
        // `revoked:true` answer); ALL grants `revoked`; queued
        // `cancelled`; claimed/running `cancel_requested` for the next
        // heartbeat's cancel set. `None` ⇒ idempotent `Ok` (BR6.7).
        let outcome = self
            .db
            .revoke_oxpilot_target_tx(&t.id, &now_rfc())
            .await
            .map_err(db_err)?;
        drop(_g);
        if let Some(o) = outcome {
            self.audit(
                client,
                Some(user_id),
                "oxpilot.target_revoked",
                Some(("oxpilot_target", &t.id)),
                json!({ "target_id": t.id }),
            )
            .await;
            self.audit_lifecycle_outcome(
                client,
                user_id,
                &t.id,
                &o,
                "oxpilot.grant_revoked",
                "target_revoked",
            )
            .await;
            self.notify_waiters(&t.id);
        }
        Ok(())
    }

    async fn list_grants(
        &self,
        client: &ClientMeta,
        user_id: &str,
        target_id: Option<&str>,
        session_id: Option<&str>,
    ) -> Result<Vec<GrantView>, AppError> {
        // BR3.2 — the owner read of a pending_consent target re-mints;
        // scoped to the queried target when one is named.
        let targets = self
            .db
            .list_oxpilot_targets_for_user(user_id)
            .await
            .map_err(db_err)?;
        let caps_by_target: HashMap<&str, &str> = targets
            .iter()
            .map(|t| (t.id.as_str(), t.caps.as_str()))
            .collect();
        for t in &targets {
            if t.status == "pending_consent"
                && (target_id.is_none() || target_id == Some(t.id.as_str()))
            {
                self.ensure_live_connect_grant(client, user_id, t).await?;
            }
        }
        let now = now_rfc();
        let mut rows = match session_id {
            // U8 (`grant_list{session_id}`) — the session-scoped owner
            // read: `get_session` resolves foreign/absent ids into
            // `not_found` FIRST (BR3.3 anti-enumeration), then the
            // session-bound rows answer; a `target_id` sent alongside
            // ANDs in at the store layer.
            Some(sid) => {
                self.sessions.get_session(user_id, sid).await?;
                self.db
                    .list_oxpilot_grants_for_session(user_id, sid, target_id)
                    .await
                    .map_err(db_err)?
            }
            None => self
                .db
                .list_oxpilot_grants_for_user(user_id, target_id)
                .await
                .map_err(db_err)?,
        };
        // BR3.1 — every observed past-due pending row stamps `expired`
        // on read, kind-branched audit.
        for g in rows.iter_mut() {
            if g.state == "pending"
                && past_due(&g.expires_at, &now)
                && self
                    .db
                    .expire_oxpilot_grant(&g.id, &now)
                    .await
                    .map_err(db_err)?
                    == 1
            {
                g.state = "expired".to_string();
                let event = if g.kind == "connect" {
                    "oxpilot.connect_grant_expired"
                } else {
                    "oxpilot.action_grant_expired"
                };
                self.audit(
                    client,
                    Some(user_id),
                    event,
                    Some(("oxpilot_grant", &g.id)),
                    json!({ "grant_id": g.id, "target_id": g.target_id }),
                )
                .await;
            }
        }
        Ok(rows
            .iter()
            .map(|g| Self::grant_view(g, caps_by_target.get(g.target_id.as_str()).copied()))
            .collect())
    }

    async fn get_grant(
        &self,
        client: &ClientMeta,
        user_id: &str,
        grant_id: &str,
    ) -> Result<GrantView, AppError> {
        let g = self.db.find_oxpilot_grant(grant_id).await.map_err(db_err)?;
        let Some(mut g) = g.filter(|g| g.user_id == user_id) else {
            return Err(not_found());
        };
        let now = now_rfc();
        if g.state == "pending"
            && past_due(&g.expires_at, &now)
            && self
                .db
                .expire_oxpilot_grant(&g.id, &now)
                .await
                .map_err(db_err)?
                == 1
        {
            g.state = "expired".to_string();
            let event = if g.kind == "connect" {
                "oxpilot.connect_grant_expired"
            } else {
                "oxpilot.action_grant_expired"
            };
            self.audit(
                client,
                Some(user_id),
                event,
                Some(("oxpilot_grant", &g.id)),
                json!({ "grant_id": g.id, "target_id": g.target_id }),
            )
            .await;
        }
        let caps = if g.kind == "connect" {
            self.db
                .find_oxpilot_target(&g.target_id)
                .await
                .map_err(db_err)?
                .map(|t| t.caps)
        } else {
            None
        };
        Ok(Self::grant_view(&g, caps.as_deref()))
    }

    async fn respond_grant(
        &self,
        client: &ClientMeta,
        user_id: &str,
        grant_id: &str,
        allow: bool,
        scope: Option<&str>,
    ) -> Result<GrantView, AppError> {
        let g = self.db.find_oxpilot_grant(grant_id).await.map_err(db_err)?;
        let Some(g) = g.filter(|g| g.user_id == user_id) else {
            return Err(not_found());
        };
        let lock = self.lock_for(&g.target_id);
        let _g = lock.lock().await;
        // Re-read under the lock — a concurrent respond/revoke on the
        // same target may have resolved the row since the pre-check.
        let g = match self.db.find_oxpilot_grant(grant_id).await.map_err(db_err)? {
            Some(g2) if g2.user_id == user_id => g2,
            _ => return Err(not_found()),
        };
        let now = now_rfc();
        // BR3.1 lazy expiry FIRST — a past-due pending grant is
        // `expired` before the write evaluates, never allow-able.
        if g.state == "pending" && past_due(&g.expires_at, &now) {
            let mut audits = Vec::new();
            self.expire_if_past_due(&g, &now, &mut audits).await?;
            drop(_g);
            self.emit_deferred(client, audits).await;
            return Err(grant_not_pending());
        }
        if g.state != "pending" {
            // BR6.4 carve-out — `deny` on an ALLOWED `action_session`
            // grant is the session-grant revoke arm.
            if g.state == "allowed" && g.kind == "action_session" && !allow {
                let Some(outcome) = self
                    .db
                    .revoke_oxpilot_session_grant_tx(&g.id, &now)
                    .await
                    .map_err(db_err)?
                else {
                    return Err(grant_not_pending());
                };
                drop(_g);
                self.audit(
                    client,
                    Some(user_id),
                    "oxpilot.grant_revoked",
                    Some(("oxpilot_grant", &g.id)),
                    json!({ "grant_id": g.id, "target_id": g.target_id }),
                )
                .await;
                for a in &outcome.cancelled_actions {
                    self.audit(
                        client,
                        Some(user_id),
                        "oxpilot.action_cancelled",
                        Some(("oxpilot_action", &a.id)),
                        json!({
                            "action_id": a.id,
                            "target_id": g.target_id,
                            "kind": a.kind,
                            "reason": "grant_revoked",
                        }),
                    )
                    .await;
                }
                self.notify_waiters(&g.target_id);
                return Ok(Self::grant_view(&outcome.grant, None));
            }
            // Resolved-on-dead-target folds here too (BR6.3) — the
            // target's lifecycle tx already moved the grant out of
            // pending/allowed.
            return Err(grant_not_pending());
        }
        if !allow {
            let n = self
                .db
                .deny_oxpilot_grant(&g.id, &now)
                .await
                .map_err(db_err)?;
            if n == 0 {
                return Err(grant_not_pending());
            }
            drop(_g);
            let event = if g.kind == "connect" {
                "oxpilot.connect_grant_denied"
            } else {
                "oxpilot.action_grant_denied"
            };
            self.audit(
                client,
                Some(user_id),
                event,
                Some(("oxpilot_grant", &g.id)),
                json!({ "grant_id": g.id, "target_id": g.target_id }),
            )
            .await;
            self.notify_waiters(&g.target_id);
            let g2 = self
                .db
                .find_oxpilot_grant(&g.id)
                .await
                .map_err(db_err)?
                .unwrap_or(g);
            return Ok(Self::grant_view(&g2, None));
        }
        if g.kind == "connect" {
            // BR3.2 — allow flips `pending_consent → connected` in the
            // same tx; `scope` is meaningless on connect (ignored).
            let won = self
                .db
                .allow_oxpilot_connect_grant_tx(&g.id, &g.target_id, &now)
                .await
                .map_err(db_err)?;
            if !won {
                return Err(grant_not_pending());
            }
            drop(_g);
            self.audit(
                client,
                Some(user_id),
                "oxpilot.connect_grant_allowed",
                Some(("oxpilot_grant", &g.id)),
                json!({ "grant_id": g.id, "target_id": g.target_id }),
            )
            .await;
            self.audit(
                client,
                Some(user_id),
                "oxpilot.target_connected",
                Some(("oxpilot_target", &g.target_id)),
                json!({ "target_id": g.target_id }),
            )
            .await;
            self.notify_waiters(&g.target_id);
            let g2 = self
                .db
                .find_oxpilot_grant(&g.id)
                .await
                .map_err(db_err)?
                .unwrap_or(g);
            return Ok(Self::grant_view(&g2, None));
        }
        // Action grant — `scope` stamps the flavor (default `once`,
        // BR3.5/C1 amendment) and materializes the Action atomically.
        let stamped = match scope {
            None | Some("once") => "action_once",
            Some("session") => "action_session",
            Some(_) => return Err(bad_input("scope must be once|session")),
        };
        let action_id = Uuid::new_v4().to_string();
        let action = NewOxpilotAction {
            id: &action_id,
            target_id: &g.target_id,
            user_id,
            session_id: g.session_id.as_deref().unwrap_or_default(),
            grant_id: &g.id,
            kind: g.action_kind.as_deref().unwrap_or_default(),
            args: g.action_args.as_deref().unwrap_or_default(),
            args_summary: g.action_summary.as_deref().unwrap_or_default(),
        };
        let Some(a) = self
            .db
            .allow_oxpilot_action_grant_tx(&g.id, stamped, &action, &now)
            .await
            .map_err(db_err)?
        else {
            return Err(grant_not_pending());
        };
        drop(_g);
        self.audit(
            client,
            Some(user_id),
            "oxpilot.action_grant_allowed",
            Some(("oxpilot_grant", &g.id)),
            json!({ "grant_id": g.id, "target_id": g.target_id, "scope": stamped }),
        )
        .await;
        self.audit(
            client,
            Some(user_id),
            "oxpilot.action_dispatched",
            Some(("oxpilot_action", &a.id)),
            json!({
                "action_id": a.id,
                "target_id": g.target_id,
                "session_id": a.session_id,
                "kind": a.kind,
                "grant_id": g.id,
                "args_summary": a.args_summary,
            }),
        )
        .await;
        self.notify_waiters(&g.target_id);
        let g2 = self
            .db
            .find_oxpilot_grant(&g.id)
            .await
            .map_err(db_err)?
            .unwrap_or(g);
        Ok(Self::grant_view(&g2, None))
    }

    // ---- connector verbs (C3 routes) ----

    async fn redeem_pairing(
        &self,
        client: &ClientMeta,
        pairing_code: &str,
        device_label: &str,
        requested_caps: &[String],
    ) -> Result<RedeemedPairing, AppError> {
        let code = pairing_code.trim();
        if code.is_empty() || code.len() > 128 {
            return Err(bad_input("pairing_code is required"));
        }
        let hash = sha256_hex(code.as_bytes());
        let now = now_rfc();
        let pairing = self
            .db
            .find_oxpilot_pairing_by_code_hash(&hash)
            .await
            .map_err(db_err)?;
        let Some(p) = pairing else {
            // Unknown-but-wellformed folds into `used` — C3's refusal
            // trio admits no never-existed/consumed split.
            self.audit(
                client,
                None,
                "oxpilot.pairing_used",
                None,
                json!({ "reason": "unknown_code" }),
            )
            .await;
            return Err(AppError::new("pairing_used", "pairing code already used"));
        };
        // BR1.2 ordering — expiry evaluated FIRST (lazily stamped +
        // audited), then the state check; an expired-then-retried code
        // must always read `pairing_expired`.
        if past_due(&p.expires_at, &now) {
            if p.state == "pending" {
                let _ = self.db.expire_oxpilot_pairing(&p.id).await;
            }
            self.audit(
                client,
                Some(&p.user_id),
                "oxpilot.pairing_expired",
                Some(("oxpilot_pairing", &p.id)),
                json!({ "pairing_id": p.id }),
            )
            .await;
            return Err(AppError::new("pairing_expired", "pairing code expired"));
        }
        if p.state != "pending" {
            self.audit(
                client,
                Some(&p.user_id),
                "oxpilot.pairing_used",
                Some(("oxpilot_pairing", &p.id)),
                json!({ "pairing_id": p.id }),
            )
            .await;
            return Err(AppError::new("pairing_used", "pairing code already used"));
        }
        // Caps ceiling — the mint-time set is the hard bound (BR1.2).
        let minted: Vec<String> = serde_json::from_str(&p.requested_caps).unwrap_or_default();
        let mut granted: Vec<String> = Vec::new();
        for c in requested_caps {
            let c = c.trim();
            if !minted.iter().any(|m| m == c) {
                return Err(forbidden("requested capabilities exceed the pairing grant"));
            }
            if !granted.iter().any(|g| g == c) {
                granted.push(c.to_string());
            }
        }
        if granted.is_empty() {
            // An empty request degenerates the intersection to the full
            // minted set — the connector asked for what was offered.
            granted = minted;
        }
        let verifier = rand_hex::<VERIFIER_BYTES>();
        let verifier_hash = sha256_hex(verifier.as_bytes());
        let target_id = Uuid::new_v4().to_string();
        let grant_id = Uuid::new_v4().to_string();
        let label = {
            let d = device_label.trim();
            if d.is_empty() {
                p.target_name.clone()
            } else {
                head_bounded(d, TARGET_NAME_MAX)
            }
        };
        let caps_json = serde_json::to_string(&granted).unwrap_or_else(|_| "[]".into());
        let target = NewOxpilotTarget {
            id: &target_id,
            user_id: &p.user_id,
            pairing_id: &p.id,
            name: &label,
            caps: &caps_json,
            verifier_hash: &verifier_hash,
        };
        let grant = NewOxpilotGrant {
            id: &grant_id,
            target_id: &target_id,
            user_id: &p.user_id,
            session_id: None,
            kind: "connect",
            action_kind: None,
            action_args: None,
            action_summary: None,
            expires_at: &after_secs(CONNECT_GRANT_TTL_SECS),
        };
        match self
            .db
            .claim_oxpilot_pairing_tx(&hash, &now_rfc(), &target, &grant)
            .await
            .map_err(db_err)?
        {
            Some(t) => {
                self.audit(
                    client,
                    Some(&p.user_id),
                    "oxpilot.pairing_claimed",
                    Some(("oxpilot_target", &t.id)),
                    json!({ "pairing_id": p.id, "target_id": t.id, "caps": granted }),
                )
                .await;
                // `connector_id == target_id` in v1 (BR1.2) — the
                // verifier's single custody surface.
                Ok(RedeemedPairing {
                    connector_id: t.id.clone(),
                    target_id: t.id,
                    verifier,
                })
            }
            None => {
                // CAS lost — re-read to answer honestly: expired wins
                // over used (BR1.2's pinned order).
                match self
                    .db
                    .find_oxpilot_pairing_by_code_hash(&hash)
                    .await
                    .map_err(db_err)?
                {
                    Some(p2) if past_due(&p2.expires_at, &now_rfc()) => {
                        if p2.state == "pending" {
                            let _ = self.db.expire_oxpilot_pairing(&p2.id).await;
                        }
                        self.audit(
                            client,
                            Some(&p2.user_id),
                            "oxpilot.pairing_expired",
                            Some(("oxpilot_pairing", &p2.id)),
                            json!({ "pairing_id": p2.id }),
                        )
                        .await;
                        Err(AppError::new("pairing_expired", "pairing code expired"))
                    }
                    _ => {
                        self.audit(
                            client,
                            Some(&p.user_id),
                            "oxpilot.pairing_used",
                            Some(("oxpilot_pairing", &p.id)),
                            json!({ "pairing_id": p.id }),
                        )
                        .await;
                        Err(AppError::new("pairing_used", "pairing code already used"))
                    }
                }
            }
        }
    }

    async fn resolve_verifier(
        &self,
        client: &ClientMeta,
        verifier: &str,
    ) -> Result<ConnectorIdentity, AppError> {
        let ip = client.ip_address.as_deref().unwrap_or("unknown");
        // NFR1.3 — the connector-auth IP bucket gates BEFORE the hash
        // lookup. A tripped bucket answers rate_limited and writes NO
        // audit row: the failures it recorded bound the write rate.
        if let Err(retry) = self.limiter().check_ip(ip) {
            return Err(rate_limited(retry));
        }
        let hash = sha256_hex(verifier.as_bytes());
        match self
            .db
            .find_oxpilot_target_by_verifier_hash(&hash)
            .await
            .map_err(db_err)?
        {
            Some(t) => Ok(ConnectorIdentity {
                target_id: t.id,
                user_id: t.user_id,
                caps: t.caps,
                status: t.status,
            }),
            None => {
                self.limiter().record_ip(ip);
                self.audit(
                    client,
                    None,
                    "oxpilot.connector_auth_failed",
                    None,
                    json!({ "reason": "unknown_verifier" }),
                )
                .await;
                Err(unauthorized())
            }
        }
    }

    async fn claim(
        &self,
        client: &ClientMeta,
        identity: &ConnectorIdentity,
        wait_seconds: u64,
        capabilities: &[String],
    ) -> Result<ClaimOutcome, AppError> {
        // BR2.2 — the claim shape has no revoked field; a revoked
        // target's only accepted verbs are heartbeat + report-close.
        if identity.status == "revoked" {
            return Err(AppError::new("revoked", "target revoked"));
        }
        let wait = wait_seconds.min(CLAIM_WAIT_MAX_SECS);
        // (0) connector-touch proof FIRST (BR4.3/NFR3.4): flip PRIOR
        // deliveries `claimed→running` + stamp liveness, BEFORE this
        // request's dequeue — a claim response alone never marks
        // running.
        self.db
            .flip_oxpilot_claimed_to_running(&identity.target_id)
            .await
            .map_err(db_err)?;
        self.db
            .touch_oxpilot_target_seen(&identity.target_id, &now_rfc())
            .await
            .map_err(db_err)?;
        // (1) arm the waiter BEFORE the first dequeue — `Notified` only
        // registers on first poll and `notify_waiters` stores no permit
        // for unregistered futures; arming ahead closes the lost-wake
        // window between the empty dequeue's lock release and
        // subscription (NFR3.2).
        let notify = self.notifier(&identity.target_id);
        let notified = notify.notified();
        tokio::pin!(notified);
        notified.as_mut().enable();
        let lock = self.lock_for(&identity.target_id);
        let mut audits = Vec::new();
        let first = {
            let _g = lock.lock().await;
            self.dequeue(identity, capabilities, &mut audits).await
        };
        self.emit_deferred(client, std::mem::take(&mut audits))
            .await;
        let mut batch = first?;
        if batch.is_empty() {
            // A pending cancel set answers immediately — kill delivery
            // never waits out a park (BR5.3 re-announces every answer).
            let cancel = self
                .db
                .oxpilot_cancel_requested_ids(&identity.target_id)
                .await
                .map_err(db_err)?;
            if !cancel.is_empty() {
                return Ok(ClaimOutcome::Batch {
                    actions: Vec::new(),
                    cancel,
                    retry_after: CLAIM_RETRY_AFTER_SECS,
                });
            }
            // (2) park on the already-armed waiter; on wake re-dequeue
            // under the lock.
            match tokio::time::timeout(Duration::from_secs(wait), notified).await {
                Err(_) => {
                    return Ok(ClaimOutcome::Timeout {
                        retry_after: CLAIM_RETRY_AFTER_SECS,
                    })
                }
                Ok(_) => {
                    let again = {
                        let _g = lock.lock().await;
                        self.dequeue(identity, capabilities, &mut audits).await
                    };
                    self.emit_deferred(client, std::mem::take(&mut audits))
                        .await;
                    batch = again?;
                }
            }
        }
        let cancel = self
            .db
            .oxpilot_cancel_requested_ids(&identity.target_id)
            .await
            .map_err(db_err)?;
        if batch.is_empty() && cancel.is_empty() {
            // Spurious wake — nothing changed the answer.
            return Ok(ClaimOutcome::Timeout {
                retry_after: CLAIM_RETRY_AFTER_SECS,
            });
        }
        Ok(ClaimOutcome::Batch {
            actions: batch,
            cancel,
            retry_after: CLAIM_RETRY_AFTER_SECS,
        })
    }

    async fn report(
        &self,
        client: &ClientMeta,
        identity: &ConnectorIdentity,
        action_id: &str,
        status: &str,
        exit_code: Option<i64>,
        output: &str,
    ) -> Result<(), AppError> {
        if status != "done" && status != "failed" {
            return Err(bad_input("status must be done|failed"));
        }
        // Every authenticated touch is execution proof (BR4.3).
        self.db
            .flip_oxpilot_claimed_to_running(&identity.target_id)
            .await
            .map_err(db_err)?;
        self.db
            .touch_oxpilot_target_seen(&identity.target_id, &now_rfc())
            .await
            .map_err(db_err)?;
        let a = self
            .db
            .find_oxpilot_action(action_id)
            .await
            .map_err(db_err)?;
        // Cross-target OR unknown ⇒ 404 `not_found` — the connector
        // kills local exec on it (BR5.2 anti-enumeration, not a
        // fabricated ack).
        let Some(a) = a.filter(|a| a.target_id == identity.target_id) else {
            return Err(not_found());
        };
        let close_state = match a.state.as_str() {
            // Terminal or pre-delivery rows answer the idempotent
            // `{ack:true}` — no rewrite, no re-audit (BR5.2).
            "done" | "failed" | "cancelled" | "queued" => return Ok(()),
            // A report on `cancel_requested` lands `cancelled` (C3's
            // "its report still closes the row") — the reported
            // status/exit_code ride the audit detail.
            "cancel_requested" => "cancelled",
            _ => status,
        };
        // Server-side redact + ≤2 KiB TAIL bound BEFORE any store or
        // audit — the connector's own redaction is defense in depth,
        // never the barrier (NFR5.4/BR5.2).
        let result_summary = tail_bounded(&redact(output), RESULT_MAX);
        let n = self
            .db
            .close_oxpilot_action_report(&a.id, close_state, Some(&result_summary), &now_rfc())
            .await
            .map_err(db_err)?;
        if n == 0 {
            // Raced terminal — the close already landed; the answer is
            // the same idempotent ack.
            return Ok(());
        }
        if close_state == "cancelled" {
            // The row carries no reason column — infer AT CLOSE
            // (reliability-design): revoked target ⇒ `target_revoked`;
            // otherwise the covering grant's revoke is the only path
            // into `cancel_requested` a live verifier can report on.
            let reason = if identity.status == "revoked" {
                "target_revoked"
            } else {
                match self
                    .db
                    .find_oxpilot_grant(&a.grant_id)
                    .await
                    .map_err(db_err)?
                {
                    Some(g) if g.state == "revoked" => "grant_revoked",
                    _ => "grant_revoked",
                }
            };
            self.audit(
                client,
                Some(&identity.user_id),
                "oxpilot.action_cancelled",
                Some(("oxpilot_action", &a.id)),
                json!({
                    "action_id": a.id,
                    "target_id": identity.target_id,
                    "kind": a.kind,
                    "reason": reason,
                    "reported_status": status,
                    "exit_code": exit_code,
                    "result_summary": result_summary,
                }),
            )
            .await;
        } else {
            self.audit(
                client,
                Some(&identity.user_id),
                "oxpilot.action_result",
                Some(("oxpilot_action", &a.id)),
                json!({
                    "action_id": a.id,
                    "target_id": identity.target_id,
                    "kind": a.kind,
                    "status": close_state,
                    "exit_code": exit_code,
                    "result_summary": result_summary,
                }),
            )
            .await;
        }
        Ok(())
    }

    async fn heartbeat(
        &self,
        _client: &ClientMeta,
        identity: &ConnectorIdentity,
    ) -> Result<HeartbeatOutcome, AppError> {
        // BR5.3 — the touch proves the connector is alive and its
        // delivered work executing; the cancel set re-announces on
        // every heartbeat until the rows close.
        self.db
            .flip_oxpilot_claimed_to_running(&identity.target_id)
            .await
            .map_err(db_err)?;
        self.db
            .touch_oxpilot_target_seen(&identity.target_id, &now_rfc())
            .await
            .map_err(db_err)?;
        let cancel = self
            .db
            .oxpilot_cancel_requested_ids(&identity.target_id)
            .await
            .map_err(db_err)?;
        Ok(HeartbeatOutcome {
            revoked: identity.status == "revoked",
            cancel,
        })
    }

    // ---- C8 hook backing ----

    async fn on_session_delete(
        &self,
        client: &ClientMeta,
        session_id: &str,
        user_id: &str,
    ) -> Result<(), AppError> {
        // BR6.5 — one tx: session-scoped `pending|allowed` grants and
        // every non-terminal action directly `cancelled` (the rows
        // cascade-delete, so `cancel_requested` is undeliverable);
        // connect grants (`session_id NULL`) survive (BR6.6). The
        // returned rows feed per-row `session_deleted` audits; the
        // audit_events themselves have no FK and SURVIVE the cascade.
        let (grants, actions) = self
            .db
            .cancel_oxpilot_session_connector_rows_tx(session_id, &now_rfc())
            .await
            .map_err(db_err)?;
        let mut touched_targets: Vec<&str> = Vec::new();
        for g in &grants {
            if !touched_targets.contains(&g.target_id.as_str()) {
                touched_targets.push(&g.target_id);
            }
            self.audit(
                client,
                Some(user_id),
                "oxpilot.grant_cancelled",
                Some(("oxpilot_grant", &g.id)),
                json!({
                    "grant_id": g.id,
                    "target_id": g.target_id,
                    "kind": g.kind,
                    "reason": "session_deleted",
                }),
            )
            .await;
        }
        for a in &actions {
            if !touched_targets.contains(&a.target_id.as_str()) {
                touched_targets.push(&a.target_id);
            }
            self.audit(
                client,
                Some(user_id),
                "oxpilot.action_cancelled",
                Some(("oxpilot_action", &a.id)),
                json!({
                    "action_id": a.id,
                    "target_id": a.target_id,
                    "kind": a.kind,
                    "reason": "session_deleted",
                }),
            )
            .await;
        }
        for t in touched_targets {
            self.notify_waiters(t);
        }
        Ok(())
    }
}
