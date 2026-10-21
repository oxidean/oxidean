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
    merged: bool,
) -> JsonValue {
    let base_repo_name = base_repo
        .get("full_name")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    let head_repo_name = head_repo
        .get("full_name")
        .and_then(|v| v.as_str())
        .unwrap_or(base_repo_name);
    let state = if action == "closed" { "closed" } else { "open" };
    json!({
        "action": action,
        "number": number,
        "pull_request": {
            "number": number,
            "title": title,
            "state": state,
            "merged": merged,
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

/// `deployment` object in GitHub's Deployments-API shape — shared by the
/// `deployment` event payload and the `deployment_status.deployment` embed.
pub fn deployment_embed(row: &oxidean_db::ActionDeploymentRow, creator_login: &str) -> JsonValue {
    json!({
        "id": row.id,
        "sha": row.sha,
        "ref": row.git_ref,
        "task": row.task,
        "payload": row.payload.as_deref().and_then(|p| serde_json::from_str::<JsonValue>(p).ok()).unwrap_or(JsonValue::Object(Default::default())),
        "environment": row.environment,
        "original_environment": row.environment,
        "description": if row.description.is_empty() { JsonValue::Null } else { JsonValue::String(row.description.clone()) },
        "creator": {"login": creator_login, "id": row.creator_id},
        "url": row.url,
        "transient_environment": row.transient_environment,
        "production_environment": row.production_environment,
        "created_at": row.created_at,
        "updated_at": row.updated_at,
    })
}

/// `deployment` event payload — `action` is always `created` (GitHub defines
/// no activity types for deployments).
pub fn deployment_payload(
    row: &oxidean_db::ActionDeploymentRow,
    creator_login: &str,
    repository: &JsonValue,
    sender: &JsonValue,
) -> JsonValue {
    json!({
        "action": "created",
        "deployment": deployment_embed(row, creator_login),
        "repository": repository,
        "sender": sender,
    })
}

/// `deployment_status` object in GitHub's shape.
pub fn deployment_status_embed(
    row: &oxidean_db::ActionDeploymentStatusRow,
    creator_login: &str,
) -> JsonValue {
    json!({
        "id": row.id,
        "state": row.state,
        "description": if row.description.is_empty() { JsonValue::Null } else { JsonValue::String(row.description.clone()) },
        "environment": row.environment,
        "environment_url": row.environment_url,
        "target_url": row.target_url,
        "log_url": row.log_url,
        "creator": {"login": creator_login, "id": row.creator_id},
        "created_at": row.created_at,
        "updated_at": row.created_at,
    })
}

/// `deployment_status` event payload — `action` is always `created`.
pub fn deployment_status_payload(
    status: &oxidean_db::ActionDeploymentStatusRow,
    deployment: &oxidean_db::ActionDeploymentRow,
    status_creator_login: &str,
    deployment_creator_login: &str,
    repository: &JsonValue,
    sender: &JsonValue,
) -> JsonValue {
    json!({
        "action": "created",
        "deployment_status": deployment_status_embed(status, status_creator_login),
        "deployment": deployment_embed(deployment, deployment_creator_login),
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

/// `check_suite` object in GitHub's shape (embedded in `check_suite`
/// payloads and inside `check_run` objects).
pub fn check_suite_embed(
    row: &oxidean_db::CheckSuiteRow,
    creator_login: &str,
    repository: &JsonValue,
) -> JsonValue {
    json!({
        "id": row.id,
        "head_sha": row.head_sha,
        "head_branch": row.head_branch,
        "status": row.status,
        "conclusion": row.conclusion,
        "url": row.url,
        "app": {"slug": row.app},
        "check_runs_url": row.run_id,
        "latest_check_runs_count": row.check_runs_count,
        "repository": repository,
        "creator": {"login": creator_login, "id": row.creator_id},
        "created_at": row.created_at,
        "updated_at": row.updated_at,
    })
}

/// `check_run` object in GitHub's shape. `annotations_count` reflects stored
/// annotation rows; the suite embeds as GitHub's partial `check_suite`.
pub fn check_run_embed(
    row: &oxidean_db::CheckRunRow,
    app: &str,
    creator_login: &str,
    annotations_count: i64,
) -> JsonValue {
    json!({
        "id": row.id,
        "name": row.name,
        "head_sha": row.head_sha,
        "status": row.status,
        "conclusion": row.conclusion,
        "details_url": row.details_url,
        "external_id": row.external_id,
        "check_suite": {"id": row.check_suite_id},
        "app": {"slug": app},
        "output": {
            "title": row.output_title,
            "summary": row.output_summary,
            "text": row.output_text,
            "annotations_count": annotations_count,
        },
        "creator": {"login": creator_login, "id": row.creator_id},
        "started_at": row.started_at,
        "completed_at": row.completed_at,
        "created_at": row.created_at,
        "updated_at": row.updated_at,
    })
}

/// `check_run` event payload — `action` is `created` | `completed` |
/// `rerequested` (GitHub vocabulary).
pub fn check_run_payload(
    action: &str,
    run: &oxidean_db::CheckRunRow,
    app: &str,
    creator_login: &str,
    annotations_count: i64,
    repository: &JsonValue,
    sender: &JsonValue,
) -> JsonValue {
    json!({
        "action": action,
        "check_run": check_run_embed(run, app, creator_login, annotations_count),
        "repository": repository,
        "sender": sender,
    })
}

/// `check_suite` event payload — `action` is `requested` | `rerequested` |
/// `completed` (GitHub vocabulary).
pub fn check_suite_payload(
    action: &str,
    suite: &oxidean_db::CheckSuiteRow,
    creator_login: &str,
    repository: &JsonValue,
    sender: &JsonValue,
) -> JsonValue {
    json!({
        "action": action,
        "check_suite": check_suite_embed(suite, creator_login, repository),
        "repository": repository,
        "sender": sender,
    })
}

/// `gollum` event payload — GitHub fires one gollum event per wiki change
/// set with a `pages[]` array; each entry carries its own `action`
/// (`created` | `edited`). `summary` is always `null` (GitHub never
/// populates it). RPC page writes emit one page; direct wiki pushes can
/// carry several.
pub fn gollum_payload(
    pages: &[GollumPage],
    repository: &JsonValue,
    sender: &JsonValue,
) -> JsonValue {
    let pages: Vec<JsonValue> = pages
        .iter()
        .map(|p| {
            json!({
                "page_name": p.page_name,
                "title": p.page_name,
                "summary": JsonValue::Null,
                "action": p.action,
                "sha": p.sha,
                "html_url": p.html_url,
            })
        })
        .collect();
    json!({
        "pages": pages,
        "repository": repository,
        "sender": sender,
    })
}

/// One entry in a `gollum.pages[]` array (see [`gollum_payload`]).
pub struct GollumPage {
    /// Page title without extension — `My-Page` for `My-Page.md`.
    pub page_name: String,
    /// `created` | `edited` (GitHub's gollum activity vocabulary).
    pub action: String,
    /// Commit SHA that produced the page state.
    pub sha: String,
    /// Web-UI link for the page.
    pub html_url: String,
}

/// `project` payload — `action` is the board activity type
/// (`created`/`edited`/`closed`/`reopened`/`deleted`); `project` embeds the
/// board row in GitHub's Projects-API shape. `changes` carries
/// `name.from`/`body.from` on `edited` (GitHub edit-diff convention).
pub fn project_payload(
    action: &str,
    row: &oxidean_db::ProjectRow,
    html_url: &str,
    changes: Option<&JsonValue>,
    repository: &JsonValue,
    sender: &JsonValue,
) -> JsonValue {
    let mut payload = json!({
        "action": action,
        "project": {
            "id": row.id,
            "number": row.number,
            "name": row.title,
            "body": row.body,
            "state": row.state,
            "html_url": html_url,
            "closed_at": row.closed_at,
            "created_at": row.created_at,
            "updated_at": row.updated_at,
        },
        "repository": repository,
        "sender": sender,
    });
    if let Some(c) = changes {
        payload["changes"] = c.clone();
    }
    payload
}

/// `project_column` payload — `action` is `created`/`edited`/`moved`/`deleted`.
pub fn project_column_payload(
    action: &str,
    row: &oxidean_db::ProjectColumnRow,
    project_html_url: &str,
    changes: Option<&JsonValue>,
    repository: &JsonValue,
    sender: &JsonValue,
) -> JsonValue {
    let mut payload = json!({
        "action": action,
        "project_column": {
            "id": row.id,
            "project_id": row.project_id,
            "name": row.name,
            "project_url": project_html_url,
            "created_at": row.created_at,
            "updated_at": row.updated_at,
        },
        "repository": repository,
        "sender": sender,
    });
    if let Some(c) = changes {
        payload["changes"] = c.clone();
    }
    payload
}

/// `project_card` payload — `action` is `created`/`edited`/`moved`/
/// `converted`/`deleted`. `content_url` points at the linked issue/pull;
/// `changes` carries `note.from`/`column_id.from`/`content_id` diffs.
pub fn project_card_payload(
    action: &str,
    row: &oxidean_db::ProjectCardRow,
    content_url: Option<&str>,
    changes: Option<&JsonValue>,
    repository: &JsonValue,
    sender: &JsonValue,
) -> JsonValue {
    let mut payload = json!({
        "action": action,
        "project_card": {
            "id": row.id,
            "column_id": row.column_id,
            "note": row.note,
            "content_url": content_url,
            "archived": row.archived,
            "created_at": row.created_at,
            "updated_at": row.updated_at,
        },
        "repository": repository,
        "sender": sender,
    });
    if let Some(c) = changes {
        payload["changes"] = c.clone();
    }
    payload
}

/// `discussion` object in GitHub's webhook embed shape — shared by the
/// `discussion` and `discussion_comment` event payloads.
#[allow(clippy::too_many_arguments)]
pub fn discussion_embed(
    row: &oxidean_db::DiscussionRow,
    category: Option<&oxidean_db::DiscussionCategoryRow>,
    author_login: Option<&str>,
    label_names: &[String],
    comments_count: i64,
    html_url: &str,
    answer_html_url: Option<&str>,
) -> JsonValue {
    let cat = category.map(|c| {
        json!({
            "id": c.id,
            "name": c.name,
            "slug": c.slug,
            "emoji": c.emoji,
            "description": c.description,
            "is_answerable": c.is_answerable,
            "created_at": c.created_at,
            "updated_at": c.updated_at,
        })
    });
    json!({
        "id": row.id,
        "number": row.number,
        "title": row.title,
        "body": row.body,
        "state": "open",
        "locked": row.locked,
        "comments": comments_count,
        "user": author_login.map(|u| json!({ "login": u })),
        "category": cat,
        "labels": label_names,
        "answer_html_url": answer_html_url,
        "answer_chosen_at": row.answered_at,
        "answer_chosen_by": row.answered_by,
        "html_url": html_url,
        "created_at": row.created_at,
        "updated_at": row.updated_at,
    })
}

/// `discussion` payload — `action` is the GitHub activity type:
/// `created`/`edited`/`deleted`/`transferred`/`pinned`/`unpinned`/`locked`/
/// `unlocked`/`category_changed`/`answered`/`unanswered`/`labeled`/`unlabeled`.
/// `changes` carries GitHub's edit-diff convention (`title.from`/`body.from`).
#[allow(clippy::too_many_arguments)]
pub fn discussion_payload(
    action: &str,
    embed: &JsonValue,
    changes: Option<&JsonValue>,
    label: Option<&JsonValue>,
    answer: Option<&JsonValue>,
    repository: &JsonValue,
    sender: &JsonValue,
) -> JsonValue {
    let mut payload = json!({
        "action": action,
        "discussion": embed,
        "repository": repository,
        "sender": sender,
    });
    if let Some(c) = changes {
        payload["changes"] = c.clone();
    }
    if let Some(l) = label {
        payload["label"] = l.clone();
    }
    if let Some(a) = answer {
        payload["answer"] = a.clone();
    }
    payload
}

/// `page_build` payload — GitHub's event is typeless (no `action` key): the
/// `build` embed carries the completed build's final `status`
/// (`built` | `errored`), `error.message`, `commit`, `duration`, `pusher`.
pub fn page_build_payload(
    build: &oxidean_db::PagesBuildRow,
    pusher_login: Option<&str>,
    repository: &JsonValue,
    sender: &JsonValue,
) -> JsonValue {
    let error = if build.status == "errored" {
        json!({ "message": build.error_message })
    } else {
        json!({ "message": JsonValue::Null })
    };
    let pusher = match pusher_login {
        Some(login) => json!({ "login": login }),
        None => JsonValue::Null,
    };
    json!({
        "id": build.id,
        "build": {
            "url": build.url,
            "status": build.status,
            "error": error,
            "commit": build.commit_sha,
            "duration": build.duration_ms,
            "created_at": build.created_at,
            "updated_at": build.updated_at,
            "pusher": pusher,
        },
        "repository": repository,
        "sender": sender,
    })
}

/// `merge_group` payload — `action` is `checks_requested` (a group started
/// verification), `dequeued` (a PR left the queue), or `destroyed` (the
/// group was invalidated/merged). The `merge_group` object mirrors GitHub:
/// `head_ref` is the `gh-readonly-queue/…` branch, `head_sha` the chain tip
/// Actions checks out; `pulls` lists the PR numbers in merge order.
pub fn merge_group_payload(
    action: &str,
    group: &oxidean_db::MergeGroupRow,
    head_commit: &JsonValue,
    pull_numbers: &[i64],
    repository: &JsonValue,
    sender: &JsonValue,
) -> JsonValue {
    let pulls: Vec<JsonValue> = pull_numbers.iter().map(|n| json!(n)).collect();
    json!({
        "action": action,
        "merge_group": {
            "head_sha": group.head_sha,
            "head_ref": group.head_ref,
            "base_sha": group.base_sha,
            "base_ref": format!("refs/heads/{}", group.base_ref),
            "head_commit": head_commit,
        },
        "pulls": pulls,
        "repository": repository,
        "sender": sender,
    })
}

/// `discussion_comment` payload — `action` is `created`/`edited`/`deleted`;
/// `discussion` carries the same embed as `discussion_payload`.
#[allow(clippy::too_many_arguments)]
pub fn discussion_comment_payload(
    action: &str,
    discussion_embed: &JsonValue,
    comment: &oxidean_db::DiscussionCommentRow,
    comment_html_url: &str,
    author_login: Option<&str>,
    changes: Option<&JsonValue>,
    repository: &JsonValue,
    sender: &JsonValue,
) -> JsonValue {
    let mut payload = json!({
        "action": action,
        "discussion": discussion_embed,
        "comment": {
            "id": comment.id,
            "discussion_id": comment.discussion_id,
            "parent_id": comment.parent_id,
            "user": author_login.map(|u| json!({ "login": u })),
            "body": comment.body,
            "html_url": comment_html_url,
            "created_at": comment.created_at,
            "updated_at": comment.updated_at,
        },
        "repository": repository,
        "sender": sender,
    });
    if let Some(c) = changes {
        payload["changes"] = c.clone();
    }
    payload
}
