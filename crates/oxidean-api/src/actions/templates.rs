//! Workflow template catalog — GitHub "New workflow" parity.
//!
//! Three provenances merge into one catalog:
//! - `builtin:*` — starter-workflows vendored byte-for-byte under
//!   `assets/workflow-templates` (embedded at compile time);
//! - `instance:*` — admin-managed rows in `instance_workflow_templates`;
//! - `org:*` — `<owner>/.github` repo `workflow-templates/` directory,
//!   when the repository owner is an org and the caller can read it.
//!
//! `workflows.create` drafts a template (or authored content) into
//! `.github/workflows/` through the same commit pipeline `repo.file.create`
//! uses — protected-branch fallback included.

use std::collections::BTreeSet;

use axum::response::IntoResponse;
use include_dir::{include_dir, Dir};
use oxidean_core::{
    AdminWorkflowTemplateCreateRequest, AdminWorkflowTemplateDeleteRequest,
    AdminWorkflowTemplateSetEnabledRequest, AdminWorkflowTemplateUpdateRequest,
    AdminWorkflowTemplatesListResponse, AppError, InstanceWorkflowTemplatePublic,
    RepoFileCommitOptions, RepoFileCommitResponse, WorkflowCreateRequest,
    WorkflowTemplateGetRequest, WorkflowTemplateGetResponse, WorkflowTemplateProvenance,
    WorkflowTemplateSummary, WorkflowTemplatesListRequest, WorkflowTemplatesListResponse,
};
use oxidean_db::InstanceWorkflowTemplateRow;
use oxidean_git::FileChange;
use serde::Deserialize;
use uuid::Uuid;

use crate::actions::parse::{parse_workflow_yaml, WorkflowDocument};
use crate::actions::rpc::resolve_treeish;
use crate::actions::workflow::{confine_workflow_path, is_workflow_filename};
use crate::admin::require_admin_user;
use crate::auth::gate::require_verified;
use crate::git::bare_repo_path;
use crate::repo::{
    file_assert_path_creatable, file_commit_changes, file_commit_message, file_resolve_base_branch,
    resolve_repo_for_owner_mutate, resolve_repo_for_read, AccessibleRepo,
};
use crate::rpc::RpcCtx;

/// Embedded vendored catalog (`assets/workflow-templates`).
static TEMPLATES: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/assets/workflow-templates");

/// Anonymous icon route prefix (nested under the actions HTTP router —
/// `GET /api/actions/template-icons/{name}.svg`).
pub const ICON_ROUTE_PREFIX: &str = "/api/actions/template-icons";

/// GitHub display order for the headline categories; the rest sort
/// alphabetically after these.
const CATEGORY_ORDER: &[&str] = &["Deployment", "Continuous integration", "Automation"];

const TEMPLATES_DEFAULT_PER_PAGE: u32 = 30;
const TEMPLATES_MAX_PER_PAGE: u32 = 100;

fn db_err(e: String) -> AppError {
    if e == "database not configured" {
        AppError::new(
            "db.not_configured",
            "no database configured for this instance",
        )
    } else {
        tracing::error!(error = %e, "workflow templates db error");
        AppError::new("repo.internal", "repository operation failed")
    }
}

fn bad_input(msg: impl Into<String>) -> AppError {
    AppError::new("rpc.bad_input", msg.into())
}

fn template_not_found() -> AppError {
    AppError::new("actions.template_not_found", "workflow template not found")
}

// ---------------------------------------------------------------------------
// Vendored builtin catalog
// ---------------------------------------------------------------------------

/// `properties.json` sidecar (GitHub camelCase fields).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TemplateProperties {
    name: Option<String>,
    #[serde(default)]
    description: String,
    #[serde(default)]
    icon_name: Option<String>,
    #[serde(default)]
    categories: Vec<String>,
    /// GitHub-style suggestion globs (rarely present upstream; our overlay
    /// in catalog.json supplies most of them).
    #[serde(default)]
    file_patterns: Vec<String>,
}

/// `catalog.json` — sync metadata + the hand-maintained `templates` overlay
/// (`file_patterns` keyed `dir/<file-stem>`).
#[derive(Debug, Deserialize)]
struct TemplateCatalog {
    #[serde(default)]
    templates: std::collections::HashMap<String, CatalogOverlay>,
}

#[derive(Debug, Deserialize)]
struct CatalogOverlay {
    #[serde(default)]
    file_patterns: Vec<String>,
}

fn catalog_overlay() -> TemplateCatalog {
    TEMPLATES
        .get_file("catalog.json")
        .and_then(|f| serde_json::from_str(f.contents_utf8()?).ok())
        .unwrap_or(TemplateCatalog {
            templates: std::collections::HashMap::new(),
        })
}

/// One resolved template with content — the internal join point for
/// builtin/instance/org before `WorkflowTemplateSummary` shaping.
struct ResolvedTemplate {
    id: String,
    name: String,
    description: String,
    icon_name: Option<String>,
    categories: Vec<String>,
    file_patterns: Vec<String>,
    filename: String,
    provenance: WorkflowTemplateProvenance,
    source_label: Option<String>,
    content: String,
}

fn icon_url(icon_name: Option<&str>) -> Option<String> {
    let name = icon_name?;
    if valid_icon_name(name) && TEMPLATES.get_file(format!("icons/{name}.svg")).is_some() {
        Some(format!("{ICON_ROUTE_PREFIX}/{name}.svg"))
    } else {
        None
    }
}

/// Distinct `uses:` refs the bundled runner cannot execute — anything not
/// `actions/checkout*` (Unit A runner supports local steps + checkout only).
fn unsupported_actions(doc: &WorkflowDocument) -> Vec<String> {
    let mut out = BTreeSet::new();
    for job in &doc.jobs {
        for step in &job.steps {
            if let Some(uses) = &step.uses {
                if uses != "actions/checkout" && !uses.starts_with("actions/checkout@") {
                    out.insert(uses.clone());
                }
            }
        }
    }
    out.into_iter().collect()
}

/// Compatibility classification for a template — the single source the
/// `catalog.json` `compat` ledger is checked against (Phase 0 conformance).
/// `missing_features` = structural features that force the validation-failure
/// path; `unsupported_actions` = non-checkout `uses:` refs the runner can't
/// execute yet; `warnings` = features that run degraded.
#[derive(Debug, Clone)]
pub struct TemplateCompat {
    pub status: &'static str,
    pub missing_features: Vec<String>,
    pub unsupported_actions: Vec<String>,
    pub warnings: Vec<String>,
    pub parse_error: Option<String>,
}

pub fn classify_template_content(content: &str) -> TemplateCompat {
    let substituted = content.replace("$default-branch", "main");
    match parse_workflow_yaml(substituted.as_bytes()) {
        Err(e) => TemplateCompat {
            status: "parse_error",
            missing_features: Vec::new(),
            unsupported_actions: Vec::new(),
            warnings: Vec::new(),
            parse_error: Some(e.message),
        },
        Ok(doc) => {
            let unsupported = unsupported_actions(&doc);
            let status = if doc.unsupported.iter().any(|u| u == "pull_request_target") {
                "restricted"
            } else if doc.unsupported.iter().any(|u| u == "reusable-workflow") {
                "reusable-workflow"
            } else if !doc.unsupported.is_empty() {
                "unsupported-runtime"
            } else if !doc.warnings.is_empty() || !unsupported.is_empty() {
                "partial"
            } else {
                "runnable"
            };
            TemplateCompat {
                status,
                missing_features: doc.unsupported,
                unsupported_actions: unsupported,
                warnings: doc.warnings,
                parse_error: None,
            }
        }
    }
}

fn to_summary(t: &ResolvedTemplate, root_entries: &[String]) -> WorkflowTemplateSummary {
    let compat = classify_template_content(&t.content);
    let (unsupported, parse_error) = (compat.unsupported_actions, compat.parse_error);
    WorkflowTemplateSummary {
        id: t.id.clone(),
        name: t.name.clone(),
        description: t.description.clone(),
        icon_name: t.icon_name.clone(),
        icon_url: icon_url(t.icon_name.as_deref()),
        categories: t.categories.clone(),
        file_patterns: t.file_patterns.clone(),
        filename: t.filename.clone(),
        provenance: t.provenance,
        source_label: t.source_label.clone(),
        suggested: t
            .file_patterns
            .iter()
            .any(|p| file_pattern_matches(p, root_entries)),
        unsupported_actions: unsupported,
        parse_error,
    }
}

/// Load every vendored template (`<dir>/<name>.yml` + sibling properties).
fn builtin_templates() -> Vec<ResolvedTemplate> {
    let overlay = catalog_overlay();
    let mut out = Vec::new();
    for dir in TEMPLATES.dirs() {
        let dir_name = dir.path().file_name().unwrap_or_default().to_string_lossy();
        for file in dir.files() {
            let Some(name) = file
                .path()
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
            else {
                continue;
            };
            if !is_workflow_filename(&name) {
                continue;
            }
            let stem = name
                .rsplit_once('.')
                .map(|(s, _)| s.to_string())
                .unwrap_or_default();
            let key = format!("{dir_name}/{stem}");
            // include_dir paths are absolute within the embedded root —
            // look the sidecar up via the root dir, not `dir.get_file`.
            let props = TEMPLATES
                .get_file(format!("{dir_name}/{stem}.properties.json"))
                .and_then(|f| f.contents_utf8())
                .and_then(|s| serde_json::from_str::<TemplateProperties>(s).ok())
                .unwrap_or(TemplateProperties {
                    name: None,
                    description: String::new(),
                    icon_name: None,
                    categories: Vec::new(),
                    file_patterns: Vec::new(),
                });
            let mut file_patterns = props.file_patterns.clone();
            if let Some(extra) = overlay.templates.get(&key) {
                file_patterns.extend(extra.file_patterns.iter().cloned());
            }
            let Some(content) = file.contents_utf8().map(str::to_string) else {
                continue;
            };
            out.push(ResolvedTemplate {
                id: format!("builtin:{key}"),
                name: props.name.unwrap_or(stem.clone()),
                description: props.description,
                icon_name: props.icon_name,
                categories: props.categories,
                file_patterns,
                filename: format!("{stem}.yml"),
                provenance: WorkflowTemplateProvenance::Builtin,
                source_label: None,
                content,
            });
        }
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

fn instance_templates(rows: &[InstanceWorkflowTemplateRow]) -> Vec<ResolvedTemplate> {
    rows.iter()
        .map(|r| ResolvedTemplate {
            id: format!("instance:{}", r.slug),
            name: r.name.clone(),
            description: r.description.clone(),
            icon_name: r.icon_name.clone(),
            categories: parse_string_list(&r.categories_json),
            file_patterns: parse_string_list(&r.file_patterns_json),
            filename: r.filename.clone(),
            provenance: WorkflowTemplateProvenance::Instance,
            source_label: Some(r.slug.clone()),
            content: r.content.clone(),
        })
        .collect()
}

fn parse_string_list(raw: &str) -> Vec<String> {
    serde_json::from_str(raw).unwrap_or_default()
}

/// Org templates from `<owner>/.github` → `workflow-templates/`. Best effort:
/// an absent or unreadable `.github` repo contributes nothing.
async fn org_templates(ctx: &RpcCtx, accessible: &AccessibleRepo) -> Vec<ResolvedTemplate> {
    if accessible.row.owner_type != "org" {
        return Vec::new();
    }
    let Ok(dot_github) = resolve_repo_for_read(ctx, &accessible.owner_username, ".github").await
    else {
        return Vec::new();
    };
    let Ok(bare) = bare_repo_path(
        &ctx.repos_dir,
        &dot_github.owner_username,
        &dot_github.row.name,
    ) else {
        return Vec::new();
    };
    let treeish = format!("refs/heads/{}", dot_github.row.default_branch);
    let entries = match ctx.git.ls_tree(&bare, &treeish, "workflow-templates").await {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };
    let source_label = Some(format!("@{}/.github", dot_github.owner_username));
    let mut out = Vec::new();
    for entry in entries {
        if entry.kind != oxidean_git::TreeEntryKind::Blob || !is_workflow_filename(&entry.name) {
            continue;
        }
        let stem = entry
            .name
            .rsplit_once('.')
            .map(|(s, _)| s.to_string())
            .unwrap_or_default();
        let Ok(content) = ctx
            .git
            .cat_blob(
                &bare,
                &treeish,
                &format!("workflow-templates/{}", entry.name),
            )
            .await
        else {
            continue;
        };
        let Ok(content) = String::from_utf8(content) else {
            continue;
        };
        let props = ctx
            .git
            .cat_blob(
                &bare,
                &treeish,
                &format!("workflow-templates/{stem}.properties.json"),
            )
            .await
            .ok()
            .and_then(|b| String::from_utf8(b).ok())
            .and_then(|s| serde_json::from_str::<TemplateProperties>(&s).ok())
            .unwrap_or(TemplateProperties {
                name: None,
                description: String::new(),
                icon_name: None,
                categories: Vec::new(),
                file_patterns: Vec::new(),
            });
        out.push(ResolvedTemplate {
            id: format!("org:{stem}"),
            name: props.name.unwrap_or(stem.clone()),
            description: props.description,
            icon_name: props.icon_name,
            categories: props.categories,
            file_patterns: props.file_patterns,
            filename: entry.name.clone(),
            provenance: WorkflowTemplateProvenance::Org,
            source_label: source_label.clone(),
            content,
        });
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

// ---------------------------------------------------------------------------
// Glob matching for suggested detection
// ---------------------------------------------------------------------------

/// `*` — any run of non-`/` chars; `?` — one non-`/` char. Byte-wise
/// backtracking match, case-sensitive (git paths are).
fn glob_match(pattern: &str, name: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let n: Vec<char> = name.chars().collect();
    let (mut pi, mut ni) = (0usize, 0usize);
    let (mut star_p, mut star_n) = (usize::MAX, 0usize);
    while ni < n.len() {
        if pi < p.len() && (p[pi] == '?' && n[ni] != '/' || p[pi] == n[ni]) {
            pi += 1;
            ni += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star_p = pi;
            star_n = ni;
            pi += 1;
        } else if star_p != usize::MAX {
            pi = star_p + 1;
            star_n += 1;
            ni = star_n;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

/// Whether a `file_patterns` entry matches any root-tree entry. A leading
/// `**/` anchors on the basename; patterns containing other `/` segments can
/// only match a root entry literally (root entries are single path segments).
fn file_pattern_matches(pattern: &str, root_entries: &[String]) -> bool {
    let pat = pattern.strip_prefix("**/").unwrap_or(pattern);
    root_entries.iter().any(|name| glob_match(pat, name))
}

// ---------------------------------------------------------------------------
// repo.actions.workflowTemplates.list / .get
// ---------------------------------------------------------------------------

async fn resolve_all_templates(
    ctx: &RpcCtx,
    accessible: &AccessibleRepo,
) -> Result<Vec<ResolvedTemplate>, AppError> {
    let mut all = builtin_templates();
    let instance_rows = ctx
        .db
        .list_instance_workflow_templates(true)
        .await
        .map_err(db_err)?;
    all.extend(instance_templates(&instance_rows));
    all.extend(org_templates(ctx, accessible).await);
    Ok(all)
}

/// `repo.actions.workflowTemplates.list` — ActionsRead.
pub async fn list_templates(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<WorkflowTemplatesListResponse, AppError> {
    let req: WorkflowTemplatesListRequest = serde_json::from_value(input).map_err(|e| {
        bad_input(format!(
            "invalid repo.actions.workflowTemplates.list input: {e}"
        ))
    })?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    let bare = bare_repo_path(
        &ctx.repos_dir,
        &accessible.owner_username,
        &accessible.row.name,
    )?;
    let treeish = resolve_treeish(
        ctx,
        &accessible.row.default_branch,
        req.git_ref.as_deref(),
        &bare,
    )
    .await?;
    let root_entries: Vec<String> = ctx
        .git
        .ls_tree(&bare, &treeish, "")
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "root tree listing failed");
            AppError::new("repo.internal", "repository operation failed")
        })?
        .iter()
        .map(|e| e.name.clone())
        .collect();

    let mut templates: Vec<WorkflowTemplateSummary> = resolve_all_templates(ctx, &accessible)
        .await?
        .iter()
        .map(|t| to_summary(t, &root_entries))
        .collect();

    // Filters: provenance, category (case-insensitive), name/description query.
    if let Some(source) = req.source.as_deref() {
        let want = match source.trim().to_ascii_lowercase().as_str() {
            "builtin" => WorkflowTemplateProvenance::Builtin,
            "instance" => WorkflowTemplateProvenance::Instance,
            "org" => WorkflowTemplateProvenance::Org,
            other => return Err(bad_input(format!("unknown template source: {other}"))),
        };
        templates.retain(|t| t.provenance == want);
    }
    if let Some(category) = req
        .category
        .as_deref()
        .map(str::trim)
        .filter(|c| !c.is_empty())
    {
        let want = category.to_ascii_lowercase();
        templates.retain(|t| t.categories.iter().any(|c| c.to_ascii_lowercase() == want));
    }
    if let Some(query) = req
        .query
        .as_deref()
        .map(str::trim)
        .filter(|q| !q.is_empty())
    {
        let needle = query.to_ascii_lowercase();
        templates.retain(|t| {
            t.name.to_ascii_lowercase().contains(&needle)
                || t.description.to_ascii_lowercase().contains(&needle)
        });
    }

    templates.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.id.cmp(&b.id)));

    let mut categories = BTreeSet::new();
    for t in &templates {
        for c in &t.categories {
            categories.insert(c.clone());
        }
    }
    let mut categories: Vec<String> = categories.into_iter().collect();
    categories.sort_by(|a, b| {
        let ka = CATEGORY_ORDER
            .iter()
            .position(|k| k.eq_ignore_ascii_case(a));
        let kb = CATEGORY_ORDER
            .iter()
            .position(|k| k.eq_ignore_ascii_case(b));
        match (ka, kb) {
            (Some(x), Some(y)) => x.cmp(&y),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => a.cmp(b),
        }
    });

    // Suggested subset and total are computed before pagination.
    let suggested: Vec<WorkflowTemplateSummary> =
        templates.iter().filter(|t| t.suggested).cloned().collect();
    let total_count = templates.len() as u64;

    // Pagination is opt-in: the catalog page renders every template grouped by
    // category, so a bare request returns the whole filtered list.
    let templates = if req.page.is_some() || req.per_page.is_some() {
        let page = req.page.unwrap_or(1).max(1);
        let per_page = req
            .per_page
            .unwrap_or(TEMPLATES_DEFAULT_PER_PAGE)
            .clamp(1, TEMPLATES_MAX_PER_PAGE);
        let start = ((page - 1) * per_page) as usize;
        templates
            .into_iter()
            .skip(start)
            .take(per_page as usize)
            .collect()
    } else {
        templates
    };

    Ok(WorkflowTemplatesListResponse {
        suggested,
        templates,
        categories,
        total_count,
    })
}

/// Resolve a catalog id to its resolved template.
async fn resolve_template_id(
    ctx: &RpcCtx,
    accessible: &AccessibleRepo,
    id: &str,
) -> Result<ResolvedTemplate, AppError> {
    let all = resolve_all_templates(ctx, accessible).await?;
    all.into_iter()
        .find(|t| t.id == id)
        .ok_or_else(template_not_found)
}

/// `repo.actions.workflowTemplates.get` — ActionsRead; `$default-branch`
/// substituted with the repo's default branch.
pub async fn get_template(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<WorkflowTemplateGetResponse, AppError> {
    let req: WorkflowTemplateGetRequest = serde_json::from_value(input).map_err(|e| {
        bad_input(format!(
            "invalid repo.actions.workflowTemplates.get input: {e}"
        ))
    })?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    let template = resolve_template_id(ctx, &accessible, req.id.trim()).await?;
    let content = template
        .content
        .replace("$default-branch", &accessible.row.default_branch);
    let summary = to_summary(&template, &[]);
    Ok(WorkflowTemplateGetResponse {
        template: summary,
        content,
    })
}

// ---------------------------------------------------------------------------
// repo.actions.workflows.create
// ---------------------------------------------------------------------------

/// `repo.actions.workflows.create` — ActionsWrite + repo Write. Drafts a
/// workflow file from a catalog template or authored content, onto a branch
/// or an open same-repo PR head.
pub async fn create_workflow(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<RepoFileCommitResponse, AppError> {
    let user = require_verified(ctx).await?;
    let req: WorkflowCreateRequest = serde_json::from_value(input)
        .map_err(|e| bad_input(format!("invalid repo.actions.workflows.create input: {e}")))?;
    let accessible = resolve_repo_for_owner_mutate(ctx, &req.owner, &req.name).await?;

    let has_template = req
        .template_id
        .as_deref()
        .map(str::trim)
        .is_some_and(|s| !s.is_empty());
    let has_content = req.content.is_some();
    if has_template == has_content {
        return Err(bad_input(
            "exactly one of template_id and content is required",
        ));
    }

    let (content, default_filename) = if has_template {
        let template = resolve_template_id(
            ctx,
            &accessible,
            req.template_id.as_deref().unwrap_or_default(),
        )
        .await?;
        let content = template
            .content
            .replace("$default-branch", &accessible.row.default_branch);
        (content, Some(template.filename))
    } else {
        (req.content.clone().unwrap_or_default(), None)
    };

    if content.len() > crate::actions::workflow::MAX_WORKFLOW_BYTES {
        return Err(bad_input(format!(
            "workflow content exceeds {} bytes",
            crate::actions::workflow::MAX_WORKFLOW_BYTES
        )));
    }

    let path = match req.path.as_deref().map(str::trim).filter(|p| !p.is_empty()) {
        Some(p) => p.to_string(),
        None => match default_filename {
            Some(f) => format!(".github/workflows/{f}"),
            None => return Err(bad_input("path is required when creating from content")),
        },
    };
    let path = confine_workflow_path(&path).map_err(|e| {
        AppError::new(
            "actions.invalid_workflow_path",
            format!("workflow path must be .github/workflows/<name>.yml|yaml: {e}"),
        )
    })?;

    if let Err(e) = parse_workflow_yaml(content.as_bytes()) {
        return Err(AppError::new(
            "actions.invalid_workflow",
            format!("workflow does not parse: {}", e.message),
        ));
    }

    let filename = path
        .rsplit('/')
        .next()
        .unwrap_or("workflow.yml")
        .to_string();
    let message = file_commit_message(
        req.message.as_deref().unwrap_or_default(),
        &format!("ci: add {filename} workflow"),
    )?;

    let mut target: RepoFileCommitOptions = req.target;
    if let Some(number) = req.pull_number {
        if number < 1 {
            return Err(AppError::new("pull.not_found", "Pull request not found"));
        }
        let pr = ctx
            .db
            .find_pull_by_repo_number(&accessible.row.id, number)
            .await
            .map_err(db_err)?
            .ok_or_else(|| AppError::new("pull.not_found", "Pull request not found"))?;
        if pr.state != "open" {
            return Err(AppError::new(
                "actions.pull_not_open",
                "pull request is not open",
            ));
        }
        if pr.head_repo_id != accessible.row.id {
            return Err(AppError::new(
                "actions.pull_from_fork",
                "cannot commit a workflow to a pull request from a fork",
            ));
        }
        target.branch = Some(pr.head_ref.clone());
        target.new_branch = None;
        target.open_pr = Some(false);
    }

    let base_branch = file_resolve_base_branch(&accessible, &target)?;
    let bare = bare_repo_path(
        &ctx.repos_dir,
        &accessible.owner_username,
        &accessible.row.name,
    )?;
    file_assert_path_creatable(ctx, &bare, &base_branch, &path).await?;
    file_commit_changes(
        ctx,
        &accessible,
        &user,
        &base_branch,
        &target,
        &message,
        vec![FileChange::Upsert {
            path,
            content: content.into_bytes(),
        }],
    )
    .await
}

// ---------------------------------------------------------------------------
// admin.workflowTemplates.*
// ---------------------------------------------------------------------------

fn template_public(row: &InstanceWorkflowTemplateRow) -> InstanceWorkflowTemplatePublic {
    InstanceWorkflowTemplatePublic {
        id: row.id.clone(),
        slug: row.slug.clone(),
        name: row.name.clone(),
        description: row.description.clone(),
        icon_name: row.icon_name.clone(),
        categories: parse_string_list(&row.categories_json),
        file_patterns: parse_string_list(&row.file_patterns_json),
        filename: row.filename.clone(),
        content: row.content.clone(),
        enabled: row.enabled,
        created_by_user_id: row.created_by_user_id.clone(),
        created_at: row.created_at.clone(),
        updated_at: row.updated_at.clone(),
    }
}

/// Slug rule shared with `create_pack_from_bytes`: trimmed, non-empty, ascii
/// alnum / `-` / `_`.
fn validate_slug(slug: &str) -> Result<&str, AppError> {
    let slug = slug.trim();
    if slug.is_empty()
        || !slug
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(AppError::new(
            "admin.invalid_template_slug",
            "Slug must be ascii alphanumeric with hyphen/underscore.",
        ));
    }
    Ok(slug)
}

fn validate_template_content(content: &str) -> Result<(), AppError> {
    parse_workflow_yaml(content.as_bytes())
        .map(|_| ())
        .map_err(|e| {
            AppError::new(
                "actions.invalid_workflow",
                format!("workflow does not parse: {}", e.message),
            )
        })
}

fn validate_filename(filename: &str) -> Result<&str, AppError> {
    let f = filename.trim();
    if f.is_empty() || f.contains('/') || f.contains('\\') || f.contains("..") {
        return Err(bad_input("filename must be a flat file name"));
    }
    if !is_workflow_filename(f) {
        return Err(bad_input("filename must end in .yml or .yaml"));
    }
    Ok(f)
}

/// `admin.workflowTemplates.list`
pub async fn admin_list(ctx: &RpcCtx) -> Result<AdminWorkflowTemplatesListResponse, AppError> {
    require_admin_user(ctx).await?;
    let rows = ctx
        .db
        .list_instance_workflow_templates(false)
        .await
        .map_err(db_err)?;
    Ok(AdminWorkflowTemplatesListResponse {
        templates: rows.iter().map(template_public).collect(),
    })
}

/// `admin.workflowTemplates.create`
pub async fn admin_create(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<InstanceWorkflowTemplatePublic, AppError> {
    let admin = require_admin_user(ctx).await?;
    let req: AdminWorkflowTemplateCreateRequest = serde_json::from_value(input)
        .map_err(|e| bad_input(format!("invalid admin.workflowTemplates.create input: {e}")))?;
    let slug = validate_slug(&req.slug)?.to_string();
    let name = req.name.trim();
    if name.is_empty() {
        return Err(bad_input("name is required"));
    }
    let filename = match req.filename.as_deref() {
        Some(f) => validate_filename(f)?.to_string(),
        None => format!("{slug}.yml"),
    };
    validate_template_content(&req.content)?;
    if ctx
        .db
        .find_instance_workflow_template_by_slug(&slug)
        .await
        .map_err(db_err)?
        .is_some()
    {
        return Err(AppError::new(
            "admin.template_slug_taken",
            "A workflow template with this slug already exists.",
        ));
    }
    let id = Uuid::new_v4().to_string();
    let row = ctx
        .db
        .insert_instance_workflow_template(
            &id,
            &slug,
            name,
            req.description.trim(),
            req.icon_name.as_deref(),
            &serde_json::to_string(&req.categories).unwrap_or_else(|_| "[]".into()),
            &serde_json::to_string(&req.file_patterns).unwrap_or_else(|_| "[]".into()),
            &filename,
            &req.content,
            &admin.id,
        )
        .await
        .map_err(db_err)?;
    Ok(template_public(&row))
}

/// `admin.workflowTemplates.update`
pub async fn admin_update(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<InstanceWorkflowTemplatePublic, AppError> {
    require_admin_user(ctx).await?;
    let req: AdminWorkflowTemplateUpdateRequest = serde_json::from_value(input)
        .map_err(|e| bad_input(format!("invalid admin.workflowTemplates.update input: {e}")))?;
    let current = ctx
        .db
        .find_instance_workflow_template_by_id(&req.id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("admin.template_not_found", "Template not found."))?;

    let name = match req.name {
        Some(n) if !n.trim().is_empty() => n.trim().to_string(),
        Some(_) => return Err(bad_input("name must not be empty")),
        None => current.name.clone(),
    };
    let description = req
        .description
        .map(|d| d.trim().to_string())
        .unwrap_or_else(|| current.description.clone());
    let icon_name = match req.icon_name {
        Some(v) => v,
        None => current.icon_name.clone(),
    };
    let categories = req
        .categories
        .map(|c| serde_json::to_string(&c).unwrap_or_else(|_| "[]".into()))
        .unwrap_or_else(|| current.categories_json.clone());
    let file_patterns = req
        .file_patterns
        .map(|c| serde_json::to_string(&c).unwrap_or_else(|_| "[]".into()))
        .unwrap_or_else(|| current.file_patterns_json.clone());
    let filename = match req.filename.as_deref() {
        Some(f) => validate_filename(f)?.to_string(),
        None => current.filename.clone(),
    };
    let content = req.content.unwrap_or_else(|| current.content.clone());
    validate_template_content(&content)?;

    ctx.db
        .update_instance_workflow_template(
            &req.id,
            &name,
            &description,
            icon_name.as_deref(),
            &categories,
            &file_patterns,
            &filename,
            &content,
        )
        .await
        .map_err(db_err)?;
    let row = ctx
        .db
        .find_instance_workflow_template_by_id(&req.id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("admin.template_not_found", "Template not found."))?;
    Ok(template_public(&row))
}

/// `admin.workflowTemplates.setEnabled`
pub async fn admin_set_enabled(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<InstanceWorkflowTemplatePublic, AppError> {
    require_admin_user(ctx).await?;
    let req: AdminWorkflowTemplateSetEnabledRequest =
        serde_json::from_value(input).map_err(|e| {
            bad_input(format!(
                "invalid admin.workflowTemplates.setEnabled input: {e}"
            ))
        })?;
    ctx.db
        .find_instance_workflow_template_by_id(&req.id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("admin.template_not_found", "Template not found."))?;
    ctx.db
        .set_instance_workflow_template_enabled(&req.id, req.enabled)
        .await
        .map_err(db_err)?;
    let row = ctx
        .db
        .find_instance_workflow_template_by_id(&req.id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("admin.template_not_found", "Template not found."))?;
    Ok(template_public(&row))
}

/// `admin.workflowTemplates.delete`
pub async fn admin_delete(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    require_admin_user(ctx).await?;
    let req: AdminWorkflowTemplateDeleteRequest = serde_json::from_value(input)
        .map_err(|e| bad_input(format!("invalid admin.workflowTemplates.delete input: {e}")))?;
    ctx.db
        .find_instance_workflow_template_by_id(&req.id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("admin.template_not_found", "Template not found."))?;
    ctx.db
        .delete_instance_workflow_template(&req.id)
        .await
        .map_err(db_err)?;
    Ok(serde_json::json!({ "ok": true }))
}

// ---------------------------------------------------------------------------
// Icon route (anonymous GET)
// ---------------------------------------------------------------------------

/// `^[a-z0-9][a-z0-9_-]*$` — no regex dep; icons are lowercase-slug svgs.
fn valid_icon_name(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() || c.is_ascii_digit() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
}

/// `GET /api/actions/template-icons/{file}` where `{file}` is `<name>.svg`.
/// `{name}` captures the whole segment (matchit has no partial-segment
/// params) — the `.svg` suffix is validated here.
pub async fn template_icon(
    axum::extract::Path(file): axum::extract::Path<String>,
) -> axum::response::Response {
    let Some(name) = file.strip_suffix(".svg") else {
        return axum::http::StatusCode::NOT_FOUND.into_response();
    };
    let svg = if valid_icon_name(name) {
        TEMPLATES
            .get_file(format!("icons/{name}.svg"))
            .map(|f| f.contents())
    } else {
        None
    };
    match svg {
        Some(bytes) => (
            [
                (axum::http::header::CONTENT_TYPE, "image/svg+xml"),
                (axum::http::header::CACHE_CONTROL, "public, max-age=86400"),
            ],
            bytes,
        )
            .into_response(),
        None => axum::http::StatusCode::NOT_FOUND.into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glob_match_basics() {
        assert!(glob_match("Cargo.toml", "Cargo.toml"));
        assert!(!glob_match("Cargo.toml", "cargo.toml"));
        assert!(glob_match("*.toml", "Cargo.toml"));
        assert!(glob_match("*.py", "setup.py"));
        assert!(!glob_match("*.py", "setup.pyx"));
        assert!(glob_match("?", "x"));
        assert!(!glob_match("?", "xy"));
        assert!(glob_match("build.gradle*", "build.gradle.kts"));
        assert!(!glob_match("foo/*", "foo")); // '*' never crosses '/'
        assert!(glob_match("foo/*.txt", "foo/a.txt"));
    }

    #[test]
    fn file_pattern_matches_root_entries() {
        let entries = vec![
            "Cargo.toml".to_string(),
            "src".to_string(),
            "README.md".to_string(),
        ];
        assert!(file_pattern_matches("Cargo.toml", &entries));
        assert!(file_pattern_matches("**/Cargo.toml", &entries));
        assert!(!file_pattern_matches("package.json", &entries));
        assert!(file_pattern_matches("*.toml", &entries));
    }

    #[test]
    fn valid_icon_name_rules() {
        assert!(valid_icon_name("rust"));
        assert!(valid_icon_name("azure-staticwebapp"));
        assert!(valid_icon_name("c-cpp"));
        assert!(!valid_icon_name("octicon clock"));
        assert!(!valid_icon_name("Rust"));
        assert!(!valid_icon_name("-rust"));
        assert!(!valid_icon_name("../evil"));
        assert!(!valid_icon_name(""));
    }

    /// Every vendored template must have a properties sidecar; parse each
    /// workflow (with `$default-branch` → `main`) and report failures instead
    /// of failing the build — upstream syntax we do not support becomes
    /// `parse_error` in the catalog response.
    #[test]
    fn vendored_templates_load_and_report_parse() {
        let templates = builtin_templates();
        assert!(!templates.is_empty(), "no vendored templates found");
        let mut parsed = 0usize;
        let mut failures = Vec::new();
        for t in &templates {
            assert!(t.id.starts_with("builtin:"));
            assert!(t.filename.ends_with(".yml"));
            let content = t.content.replace("$default-branch", "main");
            match parse_workflow_yaml(content.as_bytes()) {
                Ok(_) => parsed += 1,
                Err(e) => failures.push(format!("{}: {}", t.id, e.message)),
            }
        }
        eprintln!(
            "vendored parse report: {parsed} ok, {} failed",
            failures.len()
        );
        for f in &failures {
            eprintln!("  {f}");
        }
        // The catalog round-trips: count matches the files on disk.
        let on_disk = TEMPLATES
            .dirs()
            .flat_map(|d| d.files().collect::<Vec<_>>())
            .filter(|f| {
                f.path()
                    .file_name()
                    .is_some_and(|n| is_workflow_filename(&n.to_string_lossy()))
            })
            .count();
        assert_eq!(templates.len(), on_disk);
        assert!(parsed > 0);
    }
}
