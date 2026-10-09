//! WebhookDispatcher — fan-out seam for domain events (D-HOOK-23).

use oxidean_db::Database;
use serde_json::json;
use uuid::Uuid;

use super::deliver;

/// Emit a repository event to all active subscribed webhooks (best-effort async).
pub async fn emit(
    db: &Database,
    repository_id: &str,
    event: &str,
    action: &str,
    payload: serde_json::Value,
    env_name: &str,
) {
    let hooks = match db
        .list_active_webhooks_for_event(repository_id, event)
        .await
    {
        Ok(h) => h,
        Err(e) => {
            tracing::warn!(error = %e, "webhook emit: list hooks failed");
            return;
        }
    };
    if hooks.is_empty() {
        return;
    }

    let payload_json = match serde_json::to_string(&payload) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(error = %e, "webhook emit: serialize payload failed");
            return;
        }
    };

    for hook in hooks {
        let delivery_id = Uuid::new_v4().to_string();
        let delivery_guid = Uuid::new_v4().to_string();
        match db
            .insert_webhook_delivery(
                &delivery_id,
                &hook.id,
                &delivery_guid,
                event,
                action,
                &payload_json,
            )
            .await
        {
            Ok(_) => {
                deliver::spawn_deliver(
                    db.clone(),
                    hook.id.clone(),
                    delivery_id,
                    env_name.to_string(),
                );
            }
            Err(e) => {
                tracing::warn!(error = %e, webhook_id = %hook.id, "webhook emit: insert delivery failed");
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
/// Build a GitHub-compatible issues payload (D-HOOK-07 / D-HOOK-08).
pub fn issues_payload(
    action: &str,
    issue_number: i64,
    title: &str,
    body: &str,
    state: &str,
    owner: &str,
    repo_name: &str,
    repo_id: &str,
    sender_login: &str,
    sender_id: &str,
) -> serde_json::Value {
    json!({
        "action": action,
        "issue": {
            "number": issue_number,
            "title": title,
            "body": body,
            "state": state,
            "html_url": format!("/{owner}/{repo_name}/issues/{issue_number}"),
        },
        "repository": {
            "id": repo_id,
            "name": repo_name,
            "full_name": format!("{owner}/{repo_name}"),
            "owner": { "login": owner },
        },
        "sender": {
            "login": sender_login,
            "id": sender_id,
        }
    })
}

/// Build a GitHub-compatible `issue_comment` payload (DEBT-04 / D-HOOK-08).
///
/// GitHub scopes this event to issue comments **and** pull-request conversation
/// comments: `is_pull` adds the `issue.pull_request` marker receivers use to tell
/// them apart. Line-anchored diff comments map to `pull_request_review_comment`,
/// which Oxidean does not emit yet. `edited` passes the pre-edit body as
/// `changes_from`; `created` / `deleted` pass `None`.
#[expect(clippy::too_many_arguments)]
pub fn issue_comment_payload(
    action: &str,
    issue_number: i64,
    issue_title: &str,
    issue_body: &str,
    issue_state: &str,
    is_pull: bool,
    comment_id: &str,
    comment_body: &str,
    changes_from: Option<&str>,
    author_login: &str,
    author_id: &str,
    owner: &str,
    repo_name: &str,
    repo_id: &str,
    sender_login: &str,
    sender_id: &str,
) -> serde_json::Value {
    let html_url = if is_pull {
        format!("/{owner}/{repo_name}/pulls/{issue_number}")
    } else {
        format!("/{owner}/{repo_name}/issues/{issue_number}")
    };
    let mut issue = json!({
        "number": issue_number,
        "title": issue_title,
        "body": issue_body,
        "state": issue_state,
        "html_url": html_url,
    });
    if is_pull {
        issue["pull_request"] = json!({ "html_url": html_url });
    }
    let mut payload = json!({
        "action": action,
        "issue": issue,
        "comment": {
            "id": comment_id,
            "body": comment_body,
            "html_url": format!("{html_url}#issuecomment-{comment_id}"),
            "user": { "login": author_login, "id": author_id },
        },
        "repository": {
            "id": repo_id,
            "name": repo_name,
            "full_name": format!("{owner}/{repo_name}"),
            "owner": { "login": owner },
        },
        "sender": {
            "login": sender_login,
            "id": sender_id,
        }
    });
    if let Some(from) = changes_from {
        payload["changes"] = json!({ "body": { "from": from } });
    }
    payload
}

/// Build a GitHub-compatible `pull_request_review_comment` payload — emitted
/// for diff-anchored PR comments (the `row.path.is_some()` branch; PR
/// conversation comments stay on `issue_comment` per DEBT-04). Actions:
/// `created` / `edited` / `deleted`; `edited` passes the pre-edit body as
/// `changes_from` → `changes.body.from`. `commit_id` is the SHA the comment
/// anchored to (`commit_sha` column).
#[expect(clippy::too_many_arguments)]
pub fn pull_request_review_comment_payload(
    action: &str,
    pull_number: i64,
    pull_title: &str,
    pull_state: &str,
    comment_id: &str,
    comment_body: &str,
    comment_path: &str,
    comment_side: Option<&str>,
    comment_line: Option<i64>,
    comment_start_line: Option<i64>,
    commit_id: Option<&str>,
    comment_created_at: &str,
    comment_updated_at: &str,
    changes_from: Option<&str>,
    author_login: &str,
    author_id: &str,
    owner: &str,
    repo_name: &str,
    repo_id: &str,
    sender_login: &str,
    sender_id: &str,
) -> serde_json::Value {
    let pull_url = format!("/{owner}/{repo_name}/pulls/{pull_number}");
    let comment = json!({
        "id": comment_id,
        "body": comment_body,
        "path": comment_path,
        "line": comment_line,
        "start_line": comment_start_line,
        "side": comment_side,
        "commit_id": commit_id,
        "html_url": format!("{pull_url}#discussion_r{comment_id}"),
        "user": { "login": author_login, "id": author_id },
        "created_at": comment_created_at,
        "updated_at": comment_updated_at,
    });
    let mut payload = json!({
        "action": action,
        "comment": comment,
        "pull_request": {
            "number": pull_number,
            "title": pull_title,
            "state": pull_state,
            "html_url": pull_url,
        },
        "repository": {
            "id": repo_id,
            "name": repo_name,
            "full_name": format!("{owner}/{repo_name}"),
            "owner": { "login": owner },
        },
        "sender": {
            "login": sender_login,
            "id": sender_id,
        }
    });
    if let Some(from) = changes_from {
        payload["changes"] = json!({ "body": { "from": from } });
    }
    payload
}

/// Synthetic ping payload (D-HOOK-05 / D-HOOK-21).
pub fn ping_payload(hook_id: &str, owner: &str, repo_name: &str) -> serde_json::Value {
    json!({
        "zen": "Oxidean webhooks are ready.",
        "hook_id": hook_id,
        "repository": {
            "full_name": format!("{owner}/{repo_name}"),
        }
    })
}

#[allow(clippy::too_many_arguments)]
/// Notify subscribed `push` hooks after a successful receive (D-HOOK-10 / D-HOOK-22).
pub async fn notify_push(
    db: &Database,
    repository_id: &str,
    owner: &str,
    repo_name: &str,
    pusher_login: &str,
    pusher_id: &str,
    updates: &[(String, String, String)],
    env_name: &str,
) {
    if updates.is_empty() {
        // Still emit a generic push so SSH paths without parsed pkt-lines notify subscribers.
        let payload = super::payloads::push_payload(
            owner,
            repo_name,
            repository_id,
            "refs/heads/main",
            "0000000000000000000000000000000000000000",
            "0000000000000000000000000000000000000001",
            pusher_login,
            pusher_id,
        );
        emit(db, repository_id, "push", "", payload, env_name).await;
        return;
    }
    for (before, after, ref_name) in updates {
        let payload = super::payloads::push_payload(
            owner,
            repo_name,
            repository_id,
            ref_name,
            before,
            after,
            pusher_login,
            pusher_id,
        );
        emit(db, repository_id, "push", "", payload, env_name).await;
    }
}

// ---------------------------------------------------------------------------
// API-04: broader event catalog — release / star / fork / create / delete /
// workflow_run / registry_package.
// ---------------------------------------------------------------------------

/// Shared `repository` block for repo-scoped payloads (GitHub shape).
fn repository_block(repo_id: &str, repo_name: &str, owner: &str) -> serde_json::Value {
    json!({
        "id": repo_id,
        "name": repo_name,
        "full_name": format!("{owner}/{repo_name}"),
        "owner": { "login": owner },
    })
}

/// True for an all-zero OID (`receive-pack` create/delete markers), including
/// the short `"0"` the SSH path synthesizes for new refs.
fn is_zero_oid(oid: &str) -> bool {
    !oid.is_empty() && oid.chars().all(|c| c == '0')
}

/// Resolve `repository_id` → (repo row, owner slug) for payload context.
/// Returns `None` (with a warn log) when the repo or its owner is missing.
async fn repo_context(
    db: &Database,
    repository_id: &str,
) -> Option<(oxidean_db::RepositoryRow, String)> {
    let repo = match db.find_repository_by_id(repository_id).await {
        Ok(Some(r)) => r,
        Ok(None) => return None,
        Err(e) => {
            tracing::warn!(error = %e, "webhook: repository lookup failed");
            return None;
        }
    };
    let owner = match crate::repo::owner_ref_for_repo(db, &repo).await {
        Ok(Some(o)) => o.slug().to_string(),
        Ok(None) => return None,
        Err(e) => {
            tracing::warn!(error = %e, "webhook: repo owner lookup failed");
            return None;
        }
    };
    Some((repo, owner))
}

/// Resolve a user id → login for `sender` blocks (empty string when absent).
async fn sender_login(db: &Database, user_id: Option<&str>) -> String {
    match user_id {
        Some(id) => match db.find_user_by_id(id).await {
            Ok(Some(u)) => u.username,
            Ok(None) => String::new(),
            Err(e) => {
                tracing::warn!(error = %e, "webhook: sender lookup failed");
                String::new()
            }
        },
        None => String::new(),
    }
}

/// `star` payload — emitted on `repo.star` (`created`) / `repo.unstar`
/// (`deleted`). GitHub delivers the same signal as a `watch` event with action
/// `started`; Oxidean names it `star` so it does not collide with the
/// notification-oriented `repo.watch` RPC.
pub fn star_payload(
    action: &str,
    owner: &str,
    repo_name: &str,
    repo_id: &str,
    sender_login: &str,
    sender_id: &str,
) -> serde_json::Value {
    json!({
        "action": action,
        "repository": repository_block(repo_id, repo_name, owner),
        "sender": { "login": sender_login, "id": sender_id },
    })
}

/// `fork` payload — emitted on the **source** repo when it is forked; `forkee`
/// is the new fork (GitHub shape).
#[expect(clippy::too_many_arguments)]
pub fn fork_payload(
    owner: &str,
    repo_name: &str,
    repo_id: &str,
    forkee_owner: &str,
    forkee_name: &str,
    forkee_id: &str,
    sender_login: &str,
    sender_id: &str,
) -> serde_json::Value {
    json!({
        "action": "created",
        "forkee": {
            "id": forkee_id,
            "name": forkee_name,
            "full_name": format!("{forkee_owner}/{forkee_name}"),
            "owner": { "login": forkee_owner },
            "html_url": format!("/{forkee_owner}/{forkee_name}"),
        },
        "repository": repository_block(repo_id, repo_name, owner),
        "sender": { "login": sender_login, "id": sender_id },
    })
}

/// `create` / `delete` ref payloads for branch and tag events. GitHub carries
/// no `action` on these (the event name is the action) and includes
/// `master_branch` / `description` only on `create`.
#[expect(clippy::too_many_arguments)]
pub fn ref_event_payload(
    event: &str,
    short_ref: &str,
    ref_type: &str,
    default_branch: &str,
    description: &str,
    owner: &str,
    repo_name: &str,
    repo_id: &str,
    sender_login: &str,
    sender_id: &str,
) -> serde_json::Value {
    let mut payload = json!({
        "ref": short_ref,
        "ref_type": ref_type,
        "pusher_type": "user",
        "repository": repository_block(repo_id, repo_name, owner),
        "sender": { "login": sender_login, "id": sender_id },
    });
    if event == "create" {
        payload["master_branch"] = json!(default_branch);
        payload["description"] = json!(description);
    }
    payload
}

/// GitHub-compatible `release` payload. `action` is one of `published`
/// (non-draft create or draft→publish), `created` (draft create), `edited`,
/// `unpublished` (publish→draft), or `deleted`.
#[expect(clippy::too_many_arguments)]
pub fn release_payload(
    action: &str,
    tag_name: &str,
    title: &str,
    body: &str,
    draft: bool,
    prerelease: bool,
    author_login: &str,
    author_id: &str,
    owner: &str,
    repo_name: &str,
    repo_id: &str,
    sender_login: &str,
    sender_id: &str,
) -> serde_json::Value {
    json!({
        "action": action,
        "release": {
            "tag_name": tag_name,
            "name": title,
            "body": body,
            "draft": draft,
            "prerelease": prerelease,
            "html_url": format!("/{owner}/{repo_name}/releases/{tag_name}"),
            "author": { "login": author_login, "id": author_id },
        },
        "repository": repository_block(repo_id, repo_name, owner),
        "sender": { "login": sender_login, "id": sender_id },
    })
}

/// GitHub-compatible `workflow_run` payload. Internal terminal statuses
/// (`success` / `failure` / `cancelled`) map to `status: "completed"` with the
/// internal value as `conclusion`; `queued` / `in_progress` pass through.
#[expect(clippy::too_many_arguments)]
pub fn workflow_run_payload(
    action: &str,
    run_id: &str,
    workflow_name: &str,
    workflow_path: &str,
    trigger_event: &str,
    head_sha: &str,
    head_ref: &str,
    run_status: &str,
    created_at: &str,
    updated_at: &str,
    owner: &str,
    repo_name: &str,
    repo_id: &str,
    sender_login: &str,
    sender_id: &str,
) -> serde_json::Value {
    let terminal = matches!(run_status, "success" | "failure" | "cancelled");
    let status = if terminal { "completed" } else { run_status };
    let conclusion = if terminal {
        json!(run_status)
    } else {
        serde_json::Value::Null
    };
    let head_branch = head_ref.strip_prefix("refs/heads/").unwrap_or(head_ref);
    json!({
        "action": action,
        "workflow_run": {
            "id": run_id,
            "name": workflow_name,
            "path": workflow_path,
            "event": trigger_event,
            "status": status,
            "conclusion": conclusion,
            "head_sha": head_sha,
            "head_branch": head_branch,
            "html_url": format!("/{owner}/{repo_name}/actions/{run_id}"),
            "created_at": created_at,
            "updated_at": updated_at,
        },
        "workflow": { "name": workflow_name, "path": workflow_path },
        "repository": repository_block(repo_id, repo_name, owner),
        "sender": { "login": sender_login, "id": sender_id },
    })
}

#[allow(clippy::too_many_arguments)]
/// GitHub-compatible `registry_package` payload (`published` / `updated`).
/// Only emitted for packages linked to a repository (`packages.repository_id`)
/// because webhooks are repo-scoped.
pub fn registry_package_payload(
    action: &str,
    package_name: &str,
    package_format: &str,
    version: &str,
    owner: &str,
    repo_name: &str,
    repo_id: &str,
    sender_login: &str,
    sender_id: &str,
) -> serde_json::Value {
    json!({
        "action": action,
        "package": {
            "name": package_name,
            "package_type": package_format,
            "version": version,
            "html_url": format!("/{owner}/{repo_name}/packages"),
        },
        "repository": repository_block(repo_id, repo_name, owner),
        "sender": { "login": sender_login, "id": sender_id },
    })
}

/// After a successful receive-pack: emit `create` / `delete` for branch and
/// tag refs (API-04). Updates where both sides are non-zero are `push`
/// territory; refs outside `refs/heads/` / `refs/tags/` are not reported
/// (matches GitHub's `create`/`delete` scope).
#[expect(clippy::too_many_arguments)]
pub async fn notify_ref_events(
    db: &Database,
    repository_id: &str,
    owner: &str,
    repo_name: &str,
    pusher_login: &str,
    pusher_id: &str,
    updates: &[(String, String, String)],
    env_name: &str,
) {
    // (default_branch, description) — resolved lazily, only when a
    // create/delete update is actually present.
    let mut repo_meta: Option<(String, String)> = None;
    for (before, after, ref_name) in updates {
        let (short_ref, ref_type) = if let Some(b) = ref_name.strip_prefix("refs/heads/") {
            (b, "branch")
        } else if let Some(t) = ref_name.strip_prefix("refs/tags/") {
            (t, "tag")
        } else {
            continue;
        };
        let event = if is_zero_oid(before) {
            "create"
        } else if is_zero_oid(after) {
            "delete"
        } else {
            continue;
        };
        if repo_meta.is_none() {
            repo_meta = db
                .find_repository_by_id(repository_id)
                .await
                .ok()
                .flatten()
                .map(|r| (r.default_branch, r.description));
        }
        let (default_branch, description) = repo_meta.clone().unwrap_or_default();
        let payload = ref_event_payload(
            event,
            short_ref,
            ref_type,
            &default_branch,
            &description,
            owner,
            repo_name,
            repository_id,
            pusher_login,
            pusher_id,
        );
        emit(db, repository_id, event, "", payload, env_name).await;
    }
}

/// Emit `workflow_run` for a run row by id — `requested` on enqueue/rerun,
/// `completed` once a terminal conclusion rolls up (API-04). Best-effort:
/// missing run/repo rows just skip the fan-out.
pub async fn notify_workflow_run(db: &Database, run_id: &str, action: &str, env_name: &str) {
    let Some(run) = db.find_action_run_by_id(run_id).await.ok().flatten() else {
        return;
    };
    let Some((repo, owner)) = repo_context(db, &run.repository_id).await else {
        return;
    };
    let sender = sender_login(db, run.triggered_by.as_deref()).await;
    let payload = workflow_run_payload(
        action,
        &run.id,
        &run.workflow_name,
        &run.workflow_path,
        &run.event,
        &run.head_sha,
        &run.head_ref,
        &run.status,
        &run.created_at,
        &run.updated_at,
        &owner,
        &repo.name,
        &repo.id,
        &sender,
        run.triggered_by.as_deref().unwrap_or(""),
    );
    emit(db, &run.repository_id, "workflow_run", action, payload, env_name).await;
}

/// Emit `registry_package` when a version is published on a package linked to
/// a repository (API-04). Unlinked packages have no webhook scope — nothing
/// is emitted for them.
pub async fn notify_package_publish(
    db: &Database,
    package: &oxidean_db::PackageRow,
    version: &str,
    action: &str,
    sender_id: &str,
    env_name: &str,
) {
    let Some(repo_id) = package.repository_id.as_deref() else {
        return;
    };
    let Some((repo, owner)) = repo_context(db, repo_id).await else {
        return;
    };
    let login = sender_login(db, Some(sender_id)).await;
    let payload = registry_package_payload(
        action,
        &package.name,
        &package.format,
        version,
        &owner,
        &repo.name,
        &repo.id,
        &login,
        sender_id,
    );
    emit(db, repo_id, "registry_package", action, payload, env_name).await;
}
