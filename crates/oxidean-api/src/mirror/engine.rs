//! Per-ref two-way sync: merge mode (FF/merge/PR) or exact mode (LWW + deletes).

use std::collections::HashMap;
use std::path::Path;

use oxidean_db::{Database, RepositoryMirrorRow};
use oxidean_git::{
    validate_remote_url, GitBackend, RemoteAuthKind, RemoteCredentials,
};
use uuid::Uuid;

use crate::actions::secrets::{decrypt_secret, encrypt_secret};
use crate::git::bare_repo_path;
use crate::protection::{effective_for_branch, evaluate_push, ProtectionIntent};

const MIRROR_AUTHOR: &str = "Oxidean Mirror <mirror@oxidean.invalid>";

pub async fn run_mirror_sync(
    db: &Database,
    git: &dyn GitBackend,
    repos_dir: &Path,
    mirror_id: &str,
) -> Result<(), String> {
    let mirror = db
        .get_mirror_by_id(mirror_id)
        .await?
        .ok_or_else(|| "mirror not found".to_string())?;
    if !mirror.enabled {
        return Ok(());
    }

    let _ = db
        .update_mirror_status(mirror_id, "running", "", false)
        .await;

    let inner_result = run_mirror_sync_inner(db, git, repos_dir, &mirror).await;
    // GIT-25: refresh cached size_bytes — fetches land objects without update
    // hooks, and even a partial-error sync may have written objects.
    if let Ok(Some(repo)) = db.find_repository_by_id(&mirror.repository_id).await {
        if let Ok(owner_slug) = owner_slug_for_repo(db, &repo).await {
            if let Ok(bare) = bare_repo_path(repos_dir, &owner_slug, &repo.name) {
                if let Err(e) =
                    crate::git::quota::refresh_repo_size_bytes(db, &repo.id, &bare).await
                {
                    tracing::warn!(error = %e, "refresh repo size_bytes failed");
                }
            }
        }
    }
    match inner_result {
        Ok(status) => {
            let err = if status == "ok" {
                ""
            } else {
                "one or more refs conflicted or failed"
            };
            db.update_mirror_status(mirror_id, status, err, true)
                .await?;
            Ok(())
        }
        Err(e) => {
            let _ = db
                .update_mirror_status(mirror_id, "error", &e, true)
                .await;
            Err(e)
        }
    }
}

async fn run_mirror_sync_inner(
    db: &Database,
    git: &dyn GitBackend,
    repos_dir: &Path,
    mirror: &RepositoryMirrorRow,
) -> Result<&'static str, String> {
    let repo = db
        .find_repository_by_id(&mirror.repository_id)
        .await?
        .ok_or_else(|| "repository not found".to_string())?;
    let owner_slug = owner_slug_for_repo(db, &repo).await?;
    let bare = bare_repo_path(repos_dir, &owner_slug, &repo.name)
        .map_err(|e| e.message.clone())?;

    let credentials = credentials_from_mirror(mirror)?;
    let url = validate_remote_url(&mirror.remote_url)
        .map_err(|e| e.to_string())?
        .to_string();

    // GIT-25: `git fetch` lands objects without running hooks/update, so a repo
    // already over quota pauses mirror syncs here (same "block the next write
    // once over" semantics as push enforcement).
    if let Some(quota) = crate::git::quota::effective_repo_quota_bytes(db, &repo).await? {
        let used = crate::git::quota::repo_disk_usage_bytes(&bare).await?;
        if used > quota {
            return Err(format!(
                "repository is over its git storage quota ({used} > {quota} bytes)"
            ));
        }
    }

    git.fetch_from_url(&bare, &url, &credentials)
        .await
        .map_err(|e| e.to_string())?;

    if mirror.sync_mode == "exact" {
        return run_exact_sync(db, git, &bare, &url, &credentials, mirror).await;
    }

    run_merge_sync(db, git, &bare, &url, &credentials, mirror).await
}

async fn run_merge_sync(
    db: &Database,
    git: &dyn GitBackend,
    bare: &Path,
    url: &str,
    credentials: &RemoteCredentials,
    mirror: &RepositoryMirrorRow,
) -> Result<&'static str, String> {
    let local_refs = git.list_refs(bare).await.map_err(|e| e.to_string())?;
    let mut local_map: HashMap<String, String> = HashMap::new();
    for r in &local_refs {
        local_map.insert(r.name.clone(), r.oid.clone());
    }

    let mirror_heads = list_mirror_namespace(git, bare, "refs/oxidean/mirror/heads/").await?;
    let mirror_tags = list_mirror_namespace(git, bare, "refs/oxidean/mirror/tags/").await?;

    let mut had_conflict = false;
    let mut had_error = false;

    for (short, remote_oid) in &mirror_heads {
        let local_ref = format!("refs/heads/{short}");
        let outcome = sync_branch(
            db,
            git,
            bare,
            url,
            credentials,
            &mirror.id,
            &mirror.repository_id,
            &local_ref,
            local_map.get(&local_ref).map(|s| s.as_str()),
            remote_oid,
        )
        .await;
        record_outcome(db, &mirror.id, &local_ref, &outcome).await?;
        match outcome.outcome.as_str() {
            "conflict" => had_conflict = true,
            "error" => had_error = true,
            _ => {}
        }
    }

    for (short, remote_oid) in &mirror_tags {
        let local_ref = format!("refs/tags/{short}");
        let outcome = sync_tag(
            git,
            bare,
            url,
            credentials,
            &local_ref,
            local_map.get(&local_ref).map(|s| s.as_str()),
            remote_oid,
        )
        .await;
        record_outcome(db, &mirror.id, &local_ref, &outcome).await?;
        match outcome.outcome.as_str() {
            "conflict" => had_conflict = true,
            "error" => had_error = true,
            _ => {}
        }
    }

    // Push local-only heads that remote does not have (create on remote).
    for r in &local_refs {
        if let Some(short) = r.name.strip_prefix("refs/heads/") {
            if mirror_heads.contains_key(short) {
                continue;
            }
            match git
                .push_to_url(bare, url, credentials, &r.name, &r.name)
                .await
            {
                Ok(()) => {
                    record_outcome(
                        db,
                        &mirror.id,
                        &r.name,
                        &RefOutcome {
                            outcome: "ff_out".into(),
                            local_oid: r.oid.clone(),
                            remote_oid: String::new(),
                            detail: "created on remote".into(),
                        },
                    )
                    .await?;
                }
                Err(e) => {
                    had_error = true;
                    record_outcome(
                        db,
                        &mirror.id,
                        &r.name,
                        &RefOutcome {
                            outcome: "error".into(),
                            local_oid: r.oid.clone(),
                            remote_oid: String::new(),
                            detail: e.to_string(),
                        },
                    )
                    .await?;
                }
            }
        }
    }

    if had_conflict {
        Ok("conflict")
    } else if had_error {
        Ok("error")
    } else {
        Ok("ok")
    }
}

/// Exact 1:1 sync: LWW tips, both-way deletes via snapshot, no mirror/* PRs.
async fn run_exact_sync(
    db: &Database,
    git: &dyn GitBackend,
    bare: &Path,
    url: &str,
    credentials: &RemoteCredentials,
    mirror: &RepositoryMirrorRow,
) -> Result<&'static str, String> {
    // Drop stale merge-mode helper branches so they are never pushed.
    cleanup_mirror_helper_branches(git, bare).await;

    let local_refs = git.list_refs(bare).await.map_err(|e| e.to_string())?;
    let mut local_heads: HashMap<String, String> = HashMap::new();
    let mut local_tags: HashMap<String, String> = HashMap::new();
    for r in &local_refs {
        if let Some(short) = r.name.strip_prefix("refs/heads/") {
            if short.starts_with("mirror/") {
                continue;
            }
            local_heads.insert(short.to_string(), r.oid.clone());
        } else if let Some(short) = r.name.strip_prefix("refs/tags/") {
            local_tags.insert(short.to_string(), r.oid.clone());
        }
    }

    let remote_heads = list_mirror_namespace(git, bare, "refs/oxidean/mirror/heads/").await?;
    let remote_tags = list_mirror_namespace(git, bare, "refs/oxidean/mirror/tags/").await?;

    let snapshot = parse_ref_snapshot(&mirror.last_ref_snapshot);
    let mut had_error = false;
    let mut next_snapshot: HashMap<String, String> = HashMap::new();

    // Heads
    let mut head_names: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    head_names.extend(local_heads.keys().cloned());
    head_names.extend(remote_heads.keys().cloned());

    for short in head_names {
        if short.starts_with("mirror/") {
            continue;
        }
        let local_ref = format!("refs/heads/{short}");
        let local_oid = local_heads.get(&short).cloned();
        let remote_oid = remote_heads.get(&short).cloned();
        let in_snap = snapshot.contains_key(&local_ref);

        let outcome = exact_sync_one_ref(
            db,
            git,
            bare,
            url,
            credentials,
            &mirror.repository_id,
            &local_ref,
            local_oid.as_deref(),
            remote_oid.as_deref(),
            in_snap,
            true,
        )
        .await;
        if outcome.outcome == "error" {
            had_error = true;
        }
        record_outcome(db, &mirror.id, &local_ref, &outcome).await?;

        // Agreed tip for snapshot: surviving side after sync.
        match (outcome.outcome.as_str(), local_oid.as_deref(), remote_oid.as_deref()) {
            ("error", _, _) => {
                // Keep prior snapshot entry if any so deletes can retry.
                if let Some(prev) = snapshot.get(&local_ref) {
                    next_snapshot.insert(local_ref.clone(), prev.clone());
                } else if let Some(oid) = local_oid.or(remote_oid) {
                    next_snapshot.insert(local_ref.clone(), oid);
                }
            }
            ("ff_in" | "ff_out" | "skipped", _, _) => {
                let tip = if !outcome.local_oid.is_empty() {
                    outcome.local_oid.clone()
                } else if !outcome.remote_oid.is_empty() {
                    outcome.remote_oid.clone()
                } else {
                    String::new()
                };
                if outcome.detail.starts_with("deleted") {
                    // Gone on both sides — omit from snapshot.
                } else if !tip.is_empty() {
                    next_snapshot.insert(local_ref, tip);
                }
            }
            _ => {}
        }
    }

    // Tags
    let mut tag_names: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    tag_names.extend(local_tags.keys().cloned());
    tag_names.extend(remote_tags.keys().cloned());

    for short in tag_names {
        let local_ref = format!("refs/tags/{short}");
        let local_oid = local_tags.get(&short).cloned();
        let remote_oid = remote_tags.get(&short).cloned();
        let in_snap = snapshot.contains_key(&local_ref);

        let outcome = exact_sync_one_ref(
            db,
            git,
            bare,
            url,
            credentials,
            &mirror.repository_id,
            &local_ref,
            local_oid.as_deref(),
            remote_oid.as_deref(),
            in_snap,
            false,
        )
        .await;
        if outcome.outcome == "error" {
            had_error = true;
        }
        record_outcome(db, &mirror.id, &local_ref, &outcome).await?;

        match (outcome.outcome.as_str(), local_oid.as_deref(), remote_oid.as_deref()) {
            ("error", _, _) => {
                if let Some(prev) = snapshot.get(&local_ref) {
                    next_snapshot.insert(local_ref.clone(), prev.clone());
                } else if let Some(oid) = local_oid.or(remote_oid) {
                    next_snapshot.insert(local_ref.clone(), oid);
                }
            }
            ("ff_in" | "ff_out" | "skipped", _, _) => {
                let tip = if !outcome.local_oid.is_empty() {
                    outcome.local_oid.clone()
                } else if !outcome.remote_oid.is_empty() {
                    outcome.remote_oid.clone()
                } else {
                    String::new()
                };
                if outcome.detail.starts_with("deleted") {
                    // omit
                } else if !tip.is_empty() {
                    next_snapshot.insert(local_ref, tip);
                }
            }
            _ => {}
        }
    }

    if !had_error {
        let snap_json =
            serde_json::to_string(&next_snapshot).unwrap_or_else(|_| "{}".to_string());
        db.update_mirror_ref_snapshot(&mirror.id, &snap_json)
            .await?;
        Ok("ok")
    } else {
        // Still persist partial snapshot progress so baselines advance when possible.
        let snap_json =
            serde_json::to_string(&next_snapshot).unwrap_or_else(|_| "{}".to_string());
        let _ = db
            .update_mirror_ref_snapshot(&mirror.id, &snap_json)
            .await;
        Ok("error")
    }
}

fn parse_ref_snapshot(raw: &str) -> HashMap<String, String> {
    serde_json::from_str::<HashMap<String, String>>(raw.trim())
        .unwrap_or_default()
}

async fn cleanup_mirror_helper_branches(git: &dyn GitBackend, bare: &Path) {
    let Ok(refs) = git.list_refs(bare).await else {
        return;
    };
    for r in refs {
        if let Some(short) = r.name.strip_prefix("refs/heads/") {
            if short.starts_with("mirror/") {
                let _ = git.branch_delete(bare, short).await;
            }
        }
    }
}

async fn exact_sync_one_ref(
    db: &Database,
    git: &dyn GitBackend,
    bare: &Path,
    url: &str,
    credentials: &RemoteCredentials,
    repository_id: &str,
    local_ref: &str,
    local_oid: Option<&str>,
    remote_oid: Option<&str>,
    in_snapshot: bool,
    is_branch: bool,
) -> RefOutcome {
    match (local_oid, remote_oid) {
        (Some(l), Some(r)) if l == r => RefOutcome {
            outcome: "skipped".into(),
            local_oid: l.into(),
            remote_oid: r.into(),
            detail: "equal".into(),
        },
        (Some(l), Some(r)) => {
            // LWW by committer unix time; tie → remote wins.
            let lt = git.committer_unix_time(bare, l).await.unwrap_or(0);
            let rt = git.committer_unix_time(bare, r).await.unwrap_or(0);
            if rt >= lt {
                exact_force_local(
                    db,
                    git,
                    bare,
                    repository_id,
                    local_ref,
                    r,
                    is_branch,
                    "exact: remote LWW",
                )
                .await
            } else {
                exact_force_remote(git, bare, url, credentials, local_ref, l, "exact: local LWW")
                    .await
            }
        }
        (None, Some(r)) => {
            if in_snapshot {
                // Local deleted while remote still has the tip — delete remote.
                exact_delete_remote(git, bare, url, credentials, local_ref, r).await
            } else {
                // Remote-only — create local from remote.
                exact_force_local(
                    db,
                    git,
                    bare,
                    repository_id,
                    local_ref,
                    r,
                    is_branch,
                    "exact: create from remote",
                )
                .await
            }
        }
        (Some(l), None) => {
            if in_snapshot {
                // Remote deleted — delete local.
                exact_delete_local(db, git, bare, repository_id, local_ref, l, is_branch).await
            } else {
                // Local-only — create on remote.
                exact_force_remote(
                    git,
                    bare,
                    url,
                    credentials,
                    local_ref,
                    l,
                    "exact: create on remote",
                )
                .await
            }
        }
        (None, None) => RefOutcome {
            outcome: "skipped".into(),
            local_oid: String::new(),
            remote_oid: String::new(),
            detail: "absent both sides".into(),
        },
    }
}

async fn exact_force_local(
    db: &Database,
    git: &dyn GitBackend,
    bare: &Path,
    repository_id: &str,
    local_ref: &str,
    target_sha: &str,
    is_branch: bool,
    detail: &str,
) -> RefOutcome {
    if is_branch {
        let branch = local_ref
            .strip_prefix("refs/heads/")
            .unwrap_or(local_ref);
        if let Ok(eff) = effective_for_branch(db, repository_id, branch).await {
            if evaluate_push(
                &eff,
                ProtectionIntent::ForcePush,
                Some(crate::repo::Capability::Admin),
            )
            .is_err()
            {
                return RefOutcome {
                    outcome: "error".into(),
                    local_oid: String::new(),
                    remote_oid: target_sha.into(),
                    detail: "exact: blocked by enforce_admins / protection".into(),
                };
            }
        }
    }
    match git.force_update_ref(bare, local_ref, target_sha).await {
        Ok(()) => RefOutcome {
            outcome: "ff_in".into(),
            local_oid: target_sha.into(),
            remote_oid: target_sha.into(),
            detail: detail.into(),
        },
        Err(e) => RefOutcome {
            outcome: "error".into(),
            local_oid: String::new(),
            remote_oid: target_sha.into(),
            detail: e.to_string(),
        },
    }
}

async fn exact_force_remote(
    git: &dyn GitBackend,
    bare: &Path,
    url: &str,
    credentials: &RemoteCredentials,
    local_ref: &str,
    local_oid: &str,
    detail: &str,
) -> RefOutcome {
    match git
        .push_to_url_force(bare, url, credentials, local_ref, local_ref)
        .await
    {
        Ok(()) => RefOutcome {
            outcome: "ff_out".into(),
            local_oid: local_oid.into(),
            remote_oid: local_oid.into(),
            detail: detail.into(),
        },
        Err(e) => RefOutcome {
            outcome: "error".into(),
            local_oid: local_oid.into(),
            remote_oid: String::new(),
            detail: e.to_string(),
        },
    }
}

async fn exact_delete_local(
    db: &Database,
    git: &dyn GitBackend,
    bare: &Path,
    repository_id: &str,
    local_ref: &str,
    local_oid: &str,
    is_branch: bool,
) -> RefOutcome {
    if is_branch {
        let branch = local_ref
            .strip_prefix("refs/heads/")
            .unwrap_or(local_ref);
        if let Ok(eff) = effective_for_branch(db, repository_id, branch).await {
            if evaluate_push(
                &eff,
                ProtectionIntent::Delete,
                Some(crate::repo::Capability::Admin),
            )
            .is_err()
            {
                return RefOutcome {
                    outcome: "error".into(),
                    local_oid: local_oid.into(),
                    remote_oid: String::new(),
                    detail: "exact: delete blocked by enforce_admins / protection".into(),
                };
            }
        }
        match git.branch_delete(bare, branch).await {
            Ok(()) => RefOutcome {
                outcome: "ff_in".into(),
                local_oid: String::new(),
                remote_oid: String::new(),
                detail: "deleted local".into(),
            },
            Err(e) => RefOutcome {
                outcome: "error".into(),
                local_oid: local_oid.into(),
                remote_oid: String::new(),
                detail: e.to_string(),
            },
        }
    } else {
        // Tag delete via update-ref delete through force push empty — use branch_delete-like:
        // `git update-ref -d` not exposed; use push delete to local bare via force_update is wrong.
        // Delete tag with: push :refs/tags/X to file://bare — simpler: shell update-ref.
        let bare_s = match bare.to_str() {
            Some(s) => s,
            None => {
                return RefOutcome {
                    outcome: "error".into(),
                    local_oid: local_oid.into(),
                    remote_oid: String::new(),
                    detail: "non-utf8 bare".into(),
                };
            }
        };
        let status = tokio::process::Command::new("git")
            .args(["-C", bare_s, "update-ref", "-d", local_ref])
            .output()
            .await;
        match status {
            Ok(o) if o.status.success() => RefOutcome {
                outcome: "ff_in".into(),
                local_oid: String::new(),
                remote_oid: String::new(),
                detail: "deleted local".into(),
            },
            Ok(o) => RefOutcome {
                outcome: "error".into(),
                local_oid: local_oid.into(),
                remote_oid: String::new(),
                detail: String::from_utf8_lossy(&o.stderr).trim().to_string(),
            },
            Err(e) => RefOutcome {
                outcome: "error".into(),
                local_oid: local_oid.into(),
                remote_oid: String::new(),
                detail: e.to_string(),
            },
        }
    }
}

/// Delete a ref on the remote (local already missing).
async fn exact_delete_remote(
    git: &dyn GitBackend,
    bare: &Path,
    url: &str,
    credentials: &RemoteCredentials,
    local_ref: &str,
    remote_oid: &str,
) -> RefOutcome {
    match git
        .push_to_url_force(bare, url, credentials, "", local_ref)
        .await
    {
        Ok(()) => RefOutcome {
            outcome: "ff_out".into(),
            local_oid: String::new(),
            remote_oid: String::new(),
            detail: "deleted remote".into(),
        },
        Err(e) => RefOutcome {
            outcome: "error".into(),
            local_oid: String::new(),
            remote_oid: remote_oid.into(),
            detail: e.to_string(),
        },
    }
}

struct RefOutcome {
    outcome: String,
    local_oid: String,
    remote_oid: String,
    detail: String,
}

async fn sync_branch(
    db: &Database,
    git: &dyn GitBackend,
    bare: &Path,
    url: &str,
    credentials: &RemoteCredentials,
    mirror_id: &str,
    repository_id: &str,
    local_ref: &str,
    local_oid: Option<&str>,
    remote_oid: &str,
) -> RefOutcome {
    let branch = local_ref
        .strip_prefix("refs/heads/")
        .unwrap_or(local_ref);

    match local_oid {
        None => {
            // Create local from remote (through FF push path for hooks).
            if let Err(e) =
                apply_local_ff(db, git, bare, mirror_id, repository_id, local_ref, remote_oid, false)
                    .await
            {
                return RefOutcome {
                    outcome: "error".into(),
                    local_oid: String::new(),
                    remote_oid: remote_oid.into(),
                    detail: e,
                };
            }
            RefOutcome {
                outcome: "ff_in".into(),
                local_oid: remote_oid.into(),
                remote_oid: remote_oid.into(),
                detail: String::new(),
            }
        }
        Some(local) if local == remote_oid => RefOutcome {
            outcome: "skipped".into(),
            local_oid: local.into(),
            remote_oid: remote_oid.into(),
            detail: "equal".into(),
        },
        Some(local) => {
            let remote_anc = git
                .is_ancestor(bare, remote_oid, local)
                .await
                .unwrap_or(false);
            let local_anc = git
                .is_ancestor(bare, local, remote_oid)
                .await
                .unwrap_or(false);

            if remote_anc && !local_anc {
                // Local ahead — push out.
                match git
                    .push_to_url(bare, url, credentials, local_ref, local_ref)
                    .await
                {
                    Ok(()) => RefOutcome {
                        outcome: "ff_out".into(),
                        local_oid: local.into(),
                        remote_oid: remote_oid.into(),
                        detail: String::new(),
                    },
                    Err(e) => RefOutcome {
                        outcome: "error".into(),
                        local_oid: local.into(),
                        remote_oid: remote_oid.into(),
                        detail: e.to_string(),
                    },
                }
            } else if local_anc && !remote_anc {
                // Remote ahead — FF local.
                if let Err(e) = apply_local_ff(
                    db,
                    git,
                    bare,
                    mirror_id,
                    repository_id,
                    local_ref,
                    remote_oid,
                    true,
                )
                .await
                {
                    return RefOutcome {
                        outcome: "error".into(),
                        local_oid: local.into(),
                        remote_oid: remote_oid.into(),
                        detail: e,
                    };
                }
                RefOutcome {
                    outcome: "ff_in".into(),
                    local_oid: remote_oid.into(),
                    remote_oid: remote_oid.into(),
                    detail: String::new(),
                }
            } else {
                // Diverged — merge.
                let msg = format!("Mirror merge of {branch} ({MIRROR_AUTHOR})");
                match git.merge_commit(bare, branch, remote_oid, &msg).await {
                    Ok(merged) => {
                        match git
                            .push_to_url(bare, url, credentials, local_ref, local_ref)
                            .await
                        {
                            Ok(()) => RefOutcome {
                                outcome: "merged".into(),
                                local_oid: merged,
                                remote_oid: remote_oid.into(),
                                detail: String::new(),
                            },
                            Err(e) => RefOutcome {
                                outcome: "error".into(),
                                local_oid: merged,
                                remote_oid: remote_oid.into(),
                                detail: format!("merged locally but push failed: {e}"),
                            },
                        }
                    }
                    Err(e) => {
                        let detail = e.to_string();
                        // Conflict PR (best-effort) — do not force.
                        let _ = open_conflict_pr(
                            db,
                            git,
                            bare,
                            mirror_id,
                            repository_id,
                            branch,
                            remote_oid,
                        )
                        .await;
                        RefOutcome {
                            outcome: "conflict".into(),
                            local_oid: local.into(),
                            remote_oid: remote_oid.into(),
                            detail,
                        }
                    }
                }
            }
        }
    }
}

async fn sync_tag(
    git: &dyn GitBackend,
    bare: &Path,
    url: &str,
    credentials: &RemoteCredentials,
    local_ref: &str,
    local_oid: Option<&str>,
    remote_oid: &str,
) -> RefOutcome {
    match local_oid {
        None => match git
            .fast_forward_ref(bare, local_ref, remote_oid)
            .await
        {
            // Tags: create via update — fast_forward_ref uses refs/heads path.
            // For tags, push into bare with update-ref via a dedicated path:
            Ok(()) => RefOutcome {
                outcome: "ff_in".into(),
                local_oid: remote_oid.into(),
                remote_oid: remote_oid.into(),
                detail: String::new(),
            },
            Err(_) => {
                // Fallback: push from a temp tracking ref by fetching already done —
                // use push to local? For tags, call update via git push . 
                match push_tag_local(git, bare, local_ref, remote_oid).await {
                    Ok(()) => RefOutcome {
                        outcome: "ff_in".into(),
                        local_oid: remote_oid.into(),
                        remote_oid: remote_oid.into(),
                        detail: String::new(),
                    },
                    Err(e) => RefOutcome {
                        outcome: "error".into(),
                        local_oid: String::new(),
                        remote_oid: remote_oid.into(),
                        detail: e,
                    },
                }
            }
        },
        Some(local) if local == remote_oid => RefOutcome {
            outcome: "skipped".into(),
            local_oid: local.into(),
            remote_oid: remote_oid.into(),
            detail: "equal".into(),
        },
        Some(local) => {
            // Tag mismatch — never merge.
            // If remote doesn't have our tag, push ours; if both differ, conflict.
            let push_ours = git
                .push_to_url(bare, url, credentials, local_ref, local_ref)
                .await;
            // If tips differ, conflict regardless.
            let _ = push_ours;
            RefOutcome {
                outcome: "conflict".into(),
                local_oid: local.into(),
                remote_oid: remote_oid.into(),
                detail: "tag tips differ; tags are never merged".into(),
            }
        }
    }
}

async fn push_tag_local(
    git: &dyn GitBackend,
    bare: &Path,
    local_ref: &str,
    target_sha: &str,
) -> Result<(), String> {
    // Use fast_forward_ref only for heads; for tags create via clone+push.
    // Simpler: call git update-ref through a worktree push of an annotated/lightweight tag.
    // We reuse GitBackend by temporarily using branch_create-like path — push tag ref.
    let bare_s = bare
        .to_str()
        .ok_or_else(|| "non-utf8 bare".to_string())?;
    // Ensure object present then update-ref (tags typically don't run branch protection hooks
    // the same way; still prefer push). Use `git -C bare update-ref` via CliGitBackend is not
    // exposed — call through fast_forward by treating as heads is wrong.
    // Implement via push from a temp clone:
    let _ = git; // silence if unused in some builds
    let tmp = tempfile::tempdir().map_err(|e| e.to_string())?;
    let work = tmp.path();
    let work_s = work.to_str().ok_or_else(|| "non-utf8 work".to_string())?;
    let status = tokio::process::Command::new("git")
        .args(["clone", bare_s, work_s])
        .output()
        .await
        .map_err(|e| e.to_string())?;
    if !status.status.success() {
        return Err(String::from_utf8_lossy(&status.stderr).to_string());
    }
    let tag_name = local_ref.strip_prefix("refs/tags/").unwrap_or(local_ref);
    let status = tokio::process::Command::new("git")
        .args(["-C", work_s, "tag", tag_name, target_sha])
        .output()
        .await
        .map_err(|e| e.to_string())?;
    if !status.status.success() {
        // Tag may already exist pointing elsewhere.
        let _ = tokio::process::Command::new("git")
            .args(["-C", work_s, "tag", "-f", tag_name, target_sha])
            .output()
            .await;
    }
    let refspec = format!("refs/tags/{tag_name}:refs/tags/{tag_name}");
    let status = tokio::process::Command::new("git")
        .args(["-C", work_s, "push", "origin", &refspec])
        .output()
        .await
        .map_err(|e| e.to_string())?;
    if !status.status.success() {
        return Err(String::from_utf8_lossy(&status.stderr).trim().to_string());
    }
    Ok(())
}

async fn apply_local_ff(
    db: &Database,
    git: &dyn GitBackend,
    bare: &Path,
    mirror_id: &str,
    repository_id: &str,
    local_ref: &str,
    target_sha: &str,
    local_exists: bool,
) -> Result<(), String> {
    let branch = local_ref
        .strip_prefix("refs/heads/")
        .unwrap_or(local_ref);
    // Unborn / missing local branch: always create from the remote tip. Branch
    // protection cannot apply to a ref that does not exist yet — diverting to a
    // mirror/*/sync/* PR leaves the default branch empty (seen in production).
    if local_exists {
        let eff = effective_for_branch(db, repository_id, branch)
            .await
            .map_err(|e| e.message)?;
        // Mirror actor has no capability → cannot bypass require_reviews / lock.
        if evaluate_push(&eff, ProtectionIntent::Push, None).is_err() {
            return open_protection_pr(db, git, bare, mirror_id, repository_id, branch, target_sha)
                .await;
        }
    }
    git.fast_forward_ref(bare, local_ref, target_sha)
        .await
        .map_err(|e| e.to_string())
}

async fn open_protection_pr(
    db: &Database,
    git: &dyn GitBackend,
    bare: &Path,
    mirror_id: &str,
    repository_id: &str,
    base_branch: &str,
    head_sha: &str,
) -> Result<(), String> {
    let short_id = &mirror_id[..mirror_id.len().min(8)];
    let head_branch = format!("mirror/{short_id}/sync/{base_branch}");
    // Point head branch at remote tip.
    let _ = git.branch_delete(bare, &head_branch).await;
    git.branch_create(bare, &head_branch, head_sha)
        .await
        .map_err(|e| e.to_string())?;
    create_mirror_pr(db, repository_id, base_branch, &head_branch, "Mirror sync (protected branch)")
        .await
}

async fn open_conflict_pr(
    db: &Database,
    git: &dyn GitBackend,
    bare: &Path,
    mirror_id: &str,
    repository_id: &str,
    base_branch: &str,
    remote_oid: &str,
) -> Result<(), String> {
    let short_id = &mirror_id[..mirror_id.len().min(8)];
    let head_branch = format!("mirror/{short_id}/{base_branch}");
    let _ = git.branch_delete(bare, &head_branch).await;
    git.branch_create(bare, &head_branch, remote_oid)
        .await
        .map_err(|e| e.to_string())?;
    create_mirror_pr(
        db,
        repository_id,
        base_branch,
        &head_branch,
        "Mirror conflict — resolve manually",
    )
    .await
}

async fn create_mirror_pr(
    db: &Database,
    repository_id: &str,
    base: &str,
    head: &str,
    title: &str,
) -> Result<(), String> {
    let repo = db
        .find_repository_by_id(repository_id)
        .await?
        .ok_or_else(|| "repository not found".to_string())?;
    let author_id = match repo.owner_type.as_str() {
        "user" => repo.owner_id.clone(),
        _ => {
            let members = db.list_org_members(&repo.owner_id).await.unwrap_or_default();
            members
                .into_iter()
                .find(|m| {
                    m.role.eq_ignore_ascii_case("owner") || m.role.eq_ignore_ascii_case("admin")
                })
                .map(|m| m.user_id)
                .ok_or_else(|| "no org admin to author mirror PR".to_string())?
        }
    };
    let number = db.allocate_next_issue_number(repository_id).await?;
    let id = Uuid::new_v4().to_string();
    let _ = db
        .insert_pull(
            &id,
            repository_id,
            number,
            title,
            &format!(
                "Opened automatically by Oxidean two-way mirroring.\n\nBase: `{base}` ← Head: `{head}`"
            ),
            &author_id,
            base,
            "",
            repository_id,
            head,
            "",
            false,
        )
        .await?;
    Ok(())
}

async fn list_mirror_namespace(
    git: &dyn GitBackend,
    bare: &Path,
    prefix: &str,
) -> Result<HashMap<String, String>, String> {
    // list_refs only returns heads+tags — use rev-parse via for-each-ref shell.
    let bare_s = bare
        .to_str()
        .ok_or_else(|| "non-utf8 bare".to_string())?;
    let output = tokio::process::Command::new("git")
        .args([
            "-C",
            bare_s,
            "for-each-ref",
            "--format=%(objectname) %(refname)",
            prefix,
        ])
        .output()
        .await
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Ok(HashMap::new());
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut map = HashMap::new();
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        let oid = parts.next().unwrap_or("");
        let name = parts.next().unwrap_or("");
        if oid.is_empty() || name.is_empty() {
            continue;
        }
        if let Some(short) = name.strip_prefix(prefix) {
            map.insert(short.to_string(), oid.to_string());
        }
    }
    let _ = git;
    Ok(map)
}

async fn record_outcome(
    db: &Database,
    mirror_id: &str,
    refname: &str,
    outcome: &RefOutcome,
) -> Result<(), String> {
    db.upsert_mirror_ref_result(
        &Uuid::new_v4().to_string(),
        mirror_id,
        refname,
        &outcome.outcome,
        &outcome.local_oid,
        &outcome.remote_oid,
        &outcome.detail,
    )
    .await
}

pub fn credentials_from_mirror(mirror: &RepositoryMirrorRow) -> Result<RemoteCredentials, String> {
    let secret = if mirror.secret_ciphertext.is_empty() {
        return Err("mirror has no credentials configured".into());
    } else {
        decrypt_secret(&mirror.secret_ciphertext)?
    };
    match mirror.auth_kind.as_str() {
        "https_token" => Ok(RemoteCredentials {
            kind: RemoteAuthKind::HttpsToken,
            username: Some(if mirror.username.is_empty() {
                "git".into()
            } else {
                mirror.username.clone()
            }),
            secret,
            known_hosts: None,
        }),
        "ssh_key" => Ok(RemoteCredentials {
            kind: RemoteAuthKind::SshKey,
            username: None,
            secret,
            known_hosts: Some(mirror.known_hosts.clone()),
        }),
        other => Err(format!("unknown auth_kind: {other}")),
    }
}

async fn owner_slug_for_repo(
    db: &Database,
    repo: &oxidean_db::RepositoryRow,
) -> Result<String, String> {
    match repo.owner_type.as_str() {
        "user" => {
            let u = db
                .find_user_by_id(&repo.owner_id)
                .await?
                .ok_or_else(|| "owner user not found".to_string())?;
            Ok(u.username)
        }
        "org" => {
            let o = db
                .find_organization_by_id(&repo.owner_id)
                .await?
                .ok_or_else(|| "owner org not found".to_string())?;
            Ok(o.slug)
        }
        other => Err(format!("unknown owner_type: {other}")),
    }
}

pub fn encrypt_mirror_secret(plaintext: &str) -> Result<String, String> {
    encrypt_secret(plaintext)
}

pub fn generate_webhook_secret() -> String {
    use sha2::{Digest, Sha256};
    let mut raw = [0u8; 32];
    let _ = getrandom::getrandom(&mut raw);
    let mut h = Sha256::new();
    h.update(raw);
    let dig = h.finalize();
    let mut out = String::from("octamh_");
    for b in dig {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

#[cfg(test)]
mod exact_snapshot_tests {
    use super::parse_ref_snapshot;

    #[test]
    fn parse_ref_snapshot_empty_and_object() {
        assert!(parse_ref_snapshot("{}").is_empty());
        assert!(parse_ref_snapshot("").is_empty());
        assert!(parse_ref_snapshot("not-json").is_empty());
        let m = parse_ref_snapshot(r#"{"refs/heads/main":"abc","refs/tags/v1":"def"}"#);
        assert_eq!(m.get("refs/heads/main").map(String::as_str), Some("abc"));
        assert_eq!(m.get("refs/tags/v1").map(String::as_str), Some("def"));
    }
}
