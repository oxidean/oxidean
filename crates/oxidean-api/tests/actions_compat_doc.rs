//! Drift gate for `docs/actions/compatibility.md`.
//!
//! The compatibility reference is machine-checked so it cannot silently rot:
//! every table row carries a stable ID, a status, and evidence tokens that are
//! validated against the conformance corpus (`tests/conformance/cases.json`)
//! and the test tree. The gate also pins the Summary counts and the section
//! list, so adding/removing rows or sections requires updating the doc's own
//! accounting.
//!
//! Row contract (one GitHub feature per row):
//!
//!   | `id` | Feature | Status | Evidence | Notes |
//!
//! - `id` — lowercase dotted, `^[a0-9]+([._-][a-z0-9]+)*$`, unique, stable.
//! - Status — `supported` | `partial` | `unsupported` | `n/a`.
//! - Evidence — comma-separated `case:<cases.json id>` and/or
//!   `test:<repo-relative path>::<fn>`; required on every non-`n/a` row.
//! - `divergence` — cases.json may carry `"divergence": "<row id>"` for cases
//!   that pin a `partial`/`unsupported` row; the row must exist and cite the
//!   case back.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;

/// Runtime `CARGO_MANIFEST_DIR` — nextest sets it under `--workspace-remap`
/// where the build-time `env!` still points at the (absent) image path.
fn manifest_dir() -> PathBuf {
    match std::env::var("CARGO_MANIFEST_DIR") {
        Ok(d) if !d.is_empty() => PathBuf::from(d),
        _ => PathBuf::from(env!("CARGO_MANIFEST_DIR")),
    }
}

fn repo_root() -> PathBuf {
    manifest_dir()
        .parent()
        .and_then(|p| p.parent())
        .expect("crate lives under crates/")
        .to_path_buf()
}

const DOC_REL: &str = "docs/actions/compatibility.md";

const VALID_STATUSES: &[&str] = &["supported", "partial", "unsupported", "n/a"];

/// Required H2 section headings — `<number>. <title>` must match the doc's
/// `##` lines (the `(\`prefix.\`)` suffix is ignored).
const SECTIONS: &[&str] = &[
    "Workflow files & discovery",
    "Triggers & filters",
    "Events",
    "Workflow-level keys",
    "Jobs",
    "Steps",
    "Expressions",
    "Contexts",
    "Workflow commands & environment files",
    "Default environment variables",
    "Action metadata",
    "Reusable workflows",
    "Artifacts",
    "Dependency caching",
    "Concurrency",
    "Environments & deployments",
    "permissions",
    "OIDC",
    "Secrets & configuration variables",
    "Runners & labels",
    "Security behaviours",
    "Limits",
    "Run lifecycle & integrations",
    "GitHub REST API for Actions",
];

struct Row {
    id: String,
    status: String,
    evidence: String,
    section: String, // "N. Title" heading text
    line: usize,
}

fn is_doc_row(line: &str) -> bool {
    let t = line.trim();
    t.starts_with("| `") && t.ends_with('|')
}

/// Split a `| a | b | c |` markdown row into trimmed cell texts.
fn cells(line: &str) -> Vec<String> {
    line.trim()
        .trim_matches('|')
        .split('|')
        .map(|c| c.trim().to_string())
        .collect()
}

fn id_regex_ok(id: &str) -> bool {
    !id.is_empty()
        && id.split(['.', '_', '-']).all(|seg| {
            !seg.is_empty()
                && seg
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        })
        && !id.contains("..")
}

fn parse_doc() -> (Vec<Row>, BTreeMap<String, [usize; 4]>, Vec<String>) {
    let path = repo_root().join(DOC_REL);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {DOC_REL}: {e}"));
    let mut rows = Vec::new();
    let mut summary: BTreeMap<String, [usize; 4]> = BTreeMap::new();
    let mut headings: Vec<String> = Vec::new();
    let mut section = String::new();
    let mut in_summary = false;
    for (i, line) in text.lines().enumerate() {
        if let Some(h) = line.strip_prefix("## ") {
            section = h.trim().to_string();
            headings.push(section.clone());
            in_summary = section.to_ascii_lowercase().starts_with("summary");
            continue;
        }
        if in_summary {
            let t = line.trim();
            if t.starts_with("| ") && !t.starts_with("| `") && t.contains('|') {
                let cs = cells(line);
                // `| <sec> | s | p | u | n |` — skip header/separator.
                if cs.len() == 5
                    && !cs[0].eq_ignore_ascii_case("section")
                    && !cs[0].chars().all(|c| c == '-')
                {
                    let nums: Vec<usize> = cs[1..]
                        .iter()
                        .filter_map(|c| c.parse::<usize>().ok())
                        .collect();
                    if nums.len() == 4 {
                        summary.insert(cs[0].clone(), [nums[0], nums[1], nums[2], nums[3]]);
                    }
                }
            }
            continue;
        }
        if !is_doc_row(line) {
            continue;
        }
        let cs = cells(line);
        if cs.len() != 5 {
            panic!(
                "row at line {} has {} cells (want 5): {line}",
                i + 1,
                cs.len()
            );
        }
        let id = cs[0].trim_matches('`').to_string();
        if id == "id" {
            continue; // table header row
        }
        rows.push(Row {
            id,
            status: cs[2].clone(),
            evidence: cs[3].clone(),
            section: section.clone(),
            line: i + 1,
        });
    }
    (rows, summary, headings)
}

struct CaseMeta {
    divergence: Option<String>,
}

fn cases_json() -> BTreeMap<String, CaseMeta> {
    let path = manifest_dir().join("tests/conformance/cases.json");
    let raw = std::fs::read_to_string(&path).expect("read cases.json");
    let file: serde_json::Value = serde_json::from_str(&raw).expect("parse cases.json");
    let mut out = BTreeMap::new();
    for c in file["cases"].as_array().expect("cases array") {
        out.insert(
            c["id"].as_str().unwrap().to_string(),
            CaseMeta {
                divergence: c["divergence"].as_str().map(str::to_string),
            },
        );
    }
    out
}

#[test]
fn compat_doc_is_consistent() {
    let (rows, summary, headings) = parse_doc();
    let root = repo_root();
    assert!(!rows.is_empty(), "no rows parsed from {DOC_REL}");

    // Section headings all present (in order by leading number).
    for (i, want) in SECTIONS.iter().enumerate() {
        let prefix = format!("{}.", i + 1);
        assert!(
            headings
                .iter()
                .any(|h| h.starts_with(&prefix) && h.contains(want)),
            "missing section heading '{prefix} {want}'"
        );
    }

    // IDs unique + valid; statuses valid.
    let mut seen: HashSet<&str> = HashSet::new();
    for r in &rows {
        assert!(
            id_regex_ok(&r.id),
            "row '{}' (line {}) has an invalid id",
            r.id,
            r.line
        );
        assert!(seen.insert(r.id.as_str()), "duplicate row id '{}'", r.id);
        assert!(
            VALID_STATUSES.contains(&r.status.as_str()),
            "row '{}' (line {}) has invalid status '{}'",
            r.id,
            r.line,
            r.status
        );
        assert!(
            !r.section.is_empty(),
            "row '{}' (line {}) sits outside a numbered section",
            r.id,
            r.line
        );
    }

    // Evidence tokens resolve.
    let case_meta = cases_json();
    let mut cited_cases: HashMap<String, HashSet<String>> = HashMap::new(); // case -> rows
    for r in &rows {
        let tokens: Vec<&str> = r
            .evidence
            .split(',')
            .map(|t| t.trim())
            .filter(|t| !t.is_empty() && *t != "-")
            .collect();
        assert!(
            r.status == "n/a" || !tokens.is_empty(),
            "row '{}' (line {}) is '{}' but has no evidence",
            r.id,
            r.line,
            r.status
        );
        for tok in tokens {
            if let Some(id) = tok.strip_prefix("case:") {
                assert!(
                    case_meta.contains_key(id),
                    "row '{}' (line {}) cites unknown case '{id}'",
                    r.id,
                    r.line
                );
                cited_cases
                    .entry(id.to_string())
                    .or_default()
                    .insert(r.id.clone());
            } else if let Some(rest) = tok.strip_prefix("test:") {
                let (path, f) = rest
                    .split_once("::")
                    .unwrap_or_else(|| panic!("row '{}' bad test token '{tok}'", r.id));
                let full = root.join(path);
                assert!(
                    full.exists(),
                    "row '{}' cites missing test path '{path}'",
                    r.id
                );
                let src = std::fs::read_to_string(&full).unwrap();
                assert!(
                    src.contains(&format!("fn {f}")),
                    "row '{}' cites '{path}::{f}' but no 'fn {f}' exists there",
                    r.id
                );
            } else {
                panic!(
                    "row '{}' (line {}) has bad evidence token '{tok}'",
                    r.id, r.line
                );
            }
        }
    }

    // Every corpus case is cited by ≥1 row.
    for id in case_meta.keys() {
        assert!(
            cited_cases.contains_key(id),
            "case '{id}' is not cited by any doc row"
        );
    }

    // Case divergence fields name a partial/unsupported row that cites back.
    let by_id: HashMap<&str, &Row> = rows.iter().map(|r| (r.id.as_str(), r)).collect();
    for (id, meta) in &case_meta {
        let Some(div) = &meta.divergence else {
            continue;
        };
        let row = by_id
            .get(div.as_str())
            .unwrap_or_else(|| panic!("case '{id}' divergence names unknown row '{div}'"));
        assert!(
            matches!(row.status.as_str(), "partial" | "unsupported"),
            "case '{id}' divergence '{div}' must be partial or unsupported, not '{}'",
            row.status
        );
        assert!(
            row.evidence
                .split(',')
                .map(|t| t.trim())
                .any(|t| t == format!("case:{id}")),
            "divergence row '{div}' must cite case '{id}'"
        );
    }

    // Summary table counts equal the real per-section counts.
    let mut actual: BTreeMap<String, [usize; 4]> = BTreeMap::new();
    for r in &rows {
        let e = actual.entry(r.section.clone()).or_default();
        let idx = match r.status.as_str() {
            "supported" => 0,
            "partial" => 1,
            "unsupported" => 2,
            "n/a" => 3,
            _ => unreachable!(),
        };
        e[idx] += 1;
    }
    for (sec, counts) in &actual {
        // Headings carry a `(\`prefix.\`)` suffix — strip it; mid-title
        // backticks (e.g. `GITHUB_TOKEN`) stay.
        let key = sec.split(" (`").next().unwrap_or(sec).trim().to_string();
        let want = summary
            .get(&key)
            .unwrap_or_else(|| panic!("summary table missing section '{key}'"));
        assert_eq!(
            want, counts,
            "summary counts for '{key}' drifted: table={want:?} actual={counts:?}"
        );
    }
    assert_eq!(
        summary.len(),
        actual.len(),
        "summary table has {} rows but {} sections carry rows",
        summary.len(),
        actual.len()
    );
}
