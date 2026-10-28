//! U5 — the closed, fixed-order static tool catalog (BR1.1/BR1.2).
//!
//! Nothing here loads tool definitions from data, config, or provider
//! output: the 23 entries below ARE the catalog, identical for every
//! session/provider. Adding or removing a tool is a reviewed code
//! change against the BR3.2/BR6.2 proc allowlists — never a runtime
//! filter.

use oxidean_api_core::oxpilot::ToolSpec;
use serde_json::{json, Value};

/// C6 read|write classification — internal to the catalog, never on
/// the wire `ToolSpec`. Args can never reclassify a call (BR2.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolClass {
    Read,
    Write,
}

/// Which dispatch family an entry routes to (BR1.1's handler binding).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    /// Read-class — forward to the named backing RPC procedure through
    /// `dispatch_proc` (BR3.1). The `&'static str` is the literal proc
    /// name — the BR3.2/BR6.2 allowlist lives verbatim in [`CATALOG`].
    Proc(&'static str),
    /// Write-class — `pull.comments.create` through `dispatch_proc`
    /// after the ArtifactScope gate (BR8).
    PrComment,
    /// Write-class — C7 gateway dispatch (`exec_shell`/`fs_read`/
    /// `fs_write`; `kind == name` verbatim, BR7.1).
    Action,
    /// Read-class — `gateway.list_targets` (BR7.6).
    TargetsList,
    /// Read-class — `gateway.action_status` (BR7.6).
    ActionStatus,
    /// Write-class — pending `oxpilot_proposals` insert (BR4).
    ProposeDiff,
    /// Read-class — session-pinned proposal read (BR4.6).
    ProposalGet,
}

/// One catalog entry — spec halves + class + dispatch binding +
/// schema builder. `parameters_json` is a builder fn (not a stored
/// `Value`) so the table stays `&'static` without a lazy-static dep.
pub struct CatalogEntry {
    pub name: &'static str,
    pub description: &'static str,
    pub class: ToolClass,
    pub kind: EntryKind,
    pub parameters_json: fn() -> Value,
}

impl CatalogEntry {
    pub fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: self.name.to_string(),
            description: self.description.to_string(),
            parameters_json: (self.parameters_json)(),
        }
    }
}

// ---------- hand-authored schemas (NFR9/BR1.4) ----------

fn empty() -> Value {
    json!({"type": "object", "properties": {}, "additionalProperties": false})
}

fn path_prop() -> Value {
    json!({"type": "string", "minLength": 1, "maxLength": 4096, "pattern": "repo_path"})
}

fn ref_prop() -> Value {
    json!({"type": "string", "minLength": 1, "maxLength": 256})
}

fn id_prop() -> Value {
    json!({"type": "string", "minLength": 1, "maxLength": 128})
}

fn merge_props(mut a: serde_json::Map<String, Value>, b: Value) -> serde_json::Map<String, Value> {
    if let Some(m) = b.as_object() {
        for (k, v) in m {
            a.insert(k.clone(), v.clone());
        }
    }
    a
}

fn obj(required: &[&str], properties: Value) -> Value {
    json!({
        "type": "object",
        "required": required,
        "properties": properties,
        "additionalProperties": false
    })
}

fn repo_tree_schema() -> Value {
    obj(
        &[],
        json!({
            "ref": ref_prop(),
            "path": path_prop(),
        }),
    )
}

fn repo_file_read_schema() -> Value {
    obj(
        &["path"],
        json!({
            "path": path_prop(),
            "ref": ref_prop(),
        }),
    )
}

fn repo_log_schema() -> Value {
    obj(
        &[],
        json!({
            "ref": ref_prop(),
            "skip": {"type": "integer", "minimum": 0},
            "limit": {"type": "integer", "minimum": 1, "maximum": 100},
        }),
    )
}

fn repo_commit_get_schema() -> Value {
    obj(&["sha"], json!({"sha": ref_prop()}))
}

fn issue_list_schema() -> Value {
    obj(
        &[],
        json!({
            "state": {"enum": ["open", "closed", "all"]},
            "q": {"type": "string", "maxLength": 256},
            "offset": {"type": "integer", "minimum": 0},
            "limit": {"type": "integer", "minimum": 1, "maximum": 100},
        }),
    )
}

fn number_schema() -> Value {
    obj(
        &["number"],
        json!({"number": {"type": "integer", "minimum": 1}}),
    )
}

fn actions_list_runs_schema() -> Value {
    obj(
        &[],
        Value::Object(merge_props(
            json!({
                "status": {"enum": ["queued", "in_progress", "success", "failure", "cancelled"]},
                "branch": ref_prop(),
                "workflow": {"type": "string", "maxLength": 256},
                "page": {"type": "integer", "minimum": 1},
            })
            .as_object()
            .cloned()
            .unwrap_or_default(),
            json!({"per_page": {"type": "integer", "minimum": 1, "maximum": 100}}),
        )),
    )
}

fn run_id_schema() -> Value {
    obj(&["run_id"], json!({"run_id": id_prop()}))
}

fn job_id_schema() -> Value {
    obj(&["job_id"], json!({"job_id": id_prop()}))
}

fn job_log_schema() -> Value {
    obj(
        &["run_id", "job_id"],
        json!({
            "run_id": id_prop(),
            "job_id": id_prop(),
            "offset": {"type": "integer", "minimum": 0},
        }),
    )
}

fn workflows_schema() -> Value {
    obj(&[], json!({"ref": ref_prop()}))
}

fn annotations_schema() -> Value {
    obj(
        &["run_id"],
        json!({"run_id": id_prop(), "job_id": id_prop()}),
    )
}

fn propose_diff_schema() -> Value {
    obj(
        &["title", "files", "diff_body"],
        json!({
            "title": {"type": "string", "minLength": 1, "maxLength": 200},
            "diff_body": {"type": "string", "minLength": 1, "maxLength": 262144},
            "files": {
                "type": "array",
                "maxItems": 50,
                "items": {
                    "type": "object",
                    "required": ["path", "op"],
                    "properties": {
                        "path": path_prop(),
                        "op": {"enum": ["create", "update", "delete", "rename"]},
                        "new_path": path_prop(),
                        "content_b64": {"type": "string"},
                    },
                    "additionalProperties": false
                }
            }
        }),
    )
}

fn proposal_get_schema() -> Value {
    obj(&["proposal_id"], json!({"proposal_id": id_prop()}))
}

fn exec_shell_schema() -> Value {
    obj(
        &["command"],
        json!({
            "command": {"type": "string", "minLength": 1, "maxLength": 1024},
            "target_id": id_prop(),
        }),
    )
}

fn fs_read_schema() -> Value {
    obj(
        &["path"],
        json!({
            "path": {"type": "string", "minLength": 1, "maxLength": 4096},
            "target_id": id_prop(),
        }),
    )
}

fn fs_write_schema() -> Value {
    obj(
        &["path", "content_b64"],
        json!({
            "path": {"type": "string", "minLength": 1, "maxLength": 4096},
            "content_b64": {"type": "string"},
            "target_id": id_prop(),
        }),
    )
}

fn action_status_schema() -> Value {
    obj(&["action_id"], json!({"action_id": id_prop()}))
}

fn pr_comment_schema() -> Value {
    obj(
        &["number", "body"],
        json!({
            "number": {"type": "integer", "minimum": 1},
            "body": {"type": "string", "minLength": 1, "maxLength": 4096},
        }),
    )
}

// ---------- the catalog — closed, fixed-order (BR1.1) ----------

macro_rules! entry {
    ($name:literal, $desc:literal, $class:ident, $kind:expr, $schema:ident) => {
        CatalogEntry {
            name: $name,
            description: $desc,
            class: ToolClass::$class,
            kind: $kind,
            parameters_json: $schema,
        }
    };
}

/// The compiled-in catalog — 23 entries, fixed order, identical for
/// every session/provider (BR1.1). Proc names are literal — an
/// unlisted proc name cannot appear here (BR3.2/BR6.2/BR6.3).
pub static CATALOG: &[CatalogEntry] = &[
    entry!("repo_tree", "List the repository tree for the session-bound repo at an optional path/ref.", Read, EntryKind::Proc("repo.tree"), repo_tree_schema),
    entry!("repo_file_read", "Read a file's contents from the session-bound repo at an optional ref.", Read, EntryKind::Proc("repo.blob"), repo_file_read_schema),
    entry!("repo_log", "List recent commits for the session-bound repo, newest first.", Read, EntryKind::Proc("repo.commits"), repo_log_schema),
    entry!("repo_commit_get", "Get one commit's metadata and diff stat by sha.", Read, EntryKind::Proc("repo.commit"), repo_commit_get_schema),
    entry!("repo_refs", "List refs (branches and tags) for the session-bound repo.", Read, EntryKind::Proc("repo.refs"), empty),
    entry!("issue_list", "List issues in the session-bound repo.", Read, EntryKind::Proc("issue.list"), issue_list_schema),
    entry!("issue_get", "Get one issue by number from the session-bound repo.", Read, EntryKind::Proc("issue.get"), number_schema),
    entry!("pull_list", "List pull requests in the session-bound repo.", Read, EntryKind::Proc("pull.list"), issue_list_schema),
    entry!("pull_get", "Get one pull request by number from the session-bound repo.", Read, EntryKind::Proc("pull.get"), number_schema),
    entry!("actions_list_runs", "List Actions workflow runs for the session-bound repo.", Read, EntryKind::Proc("repo.actions.listRuns"), actions_list_runs_schema),
    entry!("actions_get_run", "Get one Actions run by id.", Read, EntryKind::Proc("repo.actions.getRun"), run_id_schema),
    entry!("actions_get_job", "Get one Actions job by id.", Read, EntryKind::Proc("repo.actions.getJob"), job_id_schema),
    entry!("actions_get_job_log", "Read a slice of one Actions job's log; page with offset/next_offset.", Read, EntryKind::Proc("repo.actions.getJobLog"), job_log_schema),
    entry!("actions_list_workflows", "List workflows defined in the session-bound repo.", Read, EntryKind::Proc("repo.actions.listWorkflows"), workflows_schema),
    entry!("actions_job_annotations", "List check annotations for an Actions run (optionally one job).", Read, EntryKind::Proc("repo.actions.jobAnnotations"), annotations_schema),
    entry!("propose_diff", "Propose a reviewable diff plus its file ops; nothing touches git until the user applies it.", Write, EntryKind::ProposeDiff, propose_diff_schema),
    entry!("proposal_get", "Get one of this session's proposals by id (pending or applied).", Read, EntryKind::ProposalGet, proposal_get_schema),
    entry!("targets_list", "List this user's connector targets and their capability/liveness state.", Read, EntryKind::TargetsList, empty),
    entry!("exec_shell", "Run a shell command on a connected connector target. Requires consent if no live grant covers it.", Write, EntryKind::Action, exec_shell_schema),
    entry!("fs_read", "Read a file from a connected connector target.", Write, EntryKind::Action, fs_read_schema),
    entry!("fs_write", "Write a file on a connected connector target.", Write, EntryKind::Action, fs_write_schema),
    entry!("action_status", "Poll the state of a dispatched connector action by action_id.", Read, EntryKind::ActionStatus, action_status_schema),
    entry!("pr_comment", "Post a comment on a pull request this session materialized via an applied proposal.", Write, EntryKind::PrComment, pr_comment_schema),
];

/// BR1.2 — provider-legal flat `domain_verb` ids:
/// `^[a-zA-Z0-9_-]{1,64}$` (dots are outside OpenAI/Anthropic tool
/// charsets, so proc names are never tool ids).
pub fn valid_tool_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Name lookup — absent names fail closed (`unknown_tool`) at the
/// invoke ladder (BR1.3).
pub fn find(name: &str) -> Option<&'static CatalogEntry> {
    CATALOG.iter().find(|e| e.name == name)
}

/// `catalog()` returns the `ToolSpec` halves verbatim (BR1.1).
pub fn catalog() -> Vec<ToolSpec> {
    CATALOG.iter().map(|e| e.spec()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn catalog_is_closed_and_ordered() {
        let expected = [
            "repo_tree",
            "repo_file_read",
            "repo_log",
            "repo_commit_get",
            "repo_refs",
            "issue_list",
            "issue_get",
            "pull_list",
            "pull_get",
            "actions_list_runs",
            "actions_get_run",
            "actions_get_job",
            "actions_get_job_log",
            "actions_list_workflows",
            "actions_job_annotations",
            "propose_diff",
            "proposal_get",
            "targets_list",
            "exec_shell",
            "fs_read",
            "fs_write",
            "action_status",
            "pr_comment",
        ];
        let names: Vec<&str> = CATALOG.iter().map(|e| e.name).collect();
        assert_eq!(names, expected);
    }

    #[test]
    fn every_entry_has_schema_description_and_legal_name() {
        let mut seen = HashSet::new();
        for e in CATALOG {
            assert!(valid_tool_name(e.name), "illegal tool name {}", e.name);
            assert!(!e.description.is_empty());
            assert_eq!((e.parameters_json)()["type"], json!("object"));
            assert_eq!((e.parameters_json)()["additionalProperties"], json!(false));
            assert!(seen.insert(e.name), "duplicate {}", e.name);
        }
    }

    #[test]
    fn catalog_specs_match_entries() {
        let specs = catalog();
        assert_eq!(specs.len(), CATALOG.len());
        for (s, e) in specs.iter().zip(CATALOG.iter()) {
            assert_eq!(s.name, e.name);
            assert_eq!(s.description, e.description);
        }
    }

    #[test]
    fn name_charset() {
        assert!(valid_tool_name("exec_shell"));
        assert!(valid_tool_name("fs-read2"));
        assert!(!valid_tool_name("repo.tree")); // dots are provider-illegal
        assert!(!valid_tool_name(""));
        assert!(!valid_tool_name(&"x".repeat(65)));
        assert!(!valid_tool_name("a b"));
    }

    #[test]
    fn exec_fs_names_equal_connector_kind_vocabulary() {
        // BR1.2 — `kind` passes `entry.name` straight into
        // dispatch_action, so the names must equal the C7 vocabulary.
        for kind in oxidean_api_core::oxpilot::CONNECTOR_CAPABILITIES {
            let e = find(kind).expect("capability kind must be a tool");
            assert!(matches!(e.kind, EntryKind::Action));
            assert_eq!(e.class, ToolClass::Write);
        }
    }

    #[test]
    fn write_class_membership_is_fixed() {
        let writes: HashSet<&str> = CATALOG
            .iter()
            .filter(|e| e.class == ToolClass::Write)
            .map(|e| e.name)
            .collect();
        let expected: HashSet<&str> = [
            "propose_diff",
            "pr_comment",
            "exec_shell",
            "fs_read",
            "fs_write",
        ]
        .into_iter()
        .collect();
        assert_eq!(writes, expected);
    }

    #[test]
    fn no_control_or_mutation_procs_are_reachable() {
        // BR6.3 — Actions control procs can never be model-callable;
        // mechanism is ABSENCE from the registry.
        let procs: HashSet<&str> = CATALOG
            .iter()
            .filter_map(|e| match e.kind {
                EntryKind::Proc(p) => Some(p),
                _ => None,
            })
            .collect();
        for banned in [
            "repo.actions.approveRun",
            "repo.actions.cancelRun",
            "repo.actions.rerunRun",
            "repo.actions.dispatchWorkflow",
            "repo.file.upload",
            "repo.branchCreate",
            "pull.create",
            "pull.comments.create",
            "issue.comments.list",
            "repo.search",
        ] {
            assert!(!procs.contains(banned), "{banned} must not be reachable");
        }
    }
}
