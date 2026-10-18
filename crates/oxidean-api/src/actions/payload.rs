//! GitHub-shaped event payloads frozen at dispatch (Phase 6).
//!
//! Each builder produces the subset of GitHub's webhook payload that
//! expressions and actions actually read — `github.event` / `$GITHUB_EVENT_PATH`
//! carry this JSON verbatim, and it persists on `action_runs.event_payload_json`
//! so the runner never re-derives event data from mutable repository state.
//!
//! Shape contract: field names mirror GitHub's webhook docs; unknown fields
//! are omitted rather than stubbed so expressions distinguish "absent" from
//! "empty".

use serde_json::{json, Value as JsonValue};

/// One `commits[]` entry in a `push` payload.
#[derive(Debug, Clone)]
pub struct PayloadCommit {
    pub id: String,
    pub message: String,
    pub author_name: String,
    pub author_email: String,
    /// RFC3339 author timestamp (commit time).
    pub timestamp: String,
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub modified: Vec<String>,
}

/// `push` event payload — `before`/`after`/`ref`/`commits`/`head_commit`
/// plus `pusher`. `before` is all-zeros on branch/tag creation (GitHub
/// parity — downstream `paths` filters treat zero-before as "match all").
pub fn push_payload(
    before: &str,
    after: &str,
    ref_name: &str,
    commits: &[PayloadCommit],
    pusher: Option<&str>,
    repository: &JsonValue,
    sender: &JsonValue,
) -> JsonValue {
    let commits_json: Vec<JsonValue> = commits
        .iter()
        .map(|c| {
            json!({
                "id": c.id,
                "message": c.message,
                "author": {"name": c.author_name, "email": c.author_email},
                "timestamp": c.timestamp,
                "added": c.added,
                "removed": c.removed,
                "modified": c.modified,
            })
        })
        .collect();
    let head = commits_json.last().cloned().unwrap_or(JsonValue::Null);
    json!({
        "before": before,
        "after": after,
        "ref": ref_name,
        "commits": commits_json,
        "head_commit": head,
        "pusher": pusher.map(|name| json!({"name": name, "email": null})),
        "repository": repository,
        "sender": sender,
    })
}

/// `pull_request` / `pull_request_target` payload. `pull_request` mirrors
/// GitHub's issue-embed shape with the fields checkout/expressions read:
/// `number`, `head.{ref,sha,repo.full_name}`, `base.{ref,sha,repo.full_name}`.
#[allow(clippy::too_many_arguments)]
pub fn pull_request_payload(
    action: &str,
    number: i64,
    title: &str,
    base_ref: &str,
    base_sha: &str,
    base_repo: &JsonValue,
    head_ref: &str,
    head_sha: &str,
    head_repo: &JsonValue,
    head_owner: Option<&str>,
    actor: &JsonValue,
    repository: &JsonValue,
) -> JsonValue {
    let base_repo_name = base_repo
        .get("full_name")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    let head_repo_name = head_repo
        .get("full_name")
        .and_then(|v| v.as_str())
        .unwrap_or(base_repo_name);
    json!({
        "action": action,
        "number": number,
        "pull_request": {
            "number": number,
            "title": title,
            "state": "open",
            "merged": false,
            "user": actor,
            "head": {
                "ref": head_ref,
                "sha": head_sha,
                "repo": {"full_name": head_repo_name, "owner": {"login": head_owner}},
            },
            "base": {
                "ref": base_ref,
                "sha": base_sha,
                "repo": {"full_name": base_repo_name},
            },
        },
        "repository": repository,
        "sender": actor,
    })
}

/// `workflow_dispatch` payload — `inputs` doubles as the `inputs` context.
pub fn workflow_dispatch_payload(
    ref_name: &str,
    inputs: &JsonValue,
    repository: &JsonValue,
    sender: &JsonValue,
) -> JsonValue {
    json!({
        "inputs": inputs,
        "ref": ref_name,
        "repository": repository,
        "sender": sender,
    })
}

/// `schedule` payload — `schedule` holds the cron spec that fired.
pub fn schedule_payload(cron: &str, repository: &JsonValue) -> JsonValue {
    json!({
        "schedule": cron,
        "repository": repository,
    })
}

/// `release` payload — `action` is `published`/`created`/`edited`/
/// `prereleased`/`released`.
#[allow(clippy::too_many_arguments)]
pub fn release_payload(
    action: &str,
    tag_name: &str,
    release_name: &str,
    draft: bool,
    prerelease: bool,
    body: &str,
    repository: &JsonValue,
    sender: &JsonValue,
) -> JsonValue {
    json!({
        "action": action,
        "release": {
            "tag_name": tag_name,
            "name": release_name,
            "draft": draft,
            "prerelease": prerelease,
            "body": body,
        },
        "repository": repository,
        "sender": sender,
    })
}

/// `issues` payload — `action` is the issue activity type
/// (`opened`/`closed`/`reopened`/`edited`/…).
#[allow(clippy::too_many_arguments)]
pub fn issues_payload(
    action: &str,
    number: i64,
    title: &str,
    state: &str,
    body: &str,
    author: &JsonValue,
    repository: &JsonValue,
    sender: &JsonValue,
) -> JsonValue {
    json!({
        "action": action,
        "issue": {
            "number": number,
            "title": title,
            "state": state,
            "body": body,
            "user": author,
        },
        "repository": repository,
        "sender": sender,
    })
}

/// `milestone` payload — `action` is the milestone activity type
/// (`created`/`closed`/`opened`/`edited`/`deleted`); `milestone` embeds
/// the row in GitHub's webhook shape.
pub fn milestone_payload(
    action: &str,
    row: &oxidean_db::MilestoneRow,
    repository: &JsonValue,
    sender: &JsonValue,
) -> JsonValue {
    json!({
        "action": action,
        "milestone": {
            "number": row.number,
            "title": row.title,
            "description": row.description,
            "state": row.state,
            "due_on": row.due_on,
            "open_issues": row.open_issues,
            "closed_issues": row.closed_issues,
            "closed_at": row.closed_at,
            "created_at": row.created_at,
            "updated_at": row.updated_at,
        },
        "repository": repository,
        "sender": sender,
    })
}

/// `workflow_run` payload — `workflow_run` embeds the completed run in
/// GitHub's webhook shape; `action` is `completed` (the only activity type
/// the engine emits today).
#[allow(clippy::too_many_arguments)]
pub fn workflow_run_payload(
    run_id: &str,
    run_number: i64,
    run_attempt: i64,
    workflow_name: &str,
    workflow_path: &str,
    head_branch: &str,
    head_sha: &str,
    event: &str,
    conclusion: &str,
    created_at: &str,
    finished_at: &str,
    pull_requests: &JsonValue,
    repository: &JsonValue,
    sender: &JsonValue,
) -> JsonValue {
    json!({
        "action": "completed",
        "workflow_run": {
            "id": run_id,
            "name": workflow_name,
            "path": workflow_path,
            "display_title": workflow_name,
            "run_number": run_number,
            "run_attempt": run_attempt,
            "event": event,
            "status": "completed",
            "conclusion": conclusion,
            "head_branch": head_branch,
            "head_sha": head_sha,
            "pull_requests": pull_requests,
            "created_at": created_at,
            "updated_at": finished_at,
            "actor": sender,
            "triggering_actor": sender,
        },
        "repository": repository,
        "sender": sender,
    })
}

/// `{full_name, name, owner.login, default_branch}` — the `repository`
/// embed every payload shares.
pub fn repository_embed(owner: &str, name: &str, default_branch: &str) -> JsonValue {
    json!({
        "name": name,
        "full_name": format!("{owner}/{name}"),
        "owner": {"login": owner},
        "default_branch": default_branch,
    })
}

/// `{login}` — the `sender`/`user` embed.
pub fn user_embed(login: &str) -> JsonValue {
    json!({"login": login})
}
