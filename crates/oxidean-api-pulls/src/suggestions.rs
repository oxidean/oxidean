//! `pull.suggestions.apply` / `pull.suggestions.apply_batch` — COL-08
//! suggested-change blocks (GitHub parity). The server parses
//! ````suggestion` fences out of the stored comment body — the client never
//! supplies replacement content — splices the comment's RIGHT-side line
//! anchor at the current head tip, and lands a signed commit on the head
//! branch with the same post-push fan-out as a receive-pack.

use std::collections::{BTreeMap, BTreeSet};

use oxidean_core::{
    AppError, ApplyPullSuggestionBatchRequest, ApplyPullSuggestionBatchResponse,
    ApplyPullSuggestionRequest, ApplyPullSuggestionResponse,
};
use oxidean_db::{PullCommentRow, PullRow, UserRow};
use oxidean_git::FileChange;

use oxidean_api_core::auth::gate::require_verified;
use oxidean_api_core::git::{bare_repo_path, branch_tip, web_flow::ensure_web_flow_key};
use oxidean_api_core::protection::{effective_for_branch, evaluate_push, ProtectionIntent};
use oxidean_api_core::pull::{acl, db_err, load_pull_in_repo};
use oxidean_api_core::repo::files::{file_git_err, normalize_edit_path};
use oxidean_api_core::repo::{capability_env_label, meets, not_found, Capability};
use oxidean_api_core::rpc::RpcCtx;

use crate::update_branch::resolve_head;

fn suggestion_not_found() -> AppError {
    AppError::new(
        "pull.suggestion_not_found",
        "no suggestion block at that index",
    )
}

fn suggestion_invalid(msg: impl Into<String>) -> AppError {
    AppError::new("pull.suggestion_invalid", msg.into())
}

fn suggestion_out_of_range() -> AppError {
    AppError::new(
        "pull.suggestion_out_of_range",
        "suggestion range exceeds the file at the head tip",
    )
}

fn suggestion_overlap() -> AppError {
    AppError::new(
        "pull.suggestion_overlap",
        "suggestions in the same file must not overlap",
    )
}

/// One parsed ````suggestion` block: the suggestion's replacement text plus
/// its position among suggestion fences in the body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SuggestionBlock {
    /// Replacement lines joined by `\n` (no trailing newline). Empty = the
    /// author proposes deleting the anchored range.
    pub body: String,
}

/// Scan markdown source for ````suggestion` fenced code blocks. Only top-level
/// fences count — a `suggestion` info string inside another fence is code
/// content, and quoted fences (`> ``` `) are not suggestion-eligible, matching
/// GitHub's comment pipeline.
///
/// Grammar (GFM): an opener is ≤3-space indentation then 3+ backticks or
/// tildes whose info string trims to exactly `suggestion`; the closer is a
/// run of the same fence char at least as long as the opener. Content lines
/// lose up to the opener's indentation.
pub fn suggestion_blocks(body: &str) -> Vec<SuggestionBlock> {
    let mut out = Vec::new();
    let lines: Vec<&str> = body.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let stripped = line.trim_start();
        let indent = line.len() - stripped.len();
        let Some((fence_ch, fence_len)) = fence_marker(stripped) else {
            i += 1;
            continue;
        };
        if indent > 3 {
            // GFM: >3-space indentation makes it a code block, not a fence.
            i += 1;
            continue;
        }
        let info = stripped[fence_len..].trim();
        if info != "suggestion" {
            // Skip the whole non-suggestion fence so a `suggestion` opener
            // nested inside regular code doesn't count.
            i += 1;
            while i < lines.len() && !is_fence_close(lines[i], fence_ch, fence_len) {
                i += 1;
            }
            i += 1; // consume the closer (or EOF)
            continue;
        }
        // Collect the suggestion body until its closer.
        i += 1;
        let mut buf = String::new();
        while i < lines.len() && !is_fence_close(lines[i], fence_ch, fence_len) {
            let content = strip_indent(lines[i], indent);
            buf.push_str(content);
            buf.push('\n');
            i += 1;
        }
        i += 1; // consume the closer (or EOF)
        if buf.ends_with('\n') {
            buf.pop();
        }
        out.push(SuggestionBlock { body: buf });
    }
    out
}

/// Leading fence marker on a trimmed line: ` ``` `/` ~~~ ` runs of length ≥3.
/// Returns `(fence_char, run_length)`.
fn fence_marker(trimmed: &str) -> Option<(char, usize)> {
    let ch = trimmed.chars().next()?;
    if ch != '`' && ch != '~' {
        return None;
    }
    let len = trimmed.chars().take_while(|c| *c == ch).count();
    (len >= 3).then_some((ch, len))
}

/// Closer line: only the fence char (allowing GFM's ≤3-space indentation and
/// trailing whitespace), run length ≥ opener length.
fn is_fence_close(line: &str, fence_ch: char, fence_len: usize) -> bool {
    let t = line.trim();
    !t.is_empty() && t.chars().all(|c| c == fence_ch) && t.chars().count() >= fence_len
}

/// GFM content de-indent: drop up to `indent` leading spaces.
fn strip_indent(line: &str, indent: usize) -> &str {
    let mut n = 0;
    let mut byte = 0;
    for c in line.chars() {
        if c == ' ' && n < indent {
            n += 1;
            byte += 1;
        } else {
            break;
        }
    }
    &line[byte..]
}

/// The anchor + replacement extracted from one comment row.
struct ResolvedSuggestion {
    path: String,
    /// 1-based first line of the replaced range.
    start_line: i64,
    /// 1-based last line (inclusive).
    end_line: i64,
    suggestion: String,
    /// Comment author — drives the Co-authored-by trailer.
    author_id: String,
}

/// Validate that `row` is a submitted, RIGHT-side line-anchored comment on
/// `pull` and pull its `index`-th suggestion block.
fn resolve_suggestion(
    row: &PullCommentRow,
    pull: &PullRow,
    index: usize,
) -> Result<ResolvedSuggestion, AppError> {
    if row.pull_id != pull.id {
        return Err(AppError::new("pull.comment_not_found", "Comment not found"));
    }
    if row.pending {
        return Err(suggestion_invalid(
            "pending review drafts cannot carry suggestions",
        ));
    }
    if row.outdated {
        return Err(suggestion_invalid(
            "suggestion is on an outdated diff position",
        ));
    }
    if row.side.as_deref() != Some("RIGHT") {
        return Err(suggestion_invalid(
            "suggestions require a RIGHT-side line anchor",
        ));
    }
    let Some(line) = row.line else {
        return Err(suggestion_invalid(
            "suggestions require a line-anchored comment",
        ));
    };
    let path = row.path.as_deref().map(str::trim).unwrap_or_default();
    if path.is_empty() {
        return Err(suggestion_invalid(
            "suggestions require a file-anchored comment",
        ));
    }
    let path = normalize_edit_path(path)?;
    let start_line = row.start_line.unwrap_or(line);
    let blocks = suggestion_blocks(&row.body);
    let Some(block) = blocks.get(index) else {
        return Err(suggestion_not_found());
    };
    Ok(ResolvedSuggestion {
        path,
        start_line,
        end_line: line,
        suggestion: block.body.clone(),
        author_id: row.author_id.clone(),
    })
}

/// Splice `lines[start..=end]` (1-based inclusive) out of `content`,
/// substituting `replacement` (empty = delete the range). Operates on
/// `\n`-terminated line pieces so retained CRLF endings survive; the
/// suggestion itself is treated as whole lines and gains a `\n` whenever
/// retained lines follow it or the replaced region was newline-terminated.
fn splice_lines(
    content: &str,
    start: i64,
    end: i64,
    replacement: &str,
) -> Result<String, AppError> {
    let pieces: Vec<&str> = content.split_inclusive('\n').collect();
    if start < 1 || end < start || end as usize > pieces.len() {
        return Err(suggestion_out_of_range());
    }
    let s = (start - 1) as usize;
    let e = end as usize; // exclusive
    let replaced_ended_nl = pieces[e - 1].ends_with('\n');
    let mut out = String::with_capacity(content.len() + replacement.len());
    for piece in &pieces[..s] {
        out.push_str(piece);
    }
    if !replacement.is_empty() {
        out.push_str(replacement);
        if !replacement.ends_with('\n') && (e < pieces.len() || replaced_ended_nl) {
            out.push('\n');
        }
    }
    for piece in &pieces[e..] {
        out.push_str(piece);
    }
    Ok(out)
}

/// Shared gates for both apply RPCs: verified user, open PR, and the
/// update-branch permission model — Write+ on the head repo OR Write+ on the
/// base repo (maintainer parity), with the head branch's protection rules
/// evaluated as a Push intent.
async fn gate(
    ctx: &RpcCtx,
    owner: &str,
    name: &str,
    number: i64,
) -> Result<(PullRow, ApplyContextParts), AppError> {
    let accessible = acl::resolve_for_read(ctx, owner, name).await?;
    oxidean_api_core::repo::ensure_not_archived(&accessible)?;
    let pull = load_pull_in_repo(ctx, &accessible.row.id, number).await?;
    if pull.state != "open" {
        return Err(AppError::new(
            "pull.invalid_state",
            "suggestions can only apply to open pull requests",
        ));
    }
    let (head_repo, head_slug, head_cap) = resolve_head(ctx, &accessible, &pull).await?;
    if head_repo.archived {
        return Err(oxidean_api_core::repo::archived_error());
    }
    if !meets(accessible.capability, Capability::Write) && !meets(head_cap, Capability::Write) {
        return Err(not_found());
    }
    let head_bare = bare_repo_path(&ctx.repos_dir, &head_slug, &head_repo.name)?;
    let head_tip = branch_tip(ctx.git.as_ref(), &head_bare, &pull.head_ref)
        .await?
        .ok_or_else(|| AppError::new("pull.head_unavailable", "head branch is unavailable"))?;
    // Head-branch protection up front — `commit_files`'s update hook
    // re-verifies against races, same contract as web file edits.
    let eff = effective_for_branch(&ctx.db, &head_repo.id, &pull.head_ref).await?;
    evaluate_push(&eff, ProtectionIntent::Push, head_cap)?;
    Ok((
        pull,
        ApplyContextParts {
            head_repo,
            head_slug,
            head_cap,
            head_bare,
            head_tip,
        },
    ))
}

struct ApplyContextParts {
    head_repo: oxidean_db::RepositoryRow,
    head_slug: String,
    head_cap: Option<Capability>,
    head_bare: std::path::PathBuf,
    head_tip: String,
}

/// Read the UTF-8 file text at the head tip.
async fn head_file_text(
    ctx: &RpcCtx,
    parts: &ApplyContextParts,
    path: &str,
) -> Result<String, AppError> {
    let bytes = ctx
        .git
        .cat_blob(&parts.head_bare, &parts.head_tip, path)
        .await
        .map_err(file_git_err)?;
    String::from_utf8(bytes)
        .map_err(|_| suggestion_invalid("suggestions apply to UTF-8 files only"))
}

/// Signed commit + the same post-push fan-out a receive-pack performs on the
/// head repo: activity rows, push webhook, PR synchronize bookkeeping,
/// Actions dispatch, mirror sync.
async fn commit_to_head(
    ctx: &RpcCtx,
    pull: &PullRow,
    parts: &ApplyContextParts,
    user: &UserRow,
    message: &str,
    changes: Vec<FileChange>,
) -> Result<String, AppError> {
    let signing_key = ensure_web_flow_key().await.map_err(|e| {
        tracing::error!(error = %e, "web-flow signing key unavailable for suggestion commit");
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
            &parts.head_bare,
            &pull.head_ref,
            // `base` is a *branch name* — the in-place tip the commit builds
            // on; passing the tip sha would read refs/heads/<sha> and also
            // trip the new-branch guard when branch != base.
            &pull.head_ref,
            message,
            &changes,
            &author_name,
            &user.email,
            Some(signing_key.as_path()),
            capability_env_label(parts.head_cap),
        )
        .await
        .map_err(file_git_err)?;

    let ref_name = format!("refs/heads/{}", pull.head_ref);
    let before = result
        .base_sha
        .clone()
        .unwrap_or_else(|| oxidean_api_core::repo::files::ZERO_OID.to_string());
    let updates = vec![(before, result.sha.clone(), ref_name)];
    oxidean_api_core::repo::record_ref_updates(
        &ctx.db,
        &parts.head_repo.id,
        &user.id,
        &updates,
        Some(ctx.git.clone()),
        Some(parts.head_bare.as_path()),
    )
    .await;
    oxidean_api_core::webhook::dispatch::notify_push(
        &ctx.db,
        &parts.head_repo.id,
        &parts.head_slug,
        &parts.head_repo.name,
        &user.username,
        &user.id,
        &updates,
        &ctx.env_name,
    )
    .await;
    oxidean_api_core::pull::synchronize_after_push(
        &ctx.db,
        &ctx.repos_dir,
        &parts.head_repo.id,
        &parts.head_slug,
        &parts.head_repo.name,
        &user.username,
        &user.id,
        &updates,
        &ctx.env_name,
        oxidean_api_core::actions::events::rpc_caller_is_job_token(ctx),
    )
    .await;
    // GITHUB_TOKEN parity — job-token commits never start `push` runs.
    if !oxidean_api_core::actions::events::rpc_caller_is_job_token(ctx) {
        oxidean_api_core::actions::notify_push_actions(
            &ctx.db,
            ctx.git.clone(),
            &ctx.repos_dir,
            &parts.head_repo.id,
            &parts.head_slug,
            &parts.head_repo.name,
            Some(&user.id),
            &updates,
            oxidean_api_core::actions::env_actions_enabled(),
        )
        .await;
    }
    oxidean_api_core::mirror::notify_mirror_after_local_mutation(
        ctx.db.clone(),
        ctx.git.clone(),
        ctx.repos_dir.clone(),
        parts.head_repo.id.clone(),
    );
    Ok(result.sha)
}

/// `{id}+{username}@users.noreply.<host>` — GitHub's co-author noreply shape;
/// resolvable back to the user by `repo::author_resolve`.
fn coauthor_email(author: &UserRow) -> String {
    format!(
        "{}+{}@users.noreply.{}",
        author.id,
        author.username,
        oxidean_api_core::repo::author_resolve::noreply_host()
    )
}

/// Append `Co-authored-by` trailers for each distinct suggestion author who
/// isn't the applier (GitHub parity — the applier is already the commit
/// author).
async fn message_with_coauthors(
    ctx: &RpcCtx,
    base: &str,
    suggestion_authors: &[String],
    applier_id: &str,
) -> String {
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut trailers = Vec::new();
    for id in suggestion_authors {
        if id == applier_id || !seen.insert(id.clone()) {
            continue;
        }
        if let Ok(Some(u)) = ctx.db.find_user_by_id(id).await {
            let name = if u.display_name.trim().is_empty() {
                u.username.clone()
            } else {
                u.display_name.clone()
            };
            trailers.push(format!("Co-authored-by: {name} <{}>", coauthor_email(&u)));
        }
    }
    if trailers.is_empty() {
        base.to_string()
    } else {
        format!("{base}\n\n{}", trailers.join("\n"))
    }
}

/// `pull.suggestions.apply` — one ````suggestion` block → one head commit.
pub async fn apply(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<ApplyPullSuggestionResponse, AppError> {
    let user = require_verified(ctx).await?;
    let req: ApplyPullSuggestionRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid pull.suggestions.apply input: {e}"),
        )
    })?;
    let (pull, parts) = gate(ctx, &req.owner, &req.name, req.number).await?;
    let row = ctx
        .db
        .find_pull_comment_by_id(&req.comment_id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("pull.comment_not_found", "Comment not found"))?;
    let index = req.suggestion_index.unwrap_or(0);
    if index < 0 {
        return Err(suggestion_not_found());
    }
    let suggestion = resolve_suggestion(&row, &pull, index as usize)?;
    let content = head_file_text(ctx, &parts, &suggestion.path).await?;
    let new_content = splice_lines(
        &content,
        suggestion.start_line,
        suggestion.end_line,
        &suggestion.suggestion,
    )?;
    if new_content == content {
        return Err(suggestion_invalid("suggestion makes no change"));
    }
    let base_message = req
        .message
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("Apply suggestion from code review");
    let message =
        message_with_coauthors(ctx, base_message, &[suggestion.author_id], &user.id).await;
    let sha = commit_to_head(
        ctx,
        &pull,
        &parts,
        &user,
        &message,
        vec![FileChange::Upsert {
            path: suggestion.path,
            content: new_content.into_bytes(),
        }],
    )
    .await?;
    Ok(ApplyPullSuggestionResponse {
        commit_sha: sha,
        branch: pull.head_ref.clone(),
    })
}

/// `pull.suggestions.apply_batch` — first ````suggestion` block of each
/// comment, one commit across all touched files.
pub async fn apply_batch(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<ApplyPullSuggestionBatchResponse, AppError> {
    let user = require_verified(ctx).await?;
    let req: ApplyPullSuggestionBatchRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid pull.suggestions.apply_batch input: {e}"),
        )
    })?;
    if req.comment_ids.is_empty() {
        return Err(AppError::new("rpc.bad_input", "comment_ids is empty"));
    }
    let (pull, parts) = gate(ctx, &req.owner, &req.name, req.number).await?;

    // Resolve + group per path. Descending application order inside a file
    // keeps earlier splices from shifting later anchors.
    let mut by_path: BTreeMap<String, Vec<ResolvedSuggestion>> = BTreeMap::new();
    let mut authors = Vec::with_capacity(req.comment_ids.len());
    for id in &req.comment_ids {
        let row = ctx
            .db
            .find_pull_comment_by_id(id)
            .await
            .map_err(db_err)?
            .ok_or_else(|| AppError::new("pull.comment_not_found", "Comment not found"))?;
        let suggestion = resolve_suggestion(&row, &pull, 0)?;
        authors.push(suggestion.author_id.clone());
        by_path
            .entry(suggestion.path.clone())
            .or_default()
            .push(suggestion);
    }
    let mut changes = Vec::with_capacity(by_path.len());
    for (path, mut group) in by_path {
        // Reject overlapping ranges in the same file — GitHub refuses the
        // batch rather than guessing intent.
        group.sort_by_key(|s| s.start_line);
        for w in group.windows(2) {
            if w[1].start_line <= w[0].end_line {
                return Err(suggestion_overlap());
            }
        }
        let content = head_file_text(ctx, &parts, &path).await?;
        let mut next = content.clone();
        for s in group.iter().rev() {
            next = splice_lines(&next, s.start_line, s.end_line, &s.suggestion)?;
        }
        if next != content {
            changes.push(FileChange::Upsert {
                path,
                content: next.into_bytes(),
            });
        }
    }
    if changes.is_empty() {
        return Err(suggestion_invalid("suggestions make no change"));
    }
    let applied = authors.len() as i64;
    let base_message = req
        .message
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("Apply suggestions from code review");
    let message = message_with_coauthors(ctx, base_message, &authors, &user.id).await;
    let sha = commit_to_head(ctx, &pull, &parts, &user, &message, changes).await?;
    Ok(ApplyPullSuggestionBatchResponse {
        commit_sha: sha,
        branch: pull.head_ref.clone(),
        applied,
    })
}

#[cfg(test)]
mod tests {
    use super::{splice_lines, suggestion_blocks};

    #[test]
    fn parses_single_suggestion_block() {
        let body = "please fix\n```suggestion\nlet x = 1;\n```\nthanks";
        let blocks = suggestion_blocks(body);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].body, "let x = 1;");
    }

    #[test]
    fn empty_block_means_delete() {
        let blocks = suggestion_blocks("```suggestion\n```");
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].body, "");
    }

    #[test]
    fn multiple_blocks_and_tilde_fences() {
        let body =
            "a\n```suggestion\none\n```\nb\n~~~suggestion\ntwo\n~~~\n```rust\nnot a suggestion\n```\n";
        let blocks = suggestion_blocks(body);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].body, "one");
        assert_eq!(blocks[1].body, "two");
    }

    #[test]
    fn suggestion_inside_other_fence_does_not_count() {
        let body = "```markdown\n```suggestion\nfake\n```\n```\n";
        assert_eq!(suggestion_blocks(body).len(), 0);
    }

    #[test]
    fn longer_closer_and_longer_opener() {
        // 4-backtick suggestion closes only on ≥4 backticks.
        let body = "````suggestion\na ``` b\n````";
        let blocks = suggestion_blocks(body);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].body, "a ``` b");
    }

    #[test]
    fn indented_opener_strips_content_indent() {
        let body = "   ```suggestion\n   foo\n    bar\n   ```";
        let blocks = suggestion_blocks(body);
        assert_eq!(blocks[0].body, "foo\n bar");
    }

    #[test]
    fn splice_replaces_single_line() {
        let out = splice_lines("a\nb\nc\n", 2, 2, "B").unwrap();
        assert_eq!(out, "a\nB\nc\n");
    }

    #[test]
    fn splice_replaces_range_and_deletes() {
        let text = "a\nb\nc\nd\n";
        assert_eq!(splice_lines(text, 2, 3, "X\nY").unwrap(), "a\nX\nY\nd\n");
        assert_eq!(splice_lines(text, 2, 3, "").unwrap(), "a\nd\n");
    }

    #[test]
    fn splice_preserves_trailing_newline_at_eof() {
        assert_eq!(splice_lines("a\nb\n", 2, 2, "B").unwrap(), "a\nB\n");
        // File without trailing newline keeps it that way.
        assert_eq!(splice_lines("a\nb", 2, 2, "B").unwrap(), "a\nB");
    }

    #[test]
    fn splice_rejects_out_of_range() {
        assert!(splice_lines("a\nb\n", 3, 3, "x").is_err());
        assert!(splice_lines("a\nb\n", 0, 1, "x").is_err());
        assert!(splice_lines("", 1, 1, "x").is_err());
    }
}
