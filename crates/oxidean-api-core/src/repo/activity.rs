//! Repository activity feed — durable push / branch / merge events + `repo.activity.list`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use oxidean_db::{Database, ObservedCommit};
use oxidean_git::{GitBackend, PushEventCommit};
use uuid::Uuid;

const ZERO_OID: &str = "0000000000000000000000000000000000000000";

/// Per-push enumeration cap — same 2048 bound GitHub puts on push-event
/// commit lists.
const PUSH_WALK_CAP: u32 = 2048;
/// One lazy history backfill walks at most this many commits (documented
/// truncation; repos deeper than that keep the newest 20k attributed).
const BACKFILL_WALK_CAP: u32 = 20_000;

fn default_head_ref(default_branch: &str) -> String {
    let b = default_branch
        .strip_prefix("refs/heads/")
        .unwrap_or(default_branch);
    format!("refs/heads/{b}")
}

fn observed(commits: Vec<PushEventCommit>) -> Vec<ObservedCommit> {
    commits
        .into_iter()
        .map(|c| ObservedCommit {
            sha: c.sha,
            author_email: c.author_email,
            authored_at: c.authored_at,
        })
        .collect()
}

pub const VALID_PUSH_TYPES: &[&str] = &[
    "push",
    "force_push",
    "pr_merge",
    "branch_creation",
    "branch_deletion",
    "branch_rename",
];

fn is_zero_oid(oid: &str) -> bool {
    !oid.is_empty() && oid.chars().all(|c| c == '0')
}

pub fn short_ref(ref_name: &str) -> String {
    ref_name
        .strip_prefix("refs/heads/")
        .or_else(|| ref_name.strip_prefix("refs/tags/"))
        .unwrap_or(ref_name)
        .to_string()
}

fn classify_push_type(before: &str, after: &str, forced: bool) -> &'static str {
    if is_zero_oid(after) {
        "branch_deletion"
    } else if is_zero_oid(before) {
        "branch_creation"
    } else if forced {
        "force_push"
    } else {
        "push"
    }
}

/// Whether `maybe_ancestor` is an ancestor of `tip`.
///
/// - `Some(true)` / `Some(false)` — definitive git result (exit 0 / 1)
/// - `None` — cannot tell (spawn failure, missing objects, bad path); callers must
///   **not** treat this as a force-push (avoids false `force_push` labels).
async fn is_ancestor(bare: &Path, maybe_ancestor: &str, tip: &str) -> Option<bool> {
    if maybe_ancestor.is_empty()
        || tip.is_empty()
        || maybe_ancestor == tip
        || is_zero_oid(maybe_ancestor)
        || is_zero_oid(tip)
    {
        return Some(true);
    }
    let bare_str = bare.to_str()?;
    let out = tokio::process::Command::new("git")
        .args([
            "-C",
            bare_str,
            "merge-base",
            "--is-ancestor",
            maybe_ancestor,
            tip,
        ])
        .output()
        .await
        .ok()?;
    match out.status.code() {
        Some(0) => Some(true),
        Some(1) => Some(false),
        _ => None,
    }
}

async fn enrich_commits(
    git: &dyn GitBackend,
    bare: &Path,
    before: &str,
    after: &str,
) -> (i64, Option<String>) {
    if is_zero_oid(after) {
        return (0, None);
    }
    let message = match git.show_commit(bare, after, None, None).await {
        Ok(detail) => {
            let subject = detail.subject.trim();
            if subject.is_empty() {
                None
            } else {
                Some(subject.to_string())
            }
        }
        Err(_) => None,
    };
    let count = if is_zero_oid(before) {
        1
    } else {
        let range = format!("{before}..{after}");
        git.rev_list_count(bare, &range).await.unwrap_or(0) as i64
    };
    (count.max(0), message)
}

/// Best-effort: write one activity row per ref update after a successful receive-pack.
pub async fn record_ref_updates(
    db: &Database,
    repository_id: &str,
    actor_id: &str,
    updates: &[(String, String, String)],
    git: Option<Arc<dyn GitBackend>>,
    bare: Option<&Path>,
) {
    if updates.is_empty() || actor_id.is_empty() {
        return;
    }
    // Authored-commit attribution (COL-12) needs the repo's default branch —
    // only commits landing on it count, per GitHub's rule. One lookup per
    // call, only when a git walk is even possible.
    let repo_row = match (git.is_some(), bare) {
        (true, Some(_)) => db.find_repository_by_id(repository_id).await.ok().flatten(),
        _ => None,
    };
    for (before, after, ref_name) in updates {
        let forced = if let (_, Some(path)) = (git.as_ref(), bare) {
            !is_zero_oid(before)
                && !is_zero_oid(after)
                && matches!(is_ancestor(path, before, after).await, Some(false))
        } else {
            false
        };
        let push_type = classify_push_type(before, after, forced);
        let (commits_count, commit_message) = if let (Some(g), Some(path)) = (git.as_ref(), bare) {
            enrich_commits(g.as_ref(), path, before, after).await
        } else {
            (0, None)
        };
        let id = Uuid::new_v4().to_string();
        if let Err(e) = db
            .insert_repo_activity(
                &id,
                repository_id,
                actor_id,
                push_type,
                Some(ref_name),
                Some(before),
                Some(after),
                Some(commits_count),
                commit_message.as_deref(),
                None,
            )
            .await
        {
            tracing::warn!(
                error = %e,
                repository_id,
                ref_name,
                "repository activity insert failed"
            );
        }
        if let (Some(g), Some(path), Some(repo)) = (git.as_ref(), bare, repo_row.as_ref()) {
            if *ref_name == default_head_ref(&repo.default_branch) && !is_zero_oid(after) {
                record_authored_range(
                    db,
                    g.as_ref(),
                    path,
                    repository_id,
                    before,
                    after,
                    PUSH_WALK_CAP,
                )
                .await;
                spawn_contribution_backfill(
                    db.clone(),
                    g.clone(),
                    path.to_path_buf(),
                    repository_id.to_string(),
                    default_head_ref(&repo.default_branch),
                );
            }
        }
    }
}

/// Walk `before..after` (zero/empty `before` = full history to `after`) and
/// record authored commits for `repository_id`. Best-effort: failures warn,
/// never fail the caller's operation.
async fn record_authored_range(
    db: &Database,
    git: &dyn GitBackend,
    bare: &Path,
    repository_id: &str,
    before: &str,
    after: &str,
    cap: u32,
) {
    if is_zero_oid(after) {
        return;
    }
    let old = if is_zero_oid(before) { "" } else { before };
    match git.push_event_commits(bare, old, after, cap).await {
        Ok(commits) => {
            if let Err(e) = db
                .record_authored_commits(repository_id, &observed(commits))
                .await
            {
                tracing::warn!(error = %e, repository_id, "authored-commit record failed");
            }
        }
        Err(e) => {
            tracing::warn!(error = %e, repository_id, "authored-commit walk failed");
        }
    }
}

/// Record authored commits a PR merge brought onto the default branch —
/// GitHub credits each merged head commit to its author once it lands on
/// default. `before` is the pre-merge base tip when the caller resolved one
/// (rebase merges rewrite several commits, so `merge_sha`'s first parent is
/// the previous rewritten commit — not the old base tip — and a parent walk
/// would credit only the last commit); `None` falls back to `merge_sha`'s
/// first parent, which is correct for merge-commit and squash results.
/// No-op for non-default bases.
pub async fn record_authored_merge(
    db: &Database,
    git: Arc<dyn GitBackend>,
    bare: PathBuf,
    repository_id: &str,
    base_ref: &str,
    merge_sha: &str,
    before: Option<&str>,
) {
    let repo = match db.find_repository_by_id(repository_id).await {
        Ok(Some(r)) => r,
        _ => return,
    };
    let base = if base_ref.starts_with("refs/") {
        base_ref.to_string()
    } else {
        format!("refs/heads/{base_ref}")
    };
    if base != default_head_ref(&repo.default_branch) {
        return;
    }
    let before = match before.filter(|b| !b.is_empty()) {
        Some(b) => b.to_string(),
        None => match git.cat_commit(&bare, merge_sha).await {
            Ok(raw) => raw.parents.first().cloned().unwrap_or_default(),
            Err(e) => {
                tracing::warn!(error = %e, repository_id, "merge parent lookup failed");
                return;
            }
        },
    };
    if before.is_empty() {
        return;
    }
    record_authored_range(
        db,
        git.as_ref(),
        &bare,
        repository_id,
        &before,
        merge_sha,
        PUSH_WALK_CAP,
    )
    .await;
    spawn_contribution_backfill(db.clone(), git, bare, repository_id.to_string(), base);
}

/// Claim the `repo:<id>` backfill scope and spawn a full-history walk of the
/// repo's default branch into `authored_commits`. No-op when another caller
/// already claimed or finished it; a failed walk releases the claim so a
/// later push/profile-view retries.
///
/// `PathBuf` + owned handles — the task outlives the request that spawned it.
pub fn spawn_contribution_backfill(
    db: Database,
    git: Arc<dyn GitBackend>,
    bare: PathBuf,
    repository_id: String,
    default_ref_name: String,
) {
    tokio::spawn(async move {
        let scope = format!("repo:{repository_id}");
        match db.claim_contribution_backfill(&scope).await {
            Ok(true) => {}
            _ => return,
        }
        let res = async {
            let tip = git
                .verify_commit_oid(&bare, &default_ref_name)
                .await
                .map_err(|e| format!("tip lookup: {e}"))?;
            let Some(tip) = tip else {
                return Ok(0u64);
            };
            let commits = git
                .push_event_commits(&bare, "", &tip, BACKFILL_WALK_CAP)
                .await
                .map_err(|e| format!("history walk: {e}"))?;
            db.record_authored_commits(&repository_id, &observed(commits))
                .await
        }
        .await;
        if let Err(e) = res {
            tracing::warn!(error = %e, repository_id = %repository_id, "contribution backfill failed");
            let _ = db.release_contribution_backfill(&scope).await;
        }
    });
}

/// Record a UI/API branch create (no receive-pack).
pub async fn record_branch_creation(
    db: &Database,
    repository_id: &str,
    actor_id: &str,
    branch: &str,
    after_oid: &str,
) {
    let ref_name = if branch.starts_with("refs/") {
        branch.to_string()
    } else {
        format!("refs/heads/{branch}")
    };
    let id = Uuid::new_v4().to_string();
    if let Err(e) = db
        .insert_repo_activity(
            &id,
            repository_id,
            actor_id,
            "branch_creation",
            Some(&ref_name),
            Some(ZERO_OID),
            Some(if after_oid.is_empty() {
                ZERO_OID
            } else {
                after_oid
            }),
            Some(0),
            None,
            None,
        )
        .await
    {
        tracing::warn!(error = %e, "branch_creation activity insert failed");
    }
}

/// Record a UI/API branch delete (no receive-pack).
pub async fn record_branch_deletion(
    db: &Database,
    repository_id: &str,
    actor_id: &str,
    branch: &str,
    before_oid: &str,
) {
    let ref_name = if branch.starts_with("refs/") {
        branch.to_string()
    } else {
        format!("refs/heads/{branch}")
    };
    let id = Uuid::new_v4().to_string();
    if let Err(e) = db
        .insert_repo_activity(
            &id,
            repository_id,
            actor_id,
            "branch_deletion",
            Some(&ref_name),
            Some(if before_oid.is_empty() {
                ZERO_OID
            } else {
                before_oid
            }),
            Some(ZERO_OID),
            Some(0),
            None,
            None,
        )
        .await
    {
        tracing::warn!(error = %e, "branch_deletion activity insert failed");
    }
}

/// Record a UI/API branch rename (no receive-pack).
pub async fn record_branch_rename(
    db: &Database,
    repository_id: &str,
    actor_id: &str,
    from: &str,
    to: &str,
    tip_oid: &str,
) {
    let ref_name = if to.starts_with("refs/") {
        to.to_string()
    } else {
        format!("refs/heads/{to}")
    };
    let from_short = short_ref(from);
    let after = if tip_oid.is_empty() {
        ZERO_OID
    } else {
        tip_oid
    };
    let id = Uuid::new_v4().to_string();
    if let Err(e) = db
        .insert_repo_activity(
            &id,
            repository_id,
            actor_id,
            "branch_rename",
            Some(&ref_name),
            Some(after),
            Some(after),
            Some(0),
            Some(from_short.as_str()),
            None,
        )
        .await
    {
        tracing::warn!(error = %e, "branch_rename activity insert failed");
    }
}

/// Record a successful pull-request merge.
pub async fn record_pr_merge(
    db: &Database,
    repository_id: &str,
    actor_id: &str,
    base_ref: &str,
    merge_sha: &str,
    pr_number: i64,
    commit_message: Option<&str>,
) {
    let ref_name = if base_ref.starts_with("refs/") {
        base_ref.to_string()
    } else {
        format!("refs/heads/{base_ref}")
    };
    let id = Uuid::new_v4().to_string();
    if let Err(e) = db
        .insert_repo_activity(
            &id,
            repository_id,
            actor_id,
            "pr_merge",
            Some(&ref_name),
            Some(ZERO_OID),
            Some(merge_sha),
            Some(1),
            commit_message,
            Some(pr_number),
        )
        .await
    {
        tracing::warn!(error = %e, "pr_merge activity insert failed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_create_delete_push_force() {
        assert_eq!(
            classify_push_type(ZERO_OID, "abc", false),
            "branch_creation"
        );
        assert_eq!(
            classify_push_type("abc", ZERO_OID, false),
            "branch_deletion"
        );
        assert_eq!(classify_push_type("aaa", "bbb", false), "push");
        assert_eq!(classify_push_type("aaa", "bbb", true), "force_push");
    }

    #[test]
    fn short_ref_strips_heads_and_tags() {
        assert_eq!(short_ref("refs/heads/main"), "main");
        assert_eq!(short_ref("refs/tags/v1"), "v1");
        assert_eq!(short_ref("main"), "main");
    }
}
