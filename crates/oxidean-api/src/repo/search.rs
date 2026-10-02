//! `repo.search` — in-repo code/commits/issues/pulls search (GIT-18 / D-SRCH-*).

use oxidean_core::languages::{language_for_path, languages_named, LanguageSpec};
use oxidean_core::{
    AppError, RepoSearchHit, RepoSearchRequest, RepoSearchResponse, RepoSearchType,
};
use oxidean_db::{IssueListFilters, PullSearchFilters};

use crate::git::bare_repo_path;
use crate::rpc::RpcCtx;

use super::search_query::parse_search_query;
use super::{language_stats, map_git_err, resolve_repo_for_read, AccessibleRepo};

/// `repo.search` — permission-aware in-repo search.
pub async fn search(ctx: &RpcCtx, input: serde_json::Value) -> Result<RepoSearchResponse, AppError> {
    let req: RepoSearchRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new("rpc.bad_input", format!("invalid repo.search input: {e}"))
    })?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    let limit = req.limit.clamp(1, 100);
    let offset = req.offset;
    let q = req.q.trim().to_string();
    let parsed = parse_search_query(&q);

    let (hits, truncated) = match req.search_type {
        RepoSearchType::Code => {
            search_code(ctx, &accessible, &req, &parsed, offset, limit).await?
        }
        RepoSearchType::Commits => {
            search_commits(ctx, &accessible, &req, &parsed, offset, limit).await?
        }
        RepoSearchType::Issues => search_issues(ctx, &accessible, &parsed, offset, limit).await?,
        RepoSearchType::Pulls => search_pulls(ctx, &accessible, &parsed, offset, limit).await?,
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
    // `language:` / `lang:` resolves against the shared taxonomy (issue #59).
    // An explicit qualifier that names nothing matches nothing.
    let langs: Vec<&'static LanguageSpec> = parsed
        .language
        .as_deref()
        .map(languages_named)
        .unwrap_or_default();
    if parsed.language.as_deref().is_some_and(|v| !v.trim().is_empty()) && langs.is_empty() {
        return Ok((Vec::new(), false));
    }

    let pattern = parsed.keywords.trim();
    if pattern.is_empty() {
        // Browse mode: `language:`/`path:` without keywords lists matching files.
        return browse_code_paths(ctx, accessible, req, parsed, &langs, offset, limit).await;
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
    let pathspecs = code_search_pathspecs(parsed.path.as_deref(), &langs);
    let timeout = std::time::Duration::from_millis(ctx.search_timeout_ms.max(1));
    let grep_fut = ctx.git.grep(&path, &ref_name, pattern, &pathspecs, fetch);
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

/// Build `git grep` pathspecs intersecting `path:` prefix with `language:`
/// matchers from the shared table. `:(icase)` keeps search consistent with
/// `language_for_path` (lowercased basename); `:(glob)` `**/` pins filename and
/// extension matches under the prefix at any depth. No qualifiers → no
/// pathspecs (full-tree search).
fn code_search_pathspecs(
    path: Option<&str>,
    langs: &[&'static LanguageSpec],
) -> Vec<String> {
    let prefix = path
        .map(|p| p.trim().trim_matches('/'))
        .filter(|p| !p.is_empty());
    if langs.is_empty() {
        return match (path, prefix) {
            (Some(raw), Some(_)) => vec![raw.trim().to_string()],
            (Some(raw), None) => vec![raw.trim().to_string()],
            (None, _) => Vec::new(),
        };
    }
    let mut specs = Vec::new();
    for lang in langs {
        for ext in lang.extensions {
            match prefix {
                Some(p) => specs.push(format!(":(icase,glob){p}/**/*.{ext}")),
                None => specs.push(format!(":(icase)*.{ext}")),
            }
        }
        for name in lang.filenames {
            match prefix {
                Some(p) => specs.push(format!(":(icase,glob){p}/**/{name}")),
                None => specs.push(format!(":(icase,glob)**/{name}")),
            }
        }
    }
    specs
}

/// `language:` / `path:` without keywords — list matching tracked files instead
/// of grepping contents (issue #59 browse mode). Hits carry `line: 0` and empty
/// `content` so callers render a file list rather than grep rows.
async fn browse_code_paths(
    ctx: &RpcCtx,
    accessible: &AccessibleRepo,
    req: &RepoSearchRequest,
    parsed: &super::search_query::ParsedSearchQuery,
    langs: &[&'static LanguageSpec],
    offset: u32,
    limit: u32,
) -> Result<(Vec<RepoSearchHit>, bool), AppError> {
    let prefix = parsed
        .path
        .as_deref()
        .map(|p| p.trim().trim_matches('/'))
        .unwrap_or("");
    if langs.is_empty() && prefix.is_empty() {
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
    let timeout = std::time::Duration::from_millis(ctx.search_timeout_ms.max(1));
    let list_fut =
        ctx.git
            .ls_tree_sized_blobs(&path, &ref_name, language_stats::MAX_BLOBS);
    let blobs = match tokio::time::timeout(timeout, list_fut).await {
        Ok(Ok(b)) => b,
        Ok(Err(e)) => return Err(map_git_err(e)),
        Err(_) => {
            return Err(AppError::new(
                "search.timeout",
                "code search timed out; narrow the query or raise OXIDEAN_SEARCH_TIMEOUT_MS",
            ));
        }
    };

    let stat_names: std::collections::HashSet<&'static str> =
        langs.iter().map(|l| l.stat_name()).collect();
    let scan_capped = blobs.len() as u32 >= language_stats::MAX_BLOBS;
    let mut hits = Vec::new();
    for b in &blobs {
        let p = b.path.as_str();
        if language_stats::is_vendored_path(p) {
            continue;
        }
        if !prefix.is_empty() && p != prefix && !p.starts_with(&format!("{prefix}/")) {
            continue;
        }
        if !stat_names.is_empty() {
            let matches = language_for_path(p).is_some_and(|s| stat_names.contains(s.stat_name()));
            if !matches {
                continue;
            }
        }
        hits.push(RepoSearchHit::Code {
            path: b.path.clone(),
            line: 0,
            content: String::new(),
        });
    }

    let truncated = scan_capped || hits.len() as u32 > offset.saturating_add(limit);
    let page: Vec<RepoSearchHit> = hits
        .into_iter()
        .skip(offset as usize)
        .take(limit as usize)
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

#[cfg(test)]
mod tests {
    use super::code_search_pathspecs;
    use oxidean_core::languages::languages_named;

    #[test]
    fn pathspecs_for_language_cover_extensions_and_filenames() {
        let ts = languages_named("typescript");
        // TypeScript + TSX rows → extension globs; no filenames.
        let specs = code_search_pathspecs(None, &ts);
        assert!(specs.contains(&":(icase)*.ts".to_string()));
        assert!(specs.contains(&":(icase)*.tsx".to_string()));

        let docker = languages_named("dockerfile");
        let specs = code_search_pathspecs(None, &docker);
        assert!(specs.contains(&":(icase)*.dockerfile".to_string()));
        assert!(specs.contains(&":(icase,glob)**/dockerfile".to_string()));
        assert!(specs.contains(&":(icase,glob)**/containerfile".to_string()));
    }

    #[test]
    fn pathspecs_intersect_path_prefix_with_language() {
        let rust = languages_named("Rust");
        let specs = code_search_pathspecs(Some("src/"), &rust);
        assert_eq!(specs, vec![":(icase,glob)src/**/*.rs".to_string()]);

        // path: alone keeps the legacy raw prefix pathspec.
        let specs = code_search_pathspecs(Some("src"), &[]);
        assert_eq!(specs, vec!["src".to_string()]);

        // No qualifiers → no pathspecs (whole tree).
        assert!(code_search_pathspecs(None, &[]).is_empty());
    }

    #[test]
    fn language_name_resolution_is_case_insensitive_and_grouped() {
        assert_eq!(languages_named("RUBY").len(), 1);
        assert!(languages_named("unknown-lang").is_empty());
        // Group name (TypeScript) pulls in the TSX row too.
        assert!(languages_named("typescript").len() > 1);
    }
}
