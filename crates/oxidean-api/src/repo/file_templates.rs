//! `repo.templates.list` — issue/PR templates read from the repo's
//! default-branch git tree (COL-02).
//!
//! Template discovery mirrors GitHub conventions (ordered candidates, first
//! location with at least one `.md` template wins), plus a leading `.oxidean/`
//! slot so repos can override without shadowing `.github/` on other forges.

use oxidean_core::{
    AppError, RepoFileTemplate, RepoTemplatesListRequest, RepoTemplatesListResponse,
};
use serde::Deserialize;

use super::{map_git_err, resolve_repo_for_read};
use crate::git::bare_repo_path;
use crate::rpc::RpcCtx;

/// Per-file cap so a giant committed blob cannot bloat one RPC response.
const TEMPLATE_FILE_MAX_BYTES: usize = 256 * 1024;
/// Bound on templates collected from a single location directory.
const MAX_TEMPLATES_PER_KIND: usize = 100;

/// A discovery candidate: either a directory of `*.md` templates or a single
/// legacy `.md` file.
enum Candidate {
    Dir(&'static str),
    File(&'static str),
}

/// Issue template locations, checked in order. First candidate that yields at
/// least one template wins (GitHub precedence for `.github/` > root > `docs/`,
/// with `.oxidean/` allowed to shadow).
const ISSUE_TEMPLATE_CANDIDATES: &[Candidate] = &[
    Candidate::Dir(".oxidean/ISSUE_TEMPLATE"),
    Candidate::Dir(".github/ISSUE_TEMPLATE"),
    Candidate::Dir("ISSUE_TEMPLATE"),
    Candidate::File("ISSUE_TEMPLATE.md"),
    Candidate::File(".github/ISSUE_TEMPLATE.md"),
    Candidate::Dir("docs/ISSUE_TEMPLATE"),
    Candidate::File("docs/ISSUE_TEMPLATE.md"),
];

/// Pull request template locations, same ordering convention.
const PULL_TEMPLATE_CANDIDATES: &[Candidate] = &[
    Candidate::Dir(".oxidean/PULL_REQUEST_TEMPLATE"),
    Candidate::Dir(".github/PULL_REQUEST_TEMPLATE"),
    Candidate::Dir("PULL_REQUEST_TEMPLATE"),
    Candidate::File("PULL_REQUEST_TEMPLATE.md"),
    Candidate::File(".github/PULL_REQUEST_TEMPLATE.md"),
    Candidate::Dir("docs/PULL_REQUEST_TEMPLATE"),
    Candidate::File("docs/PULL_REQUEST_TEMPLATE.md"),
];

/// GitHub-style `---` frontmatter on a `.md` template. All keys optional; a
/// malformed block is ignored (whole file becomes the body).
#[derive(Debug, Default, Deserialize)]
struct TemplateFrontmatter {
    name: Option<String>,
    about: Option<String>,
    title: Option<String>,
    #[serde(default, deserialize_with = "de_string_or_seq")]
    labels: Vec<String>,
}

/// `labels:` accepts a comma-separated scalar (`"bug, triage"`) or a YAML list.
fn de_string_or_seq<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_yaml::Value::deserialize(deserializer)?;
    let mut out = Vec::new();
    match value {
        serde_yaml::Value::String(s) => {
            out.extend(
                s.split(',')
                    .map(|p| p.trim().to_string())
                    .filter(|p| !p.is_empty()),
            );
        }
        serde_yaml::Value::Sequence(items) => {
            for item in items {
                match item {
                    serde_yaml::Value::String(s) => out.push(s),
                    serde_yaml::Value::Number(n) => out.push(n.to_string()),
                    _ => {}
                }
            }
        }
        serde_yaml::Value::Null => {}
        _ => {}
    }
    Ok(out)
}

/// Split a leading `---\n…\n---\n` frontmatter block off `text`.
/// Returns `(frontmatter, body)`; `None` when no well-formed block is present.
fn split_frontmatter(text: &str) -> Option<(&str, &str)> {
    let t = text.strip_prefix('\u{feff}').unwrap_or(text);
    let t = t.strip_prefix("---")?;
    // Opening fence must be a whole line.
    let t = t.strip_prefix("\r\n").or_else(|| t.strip_prefix('\n'))?;
    let mut offset = 0usize;
    for line in t.split_inclusive('\n') {
        let trimmed = line.trim_end_matches(['\n', '\r']);
        if trimmed == "---" {
            let fm = &t[..offset];
            let body = &t[offset + line.len()..];
            return Some((fm, body));
        }
        offset += line.len();
    }
    None
}

/// Display name fallback: filename stem (`bug_report.md` → `bug_report`).
fn name_from_filename(path: &str) -> String {
    let base = path.rsplit('/').next().unwrap_or(path);
    match base.rsplit_once('.') {
        Some((stem, ext)) if ext.eq_ignore_ascii_case("md") => stem.to_string(),
        _ => base.to_string(),
    }
}

fn parse_template(filename: &str, raw: &str) -> RepoFileTemplate {
    let (fm, body) = match split_frontmatter(raw) {
        Some((fm_text, body_text)) => match serde_yaml::from_str(fm_text) {
            Ok(parsed) => (parsed, body_text.trim_start().to_string()),
            // Malformed YAML → ignore the block entirely; whole file is the body.
            Err(_) => (TemplateFrontmatter::default(), raw.to_string()),
        },
        None => (TemplateFrontmatter::default(), raw.to_string()),
    };
    RepoFileTemplate {
        name: fm
            .name
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| name_from_filename(filename)),
        // `title` is a literal subject prefill (e.g. `"[BUG]: "`) — keep verbatim.
        title: fm.title.filter(|s| !s.trim().is_empty()),
        description: fm
            .about
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty()),
        labels: fm.labels,
        body,
        filename: filename.to_string(),
    }
}

/// Read one discovery candidate; empty Vec when the location is absent or has
/// no `.md` templates.
async fn read_candidate(
    ctx: &RpcCtx,
    bare: &std::path::Path,
    ref_name: &str,
    candidate: &Candidate,
) -> Result<Vec<RepoFileTemplate>, AppError> {
    let mut paths: Vec<String> = Vec::new();
    match candidate {
        Candidate::Dir(dir) => {
            let entries = ctx
                .git
                .ls_tree(bare, ref_name, dir)
                .await
                .map_err(map_git_err)?;
            for e in entries {
                if e.kind != oxidean_git::TreeEntryKind::Blob {
                    continue;
                }
                // Templates are markdown only — config.yml / .txt / etc. skip.
                if !e.name.to_ascii_lowercase().ends_with(".md") {
                    continue;
                }
                paths.push(format!("{dir}/{}", e.name));
            }
            paths.sort();
            paths.truncate(MAX_TEMPLATES_PER_KIND);
        }
        Candidate::File(file) => {
            paths.push((*file).to_string());
        }
    }

    let mut out = Vec::new();
    for path in paths {
        match ctx.git.cat_blob(bare, ref_name, &path).await {
            Ok(bytes) => {
                let bytes = if bytes.len() > TEMPLATE_FILE_MAX_BYTES {
                    &bytes[..TEMPLATE_FILE_MAX_BYTES]
                } else {
                    &bytes[..]
                };
                out.push(parse_template(&path, &String::from_utf8_lossy(bytes)));
            }
            // Missing file / vanished path → not a template; other errors bubble.
            Err(oxidean_git::GitError::NotFound(_)) => {}
            Err(e) => return Err(map_git_err(e)),
        }
    }
    Ok(out)
}

async fn collect_templates(
    ctx: &RpcCtx,
    bare: &std::path::Path,
    ref_name: &str,
    candidates: &[Candidate],
) -> Result<Vec<RepoFileTemplate>, AppError> {
    for candidate in candidates {
        let found = read_candidate(ctx, bare, ref_name, candidate).await?;
        if !found.is_empty() {
            return Ok(found);
        }
    }
    Ok(Vec::new())
}

/// `repo.templates.list` — `{ issues, pulls }` file templates from the
/// default-branch tree. Read-gated like `repo.tree` (anonymous OK on public).
pub async fn file_templates(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<RepoTemplatesListResponse, AppError> {
    let req: RepoTemplatesListRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid repo.templates.list input: {e}"),
        )
    })?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    let bare = bare_repo_path(
        &ctx.repos_dir,
        &accessible.owner_username,
        &accessible.row.name,
    )?;
    let ref_name = accessible.row.default_branch.clone();
    if ref_name.trim().is_empty() {
        return Ok(RepoTemplatesListResponse {
            issues: Vec::new(),
            pulls: Vec::new(),
        });
    }

    let issues = collect_templates(ctx, &bare, &ref_name, ISSUE_TEMPLATE_CANDIDATES).await?;
    let pulls = collect_templates(ctx, &bare, &ref_name, PULL_TEMPLATE_CANDIDATES).await?;
    Ok(RepoTemplatesListResponse { issues, pulls })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_github_style_frontmatter() {
        let raw = "---\nname: Bug report\nabout: File a bug\ntitle: \"[BUG] \"\nlabels: bug, triage\n---\n\nSteps to reproduce\n";
        let t = parse_template(".github/ISSUE_TEMPLATE/bug.md", raw);
        assert_eq!(t.name, "Bug report");
        assert_eq!(t.description.as_deref(), Some("File a bug"));
        assert_eq!(t.title.as_deref(), Some("[BUG] "));
        assert_eq!(t.labels, vec!["bug".to_string(), "triage".to_string()]);
        assert_eq!(t.body, "Steps to reproduce\n");
    }

    #[test]
    fn labels_accept_yaml_list() {
        let raw = "---\nname: X\nlabels:\n  - bug\n  - ui\n---\nbody";
        let t = parse_template("ISSUE_TEMPLATE/x.md", raw);
        assert_eq!(t.labels, vec!["bug".to_string(), "ui".to_string()]);
        assert_eq!(t.body, "body");
    }

    #[test]
    fn no_frontmatter_is_plain_body_with_stem_name() {
        let t = parse_template("PULL_REQUEST_TEMPLATE.md", "## What\nbody text\n");
        assert_eq!(t.name, "PULL_REQUEST_TEMPLATE");
        assert_eq!(t.body, "## What\nbody text\n");
        assert!(t.title.is_none());
        assert!(t.description.is_none());
    }

    #[test]
    fn malformed_frontmatter_falls_back_to_full_body() {
        let raw = "---\nname: [unclosed\n---\nbody text\n";
        let t = parse_template("ISSUE_TEMPLATE/bad.md", raw);
        assert_eq!(t.name, "bad");
        assert!(t.body.contains("name: [unclosed"));
    }

    #[test]
    fn crlf_frontmatter_parses() {
        let raw = "---\r\nname: CRLF\r\n---\r\nbody\r\n";
        let t = parse_template("a.md", raw);
        assert_eq!(t.name, "CRLF");
        assert_eq!(t.body, "body\r\n");
    }
}
