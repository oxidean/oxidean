//! Shared branch-protection evaluator (D-02, D-14..22) — used by hooks and `pull.merge`.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use oxidean_core::{AppError, CommitStatusState, ProtectionBlockReasons};
use oxidean_db::{BranchProtectionRuleRow, Database};

use crate::repo::Capability;

/// What the actor is attempting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtectionIntent {
    /// Fast-forward or create branch tip.
    Push,
    /// Non-fast-forward update.
    ForcePush,
    /// Delete branch ref.
    Delete,
    /// PR merge into base branch.
    Merge,
}

/// Effective union of matching rules (D-02).
#[derive(Debug, Clone, Default)]
pub struct EffectiveProtection {
    pub matched: bool,
    pub require_reviews: bool,
    pub required_approving_review_count: i32,
    pub dismiss_stale_reviews: bool,
    pub require_conversation_resolution: bool,
    pub require_last_push_approval: bool,
    pub required_status_contexts: BTreeSet<String>,
    pub strict_status_checks: bool,
    pub allow_force_pushes: bool,
    pub allow_deletions: bool,
    pub enforce_admins: bool,
    pub required_linear_history: bool,
    pub lock_branch: bool,
}

/// Inputs for merge evaluation beyond push.
#[derive(Debug, Clone, Default)]
pub struct MergeEvalInput {
    pub approving_review_count: i32,
    pub unresolved_review_threads: i32,
    /// User id of the latest head pusher (for require_last_push_approval).
    pub last_head_pusher_id: Option<String>,
    /// Approving reviewer user ids (latest-per-user Approves).
    pub approving_reviewer_ids: Vec<String>,
    pub head_sha: String,
    pub base_sha: String,
    /// True when head is ancestor of / up-to-date with base tip (or equal).
    pub head_up_to_date: bool,
    /// Merge method selected (`merge` | `squash` | `rebase`).
    pub merge_method: Option<String>,
    pub is_draft: bool,
    /// Status context → state string for head sha.
    pub status_by_context: std::collections::BTreeMap<String, String>,
}

/// Glob-style `*` / `?` branch pattern match (D-02). No `/` special-casing beyond literal.
pub fn pattern_matches(pattern: &str, branch: &str) -> bool {
    let pat = pattern.trim();
    let name = branch.trim();
    if pat.is_empty() {
        return false;
    }
    match_glob(pat.as_bytes(), name.as_bytes())
}

fn match_glob(pat: &[u8], text: &[u8]) -> bool {
    // `*` matches within a single `/`-delimited segment; `?` matches one non-`/` byte.
    match_glob_rec(pat, text)
}

fn match_glob_rec(pat: &[u8], text: &[u8]) -> bool {
    if pat.is_empty() {
        return text.is_empty();
    }
    match pat[0] {
        b'*' => {
            // Consume one or more `*`
            let rest = {
                let mut i = 0;
                while i < pat.len() && pat[i] == b'*' {
                    i += 1;
                }
                &pat[i..]
            };
            // Match zero or more non-slash chars
            let mut i = 0;
            loop {
                if match_glob_rec(rest, &text[i..]) {
                    return true;
                }
                if i >= text.len() || text[i] == b'/' {
                    return false;
                }
                i += 1;
            }
        }
        b'?' => {
            if text.is_empty() || text[0] == b'/' {
                return false;
            }
            match_glob_rec(&pat[1..], &text[1..])
        }
        c => {
            if text.first() == Some(&c) {
                match_glob_rec(&pat[1..], &text[1..])
            } else {
                false
            }
        }
    }
}

fn parse_contexts(raw: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(raw).unwrap_or_default()
}

/// Union matching rules for a branch name (D-02).
pub fn union_rules(rules: &[BranchProtectionRuleRow], branch: &str) -> EffectiveProtection {
    let mut eff = EffectiveProtection {
        allow_force_pushes: true,
        allow_deletions: true,
        ..Default::default()
    };
    let mut any = false;
    let mut max_reviews = 0i32;
    for rule in rules {
        if !pattern_matches(&rule.pattern, branch) {
            continue;
        }
        any = true;
        if rule.require_reviews {
            eff.require_reviews = true;
            max_reviews = max_reviews.max(rule.required_approving_review_count.clamp(1, 6));
        }
        if rule.dismiss_stale_reviews {
            eff.dismiss_stale_reviews = true;
        }
        if rule.require_conversation_resolution {
            eff.require_conversation_resolution = true;
        }
        if rule.require_last_push_approval {
            eff.require_last_push_approval = true;
        }
        for ctx in parse_contexts(&rule.required_status_contexts) {
            let t = ctx.trim();
            if !t.is_empty() {
                eff.required_status_contexts.insert(t.to_string());
            }
        }
        if rule.strict_status_checks {
            eff.strict_status_checks = true;
        }
        // Most restrictive: false wins for allow_* (D-02 / D-15).
        if !rule.allow_force_pushes {
            eff.allow_force_pushes = false;
        }
        if !rule.allow_deletions {
            eff.allow_deletions = false;
        }
        if rule.enforce_admins {
            eff.enforce_admins = true;
        }
        if rule.required_linear_history {
            eff.required_linear_history = true;
        }
        if rule.lock_branch {
            eff.lock_branch = true;
        }
    }
    if !any {
        return EffectiveProtection::default();
    }
    eff.matched = true;
    eff.required_approving_review_count = max_reviews;
    // If no matching rule set allow flags explicitly false, defaults are false when matched
    // and any rule matches — allow_force_pushes/deletions default to false.
    // Our row defaults are false; union starts true then AND-false. If no rule touched
    // allow_force_pushes (all true), keep true — but schema default is false so OR of
    // "any false" is correct. If all rules have allow_force_pushes=false, result false.
    // Fresh union starts allow=true then any false clears — correct for "most restrictive".
    // When matched but every rule somehow true for allow, allow stays true (explicit opt-in).
    eff
}

fn actor_bypasses(eff: &EffectiveProtection, capability: Option<Capability>) -> bool {
    !eff.enforce_admins && matches!(capability, Some(Capability::Admin))
}

/// Evaluate push/delete/force intents (ORG-06).
pub fn evaluate_push(
    eff: &EffectiveProtection,
    intent: ProtectionIntent,
    capability: Option<Capability>,
) -> Result<(), AppError> {
    if !eff.matched {
        return Ok(());
    }
    if actor_bypasses(eff, capability) {
        return Ok(());
    }
    let mut reasons = Vec::new();
    if eff.lock_branch {
        reasons.push("locked".to_string());
    }
    match intent {
        ProtectionIntent::Push => {
            if eff.require_reviews {
                reasons.push("reviews".to_string());
            }
        }
        ProtectionIntent::ForcePush => {
            if !eff.allow_force_pushes {
                reasons.push("force_push".to_string());
            }
            if eff.require_reviews {
                reasons.push("reviews".to_string());
            }
        }
        ProtectionIntent::Delete => {
            if !eff.allow_deletions {
                reasons.push("deletions".to_string());
            }
        }
        ProtectionIntent::Merge => {}
    }
    // Dedup while preserving order
    let mut seen = BTreeSet::new();
    reasons.retain(|r| seen.insert(r.clone()));
    if reasons.is_empty() {
        return Ok(());
    }
    Err(AppError::new(
        "repo.branch_protection",
        "Branch protection rules block this update",
    )
    .with_data(serde_json::to_value(ProtectionBlockReasons { reasons, ..Default::default() }).unwrap_or_default()))
}

/// Evaluate merge intent (PR-08 / D-22).
pub fn evaluate_merge(
    eff: &EffectiveProtection,
    capability: Option<Capability>,
    input: &MergeEvalInput,
) -> Result<(), AppError> {
    if !eff.matched {
        return Ok(());
    }
    if actor_bypasses(eff, capability) {
        return Ok(());
    }
    let mut reasons = Vec::new();
    let mut missing_statuses = Vec::new();

    if input.is_draft {
        reasons.push("draft".to_string());
    }
    if eff.lock_branch {
        reasons.push("locked".to_string());
    }
    if eff.require_reviews {
        let need = eff.required_approving_review_count.max(1);
        if input.approving_review_count < need {
            reasons.push("reviews".to_string());
        }
        if eff.require_last_push_approval {
            if let Some(pusher) = input.last_head_pusher_id.as_deref() {
                let other_approved = input
                    .approving_reviewer_ids
                    .iter()
                    .any(|id| id.as_str() != pusher);
                if !other_approved {
                    reasons.push("last_push_approval".to_string());
                }
            }
        }
    }
    if eff.require_conversation_resolution && input.unresolved_review_threads > 0 {
        reasons.push("conversations".to_string());
    }
    if !eff.required_status_contexts.is_empty() {
        for ctx in &eff.required_status_contexts {
            let ok = input
                .status_by_context
                .get(ctx)
                .map(|s| CommitStatusState::is_passing_str(s))
                .unwrap_or(false);
            if !ok {
                missing_statuses.push(ctx.clone());
            }
        }
        if !missing_statuses.is_empty() {
            reasons.push("checks".to_string());
        }
    }
    if eff.strict_status_checks && !input.head_up_to_date {
        reasons.push("up_to_date".to_string());
    }
    if eff.required_linear_history {
        if let Some(m) = input.merge_method.as_deref() {
            if m == "merge" {
                reasons.push("linear_history".to_string());
            }
        }
    }

    let mut seen = BTreeSet::new();
    reasons.retain(|r| seen.insert(r.clone()));
    if reasons.is_empty() {
        return Ok(());
    }
    Err(AppError::new(
        "pull.merge_blocked",
        "Branch protection requirements are not met",
    )
    .with_data(
        serde_json::to_value(ProtectionBlockReasons {
            reasons,
            required_approving_review_count: eff
                .require_reviews
                .then_some(eff.required_approving_review_count),
            approving_review_count: Some(input.approving_review_count),
            missing_status_contexts: missing_statuses,
        })
        .unwrap_or_default(),
    ))
}

/// Load rules and return effective protection for a branch.
pub async fn effective_for_branch(
    db: &Database,
    repo_id: &str,
    branch: &str,
) -> Result<EffectiveProtection, AppError> {
    let rules = db
        .list_branch_protection_rules(repo_id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "list branch_protection_rules");
            AppError::new("repo.internal", "repository operation failed")
        })?;
    Ok(union_rules(&rules, branch))
}

/// Strip `refs/heads/` prefix.
pub fn branch_from_ref(git_ref: &str) -> Option<&str> {
    git_ref
        .strip_prefix("refs/heads/")
        .filter(|b| !b.is_empty() && !b.contains('\0'))
}

/// Derive `owner/name` relative path from absolute bare `GIT_DIR` under `repos_dir`.
pub fn owner_name_from_git_dir(repos_dir: &Path, git_dir: &Path) -> Result<(String, String), String> {
    let repos = std::fs::canonicalize(repos_dir).unwrap_or_else(|_| repos_dir.to_path_buf());
    let bare = std::fs::canonicalize(git_dir).unwrap_or_else(|_| git_dir.to_path_buf());
    let rel = bare
        .strip_prefix(&repos)
        .map_err(|_| "GIT_DIR outside OXIDEAN_REPOS_DIR".to_string())?;
    let mut comps: Vec<_> = rel.components().collect();
    if comps.is_empty() {
        return Err("empty relative GIT_DIR".into());
    }
    // Expect owner/name.git
    let name_os = comps.pop().ok_or("missing repo component")?;
    let owner_os = comps.pop().ok_or("missing owner component")?;
    if !comps.is_empty() {
        return Err("unexpected GIT_DIR depth".into());
    }
    let owner = owner_os
        .as_os_str()
        .to_str()
        .ok_or("non-utf8 owner")?
        .to_string();
    let name_raw = name_os.as_os_str().to_str().ok_or("non-utf8 name")?;
    let name = name_raw
        .strip_suffix(".git")
        .unwrap_or(name_raw)
        .to_string();
    if owner.contains("..") || name.contains("..") || owner.contains('/') || name.contains('/') {
        return Err("path escape in owner/name".into());
    }
    Ok((owner, name))
}

/// Install/reconcile protection hooks into a bare repo (D-19).
pub async fn install_hooks(bare: &Path) -> Result<(), String> {
    oxidean_git::install_protection_hooks(bare)
        .await
        .map_err(|e| e.to_string())
}

/// True when the update hook is present.
pub async fn hooks_installed(bare: &Path) -> bool {
    let update = bare.join("hooks").join("update");
    tokio::fs::metadata(&update).await.is_ok()
}

/// Reconcile hooks if missing.
pub async fn reconcile_hooks(bare: &Path) -> Result<(), String> {
    oxidean_git::reconcile_protection_hooks(bare)
        .await
        .map_err(|e| e.to_string())
}

/// Aggregate counts from a boot-time repos_dir hook sweep (D-PKG-04).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SweepHooksStats {
    pub scanned: u32,
    pub installed: u32,
    pub skipped: u32,
    pub errors: u32,
}

/// Progress log interval while walking `OXIDEAN_REPOS_DIR` (serial walk, A4).
pub const SWEEP_PROGRESS_EVERY: u32 = 25;

/// True when `path` looks like a bare git repo (has `HEAD` and `objects/`).
pub async fn looks_like_bare_repo(path: &Path) -> bool {
    let head = path.join("HEAD");
    let objects = path.join("objects");
    match (
        tokio::fs::metadata(&head).await,
        tokio::fs::metadata(&objects).await,
    ) {
        (Ok(h), Ok(o)) => h.is_file() && o.is_dir(),
        _ => false,
    }
}

/// Boot-time walk of `repos_dir` installing/overwriting protection hooks (D-PKG-04).
///
/// Uses [`install_hooks`] (overwrite), never reconcile-only, so packaged script
/// upgrades land on pre-existing forks. Per-repo errors are counted; the sweep
/// continues so API listen is not blocked by one bad path.
pub async fn sweep_protection_hooks(repos_dir: &Path) -> SweepHooksStats {
    let mut stats = SweepHooksStats::default();
    if !repos_dir.exists() {
        tracing::info!(
            path = %repos_dir.display(),
            "protection hook sweep: repos_dir missing; skipping"
        );
        return stats;
    }

    let root = match tokio::fs::canonicalize(repos_dir).await {
        Ok(p) => p,
        Err(e) => {
            tracing::error!(
                error = %e,
                path = %repos_dir.display(),
                "protection hook sweep: canonicalize repos_dir failed"
            );
            stats.errors = stats.errors.saturating_add(1);
            return stats;
        }
    };

    let mut owners = match tokio::fs::read_dir(&root).await {
        Ok(d) => d,
        Err(e) => {
            tracing::error!(
                error = %e,
                path = %root.display(),
                "protection hook sweep: read_dir repos_dir failed"
            );
            stats.errors = stats.errors.saturating_add(1);
            return stats;
        }
    };

    loop {
        let owner_ent = match owners.next_entry().await {
            Ok(Some(e)) => e,
            Ok(None) => break,
            Err(e) => {
                tracing::error!(error = %e, "protection hook sweep: owner entry read failed");
                stats.errors = stats.errors.saturating_add(1);
                break;
            }
        };
        let owner_ft = match owner_ent.file_type().await {
            Ok(ft) => ft,
            Err(e) => {
                tracing::warn!(error = %e, "protection hook sweep: owner file_type failed");
                stats.errors = stats.errors.saturating_add(1);
                continue;
            }
        };
        if !owner_ft.is_dir() {
            stats.skipped = stats.skipped.saturating_add(1);
            continue;
        }
        let owner_name = owner_ent.file_name();
        let owner_str = owner_name.to_string_lossy();
        if owner_str == "." || owner_str == ".." || owner_str.contains('/') {
            stats.skipped = stats.skipped.saturating_add(1);
            continue;
        }

        let mut repos = match tokio::fs::read_dir(owner_ent.path()).await {
            Ok(d) => d,
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    owner = %owner_str,
                    "protection hook sweep: read_dir owner failed"
                );
                stats.errors = stats.errors.saturating_add(1);
                continue;
            }
        };

        loop {
            let repo_ent = match repos.next_entry().await {
                Ok(Some(e)) => e,
                Ok(None) => break,
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        owner = %owner_str,
                        "protection hook sweep: repo entry read failed"
                    );
                    stats.errors = stats.errors.saturating_add(1);
                    break;
                }
            };
            let name_os = repo_ent.file_name();
            let name = name_os.to_string_lossy();
            if !name.ends_with(".git") {
                stats.skipped = stats.skipped.saturating_add(1);
                continue;
            }
            let path = repo_ent.path();
            let ft = match repo_ent.file_type().await {
                Ok(ft) => ft,
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        path = %path.display(),
                        "protection hook sweep: repo file_type failed"
                    );
                    stats.errors = stats.errors.saturating_add(1);
                    continue;
                }
            };
            if !ft.is_dir() {
                stats.skipped = stats.skipped.saturating_add(1);
                continue;
            }
            if !looks_like_bare_repo(&path).await {
                stats.skipped = stats.skipped.saturating_add(1);
                continue;
            }

            stats.scanned = stats.scanned.saturating_add(1);
            match install_hooks(&path).await {
                Ok(()) => {
                    stats.installed = stats.installed.saturating_add(1);
                }
                Err(e) => {
                    tracing::error!(
                        error = %e,
                        path = %path.display(),
                        "protection hook sweep: install_hooks failed"
                    );
                    stats.errors = stats.errors.saturating_add(1);
                }
            }

            if stats.scanned > 0 && stats.scanned % SWEEP_PROGRESS_EVERY == 0 {
                tracing::info!(
                    scanned = stats.scanned,
                    installed = stats.installed,
                    errors = stats.errors,
                    skipped = stats.skipped,
                    "protection hook sweep progress"
                );
            }
        }
    }

    tracing::info!(
        scanned = stats.scanned,
        installed = stats.installed,
        errors = stats.errors,
        skipped = stats.skipped,
        path = %root.display(),
        "protection hook sweep complete (D-PKG-04)"
    );
    stats
}

/// Resolve helper binary path for current process (oxidean-protection-hook).
pub fn default_helper_path() -> Option<PathBuf> {
    std::env::current_exe().ok().and_then(|p| {
        let sibling = p.parent()?.join("oxidean-protection-hook");
        if sibling.is_file() {
            Some(sibling)
        } else {
            None
        }
    })
}

/// Prefer non-empty `OXIDEAN_PROTECTION_HELPER`; otherwise sibling `default_helper_path` (D-PKG-01).
pub fn resolve_protection_helper_with(
    env_value: Option<String>,
    default_path: Option<PathBuf>,
) -> Option<String> {
    env_value
        .filter(|s| !s.is_empty())
        .or_else(|| default_path.map(|p| p.display().to_string()))
}

/// Resolve helper from process env or [`default_helper_path`].
pub fn resolve_protection_helper() -> Option<String> {
    resolve_protection_helper_with(
        std::env::var("OXIDEAN_PROTECTION_HELPER").ok(),
        default_helper_path(),
    )
}

/// Parse capability from env (`admin` | `write` | `read`).
pub fn capability_from_env(raw: &str) -> Capability {
    match raw.trim().to_ascii_lowercase().as_str() {
        "admin" => Capability::Admin,
        "write" => Capability::Write,
        _ => Capability::Read,
    }
}

/// Zero SHA used by git for create/delete.
pub const ZERO_SHA: &str = "0000000000000000000000000000000000000000";

/// Hook/update check: load rules for repo resolved from GIT_DIR and evaluate intent.
pub async fn check_ref_update(
    db: &Database,
    repos_dir: &Path,
    git_dir: &Path,
    git_ref: &str,
    old_sha: &str,
    new_sha: &str,
    capability: Capability,
) -> Result<(), AppError> {
    // API-06: refs/pull/* is a synthesized read-only namespace written by the
    // forge itself (git update-ref, hook-free). No push — regardless of
    // capability or protection rules — may create/update/delete it.
    if git_ref.starts_with("refs/pull/") {
        return Err(AppError::new(
            "repo.pull_refs_read_only",
            "pushes to refs/pull/* are denied: pull refs are synthesized read-only",
        ));
    }
    let Some(branch) = branch_from_ref(git_ref) else {
        // Non-branch refs are not subject to classic branch protection.
        return Ok(());
    };
    let (owner, name) = owner_name_from_git_dir(repos_dir, git_dir)
        .map_err(|e| AppError::new("repo.branch_protection", e))?;
    let owner_id = if let Some(u) = db
        .find_user_by_username(&owner)
        .await
        .map_err(|e| AppError::new("repo.internal", e))?
    {
        u.id
    } else if let Some(o) = db
        .find_organization_by_slug(&owner)
        .await
        .map_err(|e| AppError::new("repo.internal", e))?
    {
        o.id
    } else {
        return Err(AppError::new("repo.not_found", "Repository not found"));
    };
    let repo = db
        .find_repository_by_owner_name(&owner_id, &name)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "find repo for protection hook");
            AppError::new("repo.internal", "repository operation failed")
        })?
        .ok_or_else(|| AppError::new("repo.not_found", "Repository not found"))?;
    let eff = effective_for_branch(db, &repo.id, branch).await?;
    let intent = if new_sha == ZERO_SHA {
        ProtectionIntent::Delete
    } else if old_sha == ZERO_SHA {
        ProtectionIntent::Push
    } else {
        // Detect force-push: not a fast-forward.
        let is_ff = git_is_fast_forward(git_dir, old_sha, new_sha)
            .await
            .unwrap_or(true);
        if is_ff {
            ProtectionIntent::Push
        } else {
            ProtectionIntent::ForcePush
        }
    };
    evaluate_push(&eff, intent, Some(capability))
}

async fn git_is_fast_forward(git_dir: &Path, old_sha: &str, new_sha: &str) -> Result<bool, String> {
    let dir = git_dir.to_str().ok_or("non-utf8 GIT_DIR")?;
    let out = tokio::process::Command::new("git")
        .args(["--git-dir", dir, "merge-base", "--is-ancestor", old_sha, new_sha])
        .output()
        .await
        .map_err(|e| format!("git ff check: {e}"))?;
    Ok(out.status.success())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pattern_exact_and_wildcards() {
        assert!(pattern_matches("main", "main"));
        assert!(!pattern_matches("main", "maint"));
        assert!(pattern_matches("release/*", "release/1.0"));
        assert!(!pattern_matches("release/*", "release/1.0/extra"));
        assert!(pattern_matches("release/*", "release/x"));
        assert!(pattern_matches("v?", "v1"));
        assert!(!pattern_matches("v?", "v10"));
        assert!(pattern_matches("feat-*", "feat-x") || pattern_matches("*", "anything"));
        assert!(pattern_matches("*", "anything"));
        assert!(!pattern_matches("*", "a/b"));
    }

    #[test]
    fn protection_helper_env_wins_over_default_helper_path() {
        let env = Some("/explicit/oxidean-protection-hook".into());
        let default = Some(PathBuf::from("/sibling/oxidean-protection-hook"));
        let got = resolve_protection_helper_with(env, default);
        assert_eq!(
            got.as_deref(),
            Some("/explicit/oxidean-protection-hook"),
            "non-empty OXIDEAN_PROTECTION_HELPER must win (D-PKG-01)"
        );
    }

    #[test]
    fn protection_helper_falls_back_to_default_helper_path_when_env_unset() {
        let default = Some(PathBuf::from("/usr/local/bin/oxidean-protection-hook"));
        let got = resolve_protection_helper_with(None, default.clone());
        assert_eq!(
            got.as_deref(),
            Some("/usr/local/bin/oxidean-protection-hook"),
            "unset env must use default_helper_path (D-PKG-01)"
        );
        let empty = resolve_protection_helper_with(Some(String::new()), default);
        assert_eq!(
            empty.as_deref(),
            Some("/usr/local/bin/oxidean-protection-hook"),
            "empty env must fall back like unset"
        );
    }

    #[test]
    fn protection_helper_none_when_env_and_default_missing() {
        assert!(resolve_protection_helper_with(None, None).is_none());
        assert!(resolve_protection_helper_with(Some(String::new()), None).is_none());
    }

    #[test]
    fn union_takes_max_reviews_and_restrictive_allows() {
        let r1 = BranchProtectionRuleRow {
            id: "1".into(),
            repo_id: "r".into(),
            pattern: "main".into(),
            require_reviews: true,
            required_approving_review_count: 1,
            dismiss_stale_reviews: false,
            require_conversation_resolution: false,
            require_last_push_approval: false,
            required_status_contexts: "[]".into(),
            strict_status_checks: false,
            allow_force_pushes: true,
            allow_deletions: true,
            enforce_admins: false,
            required_linear_history: false,
            lock_branch: false,
            created_at: String::new(),
            updated_at: String::new(),
        };
        let mut r2 = r1.clone();
        r2.id = "2".into();
        r2.required_approving_review_count = 2;
        r2.allow_force_pushes = false;
        let eff = union_rules(&[r1, r2], "main");
        assert!(eff.matched);
        assert_eq!(eff.required_approving_review_count, 2);
        assert!(!eff.allow_force_pushes);
    }
}
