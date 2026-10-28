//! `oxpilot.session_*` + `oxpilot.message_queue` / `oxpilot.queue_list` /
//! `oxpilot.usage_get` — the user-scoped session surface (WF2 + U2).
//! Every row is bound to the caller through the C5 `SessionService`
//! seam; cross-user/foreign access resolves `not_found` (BR3.3), the
//! enablement gate runs first on every proc, and `session_delete`
//! audits all three outcomes (deleted / hook_failed / not_found).

use std::sync::Arc;

use oxidean_api_core::auth::gate::require_verified;
use oxidean_api_core::oxpilot::{
    queued_message_json, require_enabled, CreateSession, Enqueue, SessionService, SharedOxpilot,
};
use oxidean_api_core::repo::resolve_repo_for_read;
use oxidean_api_core::rpc::RpcCtx;
use oxidean_core::AppError;
use oxidean_db::OxpilotSessionRow;
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

/// The runtime unwrap — the established settings idiom: the slot always
/// exists on `AppState`, an absent `ctx.oxpilot` is a wiring bug
/// (`oxpilot.internal`), never a request fault.
fn runtime(ctx: &RpcCtx) -> Result<SharedOxpilot, AppError> {
    ctx.oxpilot
        .clone()
        .ok_or_else(|| AppError::new("oxpilot.internal", "oxpilot runtime unavailable"))
}

/// The C5 port — installed unconditionally at boot (`install_sessions`);
/// an unset seam answers `oxpilot.internal` exactly like the settings
/// path's runtime unwrap.
fn sessions(ctx: &RpcCtx) -> Result<Arc<dyn SessionService>, AppError> {
    runtime(ctx)?.sessions()
}

fn session_json(row: &OxpilotSessionRow) -> Value {
    json!({
        "id": row.id,
        "user_id": row.user_id,
        "repo_id": row.repo_id,
        "repo_owner": row.repo_owner,
        "repo_name": row.repo_name,
        "branch": row.branch,
        "goal": row.goal,
        "title": row.title,
        "status": row.status,
        "created_at": row.created_at,
        "last_activity_at": row.last_activity_at,
        "computer_use_target_id": row.computer_use_target_id,
    })
}

const GOAL_MAX_CHARS: usize = 4_096;
/// Same free-text ceiling as the SSE route (NFR1.10).
const CONTENT_MAX_BYTES: usize = 64 * 1024;
/// `client_msg_id` is an opaque idempotency token — bounded like the
/// SSE route so a bloated value can't ride the unique index.
const CLIENT_MSG_ID_MAX_BYTES: usize = 128;

fn bad_input(msg: impl Into<String>) -> AppError {
    AppError::new("rpc.bad_input", msg.into())
}

fn parse<T: for<'a> Deserialize<'a>>(input: Value, proc: &str) -> Result<T, AppError> {
    serde_json::from_value(input).map_err(|e| bad_input(format!("invalid {proc} input: {e}")))
}

#[derive(Debug, Deserialize)]
struct CreateRequest {
    repo_owner: String,
    repo_name: String,
    #[serde(default)]
    branch: Option<String>,
    #[serde(default)]
    goal: Option<String>,
    /// U8 amendment — the optional computer-use target pick; absent or
    /// blank rides the shipped NULL ("no target") binding.
    #[serde(default)]
    target_id: Option<String>,
}

pub async fn create(ctx: &RpcCtx, input: Value) -> Result<Value, AppError> {
    // SessionOnly at the proc gate (PAT paths deny upstream); the enablement
    // gate runs before any repo/session work (BR2.3).
    let st = require_enabled(ctx).await?;
    let _ = &st;
    let user = require_verified(ctx).await?;
    let req: CreateRequest = parse(input, "oxpilot.session_create")?;

    // Repo must resolve under the caller's visibility — BR3.1 anti-
    // enumeration returns not_found for absent or foreign repos alike.
    let accessible = resolve_repo_for_read(ctx, &req.repo_owner, &req.repo_name).await?;
    let branch = req
        .branch
        .as_deref()
        .map(str::trim)
        .filter(|b| !b.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| accessible.row.default_branch.clone());
    let goal = req
        .goal
        .as_deref()
        .map(str::trim)
        .filter(|g| !g.is_empty())
        .map(str::to_string);
    if let Some(g) = &goal {
        if g.chars().count() > GOAL_MAX_CHARS {
            return Err(bad_input(format!(
                "goal exceeds {GOAL_MAX_CHARS} characters"
            )));
        }
    }
    // Redact-on-write covers goal/title too — same free-text paste surface
    // as message content (BR3.5). The service re-redacts at its boundary
    // (idempotent); deriving the title from the redacted goal keeps the
    // stored pair consistent.
    let goal = goal.map(|g| oxidean_core::redact(&g));
    let title = goal
        .as_deref()
        .map(|g| g.chars().take(80).collect::<String>())
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| "Untitled session".to_string());

    // U8 — an explicit `target_id` binds the session to one of the
    // caller's `connected` targets; absent/blank rides the shipped NULL
    // ("no target") binding. The gateway read is owner-checked: foreign
    // and unknown ids resolve `not_found` identically (BR7.1); a
    // known-but-not-connected lifecycle refuses — the dialog only ever
    // offers connected picks.
    let target_id = req
        .target_id
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_string);
    if let Some(tid) = &target_id {
        let view = runtime(ctx)?
            .gateway()?
            .get_target(&ctx.client, &user.id, tid)
            .await?;
        if view.lifecycle != "connected" {
            return Err(bad_input("target_id must name a connected target"));
        }
    }

    let svc = sessions(ctx)?;
    let row = svc
        .create_session(
            &user.id,
            CreateSession {
                id: Uuid::new_v4().to_string(),
                repo_owner: accessible.owner_username.clone(),
                repo_name: accessible.row.name.clone(),
                repo_id: accessible.row.id.clone(),
                branch,
                goal,
                title,
                // Validated above — the binding rides the INSERT so the
                // returned row (and every later get/list) carries it.
                computer_use_target_id: target_id,
            },
        )
        .await?;

    // Create-only audit — ids/ownership/repo metadata, never content.
    oxidean_api_core::audit::record(
        ctx,
        Some((&user.id, &user.username)),
        "oxpilot.session_create",
        Some(("oxpilot_session", &row.id)),
        Some(
            json!({
                "session_id": row.id,
                "repo_owner": row.repo_owner,
                "repo_name": row.repo_name,
                "branch": row.branch,
                "computer_use_target_id": row.computer_use_target_id,
            })
            .to_string(),
        ),
    )
    .await;

    Ok(session_json(&row))
}

pub async fn list(ctx: &RpcCtx) -> Result<Value, AppError> {
    require_enabled(ctx).await?;
    let user = require_verified(ctx).await?;
    let svc = sessions(ctx)?;
    let rows = svc.list_sessions(&user.id).await?;
    Ok(json!({
        "sessions": rows.iter().map(session_json).collect::<Vec<_>>(),
    }))
}

#[derive(Debug, Deserialize)]
struct SessionRef {
    session_id: String,
}

/// `oxpilot.session_get` — the owned-scoped read; foreign ids resolve
/// `not_found` identically to absent ones (BR3.3).
pub async fn get(ctx: &RpcCtx, input: Value) -> Result<Value, AppError> {
    require_enabled(ctx).await?;
    let user = require_verified(ctx).await?;
    let req: SessionRef = parse(input, "oxpilot.session_get")?;
    let svc = sessions(ctx)?;
    let row = svc.get_session(&user.id, &req.session_id).await?;
    Ok(session_json(&row))
}

/// `oxpilot.session_delete` — C8 hook-then-cascade through the service.
/// Audits EVERY resolved outcome: `ok`, `hook_failed` (with the failing
/// `hook_index` merged into the error's data), and `not_found` — the
/// delete attempt itself is the auditable act, not just its success.
pub async fn delete(ctx: &RpcCtx, input: Value) -> Result<Value, AppError> {
    require_enabled(ctx).await?;
    let user = require_verified(ctx).await?;
    let req: SessionRef = parse(input, "oxpilot.session_delete")?;
    let session_id = req.session_id.clone();
    let svc = sessions(ctx)?;
    match svc.delete_session(&user.id, &session_id).await {
        Ok(()) => {
            oxidean_api_core::audit::record(
                ctx,
                Some((&user.id, &user.username)),
                "oxpilot.session_delete",
                Some(("oxpilot_session", &session_id)),
                Some(
                    json!({
                        "session_id": session_id,
                        "result": "ok",
                    })
                    .to_string(),
                ),
            )
            .await;
            Ok(json!({ "deleted": true }))
        }
        Err(e) => {
            // A hook failure carries the hook's own code with
            // `hook_index` merged into `data` — its presence IS the
            // hook_failed signal (the service never rewrites the code).
            let hook_index = e.data.as_ref().and_then(|d| d.get("hook_index"));
            if hook_index.is_some() || e.code == "not_found" {
                let mut detail = json!({
                    "session_id": session_id,
                    "result": if hook_index.is_some() { "hook_failed" } else { "not_found" },
                });
                if let Some(idx) = hook_index {
                    detail["hook_index"] = idx.clone();
                }
                oxidean_api_core::audit::record(
                    ctx,
                    Some((&user.id, &user.username)),
                    "oxpilot.session_delete",
                    Some(("oxpilot_session", &session_id)),
                    Some(detail.to_string()),
                )
                .await;
            }
            Err(e)
        }
    }
}

#[derive(Debug, Deserialize)]
struct QueueRequest {
    session_id: String,
    content: String,
    #[serde(default)]
    client_msg_id: Option<String>,
}

/// `oxpilot.message_queue` — enqueue a follow-up while a turn streams
/// (US1.8). The queue exists ONLY under a live turn — no active turn
/// means the client should `POST .../chat` instead
/// (`session_active_turn_absent`, C1); the budget check runs against
/// `slot.budget_caps` before the row lands (US5.2). The receipt is the
/// C1 `{queued_id, position}` pair — `{null, null}` on a transcript
/// dedupe hit.
pub async fn message_queue(ctx: &RpcCtx, input: Value) -> Result<Value, AppError> {
    let runtime = runtime(ctx)?;
    require_enabled(ctx).await?;
    let user = require_verified(ctx).await?;
    let req: QueueRequest = parse(input, "oxpilot.message_queue")?;
    if req.content.is_empty() || req.content.len() > CONTENT_MAX_BYTES {
        return Err(bad_input("content is required and must be at most 64 KiB"));
    }
    let client_msg_id = req
        .client_msg_id
        .as_deref()
        .map(str::trim)
        .filter(|c| !c.is_empty())
        .map(str::to_string);
    if let Some(c) = &client_msg_id {
        if c.len() > CLIENT_MSG_ID_MAX_BYTES {
            return Err(bad_input("client_msg_id must be at most 128 bytes"));
        }
    }
    let svc = sessions(ctx)?;

    // BR4.1's pinned order — gate → owned session → live turn → budget →
    // dedupe → insert. Ownership resolves FIRST so a foreign/absent id
    // answers `not_found` identically regardless of liveness or budget
    // (the differing replies would otherwise disclose which guessed ids
    // exist and are mid-turn).
    svc.get_session(&user.id, &req.session_id).await?;
    // The queue exists only under a live turn — absent ⇒ the client
    // must POST /chat instead (C1 `session_active_turn_absent`). The
    // advisory live-check stays; the COMMIT re-verifies under the lock.
    if runtime.turns().await.get(&req.session_id).await.is_none() {
        return Err(AppError::new(
            "session_active_turn_absent",
            "no active turn on this session",
        ));
    }
    // Then the spend surface — a queued follow-up is still a queued
    // provider call (US5.2). Runs BEFORE the hold (shipped order).
    let caps = runtime.slot.read().await.budget_caps;
    svc.check_budget(&user.id, caps).await?;
    // The COMMIT serializes on the per-session admission lock (BR3.4 +
    // R-20's airtight seam): inside the hold the proc re-verifies the
    // SAME turn it claimed is still the map entry — a guard held across
    // the dying turn's release→recheck fails the id check, so a row can
    // never commit inside that blind window.
    let Some((entry, lock)) = runtime.turns().await.admission_lock(&req.session_id).await else {
        return Err(AppError::new(
            "session_active_turn_absent",
            "no active turn on this session",
        ));
    };
    let receipt = {
        let _g = lock.lock().await;
        match runtime.turns().await.get(&req.session_id).await {
            Some(live) if live.turn_id == entry.turn_id => {}
            _ => {
                return Err(AppError::new(
                    "session_active_turn_absent",
                    "no active turn on this session",
                ));
            }
        }
        svc.enqueue(
            &user.id,
            Enqueue {
                session_id: req.session_id.clone(),
                content: req.content,
                client_msg_id,
            },
        )
        .await?
    };
    // Queued card (WF5): `queued_id == None` ⇒ transcript-dedupe hit —
    // a null-ref card is malformed C2, emit nothing. Between commit and
    // push the turn may terminal — a dropped card is fine; the row
    // exists and `queue_list` resolves.
    if let Some(ref_id) = &receipt.queued_id {
        if let Some(turn) = runtime.turns().await.get(&req.session_id).await {
            crate::runtime::emit_frame(
                &turn,
                json!({
                    "type": "card",
                    "card_kind": "queued",
                    "ref_id": ref_id,
                }),
            )
            .await;
        }
    }
    Ok(serde_json::to_value(receipt).unwrap_or_else(|_| json!({})))
}

/// `oxpilot.queue_list` — the non-consuming pending-queue read (the
/// queued card's ref target, C1/C2).
pub async fn queue_list(ctx: &RpcCtx, input: Value) -> Result<Value, AppError> {
    require_enabled(ctx).await?;
    let user = require_verified(ctx).await?;
    let req: SessionRef = parse(input, "oxpilot.queue_list")?;
    let svc = sessions(ctx)?;
    let rows = svc.list_queue(&user.id, &req.session_id).await?;
    Ok(json!({
        "queued": rows.iter().map(queued_message_json).collect::<Vec<_>>(),
    }))
}

#[derive(Debug, Deserialize)]
struct QueueDeleteRequest {
    session_id: String,
    queued_id: String,
}

/// `oxpilot.queue_delete{session_id, queued_id}` — U8's inline
/// queued-message cancel (the M8/ix affordance, amendment #3).
/// The service takes the SAME owned-session read `queue_list` rides —
/// a foreign/absent session answers `not_found` before any row is
/// touched (BR3.3 anti-enumeration); an already-absent `queued_id`
/// under an owned session is `not_found` too, matching `grant_get`'s
/// stale-ref honesty (BR9.1 — the drained row resolves on the client's
/// next `queue_list`).
pub async fn queue_delete(ctx: &RpcCtx, input: Value) -> Result<Value, AppError> {
    require_enabled(ctx).await?;
    let user = require_verified(ctx).await?;
    let req: QueueDeleteRequest = parse(input, "oxpilot.queue_delete")?;
    sessions(ctx)?
        .delete_queued(&user.id, &req.session_id, &req.queued_id)
        .await?;
    Ok(json!({ "deleted": true }))
}

/// `oxpilot.turn_cancel` — abort the live turn on an owned session
/// (US1.6, C1's previously-orphaned proc). Live ⇒ `emit_terminal{
/// aborted}` + `cancel.cancel()` ⇒ `{aborted: true}`; a CAS-loss (the
/// turn terminalled between the live-check and the emit) answers
/// `{aborted: false}` — the caller's intent (turn is dead) is met
/// either way. Absent ⇒ `no_active_turn` (C1 — NOT
/// `session_active_turn_absent`, that code is `message_queue`'s).
pub async fn turn_cancel(ctx: &RpcCtx, input: Value) -> Result<Value, AppError> {
    let runtime = runtime(ctx)?;
    require_enabled(ctx).await?;
    let user = require_verified(ctx).await?;
    let req: SessionRef = parse(input, "oxpilot.turn_cancel")?;
    let svc = sessions(ctx)?;
    // Owned session FIRST — the proc never reads turn state for a
    // foreign id (anti-enumeration, NFR1.2).
    svc.get_session(&user.id, &req.session_id).await?;
    let turns = runtime.turns().await;
    let Some(turn) = turns.get(&req.session_id).await else {
        return Err(AppError::new(
            "no_active_turn",
            "no active turn on this session",
        ));
    };
    let won = crate::runtime::emit_terminal(
        &turn,
        json!({
            "type": "terminal",
            "message_id": turn.message_id,
            "result": "aborted",
        }),
    )
    .await;
    if won {
        tracing::warn!(session_id = %req.session_id, "oxpilot turn aborted");
    }
    // Idempotent — a CAS-loss means the terminal already landed; the
    // cancel still rides so any in-flight await wakes regardless.
    turn.cancel.cancel();
    Ok(json!({ "aborted": won }))
}

/// `oxpilot.usage_get` — the C1 five-field spend view; `*_cap` mirrors
/// the configured user axes (null ⇒ unbounded). Never refuses when over
/// budget — it IS the spend surface.
pub async fn usage_get(ctx: &RpcCtx) -> Result<Value, AppError> {
    let runtime = runtime(ctx)?;
    require_enabled(ctx).await?;
    let user = require_verified(ctx).await?;
    let svc = sessions(ctx)?;
    let caps = runtime.slot.read().await.budget_caps;
    let view = svc.usage_view(&user.id, caps).await?;
    Ok(serde_json::to_value(view).unwrap_or_else(|_| json!({})))
}
