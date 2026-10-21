//! Ready-time scheduling — the second half of the needs/matrix state
//! machine. `enqueue_run` inserts every job row up front (`waiting` for
//! needs-unmet jobs and over-cap matrix legs); this module flips `waiting`
//! rows to `queued`/`skipped` as upstream legs reach terminal states.
//!
//! GitHub semantics implemented:
//! - A `waiting` job runs only when every `needs:` key is fully terminal
//!   (all its matrix legs included).
//! - `needs.<id>.result` aggregates legs: failure > cancelled > skipped >
//!   success.
//! - Implicit `success()`: a job whose `if` lacks a status-check function
//!   is skipped when any needed leg didn't succeed — matching GitHub.
//! - `if` with a status fn (`always()`, `failure()`, …) evaluates against
//!   the real `needs` context; `if` absent + all-success → `queued`.
//! - `fail-fast` (default true): one leg's failure cancels its siblings.
//! - `max-parallel`: legs beyond the cap stay `waiting` until a slot frees.
//! - `outputs:` materialize on job completion so `needs.<id>.outputs.*`
//!   resolves for dependents and steps.
//!
//! Every pass is monotone (waiting jobs only move forward) and
//! `recompute_ready_jobs` loops to a fixpoint — a `skipped`/`failure`
//! decision can itself close out downstream `needs`.

use serde_json::{json, Map, Value as JsonValue};

use oxidean_actions_expr::{eval_bool, Context, JobStatus};
use oxidean_db::{ActionJobRow, ActionRunRow, Database};

/// All statuses that mean "no more work will happen on this row".
fn terminal(status: &str) -> bool {
    matches!(status, "success" | "failure" | "cancelled" | "skipped")
}

/// Recompute `waiting` jobs to a fixpoint — after a job's terminal
/// transition, after enqueue (jobs whose needs were skipped at enqueue
/// resolve immediately), and after requeue. Returns the number of jobs
/// newly flipped to `queued` (callers may log/notify on them).
pub async fn recompute_ready_jobs(db: &Database, run_id: &str) -> Result<usize, String> {
    // Fast path: exit when no pass could change anything — no `waiting`
    // job for needs/max-parallel to release AND no failed matrix leg with
    // live siblings for fail-fast to cancel. Skips SharedCtx's
    // repo/owner/user/secrets query chain — the common terminal-update
    // and enqueue paths stay shallow (this function runs inside
    // already-deep handler chains).
    {
        let jobs = db.list_action_jobs_for_run(run_id).await?;
        // Approval-held jobs are `waiting` but invisible to promotion —
        // a fully held run takes this fast-path exit until approve_run
        // clears the flag and the next pass re-evaluates.
        let any_waiting = jobs
            .iter()
            .any(|j| j.status == "waiting" && !j.approval_hold);
        let fail_fast_candidate = jobs.iter().any(|f| {
            f.status == "failure"
                && !f.matrix_key.is_empty()
                && jobs.iter().any(|l| {
                    l.id != f.id
                        && l.job_key == f.job_key
                        && l.caller_job_id == f.caller_job_id
                        && matches!(l.status.as_str(), "queued" | "waiting" | "in_progress")
                })
        });
        // An expanded call sits `in_progress` until its children close it —
        // the fast-path must stay live while a close-out could happen.
        let open_call = jobs.iter().any(|c| !c.caller_job_id.is_empty())
            && jobs.iter().any(|j| j.status == "in_progress");
        if !any_waiting && !fail_fast_candidate && !open_call {
            return Ok(0);
        }
    }
    let shared = SharedCtx::build(db, run_id).await;
    let mut flipped = 0usize;
    loop {
        let mut changed = false;
        changed |= fail_fast_pass(db, run_id).await?;
        let (queued, resolved) = needs_pass(db, run_id, &shared).await?;
        flipped += queued;
        changed |= resolved > 0;
        changed |= call_close_pass(db, run_id).await?;
        changed |= maxpar_pass(db, run_id).await?;
        if !changed {
            return Ok(flipped);
        }
    }
}

/// Run-level data every waiting job's `if` context shares.
struct SharedCtx {
    /// Frozen `github` context for the run (sha/ref/event provenance).
    github: JsonValue,
    /// Repo+org secrets (values) for `secrets.*` gating.
    secrets: JsonValue,
    /// Repo+org plaintext variables for `vars.*` gating.
    vars: JsonValue,
}

impl SharedCtx {
    async fn build(db: &Database, run_id: &str) -> Self {
        let run = db.find_action_run_by_id(run_id).await.ok().flatten();
        let github = match &run {
            Some(r) => github_for_run(db, r).await,
            None => json!({}),
        };
        // `secrets` context for `jobs.<id>.if` — repo + org scopes (env
        // secrets resolve per-leg at claim, not at run-level eval). No audit
        // event: values stay server-side for expression gating, the claim
        // path is what leaves the trust boundary.
        let secrets = match &run {
            Some(r) => {
                crate::actions::secrets::decrypted_secrets_for_job(db, &r.repository_id, None, None)
                    .await
                    .map(|m| serde_json::to_value(m).unwrap_or_else(|_| json!({})))
                    .unwrap_or_else(|_| json!({}))
            }
            None => json!({}),
        };
        // `vars` context — repo + org plaintext variables (env-scoped vars
        // join per-leg at claim, like env secrets).
        let vars = match &run {
            Some(r) => crate::actions::vars::variables_for_context(db, &r.repository_id)
                .await
                .unwrap_or_else(|_| json!({})),
            None => json!({}),
        };
        SharedCtx {
            github,
            secrets,
            vars,
        }
    }

    /// Per-job `if` context: github + secrets + job env (workflow env under
    /// job env) + this leg's `matrix` + real `needs` + `job.status` = the
    /// aggregated needs result (drives `success()`/`failure()`/`cancelled()`).
    /// Callee legs get `inputs`/`github.event.inputs` from `call_inputs` and
    /// their `secrets` remapped per the call's `secrets:` spec.
    async fn job_ctx(
        &self,
        db: &Database,
        job: &ActionJobRow,
        needs_ctx: JsonValue,
        job_status: JobStatus,
    ) -> Context {
        let payload = payload_for(db, job).await.unwrap_or_else(|| json!({}));
        let call_inputs = payload.get("call_inputs").filter(|v| !v.is_null());
        let github = match call_inputs {
            Some(inputs) => {
                let mut g = self.github.as_object().cloned().unwrap_or_default();
                if let Some(JsonValue::Object(ev)) = g.get("event").cloned() {
                    let mut ev = ev;
                    ev.insert("inputs".into(), inputs.clone());
                    g.insert("event".into(), JsonValue::Object(ev));
                }
                JsonValue::Object(g)
            }
            None => self.github.clone(),
        };
        let secrets = if !job.caller_job_id.is_empty() {
            let spec = payload
                .get("call_secrets")
                .cloned()
                .unwrap_or(JsonValue::Null);
            let scope = self.secrets.as_object().cloned().unwrap_or_default();
            JsonValue::Object(crate::actions::reusable::remap_call_secrets(&scope, &spec))
        } else {
            self.secrets.clone()
        };
        // `inputs` mirrors `github.event.inputs` (build_context parity):
        // callee legs see the call's resolved inputs; everything else sees
        // the dispatch payload's.
        let inputs = call_inputs.cloned().unwrap_or_else(|| {
            github
                .get("event")
                .and_then(|e| e.get("inputs"))
                .cloned()
                .unwrap_or_else(|| json!({}))
        });
        let mut ctx = Context::new()
            .with_context("github", github)
            .with_context("secrets", secrets)
            .with_context("vars", self.vars.clone())
            .with_context("inputs", inputs)
            .with_context("needs", needs_ctx)
            .with_context(
                "matrix",
                serde_json::from_str(&job.matrix_json).unwrap_or_else(|_| json!({})),
            )
            .with_context("steps", json!({}))
            .with_context("env", json!({}))
            .with_job_status(match job_status {
                JobStatus::Success => "success",
                JobStatus::Failure => "failure",
                JobStatus::Cancelled => "cancelled",
                JobStatus::Skipped => "skipped",
                JobStatus::Pending => "pending",
            });
        // env layers: workflow env evaluated first, then job env over it —
        // same layering the runner applies at step time.
        for key in ["workflow_env", "env"] {
            if let Some(v) = payload.get(key) {
                if let Ok(evaled) = oxidean_actions_expr::eval_json(v, &ctx) {
                    ctx.overlay_env(&evaled);
                }
            }
        }
        ctx
    }
}

/// Rebuild the `github` context for a stored run — mirrors
/// `eval_ctx::github_ctx` (provenance from the run row, not the request).
async fn github_for_run(db: &Database, run: &ActionRunRow) -> JsonValue {
    let repo = db
        .find_repository_by_id(&run.repository_id)
        .await
        .ok()
        .flatten();
    let (owner_slug, repo_name) = match &repo {
        Some(r) => {
            let slug = crate::repo::owner_ref_for_repo(db, r)
                .await
                .ok()
                .flatten()
                .map(|o| o.slug().to_string())
                .unwrap_or_default();
            (slug, r.name.clone())
        }
        None => (String::new(), String::new()),
    };
    let (actor, actor_id) = match &run.triggered_by {
        Some(uid) => {
            let name = db
                .find_user_by_id(uid)
                .await
                .ok()
                .flatten()
                .map(|u| u.username)
                .unwrap_or_default();
            (name, uid.clone())
        }
        None => (String::new(), String::new()),
    };
    let ref_name = run
        .head_ref
        .strip_prefix("refs/heads/")
        .or_else(|| run.head_ref.strip_prefix("refs/tags/"))
        .unwrap_or(&run.head_ref);
    json!({
        "sha": run.head_sha,
        "ref": run.head_ref,
        "ref_name": ref_name,
        "ref_type": if run.head_ref.starts_with("refs/tags/") { "tag" } else { "branch" },
        "event_name": run.event,
        "event": run
            .event_payload_json
            .as_deref()
            .and_then(|s| serde_json::from_str::<JsonValue>(s).ok())
            .unwrap_or_else(|| json!({})),
        "repository": format!("{owner_slug}/{repo_name}"),
        "repository_owner": owner_slug,
        "actor": actor,
        "actor_id": actor_id,
        "workflow": run.workflow_name,
        "run_id": run.id,
        "run_number": run.run_number,
        "run_attempt": run.attempt,
    })
}

/// ── fail-fast: a failed leg cancels its same-key siblings ──────────────
/// Returns true if anything was cancelled this pass.
async fn fail_fast_pass(db: &Database, run_id: &str) -> Result<bool, String> {
    let jobs = db.list_action_jobs_for_run(run_id).await?;
    let mut failed_keys: Vec<String> = Vec::new();
    for j in &jobs {
        if j.status == "failure" && !j.matrix_key.is_empty() && !j.continue_on_error {
            let payload = payload_for(db, j).await;
            let ff = payload
                .as_ref()
                .and_then(|p| p.get("fail_fast").and_then(|v| v.as_bool()))
                .unwrap_or(true);
            if ff && !failed_keys.contains(&j.job_key) {
                failed_keys.push(j.job_key.clone());
            }
        }
    }
    if failed_keys.is_empty() {
        return Ok(false);
    }
    let failed: Vec<&ActionJobRow> = jobs
        .iter()
        .filter(|j| j.status == "failure" && failed_keys.contains(&j.job_key))
        .collect();
    let mut changed = false;
    for j in &jobs {
        if failed
            .iter()
            .any(|f| f.job_key == j.job_key && f.caller_job_id == j.caller_job_id)
            && matches!(j.status.as_str(), "queued" | "waiting" | "in_progress")
        {
            db.update_action_job_status(&j.id, "cancelled").await?;
            let _ = db.revoke_action_job_token_for_job(&j.id).await;
            // Server-side cancel — the runner path never publishes this.
            let origin = std::env::var("OXIDEAN_PUBLIC_ORIGIN").ok();
            let _ = crate::actions::statuses::publish_from_job_update(
                db,
                &j.id,
                "cancelled",
                origin.as_deref(),
            )
            .await;
            changed = true;
        }
    }
    Ok(changed)
}

/// ── needs-gated waiting jobs — returns (newly queued, newly resolved) —
/// `resolved` counts skipped/failed transitions too: a skipped need can
/// itself close out a downstream job's gate, so the fixpoint must see it.
async fn needs_pass(
    db: &Database,
    run_id: &str,
    shared: &SharedCtx,
) -> Result<(usize, usize), String> {
    let jobs = db.list_action_jobs_for_run(run_id).await?;
    let mut queued = 0usize;
    let mut resolved = 0usize;
    let run = db.find_action_run_by_id(run_id).await.ok().flatten();
    for job in &jobs {
        if job.status != "waiting" || job.approval_hold {
            continue;
        }
        let payload = payload_for(db, job).await.unwrap_or_else(|| json!({}));
        let is_call = payload.get("call").is_some_and(|c| c.is_object());
        let needs: Vec<String> = serde_json::from_str(&job.needs_json).unwrap_or_default();
        if needs.is_empty() && !is_call {
            continue; // max-parallel leg — handled separately
        }
        // Every leg of every needed key must be terminal — scoped to the
        // caller group so callee-internal `needs` resolve naturally.
        let all_terminal = needs.iter().all(|key| {
            let legs: Vec<&ActionJobRow> = jobs
                .iter()
                .filter(|x| &x.job_key == key && x.caller_job_id == job.caller_job_id)
                .collect();
            !legs.is_empty() && legs.iter().all(|l| terminal(&l.status))
        });
        if !all_terminal {
            continue;
        }
        match decide_waiting_job(db, job, &jobs, &needs, shared).await {
            WaitDecision::Run => {
                if is_call {
                    // Reusable-workflow call — expand the callee file into
                    // durable callee rows; the caller leg sits `in_progress`
                    // until call_close_pass rolls its children up.
                    let ctx = shared
                        .job_ctx(db, job, JsonValue::Null, JobStatus::Success)
                        .await;
                    let outcome = match &run {
                        Some(r) => {
                            crate::actions::reusable::expand_call(db, r, job, &payload, &ctx).await
                        }
                        None => Err("run row missing".into()),
                    };
                    match outcome {
                        Ok(crate::actions::reusable::ExpandOutcome::Expanded) => {}
                        Ok(crate::actions::reusable::ExpandOutcome::Failed(msg)) => {
                            let _ = crate::actions::logs::append_job_log(
                                &crate::actions::logs::resolve_log_dir(),
                                run_id,
                                &job.id,
                                format!("::error::{msg}\n").as_bytes(),
                            )
                            .await;
                            db.update_action_job_status(&job.id, "failure").await?;
                            let origin = std::env::var("OXIDEAN_PUBLIC_ORIGIN").ok();
                            let _ = crate::actions::statuses::publish_from_job_update(
                                db,
                                &job.id,
                                "failure",
                                origin.as_deref(),
                            )
                            .await;
                        }
                        Err(e) => {
                            let _ = crate::actions::logs::append_job_log(
                                &crate::actions::logs::resolve_log_dir(),
                                run_id,
                                &job.id,
                                format!("::error::call expansion failed: {e}\n").as_bytes(),
                            )
                            .await;
                            db.update_action_job_status(&job.id, "failure").await?;
                            let origin = std::env::var("OXIDEAN_PUBLIC_ORIGIN").ok();
                            let _ = crate::actions::statuses::publish_from_job_update(
                                db,
                                &job.id,
                                "failure",
                                origin.as_deref(),
                            )
                            .await;
                        }
                    }
                } else if job.deployment_wait_minutes > 0 {
                    // Environment wait timer starts at the ready transition —
                    // stamp the deadline and leave the leg `waiting`; the timer
                    // sweep owns the release, so stamped legs never queue here.
                    if job.deployment_not_before.is_none() {
                        let not_before = (chrono::Utc::now()
                            + chrono::Duration::minutes(job.deployment_wait_minutes))
                        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
                        db.set_job_deployment_timer(&job.id, &not_before).await?;
                    }
                } else {
                    db.update_action_job_status(&job.id, "queued").await?;
                    crate::webhook::dispatch::notify_workflow_job(db, &job.id, "queued").await;
                    crate::webhook::dispatch::notify_deployment_job_status(db, &job.id).await;
                    queued += 1;
                }
            }
            WaitDecision::Skip => {
                db.update_action_job_status(&job.id, "skipped").await?;
                // Skipped jobs never reach a runner — publish the (green)
                // commit status here or the check stays `pending` forever.
                let origin = std::env::var("OXIDEAN_PUBLIC_ORIGIN").ok();
                let _ = crate::actions::statuses::publish_from_job_update(
                    db,
                    &job.id,
                    "skipped",
                    origin.as_deref(),
                )
                .await;
            }
            WaitDecision::Fail => {
                db.update_action_job_status(&job.id, "failure").await?;
                let origin = std::env::var("OXIDEAN_PUBLIC_ORIGIN").ok();
                let _ = crate::actions::statuses::publish_from_job_update(
                    db,
                    &job.id,
                    "failure",
                    origin.as_deref(),
                )
                .await;
            }
        }
        resolved += 1;
    }
    Ok((queued, resolved))
}

/// ── call close-out — `in_progress` caller legs roll up once every callee
/// row is terminal. Status precedence matches matrix legs; declared
/// `workflow_call.outputs` materialize onto the caller's `outputs_json` so
/// top-level `needs.<caller>.outputs` resolves.
async fn call_close_pass(db: &Database, run_id: &str) -> Result<bool, String> {
    let jobs = db.list_action_jobs_for_run(run_id).await?;
    let mut changed = false;
    for caller in &jobs {
        // Caller rows carry a `call` payload marker — distinguish them from
        // claimed runner work (runner_id set) before fetching.
        if caller.runner_id.is_some() {
            continue;
        }
        if !payload_for(db, caller)
            .await
            .is_some_and(|p| p.get("call").is_some_and(|c| c.is_object()))
        {
            continue;
        }
        let children: Vec<ActionJobRow> = jobs
            .iter()
            .filter(|c| c.caller_job_id == caller.id)
            .cloned()
            .collect();
        // A cancelled/failed caller (fail-fast sibling, run cancel) orphans
        // live children — cancel them and revoke their claim tokens.
        if caller.status != "in_progress" {
            if terminal(&caller.status) {
                for c in children.iter().filter(|c| !terminal(&c.status)) {
                    db.update_action_job_status(&c.id, "cancelled").await?;
                    let _ = db.revoke_action_job_token_for_job(&c.id).await;
                    crate::webhook::dispatch::notify_workflow_job(db, &c.id, "completed").await;
                    crate::webhook::dispatch::notify_deployment_job_status(db, &c.id).await;
                    changed = true;
                }
            }
            continue;
        }
        if children.is_empty() || !children.iter().all(|c| terminal(&c.status)) {
            continue;
        }
        let result = crate::actions::reusable::close_call(db, caller, &children).await?;
        let _ = crate::actions::logs::append_job_log(
            &crate::actions::logs::resolve_log_dir(),
            run_id,
            &caller.id,
            format!(
                "::notice::reusable call '{}' completed: {result}\n",
                caller.job_key
            )
            .as_bytes(),
        )
        .await;
        changed = true;
    }
    Ok(changed)
}

/// ── max-parallel release — legs beyond the cap wait for a free slot ────
async fn maxpar_pass(db: &Database, run_id: &str) -> Result<bool, String> {
    let jobs = db.list_action_jobs_for_run(run_id).await?;
    // Legs group by (job_key, caller_job_id) — callee groups never share
    // max-parallel pools across call boundaries.
    let mut keys: Vec<(&str, &str)> = Vec::new();
    for j in &jobs {
        if j.status == "waiting"
            && !j.approval_hold
            && j.needs_json == "[]"
            && !keys.contains(&(j.job_key.as_str(), j.caller_job_id.as_str()))
        {
            keys.push((&j.job_key, j.caller_job_id.as_str()));
        }
    }
    let mut changed = false;
    for (key, caller) in keys {
        let legs: Vec<&ActionJobRow> = jobs
            .iter()
            .filter(|j| j.job_key == key && j.caller_job_id == caller)
            .collect();
        let payload = match legs.first() {
            Some(l) => payload_for(db, l).await,
            None => continue,
        };
        // Caller legs never consume runner slots — needs_pass expanded them
        // already; a stray `waiting` caller here is a payload-miss edge.
        if payload
            .as_ref()
            .is_some_and(|p| p.get("call").is_some_and(|c| c.is_object()))
        {
            continue;
        }
        let cap = payload
            .as_ref()
            .and_then(|p| p.get("max_parallel").and_then(|v| v.as_u64()))
            .map(|n| n as usize);
        // A no-needs leg waits on exactly one gate: a max-parallel slot, a
        // deployment timer, or a released approval hold. With no cap the
        // only remaining waits are timer/approval — unheld unstamped legs
        // queue outright.
        let Some(cap) = cap else {
            for leg in legs.iter().filter(|l| {
                l.status == "waiting" && !l.approval_hold && l.deployment_not_before.is_none()
            }) {
                if leg.deployment_wait_minutes > 0 {
                    let not_before = (chrono::Utc::now()
                        + chrono::Duration::minutes(leg.deployment_wait_minutes))
                    .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
                    db.set_job_deployment_timer(&leg.id, &not_before).await?;
                } else {
                    db.update_action_job_status(&leg.id, "queued").await?;
                    crate::webhook::dispatch::notify_workflow_job(db, &leg.id, "queued").await;
                    crate::webhook::dispatch::notify_deployment_job_status(db, &leg.id).await;
                }
                changed = true;
            }
            continue;
        };
        let active = legs
            .iter()
            .filter(|l| matches!(l.status.as_str(), "queued" | "in_progress"))
            .count();
        let mut room = cap.saturating_sub(active);
        for leg in legs.iter().filter(|l| {
            l.status == "waiting" && !l.approval_hold && l.deployment_not_before.is_none()
        }) {
            if room == 0 {
                break;
            }
            // Timer-gated legs occupy the freed slot while they wait —
            // stamp the deadline instead of queueing.
            if leg.deployment_wait_minutes > 0 {
                let not_before = (chrono::Utc::now()
                    + chrono::Duration::minutes(leg.deployment_wait_minutes))
                .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
                db.set_job_deployment_timer(&leg.id, &not_before).await?;
            } else {
                db.update_action_job_status(&leg.id, "queued").await?;
                crate::webhook::dispatch::notify_workflow_job(db, &leg.id, "queued").await;
                crate::webhook::dispatch::notify_deployment_job_status(db, &leg.id).await;
            }
            room -= 1;
            changed = true;
        }
    }
    Ok(changed)
}

/// What a waiting needs-job becomes once its upstreams are terminal.
enum WaitDecision {
    Run,
    Skip,
    Fail,
}

/// Apply GitHub's needs/if rules to a waiting job.
async fn decide_waiting_job(
    db: &Database,
    job: &ActionJobRow,
    all_jobs: &[ActionJobRow],
    needs: &[String],
    shared: &SharedCtx,
) -> WaitDecision {
    let needs_ctx = build_needs_context(all_jobs, needs, &job.caller_job_id);
    let agg = aggregate_status(all_jobs, needs, &job.caller_job_id);
    let payload = payload_for(db, job).await.unwrap_or_else(|| json!({}));
    let if_src = payload.get("if").and_then(|v| v.as_str()).map(String::from);
    match &if_src {
        // No `if` — implicit success(): run iff every need succeeded.
        None => {
            if agg == JobStatus::Success {
                WaitDecision::Run
            } else {
                WaitDecision::Skip
            }
        }
        Some(src) => {
            let has_status_fn = oxidean_actions_expr::uses_status_fn(src);
            if agg != JobStatus::Success && !has_status_fn {
                // Implicit success() AND'd in — a failed need skips the job
                // without even evaluating the expression.
                return WaitDecision::Skip;
            }
            let ctx = shared.job_ctx(db, job, needs_ctx, agg).await;
            match eval_bool(src, &ctx) {
                Ok(true) => WaitDecision::Run,
                Ok(false) => WaitDecision::Skip,
                Err(_) => WaitDecision::Fail,
            }
        }
    }
}

/// Aggregate the needed keys' leg results into one `JobStatus` — drives
/// `success()`/`failure()`/`cancelled()` inside job-level `if`. Matching is
/// scoped to `caller_id` so callee `needs` never reach across call groups.
fn aggregate_status(all_jobs: &[ActionJobRow], needs: &[String], caller_id: &str) -> JobStatus {
    let mut status = JobStatus::Success;
    for key in needs {
        let legs: Vec<&ActionJobRow> = all_jobs
            .iter()
            .filter(|j| &j.job_key == key && j.caller_job_id == caller_id)
            .collect();
        match leg_result(&legs) {
            "failure" => return JobStatus::Failure,
            "cancelled" => {
                if status == JobStatus::Success || status == JobStatus::Skipped {
                    status = JobStatus::Cancelled;
                }
            }
            "skipped" if status == JobStatus::Success => {
                status = JobStatus::Skipped;
            }
            _ => {}
        }
    }
    status
}

/// `{needs: {<key>: {result, outputs}}}` — aggregated over matrix legs.
/// `caller_id` scopes matching: callee `needs` only see same-call legs.
fn build_needs_context(all_jobs: &[ActionJobRow], needs: &[String], caller_id: &str) -> JsonValue {
    let mut out = Map::new();
    for key in needs {
        let legs: Vec<&ActionJobRow> = all_jobs
            .iter()
            .filter(|j| &j.job_key == key && j.caller_job_id == caller_id)
            .collect();
        let result = leg_result(&legs);
        // Outputs merge across legs — last completed leg wins per key
        // (GitHub's matrix-outputs semantics).
        let mut outputs = Map::new();
        for leg in &legs {
            if let JsonValue::Object(o) =
                serde_json::from_str::<JsonValue>(&leg.outputs_json).unwrap_or(json!({}))
            {
                for (k, v) in o {
                    outputs.insert(k, v);
                }
            }
        }
        out.insert(
            key.clone(),
            json!({"result": result, "outputs": JsonValue::Object(outputs)}),
        );
    }
    JsonValue::Object(out)
}

/// Aggregate a job_key's legs into one `result` — failure > cancelled >
/// skipped > success.
fn leg_result(legs: &[&ActionJobRow]) -> &'static str {
    let mut result = "success";
    for l in legs {
        match l.status.as_str() {
            "failure" => return "failure",
            "cancelled" => {
                if result != "failure" {
                    result = "cancelled";
                }
            }
            "skipped" if result == "success" => {
                result = "skipped";
            }
            _ => {}
        }
    }
    result
}

/// Fetch and parse a job's durable payload (`{}` on any miss — callers
/// treat missing fields as absent).
async fn payload_for(db: &Database, job: &ActionJobRow) -> Option<JsonValue> {
    let sha = job.payload_sha.as_deref()?;
    let p = db.find_action_payload(sha).await.ok().flatten()?;
    serde_json::from_str(&p.payload_json).ok()
}

/// Materialize `jobs.<id>.outputs` against the runner-reported step
/// outputs — called when a job reaches a terminal state. Returns the
/// evaluated map (also persisted on `action_jobs.outputs_json`).
///
/// `step_outputs` is the runner's `{step_id: {name: value}}` report; the
/// `steps` context wraps it as `steps.<id>.outputs.<name>`.
pub async fn materialize_outputs(
    db: &Database,
    job: &ActionJobRow,
    step_outputs: &JsonValue,
) -> Result<JsonValue, String> {
    let declared: Map<String, JsonValue> = serde_json::from_str::<JsonValue>(&job.outputs_json)
        .ok()
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default();
    if declared.is_empty() {
        return Ok(json!({}));
    }
    let mut steps_ctx = Map::new();
    if let JsonValue::Object(m) = step_outputs {
        for (id, outs) in m {
            steps_ctx.insert(id.clone(), json!({"outputs": outs}));
        }
    }
    let ctx = Context::new().with_context("steps", JsonValue::Object(steps_ctx));
    let mut out = Map::new();
    for (name, src) in declared {
        let expr = src.as_str().unwrap_or_default();
        match oxidean_actions_expr::eval_embedded(expr, &ctx) {
            Ok(v) => {
                out.insert(name, v);
            }
            Err(e) => {
                return Err(format!("job output '{name}': {e}"));
            }
        }
    }
    let evaluated = JsonValue::Object(out);
    db.set_action_job_outputs(&job.id, &evaluated.to_string())
        .await?;
    Ok(evaluated)
}

/// A `needs` context for `fetch_task` — dependents see upstream results
/// and materialized outputs in their step expressions. `caller_id` scopes
/// the lookup within a reusable-workflow call group.
pub fn needs_context_for_task(
    all_jobs: &[ActionJobRow],
    needs_json: &str,
    caller_id: &str,
) -> Option<JsonValue> {
    let needs: Vec<String> = serde_json::from_str(needs_json).unwrap_or_default();
    if needs.is_empty() {
        return None;
    }
    Some(build_needs_context(all_jobs, &needs, caller_id))
}
