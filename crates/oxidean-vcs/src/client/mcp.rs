//! MCP tool descriptors for the VCS surface (FR13.1, contract C3).
//!
//! `oxidean-vcs` emits name/description/procedure/access/input-schema tuples;
//! the host (`oxidean-api::mcp`) maps [`McpAccess`] onto its private `Access`
//! enum and wraps each descriptor in its own `ToolDef`. The crate never
//! references `oxidean-api` types — dependency direction stays host→crate
//! (AC8.3.3, FR13.3).
//!
//! Deliberate absences per C3: `vcs.repo_enable` (admin), `vcs.ref_update`
//! (CAS surface), `vcs.git_import`/`vcs.git_export` (operator flows), and
//! `checkout` (a client-side disk op, not an RPC — agents materialize content
//! through the read tools).

use serde_json::{json, Value};

/// Token-scope tier a tool requires. Mirrors `proc_classes` (C1) — the host
/// maps these onto its `Access::{RepoRead,RepoWrite}` enforcement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpAccess {
    RepoRead,
    RepoWrite,
}

/// One MCP tool descriptor: everything `tools/list` publishes plus the wire
/// procedure `tools/call` forwards to.
#[derive(Debug, Clone)]
pub struct McpToolDef {
    pub name: &'static str,
    pub description: &'static str,
    /// Same-named `vcs.*` RPC procedure the tool thin-maps to.
    pub procedure: &'static str,
    pub access: McpAccess,
    /// JSON Schema for the tool's `arguments` object. Always requires
    /// separate `owner`/`name` (the FG repo-grant check reads exactly those).
    pub input_schema: Value,
}

fn repo_schema(extra_props: Value, required: &[&str]) -> Value {
    let mut props = json!({
        "owner": {"type": "string", "description": "Repository owner (user or org slug)"},
        "name": {"type": "string", "description": "Repository name"},
    });
    let props_map = props.as_object_mut().expect("object literal");
    for (k, v) in extra_props.as_object().expect("object literal") {
        props_map.insert(k.clone(), v.clone());
    }
    let mut req: Vec<Value> = vec![json!("owner"), json!("name")];
    req.extend(required.iter().map(|r| json!(r)));
    json!({"type": "object", "properties": props, "required": req})
}

fn page_props() -> Value {
    json!({
        "limit": {"type": "integer", "description": "Page size cap"},
        "cursor": {"type": "string", "description": "Resume cursor from a prior page"},
    })
}

fn id_props(extra: Value, required: &[&str]) -> Value {
    let mut props = page_props();
    for (k, v) in extra.as_object().expect("object literal") {
        props
            .as_object_mut()
            .expect("object literal")
            .insert(k.clone(), v.clone());
    }
    repo_schema(props, required)
}

/// The 13-tool C3 surface (FR13.1 dozen + `vcs_changeset_get`), in stable
/// list order: reads first, then writes.
pub fn tool_descriptors() -> Vec<McpToolDef> {
    let r = McpAccess::RepoRead;
    let w = McpAccess::RepoWrite;
    vec![
        McpToolDef {
            name: "vcs_status",
            description: "VCS status: enabled flag, heads (ref → changeset), changeset/object counts, storage bytes.",
            procedure: "vcs.status",
            access: r,
            input_schema: repo_schema(json!({}), &[]),
        },
        McpToolDef {
            name: "vcs_log",
            description: "Changeset log from a ref name, changeset id, or git oid (`from`, default `main`).",
            procedure: "vcs.log",
            access: r,
            input_schema: id_props(
                json!({
                    "from": {"type": "string", "description": "Ref name, cs_<ulid>, or git oid; default branch head when omitted"},
                }),
                &[],
            ),
        },
        McpToolDef {
            name: "vcs_diff",
            description: "Diff between two changesets (per-path entries; `paths` narrows the listing).",
            procedure: "vcs.diff",
            access: r,
            input_schema: id_props(
                json!({
                    "from": {"type": "string", "description": "Base changeset id (cs_<ulid>)"},
                    "to": {"type": "string", "description": "Target changeset id (cs_<ulid>)"},
                    "paths": {"type": "array", "items": {"type": "string"}, "description": "Restrict to these paths"},
                }),
                &["from", "to"],
            ),
        },
        McpToolDef {
            name: "vcs_tree",
            description: "Manifest listing of a changeset (path, mode, blob hash, size) — paged.",
            procedure: "vcs.tree",
            access: r,
            input_schema: id_props(
                json!({
                    "changeset_id": {"type": "string", "description": "Changeset id (cs_<ulid>)"},
                }),
                &["changeset_id"],
            ),
        },
        McpToolDef {
            name: "vcs_file_get",
            description: "File entry at a path inside a changeset: blob hash, mode, size.",
            procedure: "vcs.file_get",
            access: r,
            input_schema: repo_schema(
                json!({
                    "changeset_id": {"type": "string", "description": "Changeset id (cs_<ulid>)"},
                    "path": {"type": "string", "description": "File path inside the manifest"},
                }),
                &["changeset_id", "path"],
            ),
        },
        McpToolDef {
            name: "vcs_blame",
            description: "Per-line attribution for a file at a changeset (or its default head when `at` is omitted).",
            procedure: "vcs.blame",
            access: r,
            input_schema: id_props(
                json!({
                    "path": {"type": "string", "description": "File path inside the manifest"},
                    "at": {"type": "string", "description": "Changeset id to blame at; default head when omitted"},
                }),
                &["path"],
            ),
        },
        McpToolDef {
            name: "vcs_path_history",
            description: "Changesets that touched a path (rename-aware history).",
            procedure: "vcs.path_history",
            access: r,
            input_schema: id_props(
                json!({
                    "path": {"type": "string", "description": "File path inside the manifest"},
                }),
                &["path"],
            ),
        },
        McpToolDef {
            name: "vcs_merge_base",
            description: "Best common ancestor of two changesets (null when unrelated).",
            procedure: "vcs.merge_base",
            access: r,
            input_schema: repo_schema(
                json!({
                    "a": {"type": "string", "description": "First changeset id (cs_<ulid>)"},
                    "b": {"type": "string", "description": "Second changeset id (cs_<ulid>)"},
                }),
                &["a", "b"],
            ),
        },
        McpToolDef {
            name: "vcs_ref_list",
            description: "Named refs and their changeset ids — paged.",
            procedure: "vcs.ref_list",
            access: r,
            input_schema: id_props(json!({}), &[]),
        },
        McpToolDef {
            name: "vcs_changeset_get",
            description: "Point read of one changeset: parents, message, author, manifest summary.",
            procedure: "vcs.changeset_get",
            access: r,
            input_schema: repo_schema(
                json!({
                    "id": {"type": "string", "description": "Changeset id (cs_<ulid>) or git oid"},
                }),
                &["id"],
            ),
        },
        McpToolDef {
            name: "vcs_changeset_create",
            description: "Create a changeset: parent ids, message, and manifest changes ({path, kind, mode, blob|content_b64, old_path?} entries — mode required on every kind; rename adds old_path; delete carries no blob/content).",
            procedure: "vcs.changeset_create",
            access: w,
            input_schema: repo_schema(
                json!({
                    "parents": {"type": "array", "items": {"type": "string"}, "description": "Parent changeset ids ([] = root)"},
                    "message": {"type": "string", "description": "Changeset message"},
                    "author": {"type": "object", "properties": {"name": {"type": "string"}, "email": {"type": "string"}}, "description": "Author override; authenticated user when absent"},
                    "changes": {"type": "array", "items": {"type": "object"}, "description": "Manifest entries: {path, kind, mode, blob|content_b64, old_path?}"},
                }),
                &["parents", "message", "changes"],
            ),
        },
        McpToolDef {
            name: "vcs_merge",
            description: "Merge two changesets; clean result or typed `vcs.conflict` file list.",
            procedure: "vcs.merge",
            access: w,
            input_schema: repo_schema(
                json!({
                    "a": {"type": "string", "description": "Ours changeset id (cs_<ulid>)"},
                    "b": {"type": "string", "description": "Theirs changeset id (cs_<ulid>)"},
                    "message": {"type": "string", "description": "Merge message"},
                }),
                &["a", "b"],
            ),
        },
        McpToolDef {
            name: "vcs_rebase",
            description: "Replay a changeset onto a new parent; returns the new head changeset.",
            procedure: "vcs.rebase",
            access: w,
            input_schema: repo_schema(
                json!({
                    "changeset_id": {"type": "string", "description": "Changeset to replay (cs_<ulid>)"},
                    "new_parent_id": {"type": "string", "description": "New parent changeset id (cs_<ulid>)"},
                    "message": {"type": "string", "description": "Message override"},
                }),
                &["changeset_id", "new_parent_id"],
            ),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXPECTED: [(&str, &str); 13] = [
        ("vcs_status", "vcs.status"),
        ("vcs_log", "vcs.log"),
        ("vcs_diff", "vcs.diff"),
        ("vcs_tree", "vcs.tree"),
        ("vcs_file_get", "vcs.file_get"),
        ("vcs_blame", "vcs.blame"),
        ("vcs_path_history", "vcs.path_history"),
        ("vcs_merge_base", "vcs.merge_base"),
        ("vcs_ref_list", "vcs.ref_list"),
        ("vcs_changeset_get", "vcs.changeset_get"),
        ("vcs_changeset_create", "vcs.changeset_create"),
        ("vcs_merge", "vcs.merge"),
        ("vcs_rebase", "vcs.rebase"),
    ];

    #[test]
    fn thirteen_tools_map_to_procedures() {
        let defs = tool_descriptors();
        assert_eq!(defs.len(), EXPECTED.len());
        for ((name, proc), def) in EXPECTED.iter().zip(defs.iter()) {
            assert_eq!(&def.name, name);
            assert_eq!(&def.procedure, proc);
        }
    }

    #[test]
    fn every_schema_requires_owner_and_name() {
        for d in tool_descriptors() {
            let required = d.input_schema["required"].as_array().unwrap();
            assert!(required.iter().any(|v| v == "owner"), "{}", d.name);
            assert!(required.iter().any(|v| v == "name"), "{}", d.name);
            let props = d.input_schema["properties"].as_object().unwrap();
            assert!(props.contains_key("owner") && props.contains_key("name"));
        }
    }

    #[test]
    fn required_parity_with_wire() {
        let defs = tool_descriptors();
        let req = |name: &str| -> Vec<String> {
            defs.iter().find(|d| d.name == name).unwrap().input_schema["required"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().to_string())
                .collect()
        };
        assert_eq!(req("vcs_status"), ["owner", "name"]);
        assert_eq!(req("vcs_log"), ["owner", "name"]); // `from` injected host-side
        assert_eq!(req("vcs_diff"), ["owner", "name", "from", "to"]);
        assert_eq!(req("vcs_tree"), ["owner", "name", "changeset_id"]);
        assert_eq!(
            req("vcs_file_get"),
            ["owner", "name", "changeset_id", "path"]
        );
        assert_eq!(req("vcs_blame"), ["owner", "name", "path"]);
        assert_eq!(req("vcs_path_history"), ["owner", "name", "path"]);
        assert_eq!(req("vcs_merge_base"), ["owner", "name", "a", "b"]);
        assert_eq!(req("vcs_ref_list"), ["owner", "name"]);
        assert_eq!(req("vcs_changeset_get"), ["owner", "name", "id"]);
        assert_eq!(
            req("vcs_changeset_create"),
            ["owner", "name", "parents", "message", "changes"]
        );
        assert_eq!(req("vcs_merge"), ["owner", "name", "a", "b"]);
        assert_eq!(
            req("vcs_rebase"),
            ["owner", "name", "changeset_id", "new_parent_id"]
        );
    }

    #[test]
    fn access_split_mirrors_proc_classes() {
        for d in tool_descriptors() {
            let want = match d.procedure {
                "vcs.changeset_create" | "vcs.merge" | "vcs.rebase" => McpAccess::RepoWrite,
                _ => McpAccess::RepoRead,
            };
            assert_eq!(d.access, want, "{}", d.name);
        }
    }

    #[test]
    fn deliberate_absences_stay_absent() {
        let names: Vec<_> = tool_descriptors().iter().map(|d| d.name).collect();
        for absent in [
            "vcs_repo_enable",
            "vcs_ref_update",
            "vcs_git_import",
            "vcs_git_export",
            "vcs_checkout",
            "checkout",
        ] {
            assert!(!names.contains(&absent), "{absent} must stay absent");
        }
        for d in tool_descriptors() {
            assert!(d.name.starts_with("vcs_"));
            assert!(d.procedure.starts_with("vcs."));
        }
    }
}
