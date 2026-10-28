//! U5 — read-class handlers (BR3/BR6): `repo.*`, `issue.*`, `pull.*`,
//! and the six Actions reads, all through `dispatch_proc` on the
//! synthesized-session `RpcCtx` (BR3.1). `{owner, name}` inject from
//! the RESOLVED session's repo binding — never from args (BR2.2);
//! `ref`/`branch` args only narrow within that repo and default to the
//! session branch. No audit rows (BR9.2) — the durable tool-call card
//! is the read trail.

use oxidean_api_core::oxpilot::{ToolCtx, ToolError, ToolOutput};
use oxidean_db::OxpilotSessionRow;
use serde_json::{json, Map, Value};

use super::{bound_output, OxpilotTools, TOOL_OUTPUT_MAX};

/// Entries whose `ref` arg defaults to the session branch when absent
/// (`ref` is a REQUIRED field on the backing request types, so the
/// injection is load-bearing, not cosmetic).
const REF_BEARING: &[&str] = &["repo_tree", "repo_file_read", "repo_log"];

/// Build the proc input: session repo binding + schema-validated args,
/// with the per-entry field remaps the wire names need (`ref` →
/// `git_ref` on `repo.actions.listWorkflows`; `branch` stays `branch`
/// on `repo.actions.listRuns`). `run_id`/`job_id` are opaque ids —
/// never treated as refs (WF3).
fn build_input(entry_name: &str, args: &Value, session: &OxpilotSessionRow) -> Value {
    let mut input = Map::new();
    if let Some(obj) = args.as_object() {
        for (k, v) in obj {
            input.insert(k.clone(), v.clone());
        }
    }
    // The session repo binding lands LAST so a stray arg key can never
    // overwrite it (closed schemas make this unreachable today; the
    // ordering is the defense if a schema ever loosens — BR2.3).
    input.insert("owner".to_string(), json!(session.repo_owner));
    input.insert("name".to_string(), json!(session.repo_name));

    // `ref` narrowing — absent means the session's bound branch.
    if REF_BEARING.contains(&entry_name) {
        let has_ref = input
            .get("ref")
            .and_then(|r| r.as_str())
            .is_some_and(|r| !r.is_empty());
        if !has_ref {
            input.insert("ref".to_string(), json!(session.branch));
        }
    }
    // `repo.actions.listWorkflows` names its ref `git_ref`.
    if entry_name == "actions_list_workflows" {
        let git_ref = input
            .remove("ref")
            .and_then(|v| v.as_str().map(str::to_string))
            .filter(|r| !r.is_empty())
            .unwrap_or_else(|| session.branch.clone());
        input.insert("git_ref".to_string(), json!(git_ref));
    }
    // `repo.actions.listRuns.branch` — a repo-internal narrowing
    // filter; absent narrows to the session's working branch.
    if entry_name == "actions_list_runs" {
        let has_branch = input
            .get("branch")
            .and_then(|b| b.as_str())
            .is_some_and(|b| !b.is_empty());
        if !has_branch {
            input.insert("branch".to_string(), json!(session.branch));
        }
    }
    // BR6.4 — `wait_ms` is unrepresentable in the schema; nothing to
    // strip here, but the guard stays explicit for the rule's sake.
    input.remove("wait_ms");
    Value::Object(input)
}

/// `actions_get_job_log` — re-shape `ActionJobLogResponse{content,
/// next_offset, size, done}` into the tool output `{content
/// (tail-bounded), next_offset, done}` (WF3): the log's TAIL is the
/// failure signal, so truncation drops from the front, not the end.
fn bound_job_log(mut data: Value) -> Value {
    if let Some(obj) = data.as_object_mut() {
        obj.remove("size");
        if let Some(content) = obj.get("content").and_then(|c| c.as_str()) {
            // Reserve room for the framing fields under TOOL_OUTPUT_MAX.
            let cap = TOOL_OUTPUT_MAX.saturating_sub(1024);
            if content.len() > cap {
                let start = content.len() - cap;
                let start = content
                    .char_indices()
                    .find_map(|(i, _)| (i >= start).then_some(i))
                    .unwrap_or(content.len());
                obj.insert(
                    "content".to_string(),
                    Value::String(content[start..].to_string()),
                );
                obj.insert("truncated".to_string(), Value::Bool(true));
            }
        }
    }
    data
}

/// The shared read pipeline — session-bound input → `dispatch_proc` →
/// verbatim error map (BR3.4) → bounded output (BR9.4).
pub async fn handle(
    tools: &OxpilotTools,
    ctx: &ToolCtx,
    proc_name: &'static str,
    entry_name: &'static str,
    args: &Value,
    session: &OxpilotSessionRow,
) -> Result<ToolOutput, ToolError> {
    let input = build_input(entry_name, args, session);
    let data = tools.dispatch_proc(ctx, proc_name, input).await?;
    let data = if entry_name == "actions_get_job_log" {
        bound_job_log(data)
    } else {
        data
    };
    Ok(bound_output(data))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> OxpilotSessionRow {
        OxpilotSessionRow {
            id: "s1".to_string(),
            user_id: "u1".to_string(),
            repo_owner: "acme".to_string(),
            repo_name: "app".to_string(),
            repo_id: "r1".to_string(),
            branch: "feature/x".to_string(),
            goal: None,
            title: "t".to_string(),
            status: "active".to_string(),
            created_at: String::new(),
            last_activity_at: String::new(),
            computer_use_target_id: None,
        }
    }

    #[test]
    fn owner_name_inject_from_session_never_args() {
        let args = json!({"path": "src/main.rs"});
        let input = build_input("repo_file_read", &args, &session());
        assert_eq!(input["owner"], "acme");
        assert_eq!(input["name"], "app");
        assert_eq!(input["ref"], "feature/x");
        assert_eq!(input["path"], "src/main.rs");
        // Args can never supply identity fields (schema's
        // additionalProperties rejects them upstream anyway).
        let args = json!({"path": "x"});
        let input = build_input("issue_list", &args, &session());
        assert_eq!(input["owner"], "acme");
    }

    #[test]
    fn explicit_ref_narrows_within_repo() {
        let args = json!({"ref": "main"});
        let input = build_input("repo_tree", &args, &session());
        assert_eq!(input["ref"], "main");
    }

    #[test]
    fn workflows_ref_maps_to_git_ref() {
        let input = build_input("actions_list_workflows", &json!({}), &session());
        assert_eq!(input["git_ref"], "feature/x");
        assert!(input.get("ref").is_none());
        let input = build_input("actions_list_workflows", &json!({"ref": "dev"}), &session());
        assert_eq!(input["git_ref"], "dev");
    }

    #[test]
    fn list_runs_branch_defaults_to_session_branch() {
        let input = build_input(
            "actions_list_runs",
            &json!({"status": "failure"}),
            &session(),
        );
        assert_eq!(input["branch"], "feature/x");
        let input = build_input("actions_list_runs", &json!({"branch": "main"}), &session());
        assert_eq!(input["branch"], "main");
    }

    #[test]
    fn job_log_tail_bounds_content() {
        let big = "x".repeat(TOOL_OUTPUT_MAX + 500);
        let data = bound_job_log(json!({
            "content": big,
            "next_offset": 12345u64,
            "size": 99999u64,
            "done": false
        }));
        assert!(data.get("size").is_none());
        assert_eq!(data["truncated"], true);
        assert!(data["content"].as_str().unwrap().len() <= TOOL_OUTPUT_MAX);
        assert_eq!(data["next_offset"], 12345);
    }

    #[test]
    fn job_log_small_content_untouched() {
        let data = bound_job_log(json!({
            "content": "line1\nline2",
            "next_offset": 11u64,
            "size": 11u64,
            "done": true
        }));
        assert!(data.get("truncated").is_none());
        assert_eq!(data["content"], "line1\nline2");
        assert_eq!(data["done"], true);
    }
}
