//! GitHub-compatible webhook payload builders (D-HOOK-07 / D-HOOK-09 / D-HOOK-10).

use serde_json::{json, Value};

#[allow(clippy::too_many_arguments)]
pub fn push_payload(
    owner: &str,
    repo_name: &str,
    repo_id: &str,
    ref_name: &str,
    before: &str,
    after: &str,
    pusher_login: &str,
    pusher_id: &str,
) -> Value {
    json!({
        "ref": ref_name,
        "before": before,
        "after": after,
        "created": before.chars().all(|c| c == '0'),
        "deleted": after.chars().all(|c| c == '0'),
        "forced": false,
        "commits": [],
        "head_commit": null,
        "pusher": { "name": pusher_login, "email": "" },
        "sender": { "login": pusher_login, "id": pusher_id },
        "repository": {
            "id": repo_id,
            "name": repo_name,
            "full_name": format!("{owner}/{repo_name}"),
            "owner": { "login": owner },
        }
    })
}

#[allow(clippy::too_many_arguments)]
pub fn pull_request_payload(
    action: &str,
    number: i64,
    title: &str,
    body: &str,
    state: &str,
    draft: bool,
    merged: bool,
    base_ref: &str,
    head_ref: &str,
    head_sha: &str,
    head_owner: &str,
    head_repo: &str,
    owner: &str,
    repo_name: &str,
    repo_id: &str,
    sender_login: &str,
    sender_id: &str,
) -> Value {
    json!({
        "action": action,
        "number": number,
        "pull_request": {
            "number": number,
            "title": title,
            "body": body,
            "state": if merged { "closed" } else { state },
            "draft": draft,
            "merged": merged,
            "html_url": format!("/{owner}/{repo_name}/pulls/{number}"),
            "base": {
                "ref": base_ref,
                "repo": {
                    "name": repo_name,
                    "full_name": format!("{owner}/{repo_name}"),
                }
            },
            "head": {
                "ref": head_ref,
                "sha": head_sha,
                "repo": {
                    "name": head_repo,
                    "full_name": format!("{head_owner}/{head_repo}"),
                }
            }
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

/// Parse git receive-pack pkt-line ref update commands from the request body.
pub fn parse_receive_ref_updates(body: &[u8]) -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i + 4 <= body.len() {
        let len_hex = std::str::from_utf8(&body[i..i + 4]).unwrap_or("0000");
        let Ok(len) = usize::from_str_radix(len_hex, 16) else {
            break;
        };
        if len == 0 {
            break;
        }
        if i + len > body.len() || len < 4 {
            break;
        }
        let pkt = &body[i + 4..i + len];
        i += len;
        let line = String::from_utf8_lossy(pkt);
        let line = line.split('\0').next().unwrap_or("").trim();
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 3
            && parts[0].len() >= 40
            && parts[1].len() >= 40
            && parts[2].starts_with("refs/")
        {
            out.push((
                parts[0].to_string(),
                parts[1].to_string(),
                parts[2].to_string(),
            ));
        }
    }
    out
}
