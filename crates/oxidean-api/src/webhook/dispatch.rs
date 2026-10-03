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
