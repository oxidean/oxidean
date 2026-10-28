//! U5 — the ProposedDiff tool handlers (BR4).
//!
//! `propose_diff` inserts exactly one `status = pending` row — the
//! model can never set `applied`/`pr_ref`, perform git mutation, or
//! update in place (BR4.1/BR4.2). Path-set coherence pins
//! `files[].path ∪ new_path` to the `diff --git` header set so what
//! the user reviews is what apply writes (BR4.3). `proposal_get`
//! re-pins the session scope post-fetch because `svc.proposal` is
//! user-scoped (BR4.6).

use oxidean_api_core::oxpilot::{ProposalWrite, ToolCtx, ToolError, ToolOutput};
use oxidean_db::{OxpilotProposalRow, OxpilotSessionRow};
use serde_json::{json, Value};

use super::{
    bound_output, map_proc_error, tool_err, OxpilotTools, DIFF_BODY_MAX, MAX_PROPOSAL_FILES,
    TITLE_MAX,
};

/// `files[]` element — validated by the entry schema before this
/// handler runs (required `path`/`op`, closed properties).
struct FileOp {
    path: String,
    op: String,
    new_path: Option<String>,
    content_b64: Option<String>,
}

fn parse_ops(args: &Value) -> Vec<FileOp> {
    args["files"]
        .as_array()
        .map(|files| {
            files
                .iter()
                .map(|f| FileOp {
                    path: f["path"].as_str().unwrap_or_default().to_string(),
                    op: f["op"].as_str().unwrap_or_default().to_string(),
                    new_path: f["new_path"].as_str().map(str::to_string),
                    content_b64: f["content_b64"].as_str().map(str::to_string),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// `diff --git a/<old> b/<new>` header paths — both sides land in the
/// coherence set (`/dev/null` sides for create/delete are skipped).
fn diff_header_paths(diff_body: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for line in diff_body.lines() {
        let Some(rest) = line.strip_prefix("diff --git ") else {
            continue;
        };
        // Header is exactly `a/<path> b/<path>` — paths cannot contain
        // spaces in this repo's convention (normalize_edit_path), so a
        // single split on " b/" is safe.
        let Some((a, b)) = rest.split_once(" b/") else {
            continue;
        };
        let a = a.strip_prefix("a/").unwrap_or(a);
        out.push((a.to_string(), b.to_string()));
    }
    out
}

/// The wire `files[{path, patch}]` shape — one entry per `diff --git`
/// section, `path` being the post-image side (`a/` when `b/` is
/// /dev/null). Shared by `proposal_get` and the `oxpilot.diff_*` procs.
pub(crate) fn derive_files(diff_body: &str) -> Vec<Value> {
    let mut files = Vec::new();
    let mut current: Option<(String, String)> = None; // (path, patch-so-far)
    for line in diff_body.lines() {
        if let Some(rest) = line.strip_prefix("diff --git ") {
            if let Some((path, patch)) = current.take() {
                files.push(json!({"path": path, "patch": patch}));
            }
            let path = rest
                .split_once(" b/")
                .map(|(a, b)| {
                    // Model-authored diffs sometimes write `b/dev/null`
                    // for deletes — the file's path lives on the a-side.
                    if b == "dev/null" || b == "/dev/null" {
                        a.strip_prefix("a/").unwrap_or(a).to_string()
                    } else {
                        b.to_string()
                    }
                })
                .unwrap_or_else(|| rest.to_string());
            current = Some((path, format!("{line}\n")));
        } else if let Some((_, patch)) = current.as_mut() {
            patch.push_str(line);
            patch.push('\n');
        }
    }
    if let Some((path, patch)) = current {
        files.push(json!({"path": path, "patch": patch}));
    }
    files
}

/// Decoded-length estimate for base64 — the bound is on BYTES, so we
/// size the decode arithmetically (no `base64` dep) and let the apply
/// path's own `base64_decode` enforce well-formedness.
fn decoded_len(b64: &str) -> Result<usize, String> {
    let mut n = 0usize;
    let mut pad = 0usize;
    for c in b64.chars() {
        match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '+' | '/' => n += 1,
            '=' => pad += 1,
            c if c.is_whitespace() => {}
            _ => return Err("content_b64 is not base64".to_string()),
        }
    }
    if pad > 2 || (n + pad) % 4 != 0 {
        return Err("content_b64 is not base64".to_string());
    }
    Ok((n + pad) / 4 * 3 - pad)
}

/// `propose_diff{title, files[], diff_body}` → pending row + diff card
/// + `oxpilot.proposal_created` audit (BR4, WF4).
pub async fn propose_diff(
    tools: &OxpilotTools,
    ctx: &ToolCtx,
    args: &Value,
    session: &OxpilotSessionRow,
) -> Result<ToolOutput, ToolError> {
    let title = args["title"].as_str().unwrap_or_default();
    let diff_body = args["diff_body"].as_str().unwrap_or_default();
    let ops = parse_ops(args);

    // BR4.4 — bounds (the schema already enforced the shape-level
    // ceilings; these are the cross-field ones).
    if title.chars().count() > TITLE_MAX {
        return Err(tool_err("invalid_args", "title: must be at most 200 chars"));
    }
    if diff_body.len() > DIFF_BODY_MAX {
        return Err(tool_err("invalid_args", "diff_body: exceeds 256 KiB"));
    }
    if ops.len() > MAX_PROPOSAL_FILES {
        return Err(tool_err(
            "invalid_args",
            format!("files: must have at most {MAX_PROPOSAL_FILES} items"),
        ));
    }

    // Per-op rules + summed content bound.
    let mut total_content = 0usize;
    for (i, op) in ops.iter().enumerate() {
        match op.op.as_str() {
            "rename" if op.new_path.is_none() => {
                return Err(tool_err(
                    "invalid_args",
                    format!("files[{i}].new_path: required for rename"),
                ));
            }
            "create" | "update" if op.content_b64.is_none() => {
                return Err(tool_err(
                    "invalid_args",
                    format!("files[{i}].content_b64: required for {}", op.op),
                ));
            }
            _ => {}
        }
        if let Some(b64) = &op.content_b64 {
            let len = decoded_len(b64)
                .map_err(|e| tool_err("invalid_args", format!("files[{i}].content_b64: {e}")))?;
            total_content = total_content.saturating_add(len);
            if total_content > DIFF_BODY_MAX {
                return Err(tool_err(
                    "invalid_args",
                    format!("files[{i}].content_b64: summed content exceeds 256 KiB"),
                ));
            }
        }
    }

    // BR4.3 — path-set coherence: files[].path ∪ new_path == the
    // diff --git header path set (both sides of each header).
    let mut op_paths: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for op in &ops {
        op_paths.insert(op.path.clone());
        if let Some(np) = &op.new_path {
            op_paths.insert(np.clone());
        }
    }
    let mut diff_paths: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for (a, b) in diff_header_paths(diff_body) {
        diff_paths.insert(a);
        diff_paths.insert(b);
    }
    if op_paths != diff_paths {
        let missing: Vec<&String> = diff_paths.difference(&op_paths).collect();
        let extra: Vec<&String> = op_paths.difference(&diff_paths).collect();
        let mut parts = Vec::new();
        if !missing.is_empty() {
            parts.push(format!(
                "missing ops for: {}",
                missing
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        if !extra.is_empty() {
            parts.push(format!(
                "ops not in diff: {}",
                extra
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        return Err(tool_err(
            "invalid_args",
            format!("files must match diff_body paths — {}", parts.join("; ")),
        ));
    }

    // BR4.1/BR4.2 — unconditional pending insert; ZERO git mutation.
    let svc = tools
        .state
        .oxpilot
        .sessions()
        .map_err(|e| tool_err("internal", e.message))?;
    let row = svc
        .save_proposal(
            &ctx.user_id,
            ProposalWrite {
                session_id: ctx.session_id.clone(),
                title: title.to_string(),
                base: session.branch.clone(),
                diff_body: diff_body.to_string(),
                ops_json: serde_json::to_string(&args["files"])
                    .unwrap_or_else(|_| "[]".to_string()),
            },
        )
        .await
        .map_err(|e| map_proc_error(&e))?;

    // BR4.5 — the diff card rides the live turn AFTER commit; a
    // terminalled turn drops silently (the tool-call row is the
    // guaranteed record).
    tools.emit_card(&ctx.session_id, "diff", &row.id).await;

    // BR9.1 — post-commit audit; title is model-derived so it passes
    // `redact` on the way in.
    tools
        .audit(
            ctx,
            "oxpilot.proposal_created",
            Some(("proposal", &row.id)),
            json!({
                "session_id": ctx.session_id,
                "proposal_id": row.id,
                "title": oxidean_core::redact(title),
                "file_count": ops.len(),
            }),
        )
        .await;

    let msg = format!("proposal {} created — pending review", row.id);
    Ok(ToolOutput {
        content: msg.clone(),
        output_summary: msg,
    })
}

/// `proposal_get{proposal_id}` — session-pinned owned read (BR4.6).
pub async fn proposal_get(
    tools: &OxpilotTools,
    ctx: &ToolCtx,
    args: &Value,
    _session: &OxpilotSessionRow,
) -> Result<ToolOutput, ToolError> {
    let proposal_id = args["proposal_id"].as_str().unwrap_or_default();
    let svc = tools
        .state
        .oxpilot
        .sessions()
        .map_err(|e| tool_err("internal", e.message))?;
    let row: OxpilotProposalRow = svc
        .proposal(&ctx.user_id, proposal_id)
        .await
        .map_err(|e| map_proc_error(&e))?;
    // `svc.proposal` proves only that the row's session belongs to the
    // user — the model-facing read pins THIS session so same-user
    // other-session ids are indistinguishable from dead ones.
    if row.session_id != ctx.session_id {
        return Err(tool_err("not_found", "proposal not found"));
    }
    Ok(bound_output(json!({
        "id": row.id,
        "title": row.title,
        "base": row.base,
        "status": row.status,
        "pr_ref": row.pr_ref,
        "files": derive_files(&row.diff_body),
        "created_at": row.created_at,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_header_paths_both_sides() {
        let diff =
            "diff --git a/src/old.rs b/src/new.rs\nrename from src/old.rs\ndiff --git a/x b/x\n";
        let paths = diff_header_paths(diff);
        assert_eq!(
            paths,
            vec![
                ("src/old.rs".to_string(), "src/new.rs".to_string()),
                ("x".to_string(), "x".to_string())
            ]
        );
    }

    #[test]
    fn derive_files_splits_sections() {
        let diff = "diff --git a/one b/one\n@@ -1 +1 @@\n-a\n+b\ndiff --git a/two b/two\n@@ -1 +1 @@\n-c\n+d\n";
        let files = derive_files(diff);
        assert_eq!(files.len(), 2);
        assert_eq!(files[0]["path"], "one");
        assert!(files[0]["patch"].as_str().unwrap().contains("+b"));
        assert_eq!(files[1]["path"], "two");
    }

    #[test]
    fn derive_files_delete_uses_old_path() {
        let diff = "diff --git a/gone b/dev/null\ndeleted file\n";
        let files = derive_files(diff);
        assert_eq!(files[0]["path"], "gone");
    }

    #[test]
    fn decoded_len_estimates() {
        assert_eq!(decoded_len("aGVsbG8=").unwrap(), 5); // "hello"
        assert!(decoded_len("not!base64").is_err());
        assert!(decoded_len("a===").is_err());
    }
}
