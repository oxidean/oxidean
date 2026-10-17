//! Human-readable rendering for `data` payloads. `--json` bypasses all of
//! this and prints the raw `{ok, data|error}` envelope instead, so renderers
//! only ever see success payloads.

use serde_json::{json, Value};

/// Field access helpers — payloads are dynamic (`serde_json::Value`) because
/// the CLI doesn't mirror the generated DTO types.
pub fn s<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(Value::as_str)
}

fn n(v: &Value, key: &str) -> Option<i64> {
    v.get(key).and_then(Value::as_i64)
}

fn b(v: &Value, key: &str) -> bool {
    v.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn arr<'a>(v: &'a Value, key: &str) -> &'a [Value] {
    v.get(key)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

pub fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max).collect();
    out.push('…');
    out
}

fn num_display(v: Option<i64>) -> String {
    v.map(|x| format!("#{x}"))
        .unwrap_or_else(|| "#?".to_string())
}

fn labels(v: &Value) -> String {
    arr(v, "labels")
        .iter()
        .filter_map(|l| s(l, "name"))
        .collect::<Vec<_>>()
        .join(",")
}

fn join_nonempty(parts: Vec<String>) -> String {
    parts
        .into_iter()
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("  ")
}

/// `repo.listMine` / `repo.listByOwner` — `data.repos[]`.
pub fn repo_list(data: &Value) -> String {
    arr(data, "repos")
        .iter()
        .map(|r| {
            join_nonempty(vec![
                format!(
                    "{}/{}",
                    s(r, "owner_username").unwrap_or("?"),
                    s(r, "name").unwrap_or("?")
                ),
                s(r, "visibility").unwrap_or_default().to_string(),
                s(r, "description").unwrap_or_default().to_string(),
            ])
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// `repo.get`.
pub fn repo_view(data: &Value) -> String {
    let mut out = vec![format!(
        "{}/{}",
        s(data, "owner_username").unwrap_or("?"),
        s(data, "name").unwrap_or("?")
    )];
    if let Some(desc) = s(data, "description").filter(|d| !d.is_empty()) {
        out.push(desc.to_string());
    }
    let mut meta = vec![s(data, "visibility").unwrap_or("unknown").to_string()];
    if let Some(branch) = s(data, "default_branch").filter(|x| !x.is_empty()) {
        meta.push(format!("default branch: {branch}"));
    }
    if let Some(home) = s(data, "homepage").filter(|x| !x.is_empty()) {
        meta.push(home.to_string());
    }
    if let Some(topics) = data
        .get("topics")
        .and_then(Value::as_array)
        .map(|t| {
            t.iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(", ")
        })
        .filter(|t| !t.is_empty())
    {
        meta.push(topics);
    }
    out.push(meta.join(" · "));
    let mut counts = Vec::new();
    if let Some(v) = n(data, "open_issue_count") {
        counts.push(format!("{v} open issues"));
    }
    if let Some(v) = n(data, "open_pull_count") {
        counts.push(format!("{v} open pull requests"));
    }
    if let Some(v) = n(data, "star_count") {
        counts.push(format!("{v} stars"));
    }
    if let Some(v) = n(data, "fork_count") {
        counts.push(format!("{v} forks"));
    }
    if !counts.is_empty() {
        out.push(counts.join(" · "));
    }
    if let Some(t) = s(data, "updated_at") {
        out.push(format!("updated {t}"));
    }
    out.join("\n")
}

/// `repo.createDefaults` — template catalog grouped by `group`, then the
/// gitignore catalog and the instance default visibility. Stack ids are the
/// refs `--template` accepts.
pub fn repo_templates(data: &Value) -> String {
    let mut out: Vec<String> = Vec::new();
    let stacks = arr(data, "stacks");
    if stacks.is_empty() {
        out.push("No templates on this instance.".into());
    } else {
        let mut groups: Vec<(String, Vec<&Value>)> = Vec::new();
        for t in stacks {
            let g = s(t, "group").unwrap_or("Other").to_string();
            match groups.iter_mut().find(|(name, _)| *name == g) {
                Some((_, items)) => items.push(t),
                None => groups.push((g, vec![t])),
            }
        }
        out.push("Templates:".into());
        for (g, items) in groups {
            out.push(format!("  {g}"));
            for t in items {
                let ref_ = s(t, "source_label").or_else(|| s(t, "id")).unwrap_or("?");
                let prov = s(t, "provenance").unwrap_or("builtin");
                let desc = s(t, "description").unwrap_or_default();
                let label = s(t, "label").unwrap_or_default();
                let title = if label.is_empty() || label == ref_ {
                    ref_.to_string()
                } else {
                    format!("{ref_} — {label}")
                };
                out.push(join_nonempty(vec![
                    format!("    {title}"),
                    format!("[{prov}]"),
                    desc.to_string(),
                ]));
            }
        }
    }
    let gitignores = arr(data, "gitignores")
        .iter()
        .filter_map(|g| s(g, "id"))
        .collect::<Vec<_>>();
    if !gitignores.is_empty() {
        out.push(format!("Gitignores: {}", gitignores.join(", ")));
    }
    if let Some(v) = s(data, "default_visibility") {
        out.push(format!("Default visibility: {v}"));
    }
    out.join("\n")
}

/// Shared row for `issue.list` / `pull.list` items (numbers + titles).
fn numbered_row(item: &Value) -> String {
    join_nonempty(vec![
        num_display(n(item, "number")),
        s(item, "title").unwrap_or_default().to_string(),
        labels(item),
    ])
}

/// `issue.list` — `data.issues[]` + `total`.
pub fn issue_list(data: &Value) -> String {
    arr(data, "issues")
        .iter()
        .map(numbered_row)
        .collect::<Vec<_>>()
        .join("\n")
}

/// `issue.get` / `issue.create` — the issue itself.
pub fn issue_view(data: &Value) -> String {
    let mut out = vec![join_nonempty(vec![
        num_display(n(data, "number")),
        s(data, "title").unwrap_or_default().to_string(),
    ])];
    let mut meta = vec![s(data, "state").unwrap_or("unknown").to_string()];
    if let Some(author) = s(data, "author_username") {
        meta.push(format!("opened by {author}"));
    }
    if let Some(l) = labels(data).into_option_string() {
        meta.push(l);
    }
    if let Some(t) = s(data, "created_at") {
        meta.push(t.to_string());
    }
    out.push(meta.join(" · "));
    if let Some(body) = s(data, "body").filter(|x| !x.is_empty()) {
        out.push(String::new());
        out.push(crate::markdown::render(body));
    }
    out.join("\n")
}

/// `pull.list` — `data.pulls[]` + `total`.
pub fn pull_list(data: &Value) -> String {
    arr(data, "pulls")
        .iter()
        .map(|p| {
            join_nonempty(vec![
                num_display(n(p, "number")),
                s(p, "title").unwrap_or_default().to_string(),
                s(p, "head_ref").unwrap_or_default().to_string(),
                if b(p, "draft") {
                    "draft".to_string()
                } else {
                    String::new()
                },
            ])
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// `pull.get`.
pub fn pull_view(data: &Value) -> String {
    let mut out = vec![join_nonempty(vec![
        num_display(n(data, "number")),
        s(data, "title").unwrap_or_default().to_string(),
        if b(data, "draft") {
            "(draft)".to_string()
        } else {
            String::new()
        },
    ])];
    let mut meta = vec![s(data, "state").unwrap_or("unknown").to_string()];
    let head = s(data, "head_ref").unwrap_or("?");
    let base = s(data, "base_ref").unwrap_or("?");
    meta.push(format!("{head} → {base}"));
    if let Some(author) = s(data, "author_username") {
        meta.push(format!("by {author}"));
    }
    if let Some(t) = s(data, "created_at") {
        meta.push(t.to_string());
    }
    out.push(meta.join(" · "));
    if let Some(merge_sha) = s(data, "merge_commit_sha").filter(|x| !x.is_empty()) {
        out.push(format!("merged: {merge_sha}"));
    }
    if let Some(body) = s(data, "body").filter(|x| !x.is_empty()) {
        out.push(String::new());
        out.push(crate::markdown::render(body));
    }
    out.join("\n")
}

/// `repo.commitStatus.list` — `data.statuses[]` (resolved from `pull.get`'s
/// head sha by the caller).
pub fn checks(data: &Value) -> String {
    arr(data, "statuses")
        .iter()
        .map(|c| {
            join_nonempty(vec![
                s(c, "state").unwrap_or_default().to_string(),
                s(c, "context").unwrap_or_default().to_string(),
                s(c, "description").unwrap_or_default().to_string(),
            ])
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// `repo.actions.listRuns` — `data.runs[]`.
pub fn run_list(data: &Value) -> String {
    arr(data, "runs")
        .iter()
        .map(|r| {
            join_nonempty(vec![
                s(r, "id").unwrap_or_default().to_string(),
                s(r, "workflow_name").unwrap_or_default().to_string(),
                s(r, "event").unwrap_or_default().to_string(),
                s(r, "status").unwrap_or_default().to_string(),
                s(r, "head_ref").unwrap_or_default().to_string(),
            ])
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// `repo.actions.getRun` — `data.run` + `data.jobs[]`.
pub fn run_view(data: &Value) -> String {
    let run = data.get("run").cloned().unwrap_or(Value::Null);
    let mut out = vec![join_nonempty(vec![
        s(&run, "workflow_name").unwrap_or_default().to_string(),
        s(&run, "title").unwrap_or_default().to_string(),
    ])];
    let mut meta = vec![s(&run, "status").unwrap_or("unknown").to_string()];
    if let Some(e) = s(&run, "event") {
        meta.push(e.to_string());
    }
    if let Some(r) = s(&run, "head_ref") {
        meta.push(r.to_string());
    }
    if let Some(sha) = s(&run, "head_sha") {
        meta.push(truncate(sha, 7));
    }
    if let Some(id) = s(&run, "id") {
        meta.push(format!("id: {id}"));
    }
    out.push(meta.join(" · "));
    let jobs = arr(data, "jobs");
    if !jobs.is_empty() {
        out.push(String::new());
        out.push("jobs:".to_string());
        for j in jobs {
            out.push(format!(
                "  {}",
                join_nonempty(vec![
                    s(j, "status").unwrap_or_default().to_string(),
                    s(j, "name")
                        .or_else(|| s(j, "job_key"))
                        .unwrap_or_default()
                        .to_string(),
                ])
            ));
        }
    }
    out.join("\n")
}

/// `packages.list` — `data.packages[]`.
pub fn pkg_list(data: &Value) -> String {
    arr(data, "packages")
        .iter()
        .map(|p| {
            let versions = arr(p, "versions").len();
            join_nonempty(vec![
                s(p, "format").unwrap_or_default().to_string(),
                s(p, "name").unwrap_or_default().to_string(),
                s(p, "visibility").unwrap_or_default().to_string(),
                format!("{versions} version{}", if versions == 1 { "" } else { "s" }),
            ])
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// `auth.me` — `Logged in to <instance> as <username>`.
pub fn auth_status(data: &Value, instance: &str) -> String {
    let user = s(data, "username").unwrap_or("?");
    let display = s(data, "display_name").unwrap_or("");
    let mut out = format!("{instance}\n  Logged in as {user}");
    if !display.is_empty() && display != user {
        out.push_str(&format!(" ({display})"));
    }
    out
}

/// Mutations that return `{ok: true}` or a bare ack — a one-line confirmation.
pub fn generic(data: &Value) -> String {
    if let Some(m) = s(data, "message") {
        return m.to_string();
    }
    if let Some(u) = s(data, "url") {
        return u.to_string();
    }
    if b(data, "ok") || data.is_null() || data == &json!({}) {
        return "ok".to_string();
    }
    // Non-trivial payload — print it pretty so nothing is hidden in text mode.
    serde_json::to_string_pretty(data).unwrap_or_else(|_| "ok".to_string())
}

/// `repo.deployKey.list` — `data.deploy_keys[]`.
pub fn deploy_key_list(data: &Value) -> String {
    arr(data, "deploy_keys")
        .iter()
        .map(|k| {
            join_nonempty(vec![
                s(k, "id").unwrap_or_default().to_string(),
                s(k, "title").unwrap_or_default().to_string(),
                if b(k, "can_write") { "write" } else { "read" }.to_string(),
            ])
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// `repo.collaborators.list` — `data.collaborators[]`.
pub fn collaborator_list(data: &Value) -> String {
    arr(data, "collaborators")
        .iter()
        .map(|c| {
            join_nonempty(vec![
                s(c, "username").unwrap_or_default().to_string(),
                s(c, "permission").unwrap_or_default().to_string(),
            ])
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// `{repo,org,admin}.invites.list` — `data.invites[]`.
pub fn invite_list(data: &Value) -> String {
    arr(data, "invites")
        .iter()
        .map(|i| {
            join_nonempty(vec![
                s(i, "id").unwrap_or_default().to_string(),
                s(i, "email")
                    .map(str::to_string)
                    .or_else(|| s(i, "url").map(str::to_string))
                    .or_else(|| s(i, "token").map(|t| format!("link:{t}")))
                    .unwrap_or_default(),
                s(i, "role")
                    .or_else(|| s(i, "permission"))
                    .unwrap_or_default()
                    .to_string(),
                s(i, "state").unwrap_or_default().to_string(),
            ])
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// `*.invites.create` — per-email results.
pub fn invite_create_result(data: &Value) -> String {
    let rows: Vec<String> = arr(data, "results")
        .iter()
        .map(|r| {
            join_nonempty(vec![
                s(r, "email").unwrap_or_default().to_string(),
                s(r, "state")
                    .or_else(|| s(r, "status"))
                    .unwrap_or_default()
                    .to_string(),
            ])
        })
        .collect();
    if rows.is_empty() {
        return generic(data);
    }
    rows.join("\n")
}

/// `*.invites.createLink` — the link URL/token is the point; print it plainly.
pub fn invite_link_result(data: &Value) -> String {
    if let Some(u) = s(data, "url").or_else(|| s(data, "link")) {
        return u.to_string();
    }
    if let Some(t) = s(data, "token") {
        return t.to_string();
    }
    generic(data)
}

/// `repo.branchProtection.list` — `data.rules[]`.
pub fn protection_rules(data: &Value) -> String {
    arr(data, "rules")
        .iter()
        .map(|r| {
            join_nonempty(vec![
                s(r, "id").unwrap_or_default().to_string(),
                s(r, "pattern").unwrap_or_default().to_string(),
                if b(r, "require_reviews") {
                    format!(
                        "reviews:{}",
                        n(r, "required_approving_review_count").unwrap_or(1)
                    )
                } else {
                    String::new()
                },
                arr(r, "required_status_contexts")
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(","),
            ])
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// `repo.tagProtection.list` — `data.rules[]`.
pub fn tag_protection_rules(data: &Value) -> String {
    arr(data, "rules")
        .iter()
        .map(|r| {
            let mut perms = Vec::new();
            for (k, label) in [
                ("allow_create", "create"),
                ("allow_update", "update"),
                ("allow_delete", "delete"),
            ] {
                if !b(r, k) {
                    perms.push(format!("no-{label}"));
                }
            }
            join_nonempty(vec![
                s(r, "id").unwrap_or_default().to_string(),
                s(r, "pattern").unwrap_or_default().to_string(),
                perms.join(","),
            ])
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// `repo.mirror.get` / `upsert` — mirror config echo.
pub fn mirror(data: &Value) -> String {
    let m = data.get("mirror").cloned().unwrap_or_else(|| data.clone());
    if m.is_null() {
        return "no mirror configured".to_string();
    }
    let mut out = Vec::new();
    if let Some(u) = s(&m, "remote_url") {
        out.push(u.to_string());
    }
    let meta = join_nonempty(vec![
        if let Some(e) = m.get("enabled").and_then(Value::as_bool) {
            if e { "enabled" } else { "disabled" }.to_string()
        } else {
            String::new()
        },
        s(&m, "sync_mode").unwrap_or_default().to_string(),
        n(&m, "poll_interval_secs")
            .map(|i| format!("every {i}s"))
            .unwrap_or_default(),
        s(&m, "last_sync_status")
            .or_else(|| s(&m, "status"))
            .unwrap_or_default()
            .to_string(),
    ]);
    if !meta.is_empty() {
        out.push(meta);
    }
    if let Some(t) = s(&m, "last_synced_at") {
        out.push(format!("last synced {t}"));
    }
    if out.is_empty() {
        return generic(data);
    }
    out.join("\n")
}

/// `repo.mergeSettings.get`/`update` — allowed merge strategies.
pub fn merge_settings(data: &Value) -> String {
    let m = data
        .get("settings")
        .cloned()
        .unwrap_or_else(|| data.clone());
    let mut on = Vec::new();
    for (k, label) in [
        ("allow_merge_commit", "merge"),
        ("allow_squash_merge", "squash"),
        ("allow_rebase_merge", "rebase"),
    ] {
        if b(&m, k) {
            on.push(label);
        }
    }
    if on.is_empty() {
        "no merge strategies allowed".to_string()
    } else {
        format!("allowed: {}", on.join(", "))
    }
}

/// `repo.lfs.listObjects` — `data.objects[]`.
pub fn lfs_objects(data: &Value) -> String {
    arr(data, "objects")
        .iter()
        .map(|o| {
            join_nonempty(vec![
                s(o, "oid").unwrap_or_default().to_string(),
                s(o, "path").unwrap_or_default().to_string(),
                n(o, "size").map(|b| format!("{b}B")).unwrap_or_default(),
            ])
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// `pull.files` as unified patches (`ox pr diff`).
pub fn diff(data: &Value) -> String {
    arr(data, "files")
        .iter()
        .map(|f| {
            let path = s(f, "new_path")
                .or_else(|| s(f, "path"))
                .or_else(|| s(f, "old_path"))
                .unwrap_or("?");
            let patch = s(f, "patch").unwrap_or_default();
            if patch.is_empty() {
                format!("diff {path} (binary or too large)")
            } else {
                format!("diff {path}\n{patch}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// `pull.files` as a name+status list (`ox pr files`).
pub fn file_list(data: &Value) -> String {
    arr(data, "files")
        .iter()
        .map(|f| {
            let path = s(f, "new_path")
                .or_else(|| s(f, "path"))
                .or_else(|| s(f, "old_path"))
                .unwrap_or("?");
            join_nonempty(vec![
                s(f, "status").unwrap_or_default().to_string(),
                path.to_string(),
                n(f, "additions")
                    .map(|a| format!("+{a}"))
                    .unwrap_or_default(),
                n(f, "deletions")
                    .map(|d| format!("-{d}"))
                    .unwrap_or_default(),
            ])
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// `pull.reviews.list` — `data.reviews[]`.
pub fn review_list(data: &Value) -> String {
    arr(data, "reviews")
        .iter()
        .map(|r| {
            join_nonempty(vec![
                s(r, "id").unwrap_or_default().to_string(),
                s(r, "state").unwrap_or_default().to_string(),
                s(r, "reviewer_username")
                    .or_else(|| s(r, "username"))
                    .unwrap_or_default()
                    .to_string(),
                s(r, "created_at").unwrap_or_default().to_string(),
            ])
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// `repo.actions.listWorkflows` — `data.workflows[]`.
pub fn workflow_list(data: &Value) -> String {
    arr(data, "workflows")
        .iter()
        .map(|w| {
            join_nonempty(vec![
                s(w, "id")
                    .or_else(|| s(w, "path"))
                    .unwrap_or_default()
                    .to_string(),
                s(w, "name").unwrap_or_default().to_string(),
                s(w, "path").unwrap_or_default().to_string(),
            ])
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Bundled-runner compatibility cell for a template summary: `ok`,
/// `partial(<n> actions)` when some `uses:` refs can't run, or `unparsable`.
fn template_runner(t: &Value) -> String {
    if s(t, "parse_error").is_some() {
        return "unparsable".to_string();
    }
    let n = arr(t, "unsupported_actions").len();
    if n > 0 {
        format!("partial({n} actions)")
    } else {
        "ok".to_string()
    }
}

/// `repo.actions.workflowTemplates.list` — aligned `ID NAME CATEGORIES
/// SOURCE RUNNER` table. Suggested rows lead (marked `*`), then the rest by
/// name; `suggested_only` restricts the table to `data.suggested`.
pub fn workflow_template_list(data: &Value, suggested_only: bool) -> String {
    let pool: &[Value] = if suggested_only {
        arr(data, "suggested")
    } else {
        arr(data, "templates")
    };
    let mut suggested: Vec<&Value> = Vec::new();
    let mut rest: Vec<&Value> = Vec::new();
    for t in pool {
        if b(t, "suggested") {
            suggested.push(t);
        } else {
            rest.push(t);
        }
    }
    let by_name = |a: &&Value, b: &&Value| {
        s(a, "name")
            .unwrap_or_default()
            .to_lowercase()
            .cmp(&s(b, "name").unwrap_or_default().to_lowercase())
    };
    suggested.sort_by(by_name);
    rest.sort_by(by_name);

    let mut rows: Vec<[String; 5]> = Vec::new();
    for (t, mark) in suggested
        .iter()
        .map(|t| (*t, "*"))
        .chain(rest.iter().map(|t| (*t, " ")))
    {
        rows.push([
            format!("{mark} {}", s(t, "id").unwrap_or("?")),
            s(t, "name").unwrap_or_default().to_string(),
            arr(t, "categories")
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(", "),
            s(t, "provenance").unwrap_or_default().to_string(),
            template_runner(t),
        ]);
    }
    if rows.is_empty() {
        return String::new();
    }
    let header = ["ID", "NAME", "CATEGORIES", "SOURCE", "RUNNER"];
    let mut widths = header.map(str::len);
    for row in &rows {
        for (i, cell) in row.iter().enumerate() {
            widths[i] = widths[i].max(cell.chars().count());
        }
    }
    let line = |cells: &[String; 5]| {
        cells
            .iter()
            .enumerate()
            .map(|(i, c)| {
                if i == cells.len() - 1 {
                    c.clone()
                } else {
                    format!("{c:<w$}", w = widths[i])
                }
            })
            .collect::<Vec<_>>()
            .join("  ")
            .trim_end()
            .to_string()
    };
    let mut out = vec![line(&header.map(String::from))];
    out.extend(rows.iter().map(line));
    out.join("\n")
}

/// `repo.actions.workflows.create` — RepoFileCommitResponse plus request
/// context (`path` may be None when the server picked the default).
pub fn workflow_created(
    data: &Value,
    path: Option<&str>,
    instance: &str,
    repo: &str,
    onto_pr: Option<u64>,
) -> String {
    let sha = truncate(s(data, "commit_sha").unwrap_or_default(), 7);
    let branch = s(data, "branch").unwrap_or_default();
    let mut out = format!(
        "Created {} on {branch} ({sha})",
        path.unwrap_or(".github/workflows/")
    );
    if let Some(num) = onto_pr {
        out.push_str(&format!("\nCommitted onto pull request #{num}"));
    } else if let Some(num) = n(data, "pr_number") {
        out.push_str(&format!(
            "\nOpened pull request #{num}: {instance}/{repo}/pull/{num}"
        ));
    }
    out
}

/// `repo.actions.secrets.list` — `data.secrets[]` (names only — values never
/// leave the server).
pub fn secret_list(data: &Value) -> String {
    arr(data, "secrets")
        .iter()
        .map(|sc| {
            join_nonempty(vec![
                s(sc, "name").unwrap_or_default().to_string(),
                s(sc, "updated_at").unwrap_or_default().to_string(),
            ])
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// `repo.actions.getJobLog` — `{job, log}` — print the log verbatim.
pub fn job_log(data: &Value) -> String {
    s(data, "log")
        .or_else(|| s(data, "content"))
        .unwrap_or_default()
        .to_string()
}

/// `repo.blob` — `{content}` (base64 in some deployments; decode best-effort).
pub fn blob(data: &Value) -> String {
    if let Some(c) = s(data, "content") {
        return c.to_string();
    }
    generic(data)
}

/// `release.list` — `data.releases[]`.
pub fn release_list(data: &Value) -> String {
    arr(data, "releases")
        .iter()
        .map(|r| {
            join_nonempty(vec![
                s(r, "tag_name").unwrap_or_default().to_string(),
                s(r, "title").unwrap_or_default().to_string(),
                if b(r, "draft") { "draft" } else { "" }.to_string(),
                if b(r, "prerelease") { "prerelease" } else { "" }.to_string(),
            ])
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// `release.get`/`create`/`update`.
pub fn release_view(data: &Value) -> String {
    let mut out = vec![join_nonempty(vec![
        s(data, "tag_name").unwrap_or_default().to_string(),
        s(data, "title").unwrap_or_default().to_string(),
    ])];
    let meta = join_nonempty(vec![
        if b(data, "draft") { "draft" } else { "" }.to_string(),
        if b(data, "prerelease") {
            "prerelease"
        } else {
            ""
        }
        .to_string(),
        s(data, "author_username").unwrap_or_default().to_string(),
        s(data, "created_at").unwrap_or_default().to_string(),
    ]);
    if !meta.is_empty() {
        out.push(meta);
    }
    if let Some(body) = s(data, "body").filter(|b| !b.is_empty()) {
        out.push(String::new());
        out.push(crate::markdown::render(body));
    }
    out.join("\n")
}

/// `search.global` — `data.hits[]` with a `kind`.
pub fn search(data: &Value) -> String {
    arr(data, "hits")
        .iter()
        .map(|h| {
            join_nonempty(vec![
                s(h, "kind").unwrap_or_default().to_string(),
                s(h, "title")
                    .or_else(|| s(h, "name"))
                    .or_else(|| s(h, "path"))
                    .unwrap_or_default()
                    .to_string(),
                s(h, "url").unwrap_or_default().to_string(),
            ])
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// `org.listMine` — `data.orgs[]`.
pub fn org_list(data: &Value) -> String {
    arr(data, "orgs")
        .iter()
        .map(|o| {
            join_nonempty(vec![
                s(o, "slug").unwrap_or_default().to_string(),
                s(o, "display_name").unwrap_or_default().to_string(),
                s(o, "role").unwrap_or_default().to_string(),
            ])
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// `org.members.list` — `data.members[]`.
pub fn member_list(data: &Value) -> String {
    arr(data, "members")
        .iter()
        .map(|m| {
            join_nonempty(vec![
                s(m, "username").unwrap_or_default().to_string(),
                s(m, "role").unwrap_or_default().to_string(),
            ])
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// `notification.list` — `data.notifications[]`.
pub fn notification_list(data: &Value) -> String {
    arr(data, "notifications")
        .iter()
        .map(|n| {
            join_nonempty(vec![
                s(n, "id").unwrap_or_default().to_string(),
                s(n, "kind")
                    .or_else(|| s(n, "type"))
                    .unwrap_or_default()
                    .to_string(),
                s(n, "title").unwrap_or_default().to_string(),
                s(n, "repo").unwrap_or_default().to_string(),
                if b(n, "unread") { "unread" } else { "" }.to_string(),
            ])
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// `notification.unreadCount` — `{count}`.
pub fn unread_count(data: &Value) -> String {
    n(data, "count")
        .map(|c| format!("{c} unread"))
        .unwrap_or_else(|| generic(data))
}

/// `user.getPublicProfile`.
pub fn user_view(data: &Value) -> String {
    let mut out = vec![s(data, "username").unwrap_or("?").to_string()];
    let mut meta = Vec::new();
    if let Some(d) = s(data, "display_name").filter(|x| !x.is_empty()) {
        meta.push(d.to_string());
    }
    if let Some(b) = s(data, "bio").filter(|x| !x.is_empty()) {
        meta.push(b.to_string());
    }
    if !meta.is_empty() {
        out.push(meta.join(" · "));
    }
    let counts = [
        ("followers", "followers"),
        ("following", "following"),
        ("repo_count", "repos"),
    ]
    .iter()
    .filter_map(|(k, label)| n(data, k).map(|v| format!("{v} {label}")))
    .collect::<Vec<_>>()
    .join(" · ");
    if !counts.is_empty() {
        out.push(counts);
    }
    out.join("\n")
}

/// `user.lookup` — `data.users[]`.
pub fn user_list(data: &Value) -> String {
    arr(data, "users")
        .iter()
        .map(|u| {
            join_nonempty(vec![
                s(u, "username").unwrap_or_default().to_string(),
                s(u, "display_name").unwrap_or_default().to_string(),
            ])
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// `user.listStarred` — `data.repos[]`.
pub fn starred_list(data: &Value) -> String {
    repo_list(data)
}

/// `sshKey.list` — `data.keys[]`.
pub fn ssh_key_list(data: &Value) -> String {
    arr(data, "keys")
        .iter()
        .map(|k| {
            join_nonempty(vec![
                s(k, "id").unwrap_or_default().to_string(),
                s(k, "title").unwrap_or_default().to_string(),
                s(k, "fingerprint").unwrap_or_default().to_string(),
                if b(k, "can_authenticate") { "auth" } else { "" }.to_string(),
                if b(k, "can_sign") { "sign" } else { "" }.to_string(),
            ])
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// `gpgKey.list` — `data.keys[]`.
pub fn gpg_key_list(data: &Value) -> String {
    arr(data, "keys")
        .iter()
        .map(|k| {
            join_nonempty(vec![
                s(k, "id").unwrap_or_default().to_string(),
                s(k, "title").unwrap_or_default().to_string(),
                s(k, "key_id")
                    .or_else(|| s(k, "fingerprint"))
                    .unwrap_or_default()
                    .to_string(),
            ])
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// `email.list` — `data.emails[]`.
pub fn email_list(data: &Value) -> String {
    arr(data, "emails")
        .iter()
        .map(|e| {
            join_nonempty(vec![
                s(e, "email").unwrap_or_default().to_string(),
                if b(e, "primary") { "primary" } else { "" }.to_string(),
                if b(e, "verified") {
                    "verified"
                } else {
                    "unverified"
                }
                .to_string(),
            ])
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// `label.list*` — `data.labels[]`.
pub fn label_list(data: &Value) -> String {
    arr(data, "labels")
        .iter()
        .map(|l| {
            join_nonempty(vec![
                s(l, "id").unwrap_or_default().to_string(),
                s(l, "name").unwrap_or_default().to_string(),
                s(l, "color").unwrap_or_default().to_string(),
                s(l, "description").unwrap_or_default().to_string(),
            ])
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// `webhook.list` — `data.webhooks[]`.
pub fn webhook_list(data: &Value) -> String {
    arr(data, "webhooks")
        .iter()
        .map(|w| {
            join_nonempty(vec![
                s(w, "id").unwrap_or_default().to_string(),
                s(w, "url").unwrap_or_default().to_string(),
                arr(w, "events")
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(","),
                if b(w, "active") { "active" } else { "inactive" }.to_string(),
            ])
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// `webhook.deliveries.list` — `data.deliveries[]`.
pub fn delivery_list(data: &Value) -> String {
    arr(data, "deliveries")
        .iter()
        .map(|d| {
            join_nonempty(vec![
                s(d, "id").unwrap_or_default().to_string(),
                s(d, "event").unwrap_or_default().to_string(),
                n(d, "response_status")
                    .map(|c| c.to_string())
                    .or_else(|| s(d, "status").map(str::to_string))
                    .unwrap_or_default(),
                s(d, "delivered_at")
                    .or_else(|| s(d, "created_at"))
                    .unwrap_or_default()
                    .to_string(),
            ])
        })
        .collect::<Vec<_>>()
        .join("\n")
}

trait IntoOptionString {
    fn into_option_string(self) -> Option<String>;
}

impl IntoOptionString for String {
    fn into_option_string(self) -> Option<String> {
        if self.is_empty() {
            None
        } else {
            Some(self)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn repo_list_rows() {
        let data = json!({"repos": [
            {"owner_username": "octo", "name": "demo", "visibility": "public", "description": "hi"},
            {"owner_username": "org", "name": "secret", "visibility": "private", "description": ""},
        ]});
        let out = repo_list(&data);
        assert_eq!(out, "octo/demo  public  hi\norg/secret  private");
    }

    #[test]
    fn repo_list_empty() {
        assert_eq!(repo_list(&json!({"repos": []})), "");
    }

    #[test]
    fn issue_list_rows_with_labels() {
        let data = json!({"issues": [
            {"number": 3, "title": "broken", "labels": [{"name": "bug"}, {"name": "p1"}]},
            {"number": 1, "title": "first", "labels": []},
        ], "total": 2});
        assert_eq!(issue_list(&data), "#3  broken  bug,p1\n#1  first");
    }

    #[test]
    fn pull_view_shows_head_into_base() {
        let data = json!({
            "number": 7, "title": "add thing", "state": "open", "draft": true,
            "head_ref": "feat-x", "base_ref": "main", "author_username": "octo",
            "created_at": "2026-01-01", "body": "does a thing"
        });
        let out = pull_view(&data);
        assert!(out.contains("#7  add thing  (draft)"));
        assert!(out.contains("open · feat-x → main · by octo"));
        assert!(out.contains("does a thing"));
    }

    #[test]
    fn checks_rows() {
        let data = json!({"statuses": [
            {"context": "ci/build", "state": "success", "description": "ok"},
            {"context": "ci/lint", "state": "failure", "description": ""},
        ]});
        assert_eq!(checks(&data), "success  ci/build  ok\nfailure  ci/lint");
    }

    #[test]
    fn run_view_includes_jobs() {
        let data = json!({
            "run": {"id": "r1", "workflow_name": "CI", "title": "push", "status": "success",
                    "event": "push", "head_ref": "main", "head_sha": "abcdef123456"},
            "jobs": [{"status": "success", "name": "build", "job_key": "build"}]
        });
        let out = run_view(&data);
        assert!(out.contains("CI  push"));
        assert!(out.contains("success · push · main · abcdef1"));
        assert!(out.contains("  success  build"));
    }

    #[test]
    fn pkg_list_pluralizes_versions() {
        let data = json!({"packages": [
            {"format": "oci", "name": "img", "visibility": "public", "versions": [{"version": "1"}]},
            {"format": "npm", "name": "@s/p", "visibility": "private", "versions": [{"version": "1"}, {"version": "2"}]},
        ]});
        assert_eq!(
            pkg_list(&data),
            "oci  img  public  1 version\nnpm  @s/p  private  2 versions"
        );
    }

    #[test]
    fn auth_status_line() {
        let data = json!({"username": "octo", "display_name": "Octo Cat"});
        assert_eq!(
            auth_status(&data, "https://f.example.com"),
            "https://f.example.com\n  Logged in as octo (Octo Cat)"
        );
    }

    #[test]
    fn truncate_long_text() {
        assert_eq!(truncate("abc", 5), "abc");
        assert_eq!(truncate("abcdef", 5), "abcde…");
    }

    #[test]
    fn workflow_template_list_table() {
        let data = json!({
            "suggested": [
                {"id": "builtin:ci/rust", "name": "Rust", "categories": ["Continuous integration"],
                 "provenance": "builtin", "suggested": true,
                 "unsupported_actions": [], "parse_error": null},
            ],
            "templates": [
                {"id": "builtin:ci/rust", "name": "Rust", "categories": ["Continuous integration"],
                 "provenance": "builtin", "suggested": true,
                 "unsupported_actions": [], "parse_error": null},
                {"id": "instance:deploy", "name": "Deploy", "categories": ["Deployment"],
                 "provenance": "instance", "suggested": false,
                 "unsupported_actions": ["actions/setup-node", "actions/upload-artifact"],
                 "parse_error": null},
                {"id": "org:broken", "name": "Broken", "categories": [],
                 "provenance": "org", "suggested": false,
                 "unsupported_actions": [], "parse_error": "jobs.x: runs-on required"},
            ],
            "categories": ["Continuous integration", "Deployment"],
            "total_count": 3,
        });
        let out = workflow_template_list(&data, false);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), 4, "{out}");
        assert!(lines[0].starts_with("ID"), "{out}");
        assert!(lines[0].contains("RUNNER"), "{out}");
        // suggested first with the * marker, then by name
        assert!(lines[1].starts_with("* builtin:ci/rust"), "{out}");
        assert!(lines[2].contains("org:broken"), "{out}");
        assert!(lines[2].contains("unparsable"), "{out}");
        assert!(lines[3].contains("instance:deploy"), "{out}");
        assert!(lines[3].contains("partial(2 actions)"), "{out}");

        let sug = workflow_template_list(&data, true);
        assert_eq!(sug.lines().count(), 2, "{sug}");
        assert!(sug.contains("builtin:ci/rust"), "{sug}");
        assert!(!sug.contains("org:broken"), "{sug}");

        assert_eq!(workflow_template_list(&json!({"templates": []}), false), "");
    }

    #[test]
    fn workflow_created_lines() {
        let data = json!({
            "commit_sha": "abcdef1234567890", "branch": "ci/add",
            "created_branch": true, "pr_number": 9,
        });
        let out = workflow_created(
            &data,
            Some(".github/workflows/rust.yml"),
            "https://f.example.com",
            "o/n",
            None,
        );
        assert!(
            out.contains("Created .github/workflows/rust.yml on ci/add (abcdef1…)"),
            "{out}"
        );
        assert!(
            out.contains("Opened pull request #9: https://f.example.com/o/n/pull/9"),
            "{out}"
        );

        let out = workflow_created(&data, None, "https://f.example.com", "o/n", Some(9));
        assert!(out.contains("Committed onto pull request #9"), "{out}");
        assert!(!out.contains("Opened pull request"), "{out}");
    }
}
