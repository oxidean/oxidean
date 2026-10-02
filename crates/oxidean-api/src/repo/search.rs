//! `repo.search` — in-repo code/commits/issues/pulls search (GIT-18 / D-SRCH-*).

use oxidean_core::{
    AppError, RepoSearchHit, RepoSearchRequest, RepoSearchResponse, RepoSearchType,
};
use oxidean_db::{IssueListFilters, PullSearchFilters};

use crate::git::bare_repo_path;
use crate::rpc::RpcCtx;

use super::search_query::parse_search_query;
use super::units::{require_unit_enabled, RepoUnit};
use super::{map_git_err, resolve_repo_for_read, AccessibleRepo};

/// `repo.search` — permission-aware in-repo search.
pub async fn search(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<RepoSearchResponse, AppError> {
    let req: RepoSearchRequest = serde_json::from_value(input)
        .map_err(|e| AppError::new("rpc.bad_input", format!("invalid repo.search input: {e}")))?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    let limit = req.limit.clamp(1, 100);
    let offset = req.offset;
    let q = req.q.trim().to_string();
    let parsed = parse_search_query(&q);

    let (hits, truncated) = match req.search_type {
        RepoSearchType::Code => search_code(ctx, &accessible, &req, &parsed, offset, limit).await?,
        RepoSearchType::Commits => {
            search_commits(ctx, &accessible, &req, &parsed, offset, limit).await?
        }
        // Unit toggles (COL-13): issues/pulls hit lists are unit surfaces.
        RepoSearchType::Issues => {
            require_unit_enabled(ctx, RepoUnit::Issues, &accessible.row.id).await?;
            search_issues(ctx, &accessible, &parsed, offset, limit).await?
        }
        RepoSearchType::Pulls => {
            require_unit_enabled(ctx, RepoUnit::Pulls, &accessible.row.id).await?;
            search_pulls(ctx, &accessible, &parsed, offset, limit).await?
        }
    };

    Ok(RepoSearchResponse {
        search_type: req.search_type,
        q,
        hits,
        truncated,
        offset,
        limit,
    })
}

async fn search_code(
    ctx: &RpcCtx,
    accessible: &AccessibleRepo,
    req: &RepoSearchRequest,
    parsed: &super::search_query::ParsedSearchQuery,
    offset: u32,
    limit: u32,
) -> Result<(Vec<RepoSearchHit>, bool), AppError> {
    let pattern = parsed.keywords.trim();
    if pattern.is_empty() {
        return Ok((Vec::new(), false));
    }
    let path = bare_repo_path(
        &ctx.repos_dir,
        &accessible.owner_username,
        &accessible.row.name,
    )?;
    let ref_name = match req.ref_name.as_deref() {
        Some(r) if !r.trim().is_empty() => r.trim().to_string(),
        _ => accessible.row.default_branch.clone(),
    };
    let soft_cap = ctx.search_max_matches.max(1).min(10_000);
    let max_files = ctx.search_max_files.max(1).min(10_000);
    let fetch = offset
        .saturating_add(limit)
        .saturating_add(1)
        .min(soft_cap.saturating_add(1));
    let pathspec = parsed.path.as_deref();
    let timeout = std::time::Duration::from_millis(ctx.search_timeout_ms.max(1));
    let grep_fut = ctx.git.grep(&path, &ref_name, pattern, pathspec, fetch);
    let result = match tokio::time::timeout(timeout, grep_fut).await {
        Ok(Ok(r)) => r,
        Ok(Err(e)) => return Err(map_git_err(e)),
        Err(_) => {
            return Err(AppError::new(
                "search.timeout",
                "code search timed out; narrow the query or raise OXIDEAN_SEARCH_TIMEOUT_MS",
            ));
        }
    };

    // Soft-cap distinct files (D-SRCH-08).
    let mut seen_files = std::collections::HashSet::new();
    let mut filtered = Vec::new();
    let mut file_truncated = false;
    for h in result.hits {
        if !seen_files.contains(&h.path) {
            if seen_files.len() as u32 >= max_files {
                file_truncated = true;
                continue;
            }
            seen_files.insert(h.path.clone());
        }
        filtered.push(h);
    }

    let truncated = result.truncated
        || file_truncated
        || filtered.len() as u32 > offset.saturating_add(limit);
    let page: Vec<RepoSearchHit> = filtered
        .into_iter()
        .skip(offset as usize)
        .take(limit as usize)
        .map(|h| RepoSearchHit::Code {
            path: h.path,
            line: h.line,
            content: h.content,
        })
        .collect();
    Ok((page, truncated))
}

async fn search_commits(
    ctx: &RpcCtx,
    accessible: &AccessibleRepo,
    req: &RepoSearchRequest,
    parsed: &super::search_query::ParsedSearchQuery,
    offset: u32,
    limit: u32,
) -> Result<(Vec<RepoSearchHit>, bool), AppError> {
    let grep = parsed.keywords.trim();
    let author = parsed.author.as_deref();
    if grep.is_empty() && author.is_none() {
        return Ok((Vec::new(), false));
    }
    let path = bare_repo_path(
        &ctx.repos_dir,
        &accessible.owner_username,
        &accessible.row.name,
    )?;
    let ref_name = match req.ref_name.as_deref() {
        Some(r) if !r.trim().is_empty() => r.trim().to_string(),
        _ => accessible.row.default_branch.clone(),
    };
    let soft_cap = ctx.search_max_matches.max(1).min(10_000);
    let fetch = offset
        .saturating_add(limit)
        .saturating_add(1)
        .min(soft_cap.saturating_add(1));
    let timeout = std::time::Duration::from_millis(ctx.search_timeout_ms.max(1));
    let log_fut = ctx.git.log_search(
        &path,
        &ref_name,
        if grep.is_empty() { None } else { Some(grep) },
        author,
        0,
        fetch,
    );
    let rows = match tokio::time::timeout(timeout, log_fut).await {
        Ok(Ok(r)) => r,
        Ok(Err(e)) => return Err(map_git_err(e)),
        Err(_) => {
            return Err(AppError::new(
                "search.timeout",
                "commit search timed out; narrow the query or raise OXIDEAN_SEARCH_TIMEOUT_MS",
            ));
        }
    };
    let truncated = rows.len() as u32 > offset.saturating_add(limit);
    let page: Vec<RepoSearchHit> = rows
        .into_iter()
        .skip(offset as usize)
        .take(limit as usize)
        .map(|c| RepoSearchHit::Commit {
            sha: c.sha,
            short_sha: c.short_sha,
            subject: c.subject,
            author_name: c.author_name,
            authored_at: c.authored_at,
        })
        .collect();
    Ok((page, truncated))
}

async fn resolve_author_id(ctx: &RpcCtx, login: Option<&str>) -> Result<Option<String>, AppError> {
    let Some(username) = login.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    match ctx.db.find_user_by_username(username).await {
        Ok(Some(u)) => Ok(Some(u.id)),
        Ok(None) => Ok(None),
        Err(e) => {
            tracing::error!("search author resolve: {e}");
            Err(AppError::new("repo.internal", "repository operation failed"))
        }
    }
}

fn db_err(e: String) -> AppError {
    tracing::error!("search db error: {e}");
    AppError::new("repo.internal", "repository operation failed")
}

async fn search_issues(
    ctx: &RpcCtx,
    accessible: &AccessibleRepo,
    parsed: &super::search_query::ParsedSearchQuery,
    offset: u32,
    limit: u32,
) -> Result<(Vec<RepoSearchHit>, bool), AppError> {
    let author_login = parsed.author.clone();
    let author_id = resolve_author_id(ctx, author_login.as_deref()).await?;
    if author_login
        .as_deref()
        .map(str::trim)
        .is_some_and(|s| !s.is_empty())
        && author_id.is_none()
    {
        return Ok((Vec::new(), false));
    }
    let state = parsed
        .is_state
        .as_deref()
        .unwrap_or("all");
    let q = parsed.keywords.trim();
    let q_opt = if q.is_empty() { None } else { Some(q) };
    // Fetch one extra to detect truncation.
    let fetch_limit = limit.saturating_add(1).min(100);
    let filters = IssueListFilters {
        state,
        author_id: author_id.as_deref(),
        label_id: None,
        assignee_id: None,
        q: q_opt,
        offset: offset as i64,
        limit: fetch_limit as i64,
    };
    let (rows, _total) = ctx
        .db
        .list_issues_for_repo(&accessible.row.id, filters)
        .await
        .map_err(db_err)?;
    let truncated = rows.len() as u32 > limit;
    let page: Vec<RepoSearchHit> = rows
        .into_iter()
        .take(limit as usize)
        .map(|r| RepoSearchHit::Issue {
            number: r.number,
            title: r.title,
            state: r.state,
        })
        .collect();
    Ok((page, truncated))
}

async fn search_pulls(
    ctx: &RpcCtx,
    accessible: &AccessibleRepo,
    parsed: &super::search_query::ParsedSearchQuery,
    offset: u32,
    limit: u32,
) -> Result<(Vec<RepoSearchHit>, bool), AppError> {
    let author_login = parsed.author.clone();
    let author_id = resolve_author_id(ctx, author_login.as_deref()).await?;
    if author_login
        .as_deref()
        .map(str::trim)
        .is_some_and(|s| !s.is_empty())
        && author_id.is_none()
    {
        return Ok((Vec::new(), false));
    }
    let state = parsed.is_state.as_deref().unwrap_or("all");
    let q = parsed.keywords.trim();
    let q_opt = if q.is_empty() { None } else { Some(q) };
    let fetch_limit = limit.saturating_add(1).min(100);
    let filters = PullSearchFilters {
        state,
        author_id: author_id.as_deref(),
        q: q_opt,
        offset,
        limit: fetch_limit,
    };
    let (rows, _total) = ctx
        .db
        .search_pulls_for_repo(&accessible.row.id, filters)
        .await
        .map_err(db_err)?;
    let truncated = rows.len() as u32 > limit;
    let page: Vec<RepoSearchHit> = rows
        .into_iter()
        .take(limit as usize)
        .map(|r| RepoSearchHit::Pull {
            number: r.number,
            title: r.title,
            state: r.state,
        })
        .collect();
    Ok((page, truncated))
}
