//! Problem matchers — `::add-matcher::config.json` /
//! `::remove-matcher owner=<owner>::`.
//!
//! A matcher config file is `{"problemMatcher": […]}` — each entry an
//! owner, optional default `severity`/`fromPath`, and a `pattern` list.
//! Each pattern maps capture-group indices onto issue fields (`file`,
//! `line`, `column`, `severity`, `code`, `message`, `fromPath`, `loop`).
//! Multi-pattern matchers scan a line sequence as a state machine: the
//! last pattern may `loop` so a header line roots every following match.
//!
//! Ported from the runner's `IssueMatcher`/`OutputManager` semantics:
//! matchers are prepended on registration (same-owner clobbers), reset on
//! any other matcher's hit, ANSI colour codes are stripped before matching,
//! unknown severities drop the issue (the line still logs), and the file
//! value only survives when it resolves to a real path under the
//! workspace. The `regex` crate matches in linear time, so the .NET
//! timeout/retry failsafe is unnecessary here.

use std::path::{Path, PathBuf};

use regex::Regex;
use serde::Deserialize;

/// One `problemMatcher[]` entry — owns its multi-line scan state.
pub(crate) struct ProblemMatcher {
    pub owner: String,
    default_severity: String,
    default_from_path: String,
    patterns: Vec<IssuePattern>,
    /// Resolved running match per non-final pattern (len == patterns.len() - 1).
    state: Vec<Option<IssueMatch>>,
}

struct IssuePattern {
    file: Option<usize>,
    from_path: Option<usize>,
    line: Option<usize>,
    column: Option<usize>,
    severity: Option<usize>,
    code: Option<usize>,
    message: Option<usize>,
    looped: bool,
    regex: Regex,
}

/// Field values resolved from one pattern's capture groups, merged with
/// the running match carried over from earlier patterns in the sequence.
#[derive(Clone)]
pub(crate) struct IssueMatch {
    file: Option<String>,
    from_path: Option<String>,
    line: Option<String>,
    column: Option<String>,
    severity: Option<String>,
    code: Option<String>,
    message: Option<String>,
}

/// A matcher hit distilled to annotation fields.
pub(crate) struct Issue {
    pub level: &'static str,
    pub message: String,
    pub file: Option<String>,
    pub line: Option<u64>,
    pub column: Option<u64>,
    pub code: Option<String>,
}

impl IssueMatch {
    /// Merge this pattern's captures over the running match — an unset
    /// field falls through to the earlier pattern's value.
    fn new(running: Option<&IssueMatch>, pattern: &IssuePattern, caps: &regex::Captures) -> Self {
        let get = |idx: Option<usize>| {
            idx.and_then(|i| caps.get(i))
                .map(|m| m.as_str().to_string())
        };
        IssueMatch {
            file: running
                .and_then(|r| r.file.clone())
                .or_else(|| get(pattern.file)),
            from_path: running
                .and_then(|r| r.from_path.clone())
                .or_else(|| get(pattern.from_path)),
            line: running
                .and_then(|r| r.line.clone())
                .or_else(|| get(pattern.line)),
            column: running
                .and_then(|r| r.column.clone())
                .or_else(|| get(pattern.column)),
            severity: running
                .and_then(|r| r.severity.clone())
                .or_else(|| get(pattern.severity)),
            code: running
                .and_then(|r| r.code.clone())
                .or_else(|| get(pattern.code)),
            message: running
                .and_then(|r| r.message.clone())
                .or_else(|| get(pattern.message)),
        }
    }
}

impl ProblemMatcher {
    /// Scan one output line — the IssueMatcher state machine. Returns the
    /// merged match on a hit and advances/resets multi-pattern state.
    pub fn match_line(&mut self, line: &str) -> Option<IssueMatch> {
        if self.patterns.len() == 1 {
            let caps = self.patterns[0].regex.captures(line)?;
            return Some(IssueMatch::new(None, &self.patterns[0], &caps));
        }
        // Iterate patterns in reverse — a later pattern is only eligible
        // when every pattern before it holds a running match. A hit on the
        // last pattern returns immediately (C# `return`) — continuing would
        // let lower patterns clobber the state just preserved by `loop`.
        for i in (0..self.patterns.len()).rev() {
            let running = if i > 0 {
                self.state.get(i - 1).and_then(|s| s.clone())
            } else {
                None
            };
            if i > 0 && running.is_none() {
                continue;
            }
            let is_last = i == self.patterns.len() - 1;
            match self.patterns[i].regex.captures(line) {
                Some(caps) => {
                    let m = IssueMatch::new(running.as_ref(), &self.patterns[i], &caps);
                    if is_last {
                        if self.patterns[i].looped {
                            self.reset();
                            self.state[i - 1] = running;
                        } else {
                            self.reset();
                        }
                        return Some(m);
                    } else {
                        self.state[i] = Some(m);
                    }
                }
                None => {
                    if is_last {
                        self.state[i - 1] = None;
                    } else {
                        self.state[i] = None;
                    }
                }
            }
        }
        None
    }

    fn reset(&mut self) {
        self.state.fill(None);
    }

    /// Public reset — another matcher's hit clears this matcher's
    /// multi-pattern running state (OutputManager semantics).
    pub(crate) fn reset_pub(&mut self) {
        self.reset();
    }

    /// Severity → annotation level. Empty/absent → matcher default →
    /// `error`. Unrecognized values drop the issue (GitHub debug-logs and
    /// skips it — the raw line still reaches the log).
    fn level(&self, m: &IssueMatch) -> Option<&'static str> {
        let sev = m
            .severity
            .as_deref()
            .filter(|s| !s.is_empty())
            .unwrap_or(&self.default_severity);
        if sev.is_empty() || sev.eq_ignore_ascii_case("error") {
            Some("error")
        } else if sev.eq_ignore_ascii_case("warning") {
            Some("warning")
        } else if sev.eq_ignore_ascii_case("notice") {
            Some("notice")
        } else {
            None
        }
    }
}

// ---------- config file ----------

#[derive(Deserialize)]
struct MatchersConfig {
    #[serde(rename = "problemMatcher", default)]
    matchers: Vec<MatcherConfig>,
}

#[derive(Deserialize)]
struct MatcherConfig {
    owner: Option<String>,
    #[serde(default)]
    severity: String,
    #[serde(rename = "fromPath", default)]
    from_path: String,
    #[serde(default)]
    pattern: Vec<PatternConfig>,
}

#[derive(Deserialize)]
struct PatternConfig {
    regexp: Option<String>,
    file: Option<usize>,
    #[serde(rename = "fromPath")]
    from_path: Option<usize>,
    line: Option<usize>,
    column: Option<usize>,
    severity: Option<usize>,
    code: Option<usize>,
    message: Option<usize>,
    #[serde(rename = "loop", default)]
    looped: bool,
}

/// Load and validate a matcher config file. Errors become runner warnings —
/// a malformed matcher disables itself, it never fails the step.
pub(crate) fn load_matchers(path: &Path) -> Result<Vec<ProblemMatcher>, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read matcher file {}: {e}", path.display()))?;
    let cfg: MatchersConfig = serde_json::from_str(&text)
        .map_err(|e| format!("invalid matcher file {}: {e}", path.display()))?;
    if cfg.matchers.is_empty() {
        return Err(format!(
            "matcher file {} defines no matchers",
            path.display()
        ));
    }
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for m in &cfg.matchers {
        let owner = m.owner.clone().unwrap_or_default();
        if owner.is_empty() {
            return Err("problem matcher is missing required 'owner'".into());
        }
        if !seen.insert(owner.to_ascii_lowercase()) {
            return Err(format!("duplicate owner name '{owner}'"));
        }
        if m.pattern.is_empty() {
            return Err(format!("problem matcher '{owner}' has no patterns"));
        }
        if !m.pattern.iter().any(|p| p.message.is_some()) {
            return Err(format!(
                "problem matcher '{owner}' does not map a capture group to 'message'"
            ));
        }
        if m.pattern[..m.pattern.len() - 1].iter().any(|p| p.looped) {
            return Err(format!(
                "problem matcher '{owner}' sets 'loop' on a non-final pattern"
            ));
        }
        let mut patterns = Vec::with_capacity(m.pattern.len());
        for p in &m.pattern {
            let src = p.regexp.clone().ok_or_else(|| {
                format!("problem matcher '{owner}' has a pattern without 'regexp'")
            })?;
            let regex = Regex::new(&src)
                .map_err(|e| format!("problem matcher '{owner}' has an invalid 'regexp': {e}"))?;
            patterns.push(IssuePattern {
                file: p.file,
                from_path: p.from_path,
                line: p.line,
                column: p.column,
                severity: p.severity,
                code: p.code,
                message: p.message,
                looped: p.looped,
                regex,
            });
        }
        out.push(ProblemMatcher {
            owner,
            default_severity: m.severity.clone(),
            default_from_path: m.from_path.clone(),
            state: vec![None; m.pattern.len() - 1],
            patterns,
        });
    }
    Ok(out)
}

// ---------- ANSI stripping + file resolution ----------

/// `\x1b\[[0-9;]*m?` — colour codes are stripped before matching (the
/// matcher regexes are written for plain text).
static COLOR_CODES: std::sync::LazyLock<Regex> =
    std::sync::LazyLock::new(|| Regex::new("\u{1b}\\[[0-9;]*m?").unwrap());

pub(crate) fn strip_ansi(line: &str) -> std::borrow::Cow<'_, str> {
    if line.contains('\u{1b}') {
        COLOR_CODES.replace_all(line, "")
    } else {
        std::borrow::Cow::Borrowed(line)
    }
}

/// Resolve a match's `file` value the way `OutputManager` does: `fromPath`
/// roots a relative file, then the workspace roots whatever is left. The
/// file only survives when it exists AND sits under the workspace —
/// anything else is dropped (the annotation keeps line/col/message).
fn resolve_file(m: &IssueMatch, matcher: &ProblemMatcher, workspace: &Path) -> Option<String> {
    let file = m.file.as_deref().filter(|f| !f.trim().is_empty())?;
    let mut path = PathBuf::from(file);
    if path.is_relative() {
        let from = m
            .from_path
            .as_deref()
            .filter(|f| !f.trim().is_empty())
            .unwrap_or(&matcher.default_from_path);
        if !from.is_empty() {
            if let Some(dir) = Path::new(from).parent() {
                path = dir.join(&path);
            }
        }
    }
    if path.is_relative() {
        path = workspace.join(&path);
    }
    // Normalize `.`/`..` without requiring the path to exist first.
    let mut norm = PathBuf::new();
    for c in path.components() {
        match c {
            std::path::Component::ParentDir => {
                norm.pop();
            }
            std::path::Component::CurDir => {}
            other => norm.push(other.as_os_str()),
        }
    }
    if !norm.is_file() {
        return None;
    }
    let rel = norm.strip_prefix(workspace).ok()?;
    Some(
        rel.components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/"),
    )
}

/// Convert a hit into an annotation issue — `None` on an unsupported
/// severity (the raw line still forwards to the log).
pub(crate) fn convert(
    m: &IssueMatch,
    matcher: &ProblemMatcher,
    workspace: Option<&Path>,
    raw_line: &str,
) -> Option<Issue> {
    let level = matcher.level(m)?;
    let file = workspace.and_then(|ws| resolve_file(m, matcher, ws));
    Some(Issue {
        level,
        message: m
            .message
            .clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| raw_line.to_string()),
        file,
        line: m.line.as_deref().and_then(|l| l.parse().ok()),
        column: m.column.as_deref().and_then(|c| c.parse().ok()),
        code: m.code.clone().filter(|c| !c.trim().is_empty()),
    })
}
