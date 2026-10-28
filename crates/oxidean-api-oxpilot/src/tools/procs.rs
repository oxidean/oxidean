//! U5 — the `oxpilot.diff_{list,get,apply}` USER procs (C1 additive
//! surface, BR5). These are proc-layer handlers, NOT tools: only a
//! user's own proc call reaches `diff_apply` — that explicit-confirm
//! boundary is structural (BR5.1), no catalog entry maps here.
//!
//! The apply sequence runs on the CALLER's own `ctx.rpc_dispatch`
//! (BR5.3 — the real session ctx, not a synthesized one): branch →
//! ordered file ops (single batched upload for creates/updates) → PR.
//! Mid-sequence failure returns `apply_failed:<step>` with no rollback
//! and no retry (BR5.7); the default branch is never a head_ref by
//! construction (BR5.6).

use std::sync::Arc;

use oxidean_api_core::audit;
use oxidean_api_core::auth::gate::require_verified;
use oxidean_api_core::oxpilot::{require_enabled, SessionService};
use oxidean_api_core::rpc::RpcCtx;
use oxidean_core::AppError;
use oxidean_core::{RpcRequest, RpcResponse};
use oxidean_db::OxpilotProposalRow;
use serde::Deserialize;
use serde_json::{json, Value};

use super::propose::derive_files;
use super::APPLY_BRANCH_PREFIX;

fn svc(ctx: &RpcCtx) -> Result<Arc<dyn SessionService>, AppError> {
    ctx.oxpilot
        .clone()
        .ok_or_else(|| AppError::new("oxpilot.internal", "oxpilot runtime unavailable"))?
        .sessions()
}

fn bad_input(msg: impl Into<String>) -> AppError {
    AppError::new("rpc.bad_input", msg.into())
}

fn parse<T: for<'a> Deserialize<'a>>(input: Value, proc: &str) -> Result<T, AppError> {
    serde_json::from_value(input).map_err(|e| bad_input(format!("invalid {proc} input: {e}")))
}

/// `ProposedDiff` wire shape (C1) — `files[{path, patch}]` derives
/// from `diff_body` headers; `ops_json` never reaches the wire.
fn diff_wire(row: &OxpilotProposalRow) -> Value {
    json!({
        "id": row.id,
        "session_id": row.session_id,
        "title": row.title,
        "base": row.base,
        "status": row.status,
        "pr_ref": row.pr_ref,
        "files": derive_files(&row.diff_body),
        "created_at": row.created_at,
    })
}

#[derive(Debug, Deserialize)]
struct DiffListReq {
    session_id: String,
}

/// `oxpilot.diff_list{session_id}` → `{diffs: \[ProposedDiff\]}` —
/// owned-session read, newest-first (the store verb's ordering).
pub async fn diff_list(ctx: &RpcCtx, input: Value) -> Result<Value, AppError> {
    require_enabled(ctx).await?;
    let user = require_verified(ctx).await?;
    let req: DiffListReq = parse(input, "oxpilot.diff_list")?;
    // Ownership gate — foreign/absent session ids resolve identically.
    svc(ctx)?
        .get_session(&user.id, &req.session_id)
        .await
        .map_err(|_| AppError::new("not_found", "session not found"))?;
    let rows = ctx
        .db
        .find_oxpilot_proposals_by_session(&req.session_id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "oxpilot diff_list db error");
            AppError::new("oxpilot.internal", "session operation failed")
        })?;
    Ok(json!({ "diffs": rows.iter().map(diff_wire).collect::<Vec<_>>() }))
}

#[derive(Debug, Deserialize)]
struct DiffIdReq {
    proposal_id: String,
}

/// `oxpilot.diff_get{proposal_id}` → `ProposedDiff (+pr_ref)` —
/// `svc.proposal` is user-scoped and that IS the contract here (BR5.2);
/// the same-session pin applies only to the model-facing tool read.
pub async fn diff_get(ctx: &RpcCtx, input: Value) -> Result<Value, AppError> {
    require_enabled(ctx).await?;
    let user = require_verified(ctx).await?;
    let req: DiffIdReq = parse(input, "oxpilot.diff_get")?;
    let row = svc(ctx)?.proposal(&user.id, &req.proposal_id).await?;
    Ok(diff_wire(&row))
}

/// One `ops_json` element — deserialized verbatim; the propose path
/// already bound + coherence-checked it against `diff_body`.
#[derive(Debug, Deserialize)]
struct DiffFileOp {
    path: String,
    op: String,
    new_path: Option<String>,
    content_b64: Option<String>,
}

/// Dispatch one backing proc on the caller's own ctx; failures land as
/// `apply_failed:<proc>` verbatim (BR5.7). The `branch_exists` case is
/// caught by the `repo.refs` precheck in `diff_apply` — the shipped
/// `repo.branchCreate` reports a name collision as `repo.git_failed`,
/// so no per-proc carve-out exists here.
async fn apply_step(ctx: &mut RpcCtx, proc: &str, input: Value) -> Result<Value, AppError> {
    let dispatch = ctx.rpc_dispatch.clone();
    let resp = dispatch(
        ctx,
        RpcRequest {
            procedure: proc.to_string(),
            input,
        },
    )
    .await;
    match resp {
        RpcResponse::Ok { data, .. } => Ok(data),
        RpcResponse::Err { error, .. } => Err(AppError::new(
            format!("apply_failed:{proc}"),
            format!("{proc} failed: {}", error.message),
        )),
    }
}

/// `oxpilot.diff_apply{proposal_id}` → `{proposal_id, pr_ref}` — the
/// explicit-confirm materialization chain (BR5, WF5).
pub async fn diff_apply(ctx: &mut RpcCtx, input: Value) -> Result<Value, AppError> {
    require_enabled(ctx).await?;
    let user = require_verified(ctx).await?;
    let req: DiffIdReq = parse(input, "oxpilot.diff_apply")?;
    let svc = svc(ctx)?;
    let row = svc.proposal(&user.id, &req.proposal_id).await?;
    if row.status != "pending" {
        return Err(AppError::new("diff_not_pending", "proposal is not pending"));
    }
    // The repo binding comes from the proposal's OWN session (BR5.2 —
    // a same-user other-session proposal applies against its own
    // binding, deliberately).
    let session = svc
        .get_session(&user.id, &row.session_id)
        .await
        .map_err(|_| AppError::new("not_found", "session not found"))?;
    let (owner, name) = (session.repo_owner.clone(), session.repo_name.clone());
    let id8: String = row.id.chars().take(8).collect();
    let branch = format!("{APPLY_BRANCH_PREFIX}{id8}");

    // a0. apply-name prechecks, both before any effect lands:
    // - UNPROTECTED name: `repo.file.commitPolicy` answers whether a
    //   direct commit to `oxpilot/proposal-<id8>` is allowed — a
    //   protected match must fail here, else the `repo.file.*` ops
    //   would silently detour to a generated `web-edit/*` fallback
    //   branch (the RepoFileCommitOptions protected-branch fallback is
    //   excluded) and the PR head_ref would dangle.
    // - ABSENT name: `repo.branchCreate` reports a collision only as
    //   `repo.git_failed`, so existence is checked via `repo.refs` —
    //   a leftover/concurrent `oxpilot/proposal-<id8>` branch surfaces
    //   as the retry-distinguishing `apply_failed:branch_exists`
    //   (BR5.7/NFR4.11-4.12). A true check-then-create race still dies
    //   fail-stop at `apply_failed:repo.branchCreate`.
    let policy = apply_step(
        ctx,
        "repo.file.commitPolicy",
        json!({ "owner": owner, "name": name, "branch": branch }),
    )
    .await?;
    if policy["direct_commit_allowed"].as_bool() != Some(true) {
        return Err(AppError::new(
            "apply_failed:protected_branch",
            "the generated apply branch name is covered by branch protection — apply cannot proceed",
        ));
    }
    let refs = apply_step(ctx, "repo.refs", json!({ "owner": owner, "name": name })).await?;
    let want = format!("refs/heads/{branch}");
    if refs["refs"]
        .as_array()
        .map(|rs| rs.iter().any(|r| r["name"].as_str() == Some(want.as_str())))
        .unwrap_or(false)
    {
        return Err(AppError::new(
            "apply_failed:branch_exists",
            "the apply branch already exists — delete it and retry",
        ));
    }

    // a. fresh namespaced branch at the proposal's base (BR5.3/BR5.6).
    apply_step(
        ctx,
        "repo.branchCreate",
        json!({
            "owner": owner,
            "name": name,
            "branch": branch,
            "start": row.base,
        }),
    )
    .await?;

    // b. ops_json in declared order — all create/update ops batch into
    // ONE `repo.file.upload` commit (≤50, pinned by MAX_PROPOSAL_FILES),
    // issued at the first create/update's position; each
    // delete/rename is its own call in sequence.
    let ops: Vec<DiffFileOp> = serde_json::from_str(&row.ops_json)
        .map_err(|e| AppError::new("oxpilot.internal", format!("ops_json decode: {e}")))?;
    let upload_files: Vec<Value> = ops
        .iter()
        .filter(|o| o.op == "create" || o.op == "update")
        .map(|o| {
            json!({
                "path": o.path,
                "content_base64": o.content_b64,
            })
        })
        .collect();
    let mut upload_done = upload_files.is_empty();
    for op in &ops {
        match op.op.as_str() {
            "create" | "update" => {
                if upload_done {
                    continue;
                }
                apply_step(
                    ctx,
                    "repo.file.upload",
                    json!({
                        "owner": owner,
                        "name": name,
                        "files": upload_files,
                        "message": format!("Apply oxpilot proposal {id8}"),
                        "branch": branch,
                    }),
                )
                .await?;
                upload_done = true;
            }
            "delete" => {
                apply_step(
                    ctx,
                    "repo.file.delete",
                    json!({
                        "owner": owner,
                        "name": name,
                        "path": op.path,
                        "message": format!("Apply oxpilot proposal {id8}"),
                        "branch": branch,
                    }),
                )
                .await?;
            }
            "rename" => {
                apply_step(
                    ctx,
                    "repo.file.rename",
                    json!({
                        "owner": owner,
                        "name": name,
                        "from_path": op.path,
                        "to_path": op.new_path,
                        "message": format!("Apply oxpilot proposal {id8}"),
                        "branch": branch,
                    }),
                )
                .await?;
            }
            other => {
                return Err(AppError::new(
                    "oxpilot.internal",
                    format!("unknown op {other} in ops_json"),
                ));
            }
        }
    }

    // c. the PR — body = title + summary + the fixed provenance line
    // (BR5.4; carved out of the no-AI-attribution convention as a
    // session fact, not an author claim).
    let pr = apply_step(
        ctx,
        "pull.create",
        json!({
            "owner": owner,
            "name": name,
            "base_ref": row.base,
            "head_ref": branch,
            "title": row.title,
            "body": format!(
                "{}\n\nProduced by an /oxpilot agent session: {} (proposal {}).",
                row.title, row.session_id, row.id
            ),
        }),
    )
    .await?;
    let pr_number = pr["number"].as_i64().filter(|n| *n > 0).ok_or_else(|| {
        AppError::new(
            "oxpilot.internal",
            "pull.create returned no positive number",
        )
    })?;
    let pr_ref = format!("{owner}/{name}#{pr_number}");

    // BR5.5 — mark applied + audit + return the correlation ref.
    svc.mark_applied(&user.id, &req.proposal_id, &pr_ref)
        .await?;
    audit::record(
        ctx,
        Some((&user.id, &user.username)),
        "oxpilot.diff_applied",
        Some(("oxpilot_proposal", &req.proposal_id)),
        Some(
            json!({
                "session_id": row.session_id,
                "proposal_id": row.id,
                "branch": branch,
                "pr_ref": pr_ref,
            })
            .to_string(),
        ),
    )
    .await;
    Ok(json!({ "proposal_id": row.id, "pr_ref": pr_ref }))
}
