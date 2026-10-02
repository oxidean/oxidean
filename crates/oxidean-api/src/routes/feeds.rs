//! Atom 1.0 feeds — API-05.
//!
//! Served as `application/atom+xml` under `/api/...` so the edge gateway
//! (`/api` prefix → api service) routes them beside the other non-RPC HTTP
//! routes. Repo feeds enforce the same Read capability as the JSON surface —
//! a private repo answers the identical `repo.not_found` 404 anonymously.
//! The user feed is public-only: private-repo activity never appears in it.
//!
//! XML is rendered by hand with [`xml_escape`] on every interpolated value —
//! no xml crate (workspace dependency policy prefers minimal deps).

use std::collections::HashMap;
use std::fmt::Write as _;

use axum::extract::{Path as AxumPath, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use oxidean_core::AppError;
use oxidean_db::RepoActivityRow;

use crate::app::{build_rpc_ctx, session_token_from_headers, AppState};
use crate::public_origin::resolve_public_origin;
use crate::repo::{self, meets, Capability};
use crate::rpc::{self, RpcCtx};

/// Feeds return the newest N entries.
const FEED_LIMIT: i64 = 30;
const ATOM_CONTENT_TYPE: &str = "application/atom+xml; charset=utf-8";

fn err_json(status: StatusCode, code: &str, message: &str) -> Response {
    (
        status,
        Json(serde_json::json!({
            "ok": false,
            "error": { "code": code, "message": message }
        })),
    )
        .into_response()
}

/// Escape XML predefined entities; strip control chars XML 1.0 forbids.
fn xml_escape(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len() + 8);
    for c in raw.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            '\t' | '\n' | '\r' => out.push(c),
            c if (c as u32) < 0x20 => {}
            _ => out.push(c),
        }
    }
    out
}

/// Percent-encode one URL path segment (RFC 3986 unreserved set passes through).
fn path_segment(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for b in raw.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => {
                let _ = write!(out, "%{b:02X}");
            }
        }
    }
    out
}

fn is_zero_oid(oid: &str) -> bool {
    !oid.is_empty() && oid.chars().all(|c| c == '0')
}

fn short_ref(ref_name: &str) -> &str {
    ref_name
        .strip_prefix("refs/heads/")
        .or_else(|| ref_name.strip_prefix("refs/tags/"))
        .unwrap_or(ref_name)
}

struct AtomEntry {
    /// Stable id — `urn:uuid:{row id}` (UUIDv4 assigned at insert time).
    id: String,
    title: String,
    /// Absolute HTML permalink.
    link: String,
    published: String,
    updated: String,
    author_name: String,
    author_uri: String,
    content: Option<String>,
}

fn render_entry(e: &AtomEntry) -> String {
    let mut s = String::with_capacity(512);
    let _ = write!(
        s,
        "  <entry>\n    <title>{}</title>\n    <id>{}</id>\n    <link rel=\"alternate\" type=\"text/html\" href=\"{}\"/>\n    <published>{}</published>\n    <updated>{}</updated>\n",
        xml_escape(&e.title),
        xml_escape(&e.id),
        xml_escape(&e.link),
        e.published,
        e.updated
    );
    if !e.author_name.is_empty() {
        let _ = write!(
            s,
            "    <author>\n      <name>{}</name>\n      <uri>{}</uri>\n    </author>\n",
            xml_escape(&e.author_name),
            xml_escape(&e.author_uri)
        );
    }
    if let Some(content) = e.content.as_ref().filter(|c| !c.is_empty()) {
        let _ = write!(
            s,
            "    <content type=\"text\">{}</content>\n",
            xml_escape(content)
        );
    }
    s.push_str("  </entry>\n");
    s
}

fn render_feed(
    title: &str,
    self_href: &str,
    alternate_href: &str,
    entries: &[AtomEntry],
) -> String {
    // Rows are newest-first and timestamps share the `%Y-%m-%dT%H:%M:%SZ`
    // shape, so lexicographic max is the freshest entry timestamp.
    let updated = entries
        .iter()
        .map(|e| e.updated.as_str())
        .max()
        .map(str::to_string)
        .unwrap_or_else(|| chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string());
    let mut s = String::with_capacity(1024 + entries.len() * 512);
    s.push_str(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<feed xmlns=\"http://www.w3.org/2005/Atom\">\n",
    );
    let _ = write!(s, "  <title>{}</title>\n", xml_escape(title));
    let _ = write!(s, "  <id>{}</id>\n", xml_escape(self_href));
    let _ = write!(
        s,
        "  <link rel=\"self\" type=\"application/atom+xml\" href=\"{}\"/>\n  <link rel=\"alternate\" type=\"text/html\" href=\"{}\"/>\n  <updated>{updated}</updated>\n",
        xml_escape(self_href),
        xml_escape(alternate_href)
    );
    for e in entries {
        s.push_str(&render_entry(e));
    }
    s.push_str("</feed>\n");
    s
}

fn atom_response(xml: String) -> Response {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, ATOM_CONTENT_TYPE)],
        xml,
    )
        .into_response()
}

async fn feed_ctx(state: &AppState, headers: &HeaderMap) -> RpcCtx {
    let token = session_token_from_headers(headers);
    build_rpc_ctx(
        state,
        token.as_deref(),
        rpc::ClientMeta::from_headers(headers),
    )
    .await
}

/// Map a [`repo::resolve_repo_for_read`] failure like the raw/archive routes:
/// `repo.not_found` → 404 (anti-enumeration), internal failure → 500.
fn acl_err_response(e: &AppError) -> Response {
    if e.code == "repo.not_found" {
        err_json(StatusCode::NOT_FOUND, &e.code, &e.message)
    } else {
        tracing::error!(code = %e.code, error = %e.message, "feed repo resolve failed");
        err_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            &e.code,
            "repository operation failed",
        )
    }
}

/// Human-readable title for one activity row. `repo_ctx` appends
/// `in {owner}/{repo}` for feeds where the repo is not already the subject.
fn activity_title(row: &RepoActivityRow, repo_ctx: Option<&str>) -> String {
    let ref_short = short_ref(&row.ref_name);
    let n = row.commits_count;
    let verb = match row.push_type.as_str() {
        "push" => {
            if n <= 0 {
                format!("pushed to {ref_short}")
            } else if n == 1 {
                format!("pushed 1 commit to {ref_short}")
            } else {
                format!("pushed {n} commits to {ref_short}")
            }
        }
        "force_push" => {
            if n <= 0 {
                format!("force-pushed to {ref_short}")
            } else if n == 1 {
                format!("force-pushed 1 commit to {ref_short}")
            } else {
                format!("force-pushed {n} commits to {ref_short}")
            }
        }
        "branch_creation" => format!("created branch {ref_short}"),
        "branch_deletion" => format!("deleted branch {ref_short}"),
        "branch_rename" => {
            // record_branch_rename stores the old short name in commit_message.
            match row.commit_message.as_deref().map(str::trim) {
                Some(from) if !from.is_empty() => {
                    format!("renamed branch {from} to {ref_short}")
                }
                _ => format!("renamed branch to {ref_short}"),
            }
        }
        "pr_merge" => match row.pr_number {
            Some(num) => format!("merged pull request #{num} into {ref_short}"),
            None => format!("merged into {ref_short}"),
        },
        other => format!("updated {} ({other})", row.ref_name),
    };
    match repo_ctx {
        Some(ctx) => format!("{} {verb} in {ctx}", row.actor_username),
        None => format!("{} {verb}", row.actor_username),
    }
}

/// Most specific existing web page for the event, falling back to the repo root.
fn activity_link(origin: &str, owner: &str, repo_name: &str, row: &RepoActivityRow) -> String {
    let base = format!(
        "{origin}/{}/{}",
        path_segment(owner),
        path_segment(repo_name)
    );
    let before = row.before_oid.as_str();
    let after = row.after_oid.as_str();
    let after_live = !after.is_empty() && !is_zero_oid(after);
    match row.push_type.as_str() {
        "pr_merge" => match row.pr_number {
            Some(n) => format!("{base}/pull/{n}"),
            None => base,
        },
        "push" | "force_push" => {
            if after_live && !before.is_empty() && !is_zero_oid(before) {
                format!("{base}/compare/{before}...{after}")
            } else if after_live {
                format!("{base}/commit/{after}")
            } else {
                base
            }
        }
        "branch_creation" | "branch_rename" => {
            format!("{base}/tree/{}", path_segment(short_ref(&row.ref_name)))
        }
        _ => base,
    }
}

/// Commit-message content only where it carries the pushed/merged subject —
/// `branch_rename` reuses the column for the old branch name.
fn activity_content(row: &RepoActivityRow) -> Option<String> {
    match row.push_type.as_str() {
        "push" | "force_push" | "pr_merge" => {
            row.commit_message.clone().filter(|m| !m.trim().is_empty())
        }
        _ => None,
    }
}

fn activity_entry(
    origin: &str,
    owner: &str,
    repo_name: &str,
    row: &RepoActivityRow,
    repo_ctx: bool,
) -> AtomEntry {
    let repo_ctx = repo_ctx.then(|| format!("{owner}/{repo_name}"));
    AtomEntry {
        id: format!("urn:uuid:{}", row.id),
        title: activity_title(row, repo_ctx.as_deref()),
        link: activity_link(origin, owner, repo_name, row),
        published: row.created_at.clone(),
        updated: row.created_at.clone(),
        author_name: row.actor_username.clone(),
        author_uri: format!("{origin}/{}", path_segment(&row.actor_username)),
        content: activity_content(row),
    }
}

/// `GET /api/repos/{owner}/{repo}/activity.atom` — push / branch / merge feed.
pub async fn repo_activity_feed(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath((owner, repo_name)): AxumPath<(String, String)>,
) -> Response {
    let ctx = feed_ctx(&state, &headers).await;
    let accessible = match repo::resolve_repo_for_read(&ctx, &owner, &repo_name).await {
        Ok(a) => a,
        Err(e) => return acl_err_response(&e),
    };
    let (rows, _total) = match ctx
        .db
        .list_repo_activity(&accessible.row.id, None, None, 0, FEED_LIMIT)
        .await
    {
        Ok(v) => v,
        Err(e) => {
            tracing::error!(error = %e, "activity feed query failed");
            return err_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                "repo.internal",
                "repository operation failed",
            );
        }
    };

    let origin = resolve_public_origin();
    let owner_slug = accessible.owner_username.as_str();
    let name = accessible.row.name.as_str();
    let entries: Vec<AtomEntry> = rows
        .iter()
        .map(|r| activity_entry(&origin, owner_slug, name, r, false))
        .collect();
    let feed_href = format!(
        "{origin}/api/repos/{}/{}/activity.atom",
        path_segment(owner_slug),
        path_segment(name)
    );
    let html_href = format!(
        "{origin}/{}/{}/activity",
        path_segment(owner_slug),
        path_segment(name)
    );
    atom_response(render_feed(
        &format!("{owner_slug}/{name} activity"),
        &feed_href,
        &html_href,
        &entries,
    ))
}

/// `GET /api/repos/{owner}/{repo}/releases.atom` — release announcements.
/// Drafts surface only for Write+ callers (same rule as `release.list`).
pub async fn repo_releases_feed(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath((owner, repo_name)): AxumPath<(String, String)>,
) -> Response {
    let ctx = feed_ctx(&state, &headers).await;
    let accessible = match repo::resolve_repo_for_read(&ctx, &owner, &repo_name).await {
        Ok(a) => a,
        Err(e) => return acl_err_response(&e),
    };
    let include_drafts = meets(accessible.capability, Capability::Write);
    let rows = match ctx
        .db
        .list_releases_for_repo(&accessible.row.id, include_drafts)
        .await
    {
        Ok(v) => v,
        Err(e) => {
            tracing::error!(error = %e, "releases feed query failed");
            return err_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                "repo.internal",
                "repository operation failed",
            );
        }
    };
    let rows: Vec<_> = rows.into_iter().take(FEED_LIMIT as usize).collect();

    // Batch author lookup — one IN() round trip instead of N+1.
    let author_ids: Vec<String> = rows.iter().map(|r| r.author_id.clone()).collect();
    let author_names: HashMap<String, String> = ctx
        .db
        .find_users_by_ids(&author_ids)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|u| (u.id, u.username))
        .collect();

    let origin = resolve_public_origin();
    let owner_slug = accessible.owner_username.as_str();
    let name = accessible.row.name.as_str();
    let repo_base = format!(
        "{origin}/{}/{}",
        path_segment(owner_slug),
        path_segment(name)
    );
    let entries: Vec<AtomEntry> = rows
        .iter()
        .map(|r| {
            let author = author_names.get(&r.author_id).cloned().unwrap_or_default();
            let title = if r.title.trim().is_empty() {
                r.tag_name.clone()
            } else {
                r.title.clone()
            };
            AtomEntry {
                id: format!("urn:uuid:{}", r.id),
                title,
                link: format!("{repo_base}/releases/{}", path_segment(&r.tag_name)),
                published: r.created_at.clone(),
                updated: r.updated_at.clone(),
                author_name: author.clone(),
                author_uri: if author.is_empty() {
                    String::new()
                } else {
                    format!("{origin}/{}", path_segment(&author))
                },
                content: (!r.body.is_empty()).then(|| r.body.clone()),
            }
        })
        .collect();
    let feed_href = format!(
        "{origin}/api/repos/{}/{}/releases.atom",
        path_segment(owner_slug),
        path_segment(name)
    );
    atom_response(render_feed(
        &format!("{owner_slug}/{name} releases"),
        &feed_href,
        &format!("{repo_base}/releases"),
        &entries,
    ))
}

/// `GET /api/users/{username}/activity.atom` — public activity only (API-05):
/// `list_repo_activity_by_actor_public` filters `repositories.visibility =
/// 'public'`, so private-repo pushes never leak titles or URLs here.
pub async fn user_activity_feed(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(username): AxumPath<String>,
) -> Response {
    let ctx = feed_ctx(&state, &headers).await;
    let user = match ctx.db.find_user_by_username(username.trim()).await {
        Ok(Some(u)) => u,
        Ok(None) => return err_json(StatusCode::NOT_FOUND, "user.not_found", "User not found"),
        Err(e) => {
            tracing::error!(error = %e, "user feed lookup failed");
            return err_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                "user.internal",
                "user lookup failed",
            );
        }
    };
    let rows = match ctx
        .db
        .list_repo_activity_by_actor_public(&user.id, FEED_LIMIT)
        .await
    {
        Ok(v) => v,
        Err(e) => {
            tracing::error!(error = %e, "user activity feed query failed");
            return err_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                "user.internal",
                "user activity query failed",
            );
        }
    };

    let origin = resolve_public_origin();
    let entries: Vec<AtomEntry> = rows
        .iter()
        .filter_map(|r| {
            let owner = r.repo_owner.as_deref()?;
            let repo_name = r.repo_name.as_deref()?;
            Some(activity_entry(&origin, owner, repo_name, r, true))
        })
        .collect();
    let feed_href = format!(
        "{origin}/api/users/{}/activity.atom",
        path_segment(&user.username)
    );
    let html_href = format!("{origin}/{}", path_segment(&user.username));
    atom_response(render_feed(
        &format!("{} activity", user.username),
        &feed_href,
        &html_href,
        &entries,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xml_escape_covers_five_entities_and_control_chars() {
        assert_eq!(
            xml_escape("a<b>&\"'\"\u{0001}x"),
            "a&lt;b&gt;&amp;&quot;&apos;&quot;x"
        );
        assert_eq!(xml_escape("tab\tok\n"), "tab\tok\n");
    }

    #[test]
    fn path_segment_encodes_reserved_chars() {
        assert_eq!(path_segment("v1.0.0"), "v1.0.0");
        assert_eq!(path_segment("release/2026"), "release%2F2026");
        assert_eq!(path_segment("a b&c"), "a%20b%26c");
    }

    #[test]
    fn render_feed_is_well_formed_and_escaped() {
        let entries = vec![AtomEntry {
            id: "urn:uuid:abc".into(),
            title: "pushed <stuff> & \"things\"".into(),
            link: "http://x/o/r/commit/abc".into(),
            published: "2026-01-02T03:04:05Z".into(),
            updated: "2026-01-02T03:04:05Z".into(),
            author_name: "octo".into(),
            author_uri: "http://x/octo".into(),
            content: Some("msg <tag>".into()),
        }];
        let xml = render_feed("t", "http://x/f.atom", "http://x/o/r", &entries);
        assert!(xml.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
        assert!(xml.contains("<feed xmlns=\"http://www.w3.org/2005/Atom\">"));
        assert!(xml.contains("pushed &lt;stuff&gt; &amp; &quot;things&quot;"));
        assert!(xml.contains("msg &lt;tag&gt;"));
        assert!(xml.contains("<updated>2026-01-02T03:04:05Z</updated>"));
        assert!(xml.trim_end().ends_with("</feed>"));
    }

    #[test]
    fn activity_titles_read_well() {
        let base = RepoActivityRow {
            id: "i".into(),
            repository_id: "r".into(),
            actor_id: "u".into(),
            push_type: "push".into(),
            ref_name: "refs/heads/main".into(),
            before_oid: "a".into(),
            after_oid: "b".into(),
            commits_count: 2,
            commit_message: Some("x".into()),
            pr_number: None,
            created_at: "2026-01-01T00:00:00Z".into(),
            actor_username: "alice".into(),
            actor_display_name: String::new(),
            actor_avatar_path: None,
            repo_name: None,
            repo_owner: None,
        };
        assert_eq!(
            activity_title(&base, None),
            "alice pushed 2 commits to main"
        );
        let mut rename = base.clone();
        rename.push_type = "branch_rename".into();
        rename.commit_message = Some("old".into());
        rename.ref_name = "refs/heads/new".into();
        assert_eq!(
            activity_title(&rename, None),
            "alice renamed branch old to new"
        );
        let mut merge = base.clone();
        merge.push_type = "pr_merge".into();
        merge.pr_number = Some(7);
        assert_eq!(
            activity_title(&merge, None),
            "alice merged pull request #7 into main"
        );
        assert_eq!(
            activity_title(&base, Some("octo/hello")),
            "alice pushed 2 commits to main in octo/hello"
        );
    }
}
