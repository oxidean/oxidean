//! Human-readable rendering for `data` payloads. `--json` bypasses all of
//! this and prints the raw `{ok, data|error}` envelope instead, so renderers
//! only ever see success payloads.

use serde_json::Value;

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
        out.push(body.to_string());
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
        out.push(body.to_string());
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
}
