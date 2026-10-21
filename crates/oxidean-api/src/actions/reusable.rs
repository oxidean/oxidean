//! Reusable-workflow calls (P13) — `jobs.<id>.uses` expansion and close-out.
//!
//! Semantics (GitHub parity):
//!
//! - A calling job never runs on a runner. When its `needs` resolve and its
//!   `if` passes, the scheduler **expands** it: the callee file is read via
//!   `cat_blob`, parsed, validated, and its jobs are emitted as durable rows
//!   in the *same run*, linked by `caller_job_id`. The caller leg then sits
//!   `in_progress` until every child is terminal, and closes with the
//!   children's aggregated result (failure > cancelled; `skipped` only when
//!   every child skipped — the same convention `recompute_run_status` uses).
//! - `needs` inside the callee resolve within the caller group only —
//!   `caller_job_id` equality scopes every needs/fail-fast match, so callee
//!   job keys never collide with top-level keys and `needs.<id>.outputs`
//!   resolves naturally.
//! - `with:` inputs evaluate against the *caller's* context at expansion
//!   (real `needs`/`matrix`/`inputs` visible), merge over the callee's
//!   `workflow_call.inputs` defaults, and land on callee payloads as
//!   `call_inputs` — claim patches them into `github.event.inputs`, which is
//!   also the `inputs` context. Missing required inputs, undeclared inputs,
//!   and type mismatches fail the caller leg loudly.
//! - `secrets:` — absent = callee sees no repo secrets; `inherit` forwards
//!   the caller's full scope; a map forwards each callee name from the
//!   evaluated caller-scope expression value. The *spec* rides the callee
//!   payload — values never persist; remap happens at claim against the
//!   decrypted caller scope (same run → same repository).
//! - `workflow_call.outputs` declarations ride the caller row's
//!   `outputs_json`; close-out evaluates them against a `jobs.<key>.outputs`
//!   context built from the children.
//! - Callers can't carry `runs-on`/`steps`/`env`/`environment`/`container`/
//!   `services`/`timeout-minutes`/`continue-on-error`/`outputs` — parse
//!   rejects them (GitHub's supported-keyword list for calling jobs).

use serde_json::{json, Map, Value as JsonValue};

use oxidean_actions_expr::{eval_embedded, Context};
use oxidean_db::{ActionJobRow, ActionRunRow, Database};
use oxidean_git::{CliGitBackend, GitBackend};

use crate::actions::dispatch::{bare_repo_path, emit_jobs, CallerEmit, EmitCtx};
use crate::actions::parse::{self};

/// GitHub's documented nesting ceiling for reusable-workflow calls.
const MAX_CALL_DEPTH: u32 = 4;
/// Hard bound on callee legs in one run — guards fan-out explosions even
/// below the depth cap (matrix × calls multiply rows quickly).
const MAX_CALLEE_JOBS: usize = 256;

/// Parsed `uses:` target.
#[derive(Debug, Clone, PartialEq)]
pub enum UsesRef {
    /// `./.github/workflows/x.yml` — caller repo at `run.head_sha`.
    Local { path: String },
    /// `{owner}/{repo}/.github/workflows/x.yml@{ref}` — another repo on this
    /// instance at `git_ref`.
    Remote {
        owner: String,
        repo: String,
        path: String,
        git_ref: String,
    },
}

/// Parse a `uses:` spec into a resolvable target. Rejects everything GitHub
/// rejects for workflow calls (docker://, missing refs, non-workflow paths).
pub fn parse_uses(spec: &str) -> Result<UsesRef, String> {
    let spec = spec.trim();
    if spec.is_empty() {
        return Err("empty uses:".into());
    }
    if spec.starts_with("docker://") {
        return Err(format!(
            "uses '{spec}': docker targets aren't supported for workflow calls"
        ));
    }
    if let Some(path) = spec.strip_prefix("./") {
        let path = path.trim();
        validate_workflow_path(path, spec)?;
        return Ok(UsesRef::Local {
            path: path.to_string(),
        });
    }
    // Remote: {owner}/{repo}/{path}@{ref} — path must live under
    // .github/workflows/ and the ref is mandatory for remote targets.
    let (head, git_ref) = spec
        .rsplit_once('@')
        .ok_or_else(|| format!("uses '{spec}': remote calls need an @ref"))?;
    let git_ref = git_ref.trim();
    if git_ref.is_empty() {
        return Err(format!("uses '{spec}': empty @ref"));
    }
    let mut segs = head.split('/');
    let owner = segs.next().unwrap_or_default().trim();
    let repo = segs.next().unwrap_or_default().trim();
    let path = segs.collect::<Vec<_>>().join("/");
    if owner.is_empty() || repo.is_empty() {
        return Err(format!(
            "uses '{spec}': expected {{owner}}/{{repo}}/{{path}}@{{ref}}"
        ));
    }
    validate_workflow_path(&path, spec)?;
    Ok(UsesRef::Remote {
        owner: owner.to_string(),
        repo: repo.to_string(),
        path,
        git_ref: git_ref.to_string(),
    })
}

/// Workflow files must live under `.github/workflows/` and end .yml/.yaml —
/// GitHub rejects anything else for `uses:`.
fn validate_workflow_path(path: &str, spec: &str) -> Result<(), String> {
    let p = path.trim_matches('/');
    if p.is_empty() || p.contains("..") {
        return Err(format!("uses '{spec}': invalid path '{path}'"));
    }
    if !p.starts_with(".github/workflows/") {
        return Err(format!(
            "uses '{spec}': workflow path must live under .github/workflows/"
        ));
    }
    if !(p.ends_with(".yml") || p.ends_with(".yaml")) {
        return Err(format!("uses '{spec}': workflow path must end .yml/.yaml"));
    }
    Ok(())
}

/// Bare-repo root — same env source `AppState` resolves at boot; the
/// scheduler path runs outside request scope so it mirrors that logic.
fn resolve_repos_dir() -> std::path::PathBuf {
    let dir = std::env::var("OXIDEAN_REPOS_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("var/repos"));
    if dir.is_absolute() {
        dir
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| std::path::PathBuf::from("/"))
            .join(dir)
    }
}

/// Nesting depth of `caller` — number of `uses:` ancestors in its
/// `caller_job_id` chain ('' terminates the chain — top-level).
async fn call_depth(db: &Database, caller: &ActionJobRow) -> Result<u32, String> {
    let mut depth = 0u32;
    let mut cur = caller.caller_job_id.clone();
    while !cur.is_empty() {
        depth += 1;
        let parent = db
            .find_action_job_by_id(&cur)
            .await?
            .ok_or_else(|| format!("caller chain broken — job '{cur}' missing"))?;
        cur = parent.caller_job_id.clone();
    }
    Ok(depth)
}

/// Normalized `uses` specs up the caller chain — detects a file calling
/// itself (directly or through intermediates).
async fn ancestor_call_specs(db: &Database, caller: &ActionJobRow) -> Vec<String> {
    let mut specs = Vec::new();
    let mut cur = caller.caller_job_id.clone();
    while !cur.is_empty() {
        let Ok(Some(parent)) = db.find_action_job_by_id(&cur).await else {
            break;
        };
        if let Some(sha) = parent.payload_sha.as_deref() {
            if let Ok(Some(p)) = db.find_action_payload(sha).await {
                if let Ok(v) = serde_json::from_str::<JsonValue>(&p.payload_json) {
                    if let Some(u) = v
                        .get("call")
                        .and_then(|c| c.get("uses"))
                        .and_then(|u| u.as_str())
                    {
                        specs.push(u.to_string());
                    }
                }
            }
        }
        cur = parent.caller_job_id.clone();
    }
    specs
}

/// Resolve the call target to `(bare_repo_path, treeish, workflow_path,
/// callee repository_id)`.
async fn resolve_target(
    db: &Database,
    run: &ActionRunRow,
    uses: &UsesRef,
) -> Result<(std::path::PathBuf, String, String, String), String> {
    match uses {
        UsesRef::Local { path } => {
            let repo = db
                .find_repository_by_id(&run.repository_id)
                .await?
                .ok_or_else(|| "caller repository missing".to_string())?;
            let owner = crate::repo::owner_ref_for_repo(db, &repo)
                .await?
                .map(|o| o.slug().to_string())
                .ok_or_else(|| "caller repository owner missing".to_string())?;
            Ok((
                bare_repo_path(&resolve_repos_dir(), &owner, &repo.name),
                run.head_sha.clone(),
                path.clone(),
                repo.id.clone(),
            ))
        }
        UsesRef::Remote {
            owner,
            repo,
            path,
            git_ref,
        } => {
            let owner_ref = crate::repo::resolve_owner_slug(db, owner)
                .await?
                .ok_or_else(|| format!("uses target owner '{owner}' not found"))?;
            let owner_id = match &owner_ref {
                crate::repo::OwnerRef::User { id, .. } => id.as_str(),
                crate::repo::OwnerRef::Org { id, .. } => id.as_str(),
            };
            let row = db
                .find_repository_by_owner_name(owner_id, repo)
                .await?
                .ok_or_else(|| format!("uses target '{owner}/{repo}' not found"))?;
            Ok((
                bare_repo_path(&resolve_repos_dir(), owner, &row.name),
                git_ref.clone(),
                path.clone(),
                row.id.clone(),
            ))
        }
    }
}

/// Merge caller `with:` values over the callee's declared input defaults —
/// evaluates each raw `with` value against `caller_ctx` (needs/matrix/inputs
/// of the call leg). Returns the concrete `github.event.inputs` object for
/// callee rows.
fn resolve_call_inputs(
    with: Option<&JsonValue>,
    declared: Option<&JsonValue>,
    caller_ctx: &Context,
) -> Result<JsonValue, String> {
    let mut inputs = Map::new();
    // Declared defaults first — caller `with` overrides them.
    if let Some(JsonValue::Object(decls)) = declared {
        for (name, spec) in decls {
            if let Some(v) = spec.get("default") {
                if !v.is_null() {
                    inputs.insert(name.clone(), v.clone());
                }
            }
        }
    }
    if let Some(JsonValue::Object(w)) = with {
        for (name, raw) in w {
            let v = match raw {
                JsonValue::String(s) => {
                    eval_embedded(s, caller_ctx).map_err(|e| format!("with.{name}: {e}"))?
                }
                other => other.clone(),
            };
            inputs.insert(name.clone(), v);
        }
    }
    // Declared-input validation (GitHub semantics):
    // - required + no value → fail.
    // - caller-supplied name never declared → fail.
    // - `type` mismatch (string|number|boolean) → fail.
    if let Some(JsonValue::Object(decls)) = declared {
        for (name, spec) in decls {
            let required = spec
                .get("required")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if required && !inputs.contains_key(name) {
                return Err(format!("missing required input '{name}'"));
            }
            if let (Some(v), Some(ty)) =
                (inputs.get(name), spec.get("type").and_then(|t| t.as_str()))
            {
                let ok = match ty {
                    "string" => v.is_string(),
                    "number" => v.is_number(),
                    "boolean" => v.is_boolean(),
                    _ => true, // unknown types pass through (GitHub ignores them)
                };
                if !ok {
                    return Err(format!("input '{name}' must be of type {ty}"));
                }
            }
        }
        if let Some(JsonValue::Object(w)) = with {
            for name in w.keys() {
                if !decls.contains_key(name) {
                    return Err(format!(
                        "unexpected input '{name}' — not declared by callee"
                    ));
                }
            }
        }
    }
    Ok(JsonValue::Object(inputs))
}

/// Evaluate the caller's `secrets:` spec into the *forwarding descriptor*
/// stored on callee payloads — never the secret values themselves.
/// `inherit` → `"inherit"`; a map → `{callee: raw-expr-source}`.
fn resolve_call_secrets_spec(payload: &JsonValue) -> JsonValue {
    payload
        .get("call")
        .and_then(|c| c.get("secrets"))
        .cloned()
        .unwrap_or(JsonValue::Null)
}

/// Validate the caller's `secrets:` spec against the callee's
/// `workflow_call.secrets` declarations (GitHub semantics): a `required`
/// secret never delivered fails the call loudly, and a name the callee never
/// declared is rejected. `inherit` resolves against the caller repo's secret
/// names — values stay server-side either way.
fn validate_call_secrets(
    declared: Option<&JsonValue>,
    spec: &JsonValue,
    caller_secret_names: &std::collections::HashSet<String>,
) -> Result<(), String> {
    let provided: std::collections::HashSet<&str> = match spec {
        JsonValue::String(s) if s == "inherit" => {
            caller_secret_names.iter().map(String::as_str).collect()
        }
        JsonValue::Object(m) => m.keys().map(String::as_str).collect(),
        _ => std::collections::HashSet::new(),
    };
    if let Some(JsonValue::Object(decls)) = declared {
        for (name, dspec) in decls {
            let required = dspec
                .get("required")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if required && !provided.contains(name.as_str()) {
                return Err(format!("missing required secret '{name}'"));
            }
        }
        if let JsonValue::Object(m) = spec {
            for name in m.keys() {
                if !decls.contains_key(name) {
                    return Err(format!(
                        "unexpected secret '{name}' — not declared by callee"
                    ));
                }
            }
        }
    }
    Ok(())
}

/// Build the callee-visible `secrets` context/map from the caller's scope —
/// `inherit` → verbatim; `{K: expr}` → evaluate each value against a ctx
/// whose `secrets` is the caller scope; absent/null → empty.
pub fn remap_call_secrets(
    caller_scope: &Map<String, JsonValue>,
    spec: &JsonValue,
) -> Map<String, JsonValue> {
    match spec {
        JsonValue::String(s) if s == "inherit" => caller_scope.clone(),
        JsonValue::Object(m) => {
            let ctx =
                Context::new().with_context("secrets", JsonValue::Object(caller_scope.clone()));
            let mut out = Map::new();
            for (name, raw) in m {
                match raw {
                    JsonValue::String(expr) => match eval_embedded(expr, &ctx) {
                        Ok(v) => {
                            out.insert(name.clone(), v);
                        }
                        Err(e) => {
                            tracing::warn!(input = %name, error = %e, "call secrets remap failed");
                        }
                    },
                    other => {
                        out.insert(name.clone(), other.clone());
                    }
                }
            }
            out
        }
        // No `secrets:` — the callee sees nothing.
        _ => Map::new(),
    }
}

/// Patch `github.event.inputs` (and thus the `inputs` context) onto a run's
/// frozen event payload — callee tasks see the call's resolved inputs.
pub fn patch_event_inputs(event_payload: &JsonValue, inputs: &JsonValue) -> JsonValue {
    let mut event = event_payload.as_object().cloned().unwrap_or_default();
    event.insert("inputs".to_string(), inputs.clone());
    JsonValue::Object(event)
}

/// Expansion outcome marker — the caller leg transitions to `in_progress`
/// when callee rows land, or `failure`/`skipped` otherwise.
pub enum ExpandOutcome {
    Expanded,
    Failed(String),
}

/// Expand one caller leg: resolve the callee file, validate inputs/secrets,
/// emit callee rows under `caller.id`, then flip the caller `in_progress`
/// and stage the callee's declared `workflow_call.outputs` onto the caller's
/// `outputs_json` for close-out.
///
/// Everything here is durable: failure paths mark the caller `failure` with
/// a log line rather than leaving a wedged `waiting` row.
pub async fn expand_call(
    db: &Database,
    run: &ActionRunRow,
    caller: &ActionJobRow,
    payload: &JsonValue,
    caller_ctx: &Context,
) -> Result<ExpandOutcome, String> {
    let fail = |msg: String| ExpandOutcome::Failed(msg);
    let call = payload
        .get("call")
        .ok_or_else(|| "call payload missing".to_string())?;
    let uses_spec = call
        .get("uses")
        .and_then(|u| u.as_str())
        .ok_or_else(|| "call.uses missing".to_string())?;
    let uses = match parse_uses(uses_spec) {
        Ok(u) => u,
        Err(e) => return Ok(fail(e)),
    };
    // Guards — recursion depth + self-reference + total callee fan-out.
    let depth = call_depth(db, caller).await?;
    if depth >= MAX_CALL_DEPTH {
        return Ok(fail(format!(
            "reusable workflow '{uses_spec}' exceeds max nesting depth {MAX_CALL_DEPTH}"
        )));
    }
    let ancestors = ancestor_call_specs(db, caller).await;
    if ancestors.iter().any(|s| s == uses_spec) {
        return Ok(fail(format!(
            "reusable workflow '{uses_spec}' calls itself — recursion rejected"
        )));
    }
    let all_run_jobs = db.list_action_jobs_for_run(&run.id).await?;
    let callee_count = all_run_jobs
        .iter()
        .filter(|j| !j.caller_job_id.is_empty())
        .count();
    if callee_count >= MAX_CALLEE_JOBS {
        return Ok(fail(format!(
            "reusable call fan-out cap ({MAX_CALLEE_JOBS} callee jobs) reached"
        )));
    }
    // Re-expansion (rerun of a caller leg): prior callee rows are replaced —
    // revoke their tokens first so a zombie claim dies with the row.
    let prior_children: Vec<String> = all_run_jobs
        .iter()
        .filter(|j| j.caller_job_id == caller.id)
        .map(|j| j.id.clone())
        .collect();
    if !prior_children.is_empty() {
        for id in &prior_children {
            let _ = db.revoke_action_job_token_for_job(id).await;
        }
        db.delete_callee_jobs(&caller.id).await?;
    }
    let (bare, treeish, wf_path, callee_repo_id) = match resolve_target(db, run, &uses).await {
        Ok(t) => t,
        Err(e) => return Ok(fail(e)),
    };
    // A disabled workflow does not run on any trigger — `workflow_call`
    // included; the call leg fails like GitHub's "workflow is disabled".
    if db
        .is_action_workflow_disabled(&callee_repo_id, &wf_path)
        .await?
    {
        return Ok(fail(format!("reusable workflow '{uses_spec}' is disabled")));
    }
    let git = CliGitBackend::new();
    let bytes = match git.cat_blob(&bare, &treeish, &wf_path).await {
        Ok(b) => b,
        Err(e) => {
            return Ok(fail(format!(
                "reusable workflow '{uses_spec}': file unreadable at '{treeish}': {e}"
            )))
        }
    };
    let yaml = match String::from_utf8(bytes) {
        Ok(s) => s,
        Err(_) => return Ok(fail(format!("reusable workflow '{uses_spec}': not utf-8"))),
    };
    let doc = match parse::parse_workflow_yaml(yaml.as_bytes()) {
        Ok(d) => d,
        Err(e) => {
            return Ok(fail(format!(
                "reusable workflow '{uses_spec}': parse failed: {}",
                e.message
            )))
        }
    };
    if !doc.triggers.workflow_call {
        return Ok(fail(format!(
            "reusable workflow '{uses_spec}' has no `workflow_call` trigger"
        )));
    }
    if !doc.unsupported.is_empty() {
        return Ok(fail(format!(
            "reusable workflow '{uses_spec}' uses unsupported features: {}",
            doc.unsupported.join(", ")
        )));
    }
    let policy_violations =
        crate::actions::policy::check_doc(&doc, &crate::actions::policy::policy_from_env());
    if !policy_violations.is_empty() {
        return Ok(fail(format!(
            "reusable workflow '{uses_spec}' violates action source policy: {}",
            policy_violations.join("; ")
        )));
    }
    if doc.jobs.is_empty() {
        return Ok(fail(format!(
            "reusable workflow '{uses_spec}' declares no jobs"
        )));
    }
    // Resolve inputs: caller `with` over callee `workflow_call.inputs`
    // defaults, validated required/type/undeclared.
    let inputs = match resolve_call_inputs(
        call.get("with"),
        doc.triggers.call_inputs.as_ref(),
        caller_ctx,
    ) {
        Ok(i) => i,
        Err(e) => return Ok(fail(format!("reusable workflow '{uses_spec}': {e}"))),
    };
    // Secrets validation needs the caller repo's secret names for `inherit`.
    let caller_secret_names: std::collections::HashSet<String> = db
        .list_action_secret_names(&run.repository_id)
        .await?
        .into_iter()
        .map(|r| r.name)
        .collect();
    if let Err(e) = validate_call_secrets(
        doc.triggers.call_secrets.as_ref(),
        &resolve_call_secrets_spec(payload),
        &caller_secret_names,
    ) {
        return Ok(fail(format!("reusable workflow '{uses_spec}': {e}")));
    }
    // Callee eval ctx: same provenance, `inputs` + `github.event.inputs`
    // carry the resolved call inputs.
    let event_payload = run
        .event_payload_json
        .as_deref()
        .and_then(|s| serde_json::from_str::<JsonValue>(s).ok())
        .map(|p| patch_event_inputs(&p, &inputs));
    let prov = crate::actions::eval_ctx::Provenance {
        event: &run.event,
        head_sha: &run.head_sha,
        head_ref: &run.head_ref,
        triggered_by: run.triggered_by.as_deref(),
        run_id: &run.id,
    };
    let (ctx, eval_warnings) = crate::actions::eval_ctx::build_context(
        db,
        &run.repository_id,
        &doc,
        &prov,
        event_payload.as_ref(),
    )
    .await;
    let default_permissions = db
        .get_repo_actions_default_permissions(&run.repository_id)
        .await
        .unwrap_or_else(|_| "read".into());
    // `GITHUB_TOKEN` is only ever downgraded by a reusable call — clamp every
    // callee job's resolved permissions to the caller job's frozen map.
    let permissions_clamp = payload
        .get("permissions")
        .and_then(|p| p.as_object())
        .map(|m| {
            m.iter()
                .filter_map(|(k, v)| {
                    let level = match v.as_str()? {
                        "none" => crate::actions::parse::ContentsPermission::None,
                        "read" => crate::actions::parse::ContentsPermission::Read,
                        "write" => crate::actions::parse::ContentsPermission::Write,
                        _ => return None,
                    };
                    Some((k.clone(), level))
                })
                .collect()
        });
    let all_warnings: Vec<String> = doc.warnings.iter().cloned().chain(eval_warnings).collect();
    let mut emit = EmitCtx {
        db,
        repository_id: &run.repository_id,
        run_id: &run.id,
        workflow_path: &wf_path,
        doc: &doc,
        head_sha: &run.head_sha,
        head_ref: &run.head_ref,
        default_permissions: &default_permissions,
        workflow_name: &doc.name,
        triggered_by: run.triggered_by.as_deref(),
        ctx,
        all_warnings,
        log_dir: Some(crate::actions::logs::resolve_log_dir()),
        hold_for_approval: false,
        supersede_groups: Vec::new(),
        pending_groups: Vec::new(),
        caller: Some(CallerEmit {
            caller_job_id: caller.id.clone(),
            inputs,
            secrets_json: resolve_call_secrets_spec(payload),
        }),
        permissions_clamp,
    };
    let job_ids = emit_jobs(&mut emit, &doc.jobs).await?;
    if job_ids.is_empty() {
        return Ok(fail(format!(
            "reusable workflow '{uses_spec}' emitted no callee jobs"
        )));
    }
    // Post-emit bound: the pre-check gates the *accumulated* count but a
    // single matrixed callee can jump it in one shot — roll the expansion
    // back so durable callee rows never exceed the cap. `prior_children`
    // were deleted pre-emit (re-expansion), so exclude them from the live
    // total a rerun would otherwise double-count.
    let live = callee_count - prior_children.len() + job_ids.len();
    if live > MAX_CALLEE_JOBS {
        for id in &job_ids {
            let _ = db.revoke_action_job_token_for_job(id).await;
        }
        let _ = db.delete_callee_jobs(&caller.id).await;
        return Ok(fail(format!(
            "reusable call fan-out cap ({MAX_CALLEE_JOBS} callee jobs) reached"
        )));
    }
    // Pending-supersede fires for callee job-level groups too — a group
    // keeps at most one pending occupant, `cancel-in-progress` or not.
    for group in &emit.pending_groups {
        match db
            .cancel_pending_concurrency_jobs(&run.repository_id, group, &run.id)
            .await
        {
            Ok(ids) if !ids.is_empty() => {
                for id in &ids {
                    if let Err(e) = db.recompute_action_run_status(id).await {
                        tracing::warn!(error = %e, run_id = id, "post-supersede rollup failed");
                    }
                    let _ = crate::actions::statuses::publish_cancelled_for_run(
                        db,
                        id,
                        std::env::var("OXIDEAN_PUBLIC_ORIGIN").ok().as_deref(),
                    )
                    .await;
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, run_id = %run.id, group, "callee pending supersede failed");
            }
            _ => {}
        }
    }
    // `concurrency.cancel-in-progress` on callee jobs supersedes same-group
    // jobs in *other* runs — same application the enqueue path runs.
    for group in &emit.supersede_groups {
        match db
            .cancel_superseded_concurrency_jobs(&run.repository_id, group, &run.id)
            .await
        {
            Ok(ids) if !ids.is_empty() => {
                for id in &ids {
                    if let Err(e) = db.recompute_action_run_status(id).await {
                        tracing::warn!(error = %e, run_id = id, "post-supersede rollup failed");
                    }
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, run_id = %run.id, group, "callee supersede failed");
            }
            _ => {}
        }
    }
    // Caller stays `in_progress` while children run; the callee's declared
    // outputs ride `outputs_json` for close-out materialization.
    db.update_action_job_status(&caller.id, "in_progress")
        .await?;
    // Call jobs never see a runner — expansion start is their `in_progress`.
    crate::webhook::dispatch::notify_workflow_job(db, &caller.id, "in_progress").await;
    // New callee legs publish their own queued commit statuses; needs-gated
    // children re-gate on this tick's fixpoint loop.
    if let Err(e) = crate::actions::statuses::publish_queued_for_run(
        db,
        &run.repository_id,
        &run.head_sha,
        &doc.name,
        &run.id,
        std::env::var("OXIDEAN_PUBLIC_ORIGIN").ok().as_deref(),
        None,
        None,
        run.triggered_by.as_deref(),
    )
    .await
    {
        tracing::warn!(error = %e, run_id = %run.id, "callee status publish failed");
    }
    if let Some(decls) = &doc.triggers.call_outputs {
        if let Err(e) = db
            .set_action_job_outputs(&caller.id, &decls.to_string())
            .await
        {
            tracing::warn!(error = %e, job_id = %caller.id, "call outputs staging failed");
        }
    }
    Ok(ExpandOutcome::Expanded)
}

/// Close a caller leg once every child is terminal: rollup status
/// (failure > cancelled > skipped > success) + materialize the callee's
/// `workflow_call.outputs` against the children's `jobs.<key>.outputs`.
pub async fn close_call(
    db: &Database,
    caller: &ActionJobRow,
    children: &[ActionJobRow],
) -> Result<String, String> {
    // Rollup mirrors `recompute_run_status`: failure > cancelled, and
    // `skipped` only propagates when every child skipped (a mixed group is
    // `success`, matching GitHub's callee-run conclusion). `continue-on-error`
    // callee legs report `failure` but don't fail the callee run — same
    // suppression the run rollup applies.
    let result = {
        if children
            .iter()
            .any(|l| l.status == "failure" && !l.continue_on_error)
        {
            "failure"
        } else if children.iter().any(|l| l.status == "cancelled") {
            "cancelled"
        } else if children.iter().all(|l| l.status == "skipped") {
            "skipped"
        } else {
            "success"
        }
    };
    // Declared `workflow_call.outputs` — `{name: {value: "${{ jobs.* }}"}}`.
    let declared: Map<String, JsonValue> = serde_json::from_str::<JsonValue>(&caller.outputs_json)
        .ok()
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default();
    if !declared.is_empty() {
        let mut jobs_ctx = Map::new();
        for child in children {
            let outputs = serde_json::from_str::<JsonValue>(&child.outputs_json)
                .unwrap_or_else(|_| json!({}));
            jobs_ctx.insert(
                child.job_key.clone(),
                json!({"result": child.status, "outputs": outputs}),
            );
        }
        let ctx = Context::new().with_context("jobs", JsonValue::Object(jobs_ctx));
        let mut out = Map::new();
        let mut output_err = false;
        for (name, spec) in &declared {
            let src = spec.get("value").and_then(|v| v.as_str()).unwrap_or("");
            match eval_embedded(src, &ctx) {
                Ok(v) => {
                    out.insert(name.clone(), v);
                }
                Err(e) => {
                    tracing::warn!(job_id = %caller.id, output = %name, error = %e, "call output eval failed");
                    output_err = true;
                }
            }
        }
        if let Err(e) = db
            .set_action_job_outputs(&caller.id, &JsonValue::Object(out).to_string())
            .await
        {
            tracing::warn!(error = %e, job_id = %caller.id, "call outputs write failed");
        }
        if output_err {
            db.update_action_job_status(&caller.id, "failure").await?;
            crate::webhook::dispatch::notify_workflow_job(db, &caller.id, "completed").await;
            return Ok("failure".into());
        }
    }
    db.update_action_job_status(&caller.id, result).await?;
    crate::webhook::dispatch::notify_workflow_job(db, &caller.id, "completed").await;
    Ok(result.to_string())
}
