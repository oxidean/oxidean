//! `search.global` — sitewide grouped search (DEBT-03).
//!
//! Groups: repositories / users / organizations / issues / pulls are SQL-backed
//! (`oxidean-db::search`) and always ACL-filtered to repositories the viewer
//! can read. Users require a verified session (mirrors `user.lookup`
//! anti-enumeration); anonymous callers get an empty users group.
//!
//! Commits/code have no cross-repo index — the handler fans out to
//! `git log --grep` / `git grep -n` over the most recently-updated
//! [`SCAN_REPO_CAP`] visible repos. Their `total` only counts hits inside that
//! window and `truncated` reports partial coverage. SRCH-01 replaces this with
//! a real index.

use std::collections::HashSet;
use std::time::Duration;

use oxidean_core::languages::{languages_named, LanguageSpec};
use oxidean_core::{
    AppError, GlobalSearchCodeHit, GlobalSearchCommitHit, GlobalSearchGroup, GlobalSearchIssueHit,
    GlobalSearchKind, GlobalSearchOrgHit, GlobalSearchPullHit, GlobalSearchRepoHit,
    GlobalSearchRequest, GlobalSearchResponse, GlobalSearchUserHit,
};
use oxidean_db::ScanRepoRow;
use oxidean_git::GrepResult;

use crate::auth::gate::require_verified;
use crate::git::bare_repo_path;
use crate::repo::code_search_pathspecs;
use crate::repo::search_query::parse_search_query;
use crate::rpc::RpcCtx;

/// Cap on `q` length (chars) before it is truncated server-side.
const MAX_Q_CHARS: usize = 200;
/// Default per-group page size.
const DEFAULT_LIMIT: i64 = 10;
/// Hard cap on per-group page size.
const MAX_LIMIT: i64 = 50;
/// Repositories a single commits/code scan fans out across (recently updated
/// first). Fetch is `SCAN_REPO_CAP + 1` to detect truncation. SRCH-01 replaces
/// this bounded scan with an index.
const SCAN_REPO_CAP: i64 = 10;

fn db_err(e: String) -> AppError {
    tracing::error!("search.global db error: {e}");
    AppError::new("search.internal", "search failed")
}

/// `search.global` handler.
pub async fn global(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<GlobalSearchResponse, AppError> {
    let req: GlobalSearchRequest = serde_json::from_value(input)
        .map_err(|e| AppError::new("rpc.bad_input", format!("invalid search.global input: {e}")))?;

    let q: String = req.q.trim().chars().take(MAX_Q_CHARS).collect();
    let parsed = parse_search_query(&q);
    let keywords = parsed.keywords.trim().to_string();

    let offset = req.offset.unwrap_or(0).clamp(0, 10_000);
    let limit = req.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);

    // Kinds the caller asked to populate with hits. Absent/empty → all.
    let wanted: HashSet<GlobalSearchKind> = match &req.types {
        Some(ts) if !ts.is_empty() => ts.iter().copied().collect(),
        _ => GlobalSearchKind::ALL.into_iter().collect(),
    };
    let viewer = ctx.session.as_ref().map(|s| s.user_id.as_str());

    // ── Database-backed groups ────────────────────────────────────────────
    // Totals are always computed so callers can badge every tab; hits are
    // only populated for kinds in `types` (or all when `types` is absent).
    let mut repositories = GlobalSearchGroup::<GlobalSearchRepoHit>::default();
    if !keywords.is_empty() {
        let lim = if wanted.contains(&GlobalSearchKind::Repositories) {
            limit
        } else {
            1
        };
        let (rows, total) = ctx
            .db
            .search_global_repositories(viewer, &keywords, offset, lim)
            .await
            .map_err(db_err)?;
        let hits = if wanted.contains(&GlobalSearchKind::Repositories) {
            rows.into_iter()
                .map(|r| GlobalSearchRepoHit {
                    owner: r.owner_slug,
                    owner_type: r.owner_type,
                    name: r.name,
                    description: r.description,
                    visibility: r.visibility,
                    star_count: r.star_count,
                    updated_at: r.updated_at,
                })
                .collect()
        } else {
            Vec::new()
        };
        repositories = GlobalSearchGroup {
            hits,
            total,
            truncated: offset + limit < total,
        };
    }

    let mut users = GlobalSearchGroup::<GlobalSearchUserHit>::default();
    if !keywords.is_empty() && wanted.contains(&GlobalSearchKind::Users) {
        // Mirror `user.lookup`: directory search is verified-session only so
        // anonymous callers cannot enumerate accounts. Silent empty group —
        // other groups must still succeed.
        if require_verified(ctx).await.is_ok() {
            let (rows, total) = ctx
                .db
                .search_global_users(&keywords, offset, limit)
                .await
                .map_err(db_err)?;
            users = GlobalSearchGroup {
                hits: rows
                    .into_iter()
                    .map(|u| GlobalSearchUserHit {
                        username: u.username,
                        display_name: u.display_name,
                        avatar_url: u.avatar_path,
                    })
                    .collect(),
                total,
                truncated: offset + limit < total,
            };
        }
    }

    let mut organizations = GlobalSearchGroup::<GlobalSearchOrgHit>::default();
    if !keywords.is_empty() {
        let lim = if wanted.contains(&GlobalSearchKind::Organizations) {
            limit
        } else {
            1
        };
        let (rows, total) = ctx
            .db
            .search_global_orgs(&keywords, offset, lim)
            .await
            .map_err(db_err)?;
        let hits = if wanted.contains(&GlobalSearchKind::Organizations) {
            rows.into_iter()
                .map(|o| GlobalSearchOrgHit {
                    slug: o.slug,
                    display_name: o.display_name,
                })
                .collect()
        } else {
            Vec::new()
        };
        organizations = GlobalSearchGroup {
            hits,
            total,
            truncated: offset + limit < total,
        };
    }

    // Issues/pulls run for free-text, `is:`, and `author:` queries.
    let mut issues = GlobalSearchGroup::<GlobalSearchIssueHit>::default();
    let mut pulls = GlobalSearchGroup::<GlobalSearchPullHit>::default();
    let author_login = parsed.author.as_deref().map(str::trim);
    let author_id = match author_login {
        Some(login) if !login.is_empty() => ctx
            .db
            .find_user_by_username(login)
            .await
            .map_err(db_err)?
            .map(|u| u.id),
        _ => None,
    };
    // `author:x` for a user that does not exist → no hits, but still cheap.
    let author_ok = !(author_login.is_some_and(|s| !s.is_empty()) && author_id.is_none());
    let activity_q =
        !keywords.is_empty() || parsed.is_state.is_some() || (author_ok && author_id.is_some());
    if activity_q && author_ok {
        let q_opt = if keywords.is_empty() {
            None
        } else {
            Some(keywords.as_str())
        };
        let is_state = parsed.is_state.as_deref().map(|s| s.to_ascii_lowercase());
        let issue_state = match is_state.as_deref() {
            Some("open") => Some("open"),
            Some("closed") => Some("closed"),
            _ => None,
        };
        let pull_state = match is_state.as_deref() {
            Some("open") => Some("open"),
            Some("closed") => Some("closed"),
            Some("merged") => Some("merged"),
            _ => None,
        };

        {
            let lim = if wanted.contains(&GlobalSearchKind::Issues) {
                limit
            } else {
                1
            };
            let (rows, total) = ctx
                .db
                .search_global_issues(
                    viewer,
                    q_opt,
                    issue_state,
                    author_id.as_deref(),
                    offset,
                    lim,
                )
                .await
                .map_err(db_err)?;
            let hits = if wanted.contains(&GlobalSearchKind::Issues) {
                rows.into_iter()
                    .map(|r| GlobalSearchIssueHit {
                        repo_owner: r.repo_owner,
                        repo_name: r.repo_name,
                        number: r.number,
                        title: r.title,
                        state: r.state,
                        author_username: r.author_username,
                        comment_count: r.comment_count,
                        updated_at: r.updated_at,
                    })
                    .collect()
            } else {
                Vec::new()
            };
            issues = GlobalSearchGroup {
                hits,
                total,
                truncated: offset + limit < total,
            };
        }
        {
            let lim = if wanted.contains(&GlobalSearchKind::Pulls) {
                limit
            } else {
                1
            };
            let (rows, total) = ctx
                .db
                .search_global_pulls(viewer, q_opt, pull_state, author_id.as_deref(), offset, lim)
                .await
                .map_err(db_err)?;
            let hits = if wanted.contains(&GlobalSearchKind::Pulls) {
                rows.into_iter()
                    .map(|r| GlobalSearchPullHit {
                        repo_owner: r.repo_owner,
                        repo_name: r.repo_name,
                        number: r.number,
                        title: r.title,
                        state: r.state,
                        draft: r.draft,
                        author_username: r.author_username,
                        comment_count: r.comment_count,
                        updated_at: r.updated_at,
                    })
                    .collect()
            } else {
                Vec::new()
            };
            pulls = GlobalSearchGroup {
                hits,
                total,
                truncated: offset + limit < total,
            };
        }
    }

    // ── Bounded git scans (commits / code) ────────────────────────────────
    // `language:` / `lang:` resolves against the shared taxonomy, same as
    // repo.search — a qualifier naming nothing matches no code.
    let langs: Vec<&'static LanguageSpec> = parsed
        .language
        .as_deref()
        .map(languages_named)
        .unwrap_or_default();
    let lang_miss = parsed
        .language
        .as_deref()
        .is_some_and(|v| !v.trim().is_empty())
        && langs.is_empty();
    let code_pathspecs = code_search_pathspecs(parsed.path.as_deref(), &langs);
    let need_code =
        wanted.contains(&GlobalSearchKind::Code) && !keywords.is_empty() && !lang_miss;
    let need_commits = wanted.contains(&GlobalSearchKind::Commits)
        && (!keywords.is_empty() || author_login.is_some_and(|s| !s.is_empty()));
    let mut commits = GlobalSearchGroup::<GlobalSearchCommitHit>::default();
    let mut code = GlobalSearchGroup::<GlobalSearchCodeHit>::default();
    if need_code || need_commits {
        let mut scan = ctx
            .db
            .list_global_scan_repos(viewer, SCAN_REPO_CAP + 1)
            .await
            .map_err(db_err)?;
        let capped = scan.len() as i64 > SCAN_REPO_CAP;
        scan.truncate(SCAN_REPO_CAP as usize);

        let timeout = Duration::from_millis(ctx.search_timeout_ms.max(1));
        let soft_cap = ctx.search_max_matches.max(1).min(10_000);
        // Per-repo fetch ceiling; the merged list is sliced to offset+limit.
        let fetch = (offset as u32)
            .saturating_add(limit as u32)
            .saturating_add(1)
            .min(soft_cap.saturating_add(1));

        if need_code {
            code = scan_code(ctx, &scan, &keywords, &code_pathspecs, timeout, fetch).await;
            code.truncated = code.truncated || capped;
            code.hits = code
                .hits
                .into_iter()
                .skip(offset as usize)
                .take(limit as usize)
                .collect();
        }
        if need_commits {
            commits = scan_commits(ctx, &scan, &keywords, author_login, timeout, fetch).await;
            commits.truncated = commits.truncated || capped;
            commits.hits = commits
                .hits
                .into_iter()
                .skip(offset as usize)
                .take(limit as usize)
                .collect();
        }
    }

    Ok(GlobalSearchResponse {
        q,
        repositories,
        users,
        organizations,
        issues,
        pulls,
        commits,
        code,
    })
}

/// `git grep` every repo in `scan` concurrently; errors/timeouts per repo
/// degrade to partial results (with `truncated`) rather than failing the call.
async fn scan_code(
    ctx: &RpcCtx,
    scan: &[ScanRepoRow],
    keywords: &str,
    pathspecs: &[String],
    timeout: Duration,
    fetch: u32,
) -> GlobalSearchGroup<GlobalSearchCodeHit> {
    let mut set = tokio::task::JoinSet::new();
    for (idx, r) in scan.iter().enumerate() {
        let Ok(path) = bare_repo_path(&ctx.repos_dir, &r.owner_slug, &r.name) else {
            continue;
        };
        let git = ctx.git.clone();
        let branch = r.default_branch.clone();
        let kw = keywords.to_string();
        let specs = pathspecs.to_vec();
        set.spawn(async move {
            let res = tokio::time::timeout(timeout, git.grep(&path, &branch, &kw, &specs, fetch)).await;
            (idx, res)
        });
    }
    let mut per_repo: Vec<(usize, GrepResult)> = Vec::new();
    let mut degraded = false;
    while let Some(joined) = set.join_next().await {
        match joined {
            Ok((idx, Ok(Ok(res)))) => per_repo.push((idx, res)),
            Ok((_, Ok(Err(e)))) => {
                tracing::debug!("search.global code scan repo error: {e}");
            }
            Ok((_, Err(_elapsed))) => {
                tracing::warn!("search.global code scan repo timed out");
                degraded = true;
            }
            Err(e) => {
                tracing::warn!("search.global code scan join error: {e}");
                degraded = true;
            }
        }
    }
    per_repo.sort_by_key(|(idx, _)| *idx);

    // Soft-cap distinct files across the merged hits (same rule as repo.search).
    let max_files = ctx.search_max_files.max(1).min(10_000) as usize;
    let mut seen_files: HashSet<(usize, String)> = HashSet::new();
    let mut file_truncated = false;
    let mut hits: Vec<GlobalSearchCodeHit> = Vec::new();
    for (idx, res) in &per_repo {
        let repo = &scan[*idx];
        for h in &res.hits {
            let key = (*idx, h.path.clone());
            if !seen_files.contains(&key) {
                if seen_files.len() >= max_files {
                    file_truncated = true;
                    continue;
                }
                seen_files.insert(key);
            }
            hits.push(GlobalSearchCodeHit {
                repo_owner: repo.owner_slug.clone(),
                repo_name: repo.name.clone(),
                ref_name: repo.default_branch.clone(),
                path: h.path.clone(),
                line: h.line,
                content: h.content.clone(),
            });
        }
    }
    let repo_truncated = per_repo.iter().any(|(_, r)| r.truncated);
    let total = hits.len() as i64;
    GlobalSearchGroup {
        hits,
        total,
        truncated: degraded || file_truncated || repo_truncated,
    }
}

/// `git log --grep`/`--author` every repo in `scan` concurrently; results are
/// merged newest-first. Same degrade-partial policy as [`scan_code`].
async fn scan_commits(
    ctx: &RpcCtx,
    scan: &[ScanRepoRow],
    keywords: &str,
    author_login: Option<&str>,
    timeout: Duration,
    fetch: u32,
) -> GlobalSearchGroup<GlobalSearchCommitHit> {
    let mut set = tokio::task::JoinSet::new();
    for (idx, r) in scan.iter().enumerate() {
        let Ok(path) = bare_repo_path(&ctx.repos_dir, &r.owner_slug, &r.name) else {
            continue;
        };
        let git = ctx.git.clone();
        let branch = r.default_branch.clone();
        let grep = if keywords.is_empty() {
            None
        } else {
            Some(keywords.to_string())
        };
        let author = author_login.map(str::to_string);
        set.spawn(async move {
            let res = tokio::time::timeout(
                timeout,
                git.log_search(&path, &branch, grep.as_deref(), author.as_deref(), 0, fetch),
            )
            .await;
            (idx, res)
        });
    }
    let mut merged: Vec<GlobalSearchCommitHit> = Vec::new();
    let mut degraded = false;
    while let Some(joined) = set.join_next().await {
        match joined {
            Ok((idx, Ok(Ok(rows)))) => {
                let repo = &scan[idx];
                for c in rows {
                    merged.push(GlobalSearchCommitHit {
                        repo_owner: repo.owner_slug.clone(),
                        repo_name: repo.name.clone(),
                        sha: c.sha,
                        short_sha: c.short_sha,
                        subject: c.subject,
                        author_name: c.author_name,
                        authored_at: c.authored_at,
                    });
                }
            }
            Ok((_, Ok(Err(e)))) => {
                tracing::debug!("search.global commit scan repo error: {e}");
            }
            Ok((_, Err(_elapsed))) => {
                tracing::warn!("search.global commit scan repo timed out");
                degraded = true;
            }
            Err(e) => {
                tracing::warn!("search.global commit scan join error: {e}");
                degraded = true;
            }
        }
    }
    // ISO-8601 `%aI` timestamps sort lexicographically.
    merged.sort_by(|a, b| b.authored_at.cmp(&a.authored_at));
    let total = merged.len() as i64;
    GlobalSearchGroup {
        hits: merged,
        total,
        truncated: degraded,
    }
}
