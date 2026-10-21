//! Enqueue-time expression evaluation — builds the `${{ }}` [`Context`] for
//! job-level fields (`if`, `name`, `runs-on`, `timeout-minutes`, concurrency
//! group, workflow name) and evaluates them against it.
//!
//! GitHub's per-key context allowlists (`jobs.<id>.if` sees `github`,
//! `needs`, `vars`, `inputs`) are approximated by a superset context —
//! `env`/`secrets` are not injected server-side (secrets never leave the
//! claim path), but `env` IS because `env:` values may reference `github.*`
//! and job fields may legitimately reference `env.*`.

use serde_json::Value as JsonValue;

use oxidean_actions_expr::{eval_embedded, Context};
use oxidean_db::Database;

use crate::actions::parse::WorkflowDocument;

/// Run-level provenance feeding the `github` context — what the event,
/// who triggered it, and which commit/ref the run is pinned to.
pub struct Provenance<'a> {
    pub event: &'a str,
    pub head_sha: &'a str,
    pub head_ref: &'a str,
    pub triggered_by: Option<&'a str>,
    pub run_id: &'a str,
}

/// The `github` context object for enqueue-time evaluation.
fn github_ctx(
    repository: &str,
    repository_owner: &str,
    actor: &str,
    actor_id: &str,
    doc_name: &str,
    prov: &Provenance<'_>,
    event_payload: Option<&JsonValue>,
) -> JsonValue {
    let ref_name = prov
        .head_ref
        .strip_prefix("refs/heads/")
        .or_else(|| prov.head_ref.strip_prefix("refs/tags/"))
        .unwrap_or(prov.head_ref);
    serde_json::json!({
        "sha": prov.head_sha,
        "ref": prov.head_ref,
        "ref_name": ref_name,
        "ref_type": if prov.head_ref.starts_with("refs/tags/") { "tag" } else { "branch" },
        "event_name": prov.event,
        "repository": repository,
        "repository_owner": repository_owner,
        "actor": actor,
        "actor_id": actor_id,
        "workflow": doc_name,
        "run_id": prov.run_id,
        // Frozen event payload captured at dispatch — expressions see the
        // same `github.event` the runner does via `OXIDEAN_EVENT_PATH`.
        "event": event_payload.cloned().unwrap_or_else(|| serde_json::json!({})),
    })
}

/// Build the enqueue evaluation context: `github` populated from the run's
/// provenance, `env` from the evaluated workflow env, `vars` from repo+org
/// configuration variables, `inputs` from the dispatch payload;
/// `needs`/`matrix`/`steps` stay empty until the scheduler phase.
pub async fn build_context(
    db: &Database,
    repository_id: &str,
    doc: &WorkflowDocument,
    prov: &Provenance<'_>,
    event_payload: Option<&JsonValue>,
) -> (Context, Vec<String>) {
    let mut warnings = Vec::new();
    let repo = db.find_repository_by_id(repository_id).await.ok().flatten();
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
    let (actor, actor_id) = match prov.triggered_by {
        Some(uid) => {
            let name = db
                .find_user_by_id(uid)
                .await
                .ok()
                .flatten()
                .map(|u| u.username)
                .unwrap_or_default();
            (name, uid.to_string())
        }
        None => (String::new(), String::new()),
    };
    let github = github_ctx(
        &format!("{owner_slug}/{repo_name}"),
        &owner_slug,
        &actor,
        &actor_id,
        &doc.name,
        prov,
        event_payload,
    );
    // `inputs` mirrors `github.event.inputs` — `workflow_dispatch` payloads
    // carry the caller's inputs there (GitHub parity).
    let inputs = event_payload
        .and_then(|p| p.get("inputs"))
        .and_then(|i| i.as_object())
        .map(|m| JsonValue::Object(m.clone()))
        .unwrap_or_else(|| serde_json::json!({}));
    // `vars` context — repo < org configuration variables (plaintext).
    // Environment-scoped vars join per job at claim — they depend on the
    // job's `environment:` and aren't knowable at run level.
    let vars = match crate::actions::vars::variables_for_context(db, repository_id).await {
        Ok(v) => v,
        Err(e) => {
            warnings.push(format!("vars context failed: {e}"));
            serde_json::json!({})
        }
    };
    let mut ctx = Context::new()
        .with_context("github", github)
        .with_context("vars", vars)
        .with_context("inputs", inputs)
        .with_context("needs", serde_json::json!({}))
        .with_context("matrix", serde_json::json!({}))
        .with_context("steps", serde_json::json!({}))
        .with_context("runner", serde_json::json!({}))
        .with_context("env", serde_json::json!({}));
    // Workflow `env` evaluates against github/vars/inputs (no self-reference).
    if let Some(env) = &doc.env {
        match oxidean_actions_expr::eval_json(env, &ctx) {
            Ok(v) => ctx.set_context("env", v),
            Err(e) => warnings.push(format!("workflow env evaluation failed: {e}")),
        }
    }
    (ctx, warnings)
}

/// Update `github.workflow` after the workflow `name:` is interpolated —
/// the context field should carry the evaluated name.
pub fn set_workflow_name(ctx: &mut Context, evaluated: &str) {
    if let Some(JsonValue::Object(mut g)) = ctx.get_context("github").cloned() {
        g.insert("workflow".into(), JsonValue::String(evaluated.to_string()));
        ctx.set_context("github", JsonValue::Object(g));
    }
}

/// Interpolate `src` against `ctx`; on error push a `::warning::` note and
/// fall back to the raw string (a bad interpolation must not silently
/// disappear, but a display-only field shouldn't kill the job).
pub fn interp_or_raw(src: &str, ctx: &Context, notes: &mut Vec<String>) -> String {
    match eval_embedded(src, ctx) {
        Ok(v) => oxidean_actions_expr::stringify(&v),
        Err(e) => {
            notes.push(format!("expression error in '{src}': {e} — used literally"));
            src.to_string()
        }
    }
}

/// Deep-interpolate every string leaf of a normalized YAML→JSON value
/// (`container:`/`services:` shapes hold image/env/options strings that can
/// carry `${{ }}`). Non-string leaves pass through.
pub fn interp_json(v: &JsonValue, ctx: &Context, notes: &mut Vec<String>) -> JsonValue {
    match v {
        JsonValue::String(s) => JsonValue::String(interp_or_raw(s, ctx, notes)),
        JsonValue::Array(xs) => {
            JsonValue::Array(xs.iter().map(|x| interp_json(x, ctx, notes)).collect())
        }
        JsonValue::Object(m) => JsonValue::Object(
            m.iter()
                .map(|(k, x)| (k.clone(), interp_json(x, ctx, notes)))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// `timeout-minutes:` evaluation → minutes as f64 (accepts bare numbers and
/// `${{ }}` expressions).
pub fn eval_minutes(src: &str, ctx: &Context) -> Result<f64, String> {
    let trimmed = src.trim();
    if let Ok(n) = trimmed.parse::<f64>() {
        return Ok(n);
    }
    let v = eval_embedded(src, ctx).map_err(|e| e.to_string())?;
    v.as_f64()
        .ok_or_else(|| format!("timeout-minutes '{src}' is not a number"))
}

/// Add `minutes` to now → RFC3339 (the DB layer's timestamp convention).
pub fn deadline_rfc3339(minutes: f64) -> String {
    (chrono::Utc::now() + chrono::Duration::milliseconds((minutes * 60_000.0) as i64))
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}
