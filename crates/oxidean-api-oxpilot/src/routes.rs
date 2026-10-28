//! `/api/oxpilot` SSE edge — `POST /sessions/{id}/chat` starts a turn and
//! streams frames to its terminal; `GET /sessions/{id}/chat/stream` emits a
//! `snapshot` then attaches the reader to any live turn. Session-cookie auth
//! only — PAT/bearer credentials are rejected before anything else (the
//! `oxpilot.*` surface is SessionOnly, BR1.2).
//!
//! Frame contract (C2): `snapshot` → `append`* → exactly one `terminal`
//! (`done|error`), `ping` keep-alives fill gaps. Pre-stream failures return
//! typed JSON errors from the closed vocabulary:
//! 401 `unauthenticated` · 403 `forbidden`/`oxpilot.disabled` ·
//! 404 `not_found` · 409 `turn_active` · 422 `invalid_body` ·
//! 429 `budget_exceeded`.
// The handlers' helpers return `Result<_, Response>` on purpose — axum's
// early-return idiom where the Err IS the wire answer, built only on
// failure paths; boxing it would change nothing observable.
#![allow(clippy::result_large_err)]

use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::sse::{self, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use oxidean_api_core::app::{resolve_session_user_id, AppState};
use oxidean_api_core::oxpilot::{
    active_turn_view, resolve_enablement_slot, ActiveTurn, AppendMessage, SessionService,
    TURN_FANOUT_CAPACITY,
};
use oxidean_core::redact;
use oxidean_db::OxpilotMessageRow;
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

/// Chat `content` cap (NFR1.10) — oversize answers `422 invalid_body`.
const CONTENT_MAX_BYTES: usize = 64 * 1024;
/// `client_msg_id` is an opaque idempotency token — bounded so a bloated
/// value can't ride it into the unique index.
const CLIENT_MSG_ID_MAX_BYTES: usize = 128;
/// SSE ping cadence — well under the 30 s contract ceiling (C2).
const PING_INTERVAL: Duration = Duration::from_secs(15);
/// Poll for a turn appearing on an idle GET stream (WF4 step 2).
const ATTACH_POLL: Duration = Duration::from_secs(2);

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/sessions/{id}/chat", post(chat_post))
        .route("/sessions/{id}/chat/stream", get(chat_stream))
        // U6 C3 connector subtree — verifier-Bearer auth only (no
        // session cookie or PAT reaches it); enablement fails closed
        // inside each handler BEFORE auth (BR2.3/BR2.5); `/pair` is
        // the unauthenticated carve-out (BR1.5).
        .route("/connector/pair", post(crate::connector::routes::pair))
        .route("/connector/claim", post(crate::connector::routes::claim))
        .route("/connector/report", post(crate::connector::routes::report))
        .route(
            "/connector/heartbeat",
            post(crate::connector::routes::heartbeat),
        )
}

fn err_response(status: StatusCode, code: &str, message: &str) -> Response {
    (
        status,
        Json(json!({
            "ok": false,
            "error": { "code": code, "message": message }
        })),
    )
        .into_response()
}

/// Session-cookie credential — `None` ⇒ 401; PAT/bearer ⇒ 403 (SessionOnly
/// surface, BR1.2).
async fn session_user(state: &AppState, headers: &HeaderMap) -> Result<String, Response> {
    match resolve_session_user_id(state, headers).await {
        Some((user_id, None)) => Ok(user_id),
        Some((_user_id, Some(_pat))) => Err(err_response(
            StatusCode::FORBIDDEN,
            "forbidden",
            "session credential required",
        )),
        None => Err(err_response(
            StatusCode::UNAUTHORIZED,
            "unauthenticated",
            "not authenticated",
        )),
    }
}

/// The enablement gate — every SSE route runs it after auth (WF1). A
/// slot read only; the gate never waits on `rebuild_lock` or the DB.
async fn require_gate(state: &AppState) -> Result<(), Response> {
    let st = resolve_enablement_slot(&*state.oxpilot.slot.read().await);
    if st.enabled {
        Ok(())
    } else {
        Err(err_response(
            StatusCode::FORBIDDEN,
            "oxpilot.disabled",
            "oxpilot is disabled",
        ))
    }
}

/// The C5 session-service port — installed unconditionally at boot;
/// an absent seam is a wiring bug, answered `oxpilot.internal`.
fn sessions(state: &AppState) -> Result<Arc<dyn SessionService>, Response> {
    state.oxpilot.sessions().map_err(|_| {
        err_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "oxpilot.internal",
            "session service unavailable",
        )
    })
}

/// Map a service `AppError` onto the SSE route's typed error shape —
/// `not_found` ⇒ 404, `oxpilot.disabled`/`forbidden` ⇒ 403,
/// `budget_exceeded` ⇒ 429 (C1 pin), everything else 500
/// `oxpilot.internal` (BR1.1/BR3.3 anti-enumeration preserved).
fn svc_err(e: &oxidean_core::AppError, what: &str) -> Response {
    let (status, code) = match e.code.as_str() {
        "not_found" => (StatusCode::NOT_FOUND, "not_found"),
        "oxpilot.disabled" | "forbidden" => (StatusCode::FORBIDDEN, e.code.as_str()),
        "budget_exceeded" => (StatusCode::TOO_MANY_REQUESTS, e.code.as_str()),
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "oxpilot.internal"),
    };
    if status == StatusCode::INTERNAL_SERVER_ERROR {
        tracing::error!(error = %e.message, code = %e.code, "oxpilot {what} failed");
    }
    err_response(status, code, what)
}

/// Ownership-filtered session find through the service port — cross-user
/// resolves 404 `not_found`, identical to an absent row (BR3.3).
async fn owned_session(
    state: &AppState,
    session_id: &str,
    user_id: &str,
) -> Result<oxidean_db::OxpilotSessionRow, Response> {
    sessions(state)?
        .get_session(user_id, session_id)
        .await
        .map_err(|e| svc_err(&e, "session lookup failed"))
}

/// The queue half of the BR2.2 union dedupe — does `client_msg_id`
/// already exist in `oxpilot_queued_messages`? Routed through the
/// service port so the ownership/anti-enumeration layer applies.
async fn queued_cmid_hit(
    svc: &Arc<dyn SessionService>,
    user_id: &str,
    session_id: &str,
    client_msg_id: &str,
) -> Result<bool, Response> {
    svc.list_queue(user_id, session_id)
        .await
        .map(|rows| {
            rows.iter()
                .any(|q| q.client_msg_id.as_deref() == Some(client_msg_id))
        })
        .map_err(|e| svc_err(&e, "queue dedupe lookup failed"))
}

/// Message row → snapshot frame entry.
fn message_json(m: &OxpilotMessageRow) -> Value {
    json!({
        "id": m.id,
        "seq": m.seq,
        "role": m.role,
        "content": m.content,
        "client_msg_id": m.client_msg_id,
        "status": m.status,
        "created_at": m.created_at,
    })
}

/// The snapshot's message array — the UNFILTERED transcript including
/// the in-flight `streaming` row (a remount overlays the live tail onto
/// it); served by the port's `list_messages`, never `state.db`.
async fn snapshot_messages(
    state: &AppState,
    user_id: &str,
    session_id: &str,
) -> Result<Vec<Value>, Response> {
    sessions(state)?
        .list_messages(user_id, session_id)
        .await
        .map(|rows| rows.iter().map(message_json).collect())
        .map_err(|e| svc_err(&e, "transcript read failed"))
}

async fn snapshot_frame(
    state: &AppState,
    user_id: &str,
    session_id: &str,
) -> Result<Value, Response> {
    let svc = sessions(state)?;
    let messages = snapshot_messages(state, user_id, session_id).await?;
    // C2 U2 extension — the snapshot carries the pending follow-up queue
    // and durable tool-call cards so a resuming client reconstructs the
    // full surface in one frame. Both reads go through the service port.
    let pending_queue = svc
        .list_queue(user_id, session_id)
        .await
        .map_err(|e| svc_err(&e, "queue read failed"))?;
    let tool_calls = svc
        .list_tool_calls(user_id, session_id)
        .await
        .map_err(|e| svc_err(&e, "tool-call read failed"))?;
    let view = match state.oxpilot.turns().await.get(session_id).await {
        Some(turn) => Some(active_turn_view(&turn).await),
        None => None,
    };
    Ok(oxidean_api_core::oxpilot::snapshot_frame(
        messages,
        pending_queue,
        tool_calls,
        view,
    ))
}

#[derive(Debug, Deserialize)]
struct ChatBody {
    content: String,
    #[serde(default)]
    client_msg_id: Option<String>,
}

/// `POST /api/oxpilot/sessions/{id}/chat` — ordered checks
/// auth → gate → ownership → body → idempotency → turn claim (BR4.1),
/// then streams the turn's frames to its terminal.
pub async fn chat_post(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    let user_id = match session_user(&state, &headers).await {
        Ok(u) => u,
        Err(r) => return r,
    };
    if let Err(r) = require_gate(&state).await {
        return r;
    }
    let session = match owned_session(&state, &session_id, &user_id).await {
        Ok(s) => s,
        Err(r) => return r,
    };
    let body: ChatBody = match serde_json::from_slice(&body) {
        Ok(b) => b,
        Err(_) => {
            return err_response(
                StatusCode::UNPROCESSABLE_ENTITY,
                "invalid_body",
                "expected {content, client_msg_id?}",
            )
        }
    };
    if body.content.is_empty() || body.content.len() > CONTENT_MAX_BYTES {
        return err_response(
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid_body",
            "content is required and must be at most 64 KiB",
        );
    }
    let client_msg_id = body
        .client_msg_id
        .as_deref()
        .map(str::trim)
        .filter(|c| !c.is_empty())
        .map(str::to_string);
    if let Some(c) = &client_msg_id {
        if c.len() > CLIENT_MSG_ID_MAX_BYTES {
            return err_response(
                StatusCode::UNPROCESSABLE_ENTITY,
                "invalid_body",
                "client_msg_id must be at most 128 bytes",
            );
        }
    }

    // Idempotent resend (BR3.6): a persisted `client_msg_id` never inserts a
    // second user row — live turn that *owns* the row ⇒ attach; ended ⇒
    // persisted transcript. A live turn owned by a different message means
    // this resend's turn already ended — answer the snapshot, not its frames.
    let svc = match sessions(&state) {
        Ok(s) => s,
        Err(r) => return r,
    };
    let content = redact(&body.content);
    if let Some(c) = &client_msg_id {
        match svc
            .find_message_by_client_msg_id(&user_id, &session_id, c)
            .await
        {
            Ok(Some(prior)) => {
                if let Some(turn) = state.oxpilot.turns().await.get(&session_id).await {
                    if resend_matches(&turn, Some(c), &content, Some(prior.seq)) {
                        return attach_stream(turn).await;
                    }
                }
                let frame = match snapshot_frame(&state, &user_id, &session_id).await {
                    Ok(f) => f,
                    Err(r) => return r,
                };
                return single_frame_stream(frame).await;
            }
            Ok(None) => {
                // No row — but a resend can land inside the original's
                // claim→insert window: recognize it by the pending fields.
                if let Some(turn) = state.oxpilot.turns().await.get(&session_id).await {
                    if resend_matches(&turn, Some(c), &content, None) {
                        return attach_stream(turn).await;
                    }
                }
                // Union dedupe's QUEUE half (BR2.2): a resent cmid that
                // lives only in `oxpilot_queued_messages` must never mint
                // a transcript row — a duplicate `(session_id, cmid)`
                // would poison every later `drain_queue_tx` commit.
                match queued_cmid_hit(&svc, &user_id, &session_id, c).await {
                    Ok(true) => {
                        return err_response(
                            StatusCode::CONFLICT,
                            "turn_active",
                            "a turn is already streaming on this session",
                        )
                    }
                    Ok(false) => {}
                    Err(r) => return r,
                }
            }
            Err(e) => {
                tracing::error!(error = %e.message, "oxpilot dedupe lookup failed");
                return err_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "oxpilot.internal",
                    "dedupe lookup failed",
                );
            }
        }
    }

    // Budget gate (BR6.1/WF1 step 6) — AFTER the dedupe union so a resend
    // always reaches its attach/snapshot answer, BEFORE the claim so a
    // refused spend never pins the session. Maps HTTP 429 via svc_err.
    {
        let caps = state.oxpilot.slot.read().await.budget_caps;
        if let Err(e) = svc.check_budget(&user_id, caps).await {
            return svc_err(&e, "budget check failed");
        }
    }

    // Atomic one-live-turn claim (BR4.2) — claimed first, seq minted second
    // (WF3.1): only the claim winner allocates, so a lost race can never
    // collide in the unique index. `started_seq` is 0 until stored below.
    let turn_id = Uuid::new_v4().to_string();
    let message_id = Uuid::new_v4().to_string();
    let pending_cmid = client_msg_id.clone();
    let pending_content = content.clone();
    let claim = state
        .oxpilot
        .turns()
        .await
        .claim(&session_id, || {
            let (fanout, _) = broadcast::channel(TURN_FANOUT_CAPACITY);
            ActiveTurn {
                session_id: session_id.clone(),
                turn_id: turn_id.clone(),
                message_id: message_id.clone(),
                started_seq: std::sync::atomic::AtomicI64::new(0),
                pending_client_msg_id: pending_cmid,
                pending_content,
                fanout,
                emitted_deltas: tokio::sync::Mutex::new(Vec::new()),
                cancel: CancellationToken::new(),
                admission_lock: Arc::new(tokio::sync::Mutex::new(())),
                terminal_sent: std::sync::atomic::AtomicBool::new(false),
                generation: 0,
            }
        })
        .await;
    let turn = match claim {
        Some(t) => t,
        None => {
            // Lost the race — a resend landing in the original's
            // claim→insert window attaches instead of conflicting.
            if let Some(turn) = state.oxpilot.turns().await.get(&session_id).await {
                if resend_matches(&turn, client_msg_id.as_deref(), &content, None) {
                    return attach_stream(turn).await;
                }
                if let Some(c) = &client_msg_id {
                    if let Ok(Some(prior)) = svc
                        .find_message_by_client_msg_id(&user_id, &session_id, c)
                        .await
                    {
                        if resend_matches(&turn, Some(c), &content, Some(prior.seq)) {
                            return attach_stream(turn).await;
                        }
                        if let Ok(frame) = snapshot_frame(&state, &user_id, &session_id).await {
                            return single_frame_stream(frame).await;
                        }
                    }
                }
            }
            return err_response(
                StatusCode::CONFLICT,
                "turn_active",
                "a turn is already streaming on this session",
            );
        }
    };
    // Post-claim recheck (BR3.6): a resend whose dedupe read predated the
    // original's commit reaches here only after that turn released — the
    // row now exists, so answer the transcript instead of inserting a
    // `UNIQUE(session_id, client_msg_id)`-conflicting duplicate. The QUEUE
    // half is checked identically — a queued cmid never mints a transcript
    // row (it would poison every later drain on the unique index).
    if let Some(c) = &client_msg_id {
        match svc
            .find_message_by_client_msg_id(&user_id, &session_id, c)
            .await
        {
            Ok(Some(_)) => {
                state
                    .oxpilot
                    .turns()
                    .await
                    .release(&session_id, &turn.turn_id)
                    .await;
                let frame = match snapshot_frame(&state, &user_id, &session_id).await {
                    Ok(f) => f,
                    Err(r) => return r,
                };
                return single_frame_stream(frame).await;
            }
            Ok(None) => match queued_cmid_hit(&svc, &user_id, &session_id, c).await {
                Ok(true) => {
                    state
                        .oxpilot
                        .turns()
                        .await
                        .release(&session_id, &turn.turn_id)
                        .await;
                    return err_response(
                        StatusCode::CONFLICT,
                        "turn_active",
                        "a turn is already streaming on this session",
                    );
                }
                Ok(false) => {}
                Err(r) => {
                    state
                        .oxpilot
                        .turns()
                        .await
                        .release(&session_id, &turn.turn_id)
                        .await;
                    return r;
                }
            },
            Err(e) => {
                state
                    .oxpilot
                    .turns()
                    .await
                    .release(&session_id, &turn.turn_id)
                    .await;
                tracing::error!(error = %e.message, "oxpilot dedupe recheck failed");
                return err_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "oxpilot.internal",
                    "dedupe recheck failed",
                );
            }
        }
    }

    // Residual queue drain (WF1 step 9): rows a previous turn left
    // behind — raced out by claim loss or the chain depth cap — become
    // transcript rows under THIS winner BEFORE the new user row mints
    // its seq, so the prompt window and `started_seq` see one coherent
    // ordering. The early dedupe exits above release WITHOUT draining:
    // a residual batch belongs to whoever actually starts a turn. A
    // drain failure fails the send honestly — release + 500 — never
    // minting the user row's seq ahead of the undrained orphans
    // (BR4.2).
    if let Err(e) = svc.drain_queue(&user_id, &session_id).await {
        state
            .oxpilot
            .turns()
            .await
            .release(&session_id, &turn.turn_id)
            .await;
        tracing::error!(error = %e.message, "oxpilot residual drain failed");
        return err_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "oxpilot.internal",
            "residual drain failed",
        );
    }

    // Persist the user row through the service (BR3.4/BR3.5) — `seq` is
    // minted inside the insert and `last_activity_at` bumps with it;
    // failure releases the claim so the session isn't pinned at 409.
    let user_row = match svc
        .append_message(
            &user_id,
            AppendMessage {
                id: Uuid::new_v4().to_string(),
                session_id: session_id.clone(),
                role: "user".to_string(),
                content: content.clone(),
                client_msg_id: client_msg_id.clone(),
                status: "complete".to_string(),
            },
        )
        .await
    {
        Ok(r) => r,
        Err(e) => {
            state
                .oxpilot
                .turns()
                .await
                .release(&session_id, &turn.turn_id)
                .await;
            tracing::error!(error = %e.message, "oxpilot user-message persist failed");
            return err_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "oxpilot.internal",
                "message persist failed",
            );
        }
    };
    turn.started_seq
        .store(user_row.seq, std::sync::atomic::Ordering::SeqCst);

    // Detach-to-complete turn task (BR4.6) — owns the assistant row to a
    // terminal status; the wrapper owns release + the locked
    // drain-and-chain (BR5.3/BR4.1). The prompt comes from the
    // transcript window — `content` isn't passed down.
    crate::runtime::spawn_turn_task(state.clone(), session.clone(), turn.clone());

    attach_stream(turn).await
}

/// `GET /api/oxpilot/sessions/{id}/chat/stream` — `snapshot` first (BR4.4),
/// then attach to a live turn or idle on ping cadence until one appears.
pub async fn chat_stream(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    headers: HeaderMap,
) -> Response {
    let user_id = match session_user(&state, &headers).await {
        Ok(u) => u,
        Err(r) => return r,
    };
    if let Err(r) = require_gate(&state).await {
        return r;
    }
    if let Err(r) = owned_session(&state, &session_id, &user_id).await {
        return r;
    }
    let frame = match snapshot_frame(&state, &user_id, &session_id).await {
        Ok(f) => f,
        Err(r) => return r,
    };

    // Did the snapshot we are about to send advertise a live turn? If that
    // turn terminal+releases before the first attach poll, the reader must
    // not idle on pings forever — re-snapshot so it sees the terminal row.
    let mut advertised = frame
        .get("active_turn")
        .map(|v| !v.is_null())
        .unwrap_or(false);
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<sse::Event, Infallible>>(16);
    let user_id = user_id.clone();
    tokio::spawn(async move {
        if tx.send(Ok(frame_event(&frame))).await.is_err() {
            return;
        }
        let mut attach_poll = tokio::time::interval(ATTACH_POLL);
        attach_poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut ping = tokio::time::interval(PING_INTERVAL);
        ping.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut attached: Option<broadcast::Receiver<Value>> = None;
        loop {
            tokio::select! {
                r = async {
                    match attached.as_mut() {
                        Some(rx) => rx.recv().await,
                        None => std::future::pending().await,
                    }
                } => {
                    match r {
                        Ok(frame) => {
                            let terminal = frame.get("type").and_then(|t| t.as_str()) == Some("terminal");
                            if tx.send(Ok(frame_event(&frame))).await.is_err() {
                                return;
                            }
                            if terminal {
                                return;
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(_)) => {
                            // Reader-specific resync — replay the turn's
                            // buffer; every frame is intact there, and the
                            // client's (message_id, seq) dedupe drops the
                            // overlap. Turn already gone ⇒ snapshot.
                            match state.oxpilot.turns().await.get(&session_id).await {
                                Some(turn) => {
                                    let buffered = turn.emitted_deltas.lock().await.clone();
                                    for f in &buffered {
                                        let done = f.get("type").and_then(|t| t.as_str()) == Some("terminal");
                                        if tx.send(Ok(frame_event(f))).await.is_err() || done {
                                            return;
                                        }
                                    }
                                }
                                None => {
                                    if let Ok(f) = snapshot_frame(&state, &user_id, &session_id).await {
                                        if tx.send(Ok(frame_event(&f))).await.is_err() {
                                            return;
                                        }
                                    }
                                }
                            }
                        }
                        Err(broadcast::error::RecvError::Closed) => {
                            attached = None;
                        }
                    }
                }
                _ = attach_poll.tick() => {
                    if attached.is_none() {
                        if let Some(turn) = state.oxpilot.turns().await.get(&session_id).await {
                            // Subscribe BEFORE reading the buffer so a delta
                            // emitted mid-attach arrives live (a duplicate is
                            // safe — clients dedupe by message_id+seq — a
                            // missed frame is not). The lock is dropped before
                            // sends so a stalled reader can't back-pressure
                            // the turn task's pushes.
                            let rx_turn = turn.fanout.subscribe();
                            let buffered = turn.emitted_deltas.lock().await.clone();
                            for f in &buffered {
                                let ended = f.get("type").and_then(|t| t.as_str()) == Some("terminal");
                                if tx.send(Ok(frame_event(f))).await.is_err() || ended {
                                    return;
                                }
                            }
                            attached = Some(rx_turn);
                        } else if advertised {
                            // The turn our snapshot promised is gone — its
                            // terminal already persisted, so a fresh snapshot
                            // carries the reader to the terminal row.
                            match snapshot_frame(&state, &user_id, &session_id).await {
                                Ok(f) => {
                                    advertised = f
                                        .get("active_turn")
                                        .map(|v| !v.is_null())
                                        .unwrap_or(false);
                                    if tx.send(Ok(frame_event(&f))).await.is_err() {
                                        return;
                                    }
                                }
                                Err(_) => advertised = false,
                            }
                        }
                    }
                }
                _ = ping.tick() => {
                    if tx.send(Ok(ping_event())).await.is_err() {
                        return;
                    }
                }
            }
        }
    });
    sse_response(rx)
}

/// Does this POST describe the message that owns the live turn? The
/// persisted row wins (`started_seq == prior.seq`); inside the
/// claim→insert window the turn's pending fields stand in. A `None`
/// `client_msg_id` only matches byte-identical content on a turn that
/// hasn't started streaming — a tokenless later duplicate gets 409.
fn resend_matches(
    turn: &ActiveTurn,
    client_msg_id: Option<&str>,
    content: &str,
    prior_seq: Option<i64>,
) -> bool {
    if let Some(seq) = prior_seq {
        if turn.started_seq.load(std::sync::atomic::Ordering::SeqCst) == seq {
            return true;
        }
    }
    if let Some(c) = client_msg_id {
        return turn.pending_client_msg_id.as_deref() == Some(c);
    }
    turn.started_seq.load(std::sync::atomic::Ordering::SeqCst) == 0
        && turn.pending_content == content
}

/// The POST-side reader: subscribe first, then replay buffered frames
/// (subscribe-first can only duplicate — clients dedupe by
/// `message_id`+`seq` — while buffer-first can lose a frame emitted in
/// the clone→subscribe window). Ends on terminal (C2 — a stream ends
/// only there or on transport close); a Lagged reader replays the same
/// intact buffer, same as the GET path.
async fn attach_stream(turn: Arc<ActiveTurn>) -> Response {
    let mut rx_live = turn.fanout.subscribe();
    let buffered: Vec<Value> = turn.emitted_deltas.lock().await.clone();
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<sse::Event, Infallible>>(16);
    tokio::spawn(async move {
        for f in &buffered {
            let ended = f.get("type").and_then(|t| t.as_str()) == Some("terminal");
            if tx.send(Ok(frame_event(f))).await.is_err() || ended {
                return;
            }
        }
        let mut tick = tokio::time::interval(PING_INTERVAL);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                r = rx_live.recv() => {
                    match r {
                        Ok(frame) => {
                            let terminal = frame.get("type").and_then(|t| t.as_str()) == Some("terminal");
                            if tx.send(Ok(frame_event(&frame))).await.is_err() {
                                return;
                            }
                            if terminal {
                                return;
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(_)) => {
                            // Same resync as the GET path — the turn's
                            // buffer holds every frame intact; client-side
                            // (message_id, seq) dedupe drops the overlap.
                            let buffered = turn.emitted_deltas.lock().await.clone();
                            for f in &buffered {
                                let ended = f.get("type").and_then(|t| t.as_str()) == Some("terminal");
                                if tx.send(Ok(frame_event(f))).await.is_err() || ended {
                                    return;
                                }
                            }
                        }
                        Err(broadcast::error::RecvError::Closed) => return,
                    }
                }
                _ = tick.tick() => {
                    if tx.send(Ok(ping_event())).await.is_err() {
                        return;
                    }
                }
            }
        }
    });
    sse_response(rx)
}

/// Idempotent-resend answer for an ended turn — one snapshot frame carrying
/// the persisted transcript, then close (BR3.6).
async fn single_frame_stream(frame: Value) -> Response {
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<sse::Event, Infallible>>(1);
    let _ = tx.send(Ok(frame_event(&frame))).await;
    drop(tx);
    sse_response(rx)
}

fn frame_event(frame: &Value) -> sse::Event {
    sse::Event::default().data(frame.to_string())
}

fn ping_event() -> sse::Event {
    frame_event(&json!({ "type": "ping" }))
}

fn sse_response(rx: tokio::sync::mpsc::Receiver<Result<sse::Event, Infallible>>) -> Response {
    let stream = futures_util::stream::unfold(rx, |mut rx| async move {
        rx.recv().await.map(|item| (item, rx))
    });
    Sse::new(stream)
        .keep_alive(
            sse::KeepAlive::new()
                .interval(PING_INTERVAL)
                .text("keepalive"),
        )
        .into_response()
}
