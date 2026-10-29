//! U5 — the ProposedDiff tool handlers (BR4).
//!
//! `propose_diff` inserts exactly one `status = pending` row — the
//! model can never set `applied`/`pr_ref`, perform git mutation, or
//! update in place (BR4.1/BR4.2). The stored `diff_body` is GENERATED
//! here from `files[]` + the session base — never trusted from args —
//! so the reviewed patch IS the applied content by construction
//! (BR4.3). `proposal_get` re-pins the session scope post-fetch
//! because `svc.proposal` is user-scoped (BR4.6).

use std::collections::BTreeMap;

use base64::Engine as _;
use oxidean_api_core::oxpilot::{ProposalWrite, ToolCtx, ToolError, ToolOutput};
use oxidean_db::{OxpilotProposalRow, OxpilotSessionRow};
use oxidean_vcs::text_diff::{diff_lines, LineHunk};
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
/// returned set. `/dev/null` sides map to the OTHER side's path.
/// Test-only since the canonical generator replaced arg-side coherence
/// checks (BR4.3) — retained to pin the header grammar tests.
#[cfg(test)]
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
        let dev_null = |p: &str| p == "dev/null" || p == "/dev/null";
        out.push(if dev_null(a) {
            (b.to_string(), b.to_string())
        } else if dev_null(b) {
            (a.to_string(), a.to_string())
        } else {
            (a.to_string(), b.to_string())
        });
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
    if pad > 2 || !(n + pad).is_multiple_of(4) {
        return Err("content_b64 is not base64".to_string());
    }
    Ok((n + pad) / 4 * 3 - pad)
}

// ---------------------------------------------------------------------------
// Canonical patch generation (BR4.3) — the reviewed diff derives from
// `files[]` + the session base, so approve-what-you-see ==
// apply-what-was-approved BY CONSTRUCTION. The model's `diff_body` arg
// never reaches storage (an agent-authored patch can diverge from the
// base64 payloads the apply path actually writes).
// ---------------------------------------------------------------------------

/// One blob side — `Opaque` covers binary or soft-capped `repo.blob`
/// reads (neither renders honestly as a line diff); `Moved` marks a
/// rename's consumed source path (the rename stanza covers it — a
/// separate delete stanza would double-count the removal).
#[derive(Clone, PartialEq)]
enum Content {
    Absent,
    Text(Vec<u8>),
    Opaque,
    Moved,
}

/// `repo.blob` for one path — `repo.path_not_found` maps to `Absent`
/// (a legitimate create/delete base); every other failure aborts the
/// proposal rather than render an untrustworthy patch.
async fn fetch_base(
    tools: &OxpilotTools,
    ctx: &ToolCtx,
    session: &OxpilotSessionRow,
    path: &str,
) -> Result<Content, ToolError> {
    match tools
        .dispatch_proc_raw(
            ctx,
            "repo.blob",
            json!({
                "owner": session.repo_owner,
                "name": session.repo_name,
                "ref": session.branch,
                "path": path,
            }),
        )
        .await
    {
        Ok(data) => {
            if data["is_binary"].as_bool().unwrap_or(true)
                || data["truncated"].as_bool().unwrap_or(false)
            {
                return Ok(Content::Opaque);
            }
            match (data["encoding"].as_str(), data["content"].as_str()) {
                (Some("utf-8"), Some(s)) => Ok(Content::Text(s.as_bytes().to_vec())),
                _ => Ok(Content::Opaque),
            }
        }
        Err(e) if e.code == "repo.path_not_found" => Ok(Content::Absent),
        Err(e) => Err(map_proc_error(&e)),
    }
}

/// NUL-byte sniff — same convention `repo.blob`'s `is_binary` takes.
fn looks_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(8000).any(|&b| b == 0)
}

/// Fold the declared op order into the FINAL per-path outcome —
/// `path → (origin_path, content)`. `origin` trails renames so a
/// create→rename→update chain renders as the file's real lineage
/// instead of three disconnected stanzas.
fn fold_ops(
    ops: &[FileOp],
    bases: &BTreeMap<String, Content>,
) -> Result<BTreeMap<String, (String, Content)>, ToolError> {
    let mut finals: BTreeMap<String, (String, Content)> = BTreeMap::new();
    for op in &ops {
        match op.op.as_str() {
            "create" | "update" => {
                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(op.content_b64.as_deref().unwrap_or_default())
                    .map_err(|_| tool_err("invalid_args", "files[].content_b64: invalid base64"))?;
                let content = if looks_binary(&bytes) {
                    Content::Opaque
                } else {
                    Content::Text(bytes)
                };
                // Origin trails an earlier rename — `rename a→b` +
                // `update b` renders the rename stanza WITH hunks
                // (a→v2), not a bare "new file b".
                let origin = finals
                    .get(&op.path)
                    .map(|(o, _)| o.clone())
                    .unwrap_or_else(|| op.path.clone());
                finals.insert(op.path.clone(), (origin, content));
            }
            "delete" => {
                // Origin trails a rename the same way content ops do —
                // `rename a→b` + `delete b` nets out to "delete a".
                let origin = finals
                    .get(&op.path)
                    .map(|(o, _)| o.clone())
                    .unwrap_or_else(|| op.path.clone());
                finals.insert(op.path.clone(), (origin, Content::Absent));
            }
            "rename" => {
                let np = op.new_path.clone().unwrap_or_default();
                // The destination inherits the source's pending content
                // or — untouched — its fetched base blob.
                let (origin, content) = match finals.remove(&op.path) {
                    Some((o, c)) => (o, c),
                    None => (
                        op.path.clone(),
                        bases.get(&op.path).cloned().unwrap_or(Content::Absent),
                    ),
                };
                finals.insert(np, (origin, content));
                // The source path was CONSUMED — `Moved` renders no
                // stanza of its own (the destination's rename stanza
                // carries the removal). A later op on the same path
                // overwrites the marker normally.
                finals
                    .entry(op.path.clone())
                    .or_insert_with(|| (op.path.clone(), Content::Moved));
            }
            _ => {}
        }
    }
    Ok(finals)
}

/// `\n`-split WITHOUT terminators — the trailing empty element a
/// newline-ended blob yields is not a line.
fn split_lines(bytes: &[u8]) -> Vec<&[u8]> {
    let mut v: Vec<&[u8]> = bytes.split(|&b| b == b'\n').collect();
    if v.last().is_some_and(|l| l.is_empty()) {
        v.pop();
    }
    v
}

/// `@@` range label — 1-based start, `,len` omitted at len==1, and an
/// empty range prints the position BEFORE it (`-0,0` on file creation).
fn hunk_range(start0: usize, len: usize) -> String {
    if len == 0 {
        format!("{start0},0")
    } else if len == 1 {
        format!("{}", start0 + 1)
    } else {
        format!("{},{}", start0 + 1, len)
    }
}

/// The `\ No newline at end of file` marker — emitted once, after the
/// side's final line when that blob lacks a trailing newline.
fn push_body_line(out: &mut String, sigil: char, line: &[u8], no_nl_tail: bool) {
    out.push(sigil);
    out.push_str(&String::from_utf8_lossy(line));
    out.push('\n');
    if no_nl_tail {
        out.push_str("\\ No newline at end of file\n");
    }
}

/// Context-3 hunk render — `diff_lines` runs merged when their context
/// windows would touch (a gap of ≤ `2 * CONTEXT` unchanged lines).
fn render_hunks(
    out: &mut String,
    old_lines: &[&[u8]],
    new_lines: &[&[u8]],
    old_no_nl: bool,
    new_no_nl: bool,
) {
    const CONTEXT: usize = 3;
    let mut groups: Vec<Vec<LineHunk>> = Vec::new();
    for h in diff_lines(old_lines, new_lines) {
        match groups.last_mut() {
            Some(g)
                if h.old_start
                    .saturating_sub(g.last().unwrap().old_start + g.last().unwrap().old_len)
                    <= 2 * CONTEXT =>
            {
                g.push(h);
            }
            _ => groups.push(vec![h]),
        }
    }
    for g in groups {
        let first = g.first().unwrap();
        let last = g.last().unwrap();
        let old_ctx_s = first.old_start.saturating_sub(CONTEXT);
        let old_ctx_e = (last.old_start + last.old_len + CONTEXT).min(old_lines.len());
        let new_ctx_s = first.new_start.saturating_sub(CONTEXT);
        let new_ctx_e = (last.new_start + last.new_len + CONTEXT).min(new_lines.len());
        out.push_str(&format!(
            "@@ -{} +{} @@\n",
            hunk_range(old_ctx_s, old_ctx_e - old_ctx_s),
            hunk_range(new_ctx_s, new_ctx_e - new_ctx_s)
        ));
        let mut oi = old_ctx_s;
        for h in &g {
            while oi < h.old_start {
                push_body_line(
                    out,
                    ' ',
                    old_lines[oi],
                    old_no_nl && oi + 1 == old_lines.len(),
                );
                oi += 1;
            }
            for i in h.old_start..h.old_start + h.old_len {
                push_body_line(
                    out,
                    '-',
                    old_lines[i],
                    old_no_nl && i + 1 == old_lines.len(),
                );
            }
            for i in h.new_start..h.new_start + h.new_len {
                push_body_line(
                    out,
                    '+',
                    new_lines[i],
                    new_no_nl && i + 1 == new_lines.len(),
                );
            }
            oi = h.old_start + h.old_len;
        }
        while oi < old_ctx_e {
            push_body_line(
                out,
                ' ',
                old_lines[oi],
                old_no_nl && oi + 1 == old_lines.len(),
            );
            oi += 1;
        }
    }
}

/// One `diff --git` stanza — absent sides print `/dev/null`, opaque
/// sides degrade to git's `Binary files … differ` convention, and a
/// pure rename emits `similarity index 100%` with no hunks.
fn render_stanza(out: &mut String, old_path: &str, new_path: &str, old: &Content, new: &Content) {
    // Byte-equality is only provable for fetched/decoded text — two
    // opaque sides might differ invisibly, so they still render the
    // binary marker rather than skip.
    let provably_same = match (old, new) {
        (Content::Absent, Content::Absent) => true,
        (Content::Text(a), Content::Text(b)) => a == b,
        _ => false,
    };
    if provably_same && old_path == new_path {
        return; // net no-op — identical content, identical name.
    }
    if matches!(old, Content::Absent) && matches!(new, Content::Absent) {
        return;
    }
    let renamed = old_path != new_path;
    out.push_str(&format!("diff --git a/{old_path} b/{new_path}\n"));

    if matches!(old, Content::Opaque) || matches!(new, Content::Opaque) {
        // One opaque side poisons the line-diff render — fall back to
        // git's honest binary marker (real rename lineage still prints).
        if renamed && !matches!(old, Content::Absent) {
            out.push_str(&format!(
                "similarity index 100%\nrename from {old_path}\nrename to {new_path}\n"
            ));
        }
        let a = side_ref(old, old_path, 'a');
        let b = side_ref(new, new_path, 'b');
        out.push_str(&format!("Binary files {a} and {b} differ\n"));
        return;
    }

    let old_bytes: &[u8] = match old {
        Content::Text(b) => b.as_slice(),
        _ => b"",
    };
    let new_bytes: &[u8] = match new {
        Content::Text(b) => b.as_slice(),
        _ => b"",
    };
    match (
        matches!(old, Content::Absent),
        matches!(new, Content::Absent),
    ) {
        (true, false) => out.push_str("new file mode 100644\n"),
        (false, true) => out.push_str("deleted file mode 100644\n"),
        _ => {}
    }
    if renamed && !matches!(old, Content::Absent) {
        if old_bytes == new_bytes {
            out.push_str("similarity index 100%\n");
        }
        out.push_str(&format!("rename from {old_path}\nrename to {new_path}\n"));
        if old_bytes == new_bytes {
            return; // Pure rename — headers carry the whole change.
        }
    }
    if old_bytes == new_bytes {
        return; // Same content, same name — nothing left to show.
    }
    out.push_str(&format!("--- {}\n", side_ref(old, old_path, 'a')));
    out.push_str(&format!("+++ {}\n", side_ref(new, new_path, 'b')));
    render_hunks(
        out,
        &split_lines(old_bytes),
        &split_lines(new_bytes),
        !old_bytes.is_empty() && !old_bytes.ends_with(b"\n"),
        !new_bytes.is_empty() && !new_bytes.ends_with(b"\n"),
    );
}

fn side_ref(content: &Content, path: &str, prefix: char) -> String {
    if matches!(content, Content::Absent) {
        "/dev/null".to_string()
    } else {
        format!("{prefix}/{path}")
    }
}

/// The whole stored patch — each touched path's (base → final)
/// net-effect stanza in path order. `bases` must cover every op's
/// `path` AND `new_path` before this runs.
async fn canonical_diff(
    tools: &OxpilotTools,
    ctx: &ToolCtx,
    session: &OxpilotSessionRow,
    ops: &[FileOp],
) -> Result<String, ToolError> {
    // One fetch per unique touched path — `path` ∪ `new_path`, sorted
    // for a deterministic dispatch order.
    let mut paths: Vec<&str> = Vec::new();
    for op in ops {
        paths.push(op.path.as_str());
        if let Some(np) = &op.new_path {
            paths.push(np.as_str());
        }
    }
    paths.sort_unstable();
    paths.dedup();
    let mut bases: BTreeMap<String, Content> = BTreeMap::new();
    for p in paths {
        bases.insert(p.to_string(), fetch_base(tools, ctx, session, p).await?);
    }

    let finals = fold_ops(ops, &bases)?;
    let mut out = String::new();
    for (path, (origin, content)) in &finals {
        // `Moved` — the consumed rename source; its destination stanza
        // already carries the removal.
        if matches!(content, Content::Moved) {
            continue;
        }
        let base = bases.get(origin).cloned().unwrap_or(Content::Absent);
        // A removed path renders under its ORIGIN name — the net delete
        // is of the file that existed (`rename a→b` + `delete b` =
        // "delete a"), never a rename-to-/dev/null chimera.
        let new_name = if matches!(content, Content::Absent) {
            origin.as_str()
        } else {
            path.as_str()
        };
        render_stanza(&mut out, origin, new_name, &base, content);
    }
    Ok(out)
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
    let ops = parse_ops(args);

    // BR4.4 — bounds (the schema already enforced the shape-level
    // ceilings; these are the cross-field ones).
    if title.chars().count() > TITLE_MAX {
        return Err(tool_err("invalid_args", "title: must be at most 200 chars"));
    }
    if ops.is_empty() {
        return Err(tool_err(
            "invalid_args",
            "files: must have at least 1 item — an empty proposal can never apply",
        ));
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

    // BR4.3 — canonical generation: the stored patch is built HERE
    // from `files[]` + the session base (the `diff_body` arg is
    // schema-accepted but never stored — an agent-authored patch can
    // diverge from the base64 payloads the apply path writes). The
    // base fetches also fail a dead repo binding up front, better than
    // a pending row that can never apply.
    let diff_body = canonical_diff(tools, ctx, session, &ops).await?;
    if diff_body.len() > DIFF_BODY_MAX {
        return Err(tool_err(
            "invalid_args",
            "generated patch exceeds 256 KiB — split the proposal",
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
                diff_body,
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
            Some(("oxpilot_proposal", &row.id)),
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

    fn text(s: &str) -> Content {
        Content::Text(s.as_bytes().to_vec())
    }

    fn op(path: &str, op: &str, new_path: Option<&str>, content: Option<&str>) -> FileOp {
        FileOp {
            path: path.to_string(),
            op: op.to_string(),
            new_path: new_path.map(str::to_string),
            content_b64: content
                .map(|s| base64::engine::general_purpose::STANDARD.encode(s.as_bytes())),
        }
    }

    #[test]
    fn stanza_create_renders_new_file() {
        let mut out = String::new();
        render_stanza(
            &mut out,
            "x.txt",
            "x.txt",
            &Content::Absent,
            &text("hello\nworld\n"),
        );
        assert!(out.contains("diff --git a/x.txt b/x.txt"));
        assert!(out.contains("new file mode 100644"));
        assert!(out.contains("--- /dev/null"));
        assert!(out.contains("+++ b/x.txt"));
        assert!(out.contains("@@ -0,0 +1,2 @@"));
        assert!(out.contains("+hello\n+world\n"));
    }

    #[test]
    fn stanza_update_renders_context_hunk() {
        let mut out = String::new();
        render_stanza(
            &mut out,
            "x",
            "x",
            &text("a\nb\nc\nd\ne\n"),
            &text("a\nb\nC\nd\ne\n"),
        );
        assert!(out.contains("--- a/x\n+++ b/x\n"));
        assert!(out.contains("-c\n"));
        assert!(out.contains("+C\n"));
        assert!(out.contains("@@ -1,5 +1,5 @@"));
    }

    #[test]
    fn stanza_delete_renders_devnull() {
        let mut out = String::new();
        render_stanza(&mut out, "gone", "gone", &text("bye\n"), &Content::Absent);
        assert!(out.contains("deleted file mode 100644"));
        assert!(out.contains("+++ /dev/null"));
        assert!(out.contains("-bye"));
    }

    #[test]
    fn stanza_pure_rename_emits_no_hunks() {
        let mut out = String::new();
        render_stanza(&mut out, "old", "new", &text("same\n"), &text("same\n"));
        assert!(out.contains("diff --git a/old b/new"));
        assert!(out.contains("similarity index 100%"));
        assert!(out.contains("rename from old\nrename to new\n"));
        assert!(!out.contains("@@"));
    }

    #[test]
    fn stanza_noop_update_renders_nothing() {
        let mut out = String::new();
        render_stanza(&mut out, "x", "x", &text("same\n"), &text("same\n"));
        assert!(out.is_empty());
    }

    #[test]
    fn stanza_opaque_side_renders_binary_marker() {
        let mut out = String::new();
        render_stanza(&mut out, "bin", "bin", &Content::Opaque, &text("x\n"));
        assert!(out.contains("Binary files a/bin and b/bin differ"));
    }

    #[test]
    fn fold_rename_tracks_origin_and_moved() {
        let mut bases = BTreeMap::new();
        bases.insert("a".to_string(), text("body\n"));
        let finals = fold_ops(&[op("a", "rename", Some("b"), None)], &bases).unwrap();
        assert_eq!(finals["b"].0, "a");
        assert!(matches!(finals["b"].1, Content::Text(_)));
        assert!(matches!(finals["a"].1, Content::Moved));
    }

    #[test]
    fn fold_rename_then_update_carries_content() {
        let mut bases = BTreeMap::new();
        bases.insert("a".to_string(), text("v1\n"));
        let finals = fold_ops(
            &[
                op("a", "rename", Some("b"), None),
                op("b", "update", None, Some("v2\n")),
            ],
            &bases,
        )
        .unwrap();
        // The update overwrote the moved content — origin stays `a` so
        // the stanza renders `rename a→b` + hunks, not "new file b".
        assert_eq!(finals["b"].0, "a");
        assert!(matches!(&finals["b"].1, Content::Text(c) if c == b"v2\n"));
    }

    #[test]
    fn hunk_range_labels() {
        assert_eq!(hunk_range(0, 0), "0,0");
        assert_eq!(hunk_range(0, 1), "1");
        assert_eq!(hunk_range(4, 3), "5,3");
    }
}
