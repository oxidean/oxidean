//! Wiki domain — git-backed wiki pages (GitHub/Gitea parity).
//!
//! The wiki is a real git repository: `{repos_dir}/{owner}/{name}.wiki.git`,
//! initialized lazily on the first page write, default branch `master`
//! (GitHub's wiki branch). Pages are `*.md` files at the tree root —
//! `Home.md` is the landing page and `_Sidebar`/`_Footer`/`_Header` are
//! GitHub's reserved layout files. `wiki.pages.*` read and write the files
//! through `GitBackend::commit_files` so history, diffs, and `git clone
//! …/name.wiki.git` stay honest; `wiki.revisions.list` reads `git log`.
//!
//! Events: page create/update emits `gollum` — webhook delivery plus the
//! `on: gollum` Actions trigger — carrying GitHub's `pages[]` payload shape
//! (`page_name`/`title`/`action`/`sha`/`html_url`/`summary`). Deletes do not
//! fire (GitHub's gollum event covers created/edited only). Pushes direct to
//! the wiki bare emit `gollum` through the receive-pack path, never the
//! repo `push` surface.
//!
//! ACL: reads follow repo Read (anonymous on public repos), writes follow
//! repo Write — GitHub FG parity maps wiki edits to the `contents` family —
//! plus the `repo.wiki.*` unit flag (COL-13).

use std::path::{Path, PathBuf};

use oxidean_core::{
    AppError, WikiPageCreateRequest, WikiPageDeleteRequest, WikiPageDeleteResponse,
    WikiPageGetRequest, WikiPagePublic, WikiPageUpdateRequest, WikiPageWriteResponse,
    WikiPagesListRequest, WikiPagesListResponse, WikiRevisionPublic, WikiRevisionsListRequest,
    WikiRevisionsListResponse,
};
use oxidean_git::FileChange;

use oxidean_api_core::auth::gate::require_verified;
use oxidean_api_core::repo::{
    ensure_not_archived, meets, not_found, require_unit_enabled, resolve_repo_for_read,
    AccessibleRepo, Capability, RepoUnit,
};
use oxidean_api_core::rpc::RpcCtx;

/// GitHub's wiki default branch.
const WIKI_BRANCH: &str = "master";

/// GitHub's reserved layout files — real wiki pages that render in chrome,
/// never listed as ordinary pages by GitHub's Pages view.
const RESERVED_PAGES: &[&str] = &["_Sidebar", "_Footer", "_Header"];

/// `{repos_dir}/{owner}/{name}.wiki.git`.
fn wiki_bare(ctx: &RpcCtx, owner: &str, repo_name: &str) -> Result<PathBuf, AppError> {
    oxidean_api_core::git::bare_repo_path(&ctx.repos_dir, owner, &format!("{repo_name}.wiki"))
        .map_err(|_| not_found())
}

/// Resolve the repo + unit gate shared by every `wiki.*` RPC.
async fn resolve_wiki_repo(
    ctx: &RpcCtx,
    owner: &str,
    name: &str,
) -> Result<AccessibleRepo, AppError> {
    let accessible = resolve_repo_for_read(ctx, owner, name).await?;
    require_unit_enabled(ctx, RepoUnit::Wiki, &accessible.row.id).await?;
    Ok(accessible)
}

/// `My Page` → `My-Page`; returns the safe page stem or `wiki.bad_page`.
/// `/`/`..`/control characters are refused — the name becomes a root-level
/// path inside the wiki bare. `_`-prefixed names are allowed (reserved
/// layout pages are editable like any other).
fn validate_page_name(raw: &str) -> Result<String, AppError> {
    let name = raw.trim().replace(' ', "-");
    let ok = !name.is_empty()
        && name.len() <= 255
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        && !name.starts_with('.');
    if ok {
        Ok(name)
    } else {
        Err(AppError::new("wiki.bad_page", "invalid wiki page name"))
    }
}

fn page_file(page: &str) -> String {
    format!("{page}.md")
}

fn html_url(owner: &str, repo_name: &str, page: &str) -> String {
    format!("/{owner}/{repo_name}/wiki/{page}")
}

fn is_reserved(page: &str) -> bool {
    RESERVED_PAGES.contains(&page)
}

/// Whether the wiki bare exists (unborn → the Pages list is empty, every
/// read is `wiki.not_found` — GitHub shows "Create the first page").
fn wiki_exists(bare: &Path) -> bool {
    bare.is_dir()
}

/// `wiki.pages.list` — Read+. Pages are `.md` files at the wiki root;
/// `_Sidebar`/`_Footer`/`_Header` ride along flagged `reserved`.
pub async fn pages_list(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<WikiPagesListResponse, AppError> {
    let req: WikiPagesListRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid wiki.pages.list input: {e}"),
        )
    })?;
    let accessible = resolve_wiki_repo(ctx, &req.owner, &req.name).await?;
    let bare = wiki_bare(ctx, &accessible.owner_username, &accessible.row.name)?;
    if !wiki_exists(&bare) {
        return Ok(WikiPagesListResponse { pages: vec![] });
    }
    let entries = ctx
        .git
        .ls_tree(&bare, WIKI_BRANCH, "")
        .await
        .map_err(|e| git_err("wiki.pages.list", e))?;
    let names: Vec<String> = entries
        .iter()
        .filter(|e| e.name.ends_with(".md") && !e.name.contains('/'))
        .map(|e| e.name.trim_end_matches(".md").to_string())
        .collect();
    let last = ctx
        .git
        .path_last_commits(&bare, WIKI_BRANCH, "", &names)
        .await
        .unwrap_or_default();
    let mut pages: Vec<WikiPagePublic> = names
        .iter()
        .map(|page| {
            let c = last.get(page);
            WikiPagePublic {
                page_name: page.clone(),
                html_url: html_url(&accessible.owner_username, &accessible.row.name, page),
                reserved: is_reserved(page),
                sha: c.map(|s| s.sha.clone()).unwrap_or_default(),
                updated_at: c.map(|s| s.authored_at.clone()),
                content: None,
            }
        })
        .collect();
    pages.sort_by(|a, b| a.page_name.cmp(&b.page_name));
    Ok(WikiPagesListResponse { pages })
}

/// `wiki.pages.get` — Read+. Raw Markdown body + tip commit for the page.
pub async fn page_get(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<WikiPageWriteResponse, AppError> {
    let req: WikiPageGetRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid wiki.pages.get input: {e}"),
        )
    })?;
    let page = validate_page_name(&req.page)?;
    let accessible = resolve_wiki_repo(ctx, &req.owner, &req.name).await?;
    let bare = wiki_bare(ctx, &accessible.owner_username, &accessible.row.name)?;
    if !wiki_exists(&bare) {
        return Err(wiki_not_found());
    }
    let file = page_file(&page);
    let content = match ctx.git.cat_blob(&bare, WIKI_BRANCH, &file).await {
        Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
        Err(oxidean_git::GitError::NotFound(_)) => return Err(wiki_not_found()),
        Err(e) => return Err(git_err("wiki.pages.get", e)),
    };
    let last = ctx
        .git
        .path_last_commits(&bare, WIKI_BRANCH, "", std::slice::from_ref(&page))
        .await
        .unwrap_or_default();
    let c = last.get(&page);
    Ok(WikiPageWriteResponse {
        page: WikiPagePublic {
            page_name: page.clone(),
            html_url: html_url(&accessible.owner_username, &accessible.row.name, &page),
            reserved: is_reserved(&page),
            sha: c.map(|s| s.sha.clone()).unwrap_or_default(),
            updated_at: c.map(|s| s.authored_at.clone()),
            content: Some(content),
        },
    })
}

/// `wiki.pages.create` — Write+. Creates the wiki bare lazily, commits
/// `{title}.md`, and emits `gollum` (`created`).
pub async fn page_create(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<WikiPageWriteResponse, AppError> {
    let req: WikiPageCreateRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid wiki.pages.create input: {e}"),
        )
    })?;
    let page = validate_page_name(&req.title)?;
    let user = require_verified(ctx).await?;
    let accessible = resolve_wiki_repo(ctx, &req.owner, &req.name).await?;
    if !meets(accessible.capability, Capability::Write) {
        return Err(not_found());
    }
    ensure_not_archived(&accessible)?;
    let bare = ensure_wiki_bare(ctx, &accessible).await?;
    // Refuse to overwrite — GitHub's "new page" on an existing name is an
    // edit, and silent clobber loses history.
    if ctx
        .git
        .ls_tree_entry(&bare, WIKI_BRANCH, &page_file(&page))
        .await
        .ok()
        .flatten()
        .is_some()
    {
        return Err(AppError::new(
            "wiki.page_exists",
            "a page with this name already exists",
        ));
    }
    let sha = commit_page(
        ctx,
        &accessible,
        &bare,
        &page,
        FileChange::Upsert {
            path: page_file(&page),
            content: req.content.clone().into_bytes(),
        },
        req.message
            .as_deref()
            .unwrap_or(&format!("Created {page} (markdown)")),
        &user,
    )
    .await?;
    notify_gollum(ctx, &accessible, &page, "created", &sha, &user).await;
    Ok(WikiPageWriteResponse {
        page: WikiPagePublic {
            page_name: page.clone(),
            html_url: html_url(&accessible.owner_username, &accessible.row.name, &page),
            reserved: is_reserved(&page),
            sha,
            updated_at: None,
            content: None,
        },
    })
}

/// `wiki.pages.update` — Write+. The page must exist (GitHub edits a page
/// that is already there); commits `{page}.md`, emits `gollum` (`edited`).
pub async fn page_update(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<WikiPageWriteResponse, AppError> {
    let req: WikiPageUpdateRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid wiki.pages.update input: {e}"),
        )
    })?;
    let page = validate_page_name(&req.page)?;
    let user = require_verified(ctx).await?;
    let accessible = resolve_wiki_repo(ctx, &req.owner, &req.name).await?;
    if !meets(accessible.capability, Capability::Write) {
        return Err(not_found());
    }
    ensure_not_archived(&accessible)?;
    let bare = ensure_wiki_bare(ctx, &accessible).await?;
    if ctx
        .git
        .ls_tree_entry(&bare, WIKI_BRANCH, &page_file(&page))
        .await
        .ok()
        .flatten()
        .is_none()
    {
        return Err(wiki_not_found());
    }
    let sha = commit_page(
        ctx,
        &accessible,
        &bare,
        &page,
        FileChange::Upsert {
            path: page_file(&page),
            content: req.content.clone().into_bytes(),
        },
        req.message
            .as_deref()
            .unwrap_or(&format!("Updated {page} (markdown)")),
        &user,
    )
    .await?;
    notify_gollum(ctx, &accessible, &page, "edited", &sha, &user).await;
    Ok(WikiPageWriteResponse {
        page: WikiPagePublic {
            page_name: page.clone(),
            html_url: html_url(&accessible.owner_username, &accessible.row.name, &page),
            reserved: is_reserved(&page),
            sha,
            updated_at: None,
            content: None,
        },
    })
}

/// `wiki.pages.delete` — Write+. Removes the `.md` file; no `gollum` event
/// (GitHub fires gollum on create/edit only).
pub async fn page_delete(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<WikiPageDeleteResponse, AppError> {
    let req: WikiPageDeleteRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid wiki.pages.delete input: {e}"),
        )
    })?;
    let page = validate_page_name(&req.page)?;
    let user = require_verified(ctx).await?;
    let accessible = resolve_wiki_repo(ctx, &req.owner, &req.name).await?;
    if !meets(accessible.capability, Capability::Write) {
        return Err(not_found());
    }
    ensure_not_archived(&accessible)?;
    let bare = wiki_bare(ctx, &accessible.owner_username, &accessible.row.name)?;
    if !wiki_exists(&bare) {
        return Err(wiki_not_found());
    }
    commit_page(
        ctx,
        &accessible,
        &bare,
        &page,
        FileChange::Delete {
            path: page_file(&page),
        },
        req.message
            .as_deref()
            .unwrap_or(&format!("Deleted {page} (markdown)")),
        &user,
    )
    .await?;
    Ok(WikiPageDeleteResponse { ok: true })
}

/// `wiki.revisions.list` — Read+. `git log` on the page's `.md` path —
/// create, every edit, delete, and direct git pushes all land in history.
pub async fn revisions_list(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<WikiRevisionsListResponse, AppError> {
    let req: WikiRevisionsListRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid wiki.revisions.list input: {e}"),
        )
    })?;
    let page = validate_page_name(&req.page)?;
    let accessible = resolve_wiki_repo(ctx, &req.owner, &req.name).await?;
    let bare = wiki_bare(ctx, &accessible.owner_username, &accessible.row.name)?;
    if !wiki_exists(&bare) {
        return Err(wiki_not_found());
    }
    let limit = req.limit.unwrap_or(50).clamp(1, 200);
    let commits = ctx
        .git
        .log_path(&bare, WIKI_BRANCH, &page_file(&page), limit)
        .await
        .map_err(|e| git_err("wiki.revisions.list", e))?;
    let mut revisions = Vec::with_capacity(commits.len());
    for c in commits {
        let username = ctx
            .db
            .find_user_by_email(&c.author_email)
            .await
            .ok()
            .flatten()
            .map(|u| u.username);
        revisions.push(WikiRevisionPublic {
            sha: c.sha,
            message: c.subject,
            author_name: c.author_name,
            author_username: username,
            timestamp: c.authored_at,
        });
    }
    Ok(WikiRevisionsListResponse { revisions })
}

// --- internals -------------------------------------------------------------

fn wiki_not_found() -> AppError {
    AppError::new("wiki.not_found", "wiki page not found")
}

/// Init the wiki bare on first write (`master` — GitHub's wiki branch).
async fn ensure_wiki_bare(ctx: &RpcCtx, accessible: &AccessibleRepo) -> Result<PathBuf, AppError> {
    let bare = wiki_bare(ctx, &accessible.owner_username, &accessible.row.name)?;
    if !wiki_exists(&bare) {
        ctx.git
            .init_bare(&bare, WIKI_BRANCH)
            .await
            .map_err(|e| git_err("wiki init", e))?;
    }
    Ok(bare)
}

/// One signed `commit_files` on the wiki bare — web-flow key, forge
/// capability label, actor display identity.
async fn commit_page(
    ctx: &RpcCtx,
    accessible: &AccessibleRepo,
    bare: &Path,
    _page: &str,
    change: FileChange,
    message: &str,
    user: &oxidean_db::UserRow,
) -> Result<String, AppError> {
    let signing_key = oxidean_api_core::git::web_flow::ensure_web_flow_key()
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "web-flow signing key unavailable for wiki commit");
            AppError::new(
                "repo.git_signing_unavailable",
                "web-flow signing key is unavailable",
            )
        })?;
    let author_name = if user.display_name.trim().is_empty() {
        user.username.clone()
    } else {
        user.display_name.clone()
    };
    let result = ctx
        .git
        .commit_files(
            bare,
            WIKI_BRANCH,
            WIKI_BRANCH,
            message,
            &[change],
            &author_name,
            &user.email,
            Some(signing_key.as_path()),
            oxidean_api_core::repo::capability_env_label(accessible.capability),
        )
        .await
        .map_err(|e| match e {
            oxidean_git::GitError::NotFound(m) => {
                AppError::new("wiki.not_found", format!("wiki page not found: {m}"))
            }
            other => git_err("wiki commit", other),
        })?;
    Ok(result.sha)
}

/// `gollum` — webhook delivery + `on: gollum` Actions dispatch. One page
/// per emit (RPC mutations are single-page); git pushes fan the same shape
/// out per changed page in `webhook::dispatch::notify_wiki_push`.
pub async fn notify_gollum(
    ctx: &RpcCtx,
    accessible: &AccessibleRepo,
    page: &str,
    action: &str,
    sha: &str,
    user: &oxidean_db::UserRow,
) {
    let repository = oxidean_api_core::actions::payload::repository_embed(
        &accessible.owner_username,
        &accessible.row.name,
        &accessible.row.default_branch,
    );
    let sender = oxidean_api_core::actions::payload::user_embed(&user.username);
    let payload = oxidean_api_core::actions::payload::gollum_payload(
        &[oxidean_api_core::actions::payload::GollumPage {
            page_name: page.to_string(),
            action: action.to_string(),
            sha: sha.to_string(),
            html_url: html_url(&accessible.owner_username, &accessible.row.name, page),
        }],
        &repository,
        &sender,
    );
    oxidean_api_core::webhook::dispatch::emit(
        &ctx.db,
        &accessible.row.id,
        "gollum",
        "",
        payload.clone(),
        &ctx.env_name,
    )
    .await;
    let Some(event) = oxidean_api_core::actions::events::rpc_lifecycle_event(
        ctx,
        &accessible.owner_username,
        &accessible.row.name,
        &accessible.row.id,
        &accessible.row.default_branch,
        "gollum",
        "",
        payload,
        Some(user.id.clone()),
    ) else {
        return;
    };
    oxidean_api_core::actions::events::notify_lifecycle_actions(&ctx.db, ctx.git.as_ref(), &event)
        .await;
}

fn git_err(where_: &str, e: oxidean_git::GitError) -> AppError {
    tracing::error!(error = %e, "{where_} failed");
    AppError::new("wiki.internal", "wiki operation failed")
}
