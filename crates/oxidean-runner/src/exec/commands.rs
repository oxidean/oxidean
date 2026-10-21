//! `::cmd key=v::payload` — the GitHub runner workflow-command protocol.
//!
//! Interpreted inside [`Logger::send`] so `run:` steps, `uses:` actions and
//! post actions share one implementation. Effects split by kind:
//!
//! | command                     | effect                                         |
//! |-----------------------------|------------------------------------------------|
//! | `::add-mask::v`             | appended to the job mask list                  |
//! | `::group::t` / `::endgroup::` | `##[group]`/`##[endgroup]` structural markers |
//! | `::debug::msg`              | forwarded only when step-debug is enabled      |
//! | `::set-output name=k::v`    | `k=v` → `$GITHUB_OUTPUT` file (deprecation warn) |
//! | `::save-state name=k::v`    | `k=v` → `$GITHUB_STATE` file (deprecation warn)|
//! | `::set-env name=k::v`       | `k=v` → `$GITHUB_ENV` file (deprecation warn)  |
//! | `::add-path::p`             | `p` → `$GITHUB_PATH` file (deprecation warn)   |
//! | `::echo::on` / `::echo::off`| consumed — command echoing is never on here    |
//! | `::stop-commands::t`        | literal forwards until `::t::`                 |
//! | `::error/warning/notice::m` | `##[level]m` + `##[ann]{json}` annotation marker |
//! | `::add-matcher::f`        | registers problem matchers; hits become annotations |
//! | `::remove-matcher owner=o::` | drops matchers registered under `o`             |
//!
//! File-targeted commands write into the same `runner_temp` files the
//! `$GITHUB_*` env-file protocol uses, so the existing post-step drain
//! applies them with identical ordering. `set-env`/`add-path` are applied
//! (Gitea/`act` parity) — GitHub hard-fails them server-side; the deprecation
//! warning keeps the divergence loud rather than silent.

use std::path::PathBuf;
use std::sync::atomic::Ordering;

use super::matchers;
use super::Logger;

/// What one interpreted line becomes in the forwarded log chunk.
#[derive(Debug, PartialEq)]
pub(crate) enum Verdict {
    /// Pass through unchanged.
    Forward,
    /// Consumed by the runner — nothing reaches the log.
    Drop,
    /// Substituted line (group markers, re-emitted debug lines).
    Replace(String),
}

/// `%`-unescape for command payloads (`%0D`, `%0A`, `%25`).
fn unescape_data(s: &str) -> String {
    s.replace("%0D", "\r")
        .replace("%0A", "\n")
        .replace("%25", "%")
}

/// `%`-unescape for command property values — adds `%3A` (`:`) and `%2C` (`,`).
fn unescape_prop(s: &str) -> String {
    unescape_data(&s.replace("%3A", ":").replace("%2C", ","))
}

/// `name=value` out of a `k=v,k=v` command property block.
fn param<'a>(params: &'a str, key: &str) -> Option<&'a str> {
    params
        .split(',')
        .find_map(|p| p.split_once('=').filter(|(k, _)| k.trim() == key))
        .map(|(_, v)| v.trim())
}

/// `k=v` line for the env/output/state files — heredoc form for multiline
/// values (same grammar `parse_env_file` already reads).
fn kv_line(key: &str, value: &str) -> String {
    if value.contains('\n') || value.contains('\r') {
        format!("{key}<<OXIDEAN_EOF\n{value}\nOXIDEAN_EOF\n")
    } else {
        format!("{key}={value}\n")
    }
}

impl<'a> Logger<'a> {
    /// `runner_temp` dir is prepared after the Logger exists (claim errors can
    /// still log before it lands) — set once, read by file-command writes.
    pub(crate) fn set_runner_temp(&self, dir: PathBuf) {
        let _ = self.runner_temp.set(dir);
    }

    /// Checkout workspace — roots `::add-matcher::` paths and the `file`
    /// existence check on matcher hits. Set once after `prepare_workspace`.
    pub(crate) fn set_workspace(&self, dir: PathBuf) {
        let _ = self.workspace.set(dir);
    }

    /// Step boundary — matchers and the annotation cap are per-step state
    /// (GitHub builds a fresh OutputManager per step execution).
    pub(crate) fn reset_step_state(&self) {
        self.matchers.lock().unwrap().clear();
        self.annotation_counts.lock().unwrap().clear();
    }

    /// `ACTIONS_STEP_DEBUG` gate — `::debug::` lines forward only when set.
    pub(crate) fn set_step_debug(&self, on: bool) {
        self.step_debug.store(on, Ordering::Relaxed);
    }

    /// Queue a warning for emission after the current send — `interpret_line`
    /// is sync and cannot `await` the upload itself.
    fn queue_warn(&self, key: &'static str, text: String) {
        let mut seen = self.seen_cmd_warns.lock().unwrap();
        if seen.insert(key) {
            self.pending_warnings.lock().unwrap().push(text);
        }
    }

    /// Queue an unconditional warning (e.g. malformed command — not
    /// deduplicated the way uninterpreted-command notices are).
    fn queue_warn_always(&self, text: String) {
        self.pending_warnings.lock().unwrap().push(text);
    }

    /// Drain warnings queued during [`interpret_chunk`] — emitted via
    /// `send_masked` so they never re-enter interpretation.
    pub(crate) async fn drain_warnings(&self) {
        let pending: Vec<String> = std::mem::take(&mut *self.pending_warnings.lock().unwrap());
        for w in pending {
            let w = format!("::warning::{w}\n");
            self.send_masked(&w).await;
        }
    }

    /// Interpret the workflow commands in a raw output chunk and return the
    /// text to log. Buffers an unterminated tail so a command split across
    /// chunk boundaries still parses.
    pub(crate) fn interpret_chunk(&self, chunk: &str) -> String {
        let mut data = {
            let mut p = self.partial_line.lock().unwrap();
            if p.is_empty() {
                chunk.to_string()
            } else {
                let mut s = std::mem::take(&mut *p);
                s.push_str(chunk);
                s
            }
        };
        // Hold back the unterminated tail for the next chunk.
        if !data.ends_with('\n') {
            match data.rfind('\n') {
                Some(i) => {
                    let tail = data.split_off(i + 1);
                    *self.partial_line.lock().unwrap() = tail;
                }
                None => {
                    *self.partial_line.lock().unwrap() = std::mem::take(&mut data);
                }
            }
        }
        let mut out = String::with_capacity(data.len());
        for line in data.split_inclusive('\n') {
            let (body, nl) = line
                .strip_suffix('\n')
                .map(|b| (b, "\n"))
                .unwrap_or((line, ""));
            match self.interpret_line(body) {
                Verdict::Forward => {
                    // Problem matchers scan every forwarded line — on a
                    // hit the line logs as `##[level]line` with an
                    // `##[ann]` marker carrying the structured fields.
                    match self.match_line(body) {
                        Some(sub) => {
                            out.push_str(&sub);
                            out.push_str(nl);
                        }
                        None => {
                            out.push_str(body);
                            out.push_str(nl);
                        }
                    }
                }
                Verdict::Replace(r) => {
                    out.push_str(&r);
                    out.push_str(nl);
                }
                Verdict::Drop => {}
            }
        }
        out
    }

    /// Flush a trailing unterminated line so a step boundary can't misplace it.
    pub(crate) async fn flush_partial(&self) {
        let tail = std::mem::take(&mut *self.partial_line.lock().unwrap());
        if !tail.is_empty() {
            let text = self.interpret_chunk(&(tail + "\n"));
            self.send_masked(&super::mask_secrets(&text, self.mask_values))
                .await;
        }
    }

    fn interpret_line(&self, line: &str) -> Verdict {
        // `::stop-commands::token` state — everything is literal until the
        // matching `::token::` line; both commands are consumed.
        {
            let guard = self.stop_token.lock().unwrap();
            if let Some(tok) = guard.as_deref() {
                if line.trim() == format!("::{tok}::") {
                    drop(guard);
                    *self.stop_token.lock().unwrap() = None;
                    return Verdict::Drop;
                }
                // Inside a stop-commands block every other line is literal —
                // GitHub forwards it to the log verbatim rather than
                // interpreting `::cmd::` syntax.
                return Verdict::Forward;
            }
        }
        let Some(rest) = line.strip_prefix("::") else {
            return Verdict::Forward;
        };
        let Some(sep) = rest.find("::") else {
            return Verdict::Forward;
        };
        let (head, payload_raw) = (&rest[..sep], &rest[sep + 2..]);
        let (cmd, params) = match head.find(' ') {
            Some(i) => (head[..i].trim(), &head[i + 1..]),
            None => (head.trim(), ""),
        };
        let payload = unescape_data(payload_raw);
        match cmd {
            // Annotation commands — `::error file=f,line=3::msg` — log as
            // GitHub's `##[level]msg` text and ride an `##[ann]{json}`
            // marker so the server persists a structured annotation
            // (stripped at ingest; never visible in stored logs).
            "error" | "warning" | "notice" => {
                let mut ann = serde_json::json!({
                    "level": cmd,
                    "message": payload,
                });
                if let Some(obj) = ann.as_object_mut() {
                    for (prop, key) in [
                        ("file", "path"),
                        ("line", "line"),
                        ("endLine", "end_line"),
                        ("col", "col"),
                        ("endColumn", "end_col"),
                        ("title", "title"),
                    ] {
                        if let Some(v) = param(params, prop) {
                            let v = unescape_prop(v);
                            if matches!(key, "line" | "end_line" | "col" | "end_col") {
                                if let Ok(n) = v.parse::<u64>() {
                                    obj.insert(key.into(), serde_json::json!(n));
                                }
                            } else {
                                obj.insert(key.into(), serde_json::json!(v));
                            }
                        }
                    }
                }
                Verdict::Replace(format!("##[ann]{ann}\n##[{cmd}]{payload}"))
            }
            "group" => Verdict::Replace(format!("##[group]{payload}")),
            "endgroup" => Verdict::Replace("##[endgroup]".into()),
            "debug" => {
                if self.step_debug.load(Ordering::Relaxed) {
                    Verdict::Forward
                } else {
                    Verdict::Drop
                }
            }
            "add-mask" => {
                if !payload.is_empty() {
                    self.extra_masks.lock().unwrap().push(payload);
                }
                Verdict::Drop
            }
            "stop-commands" => {
                let tok = payload.trim();
                if tok.is_empty() {
                    self.queue_warn_always("::stop-commands:: requires a token — ignored".into());
                } else {
                    *self.stop_token.lock().unwrap() = Some(tok.to_string());
                }
                Verdict::Drop
            }
            // We never echo commands, so the toggle is an interpreted no-op.
            "echo" => Verdict::Drop,
            "set-output" | "save-state" | "set-env" | "add-path" => {
                self.apply_file_command(cmd, params, &payload)
            }
            "add-matcher" => {
                // `::add-matcher::path` — resolved relative to the
                // workspace (GitHub resolves against the action's dir for
                // `uses:` steps; workspace is our closest stable root).
                let rel = payload.trim();
                let path = match self.workspace.get() {
                    Some(ws) if !rel.is_empty() && !PathBuf::from(rel).is_absolute() => {
                        ws.join(rel)
                    }
                    _ => PathBuf::from(rel),
                };
                match matchers::load_matchers(&path) {
                    Ok(new) => {
                        let mut matchers = self.matchers.lock().unwrap();
                        for m in new {
                            // Same-owner clobbers; new matchers prepend so
                            // later registrations take precedence.
                            matchers.retain(|x| !x.owner.eq_ignore_ascii_case(&m.owner));
                            matchers.insert(0, m);
                        }
                    }
                    Err(e) => self.queue_warn_always(format!("add-matcher ignored: {e}")),
                }
                Verdict::Drop
            }
            "remove-matcher" => {
                // `::remove-matcher owner=<o>::` — drops every matcher
                // registered under `o` (case-insensitive).
                if let Some(owner) = param(params, "owner") {
                    let owner = unescape_prop(owner);
                    self.matchers
                        .lock()
                        .unwrap()
                        .retain(|m| !m.owner.eq_ignore_ascii_case(&owner));
                }
                Verdict::Drop
            }
            _ => Verdict::Forward,
        }
    }

    /// Scan one forwarded line through the registered problem matchers.
    /// A hit becomes `##[ann]{json}\n##[level]<stripped line>` — the
    /// `##[ann]` marker rides the annotation side channel, the visible
    /// line is GitHub's decorated form. On a hit every *other* matcher
    /// resets its multi-line state (OutputManager semantics).
    fn match_line(&self, line: &str) -> Option<String> {
        {
            let matchers = self.matchers.lock().unwrap();
            if matchers.is_empty() {
                return None;
            }
        }
        let stripped = matchers::strip_ansi(line);
        let mut list = self.matchers.lock().unwrap();
        let mut hit_idx = None;
        let mut issue = None;
        for (i, m) in list.iter_mut().enumerate() {
            if let Some(matched) = m.match_line(&stripped) {
                issue = matchers::convert(
                    &matched,
                    m,
                    self.workspace.get().map(|p| p.as_path()),
                    &stripped,
                );
                hit_idx = Some(i);
                break;
            }
        }
        let hit = hit_idx?;
        for (i, m) in list.iter_mut().enumerate() {
            if i != hit {
                m.reset_pub();
            }
        }
        drop(list);
        let issue = issue?; // unsupported severity — line forwards raw
                            // 10-annotation cap per level per step (GitHub's step cap).
        {
            let mut counts = self.annotation_counts.lock().unwrap();
            let n = counts.entry(issue.level).or_insert(0);
            if *n >= 10 {
                let key: &'static str = match issue.level {
                    "error" => "cap:error",
                    "warning" => "cap:warning",
                    _ => "cap:notice",
                };
                drop(counts);
                self.queue_warn(
                    key,
                    format!(
                        "skipped a {level} annotation — the step already emitted the 10-annotation limit",
                        level = issue.level,
                    ),
                );
                return None;
            }
            *n += 1;
        }
        let mut ann = serde_json::json!({
            "level": issue.level,
            "message": issue.message,
        });
        if let Some(obj) = ann.as_object_mut() {
            if let Some(f) = issue.file {
                obj.insert("path".into(), serde_json::json!(f));
            }
            if let Some(l) = issue.line {
                obj.insert("line".into(), serde_json::json!(l));
            }
            if let Some(c) = issue.column {
                obj.insert("col".into(), serde_json::json!(c));
            }
            if let Some(t) = issue.code {
                obj.insert("title".into(), serde_json::json!(t));
            }
        }
        Some(format!(
            "##[ann]{ann}\n##[{level}]{stripped}",
            level = issue.level
        ))
    }

    /// `set-output`/`save-state`/`set-env`/`add-path` → append the equivalent
    /// `k=v` entry to the job's command file so the post-step drain applies it.
    fn apply_file_command(&self, cmd: &str, params: &str, payload: &str) -> Verdict {
        let (file, name_required) = match cmd {
            "set-output" => ("output", true),
            "save-state" => ("state", true),
            "set-env" => ("env", true),
            _ => ("path", false),
        };
        self.queue_warn(
            match cmd {
                "set-output" => "set-output",
                "save-state" => "save-state",
                "set-env" => "set-env",
                _ => "add-path",
            },
            format!("workflow command '::{cmd}::' is deprecated — prefer the $GITHUB_* env files"),
        );
        let entry = if name_required {
            match param(params, "name") {
                Some(n) if !n.is_empty() => kv_line(&unescape_prop(n), payload),
                _ => {
                    self.queue_warn_always(format!(
                        "workflow command '::{cmd}::' is missing the `name` property — ignored"
                    ));
                    return Verdict::Drop;
                }
            }
        } else {
            format!("{payload}\n")
        };
        match self.runner_temp.get() {
            Some(dir) => {
                let path = dir.join(file);
                use std::io::Write;
                match std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&path)
                {
                    Ok(mut f) => {
                        if let Err(e) = f.write_all(entry.as_bytes()) {
                            self.queue_warn_always(format!("workflow command write failed: {e}"));
                        }
                    }
                    Err(e) => self.queue_warn_always(format!("workflow command write failed: {e}")),
                }
            }
            None => self.queue_warn_always(format!(
                "workflow command '::{cmd}::' before runner temp init — ignored"
            )),
        }
        Verdict::Drop
    }
}

#[cfg(test)]
mod tests {
    use crate::client::Client;
    use crate::exec::Logger;
    use std::collections::BTreeSet;
    use std::sync::atomic::{AtomicBool, AtomicI64};
    use std::sync::{Mutex, OnceLock};

    fn logger() -> Logger<'static> {
        static CLIENT: std::sync::OnceLock<Client> = std::sync::OnceLock::new();
        let client = CLIENT.get_or_init(|| Client::new("", None));
        Logger {
            client,
            run_id: "r".into(),
            job_id: "j".into(),
            mask_values: &[],
            extra_masks: Mutex::new(Vec::new()),
            stop_token: Mutex::new(None),
            partial_line: Mutex::new(String::new()),
            pending_warnings: Mutex::new(Vec::new()),
            cancelled: AtomicBool::new(false),
            log_cursor: AtomicI64::new(0),
            seen_cmd_warns: Mutex::new(BTreeSet::new()),
            runner_temp: OnceLock::new(),
            step_debug: AtomicBool::new(false),
            matchers: Mutex::new(Vec::new()),
            workspace: OnceLock::new(),
            annotation_counts: Mutex::new(std::collections::BTreeMap::new()),
        }
    }

    #[test]
    fn group_commands_emit_markers() {
        let l = logger();
        let out = l.interpret_chunk("::group::go env\nA=1\n::endgroup::\n");
        assert_eq!(out, "##[group]go env\nA=1\n##[endgroup]\n");
    }

    #[test]
    fn debug_lines_gate_on_step_debug() {
        let l = logger();
        assert_eq!(l.interpret_chunk("::debug::hidden\n"), "");
        l.set_step_debug(true);
        assert_eq!(l.interpret_chunk("::debug::shown\n"), "::debug::shown\n");
    }

    #[test]
    fn add_mask_extends_masks() {
        let l = logger();
        assert_eq!(l.interpret_chunk("::add-mask::hunter2\n"), "");
        assert_eq!(l.extra_masks.lock().unwrap().as_slice(), &["hunter2"]);
    }

    #[test]
    fn stop_commands_suppresses_until_token() {
        let l = logger();
        // Commands inside the block are literal text — forwarded to the log
        // but not interpreted (GitHub semantics: the text must still appear).
        let out = l.interpret_chunk(
            "::stop-commands::t9\n::add-mask::nope\nplain line\n::t9::\n::add-mask::yes\n",
        );
        assert_eq!(out, "::add-mask::nope\nplain line\n");
        let masks = l.extra_masks.lock().unwrap();
        assert_eq!(masks.as_slice(), &["yes"]);
    }

    #[test]
    fn file_commands_land_in_runner_temp_files() {
        let l = logger();
        let dir = tempfile::tempdir().unwrap();
        l.set_runner_temp(dir.path().to_path_buf());
        let out = l.interpret_chunk(
            "::set-output name=answer::42\n::set-env name=A::b\n::add-path::/opt/x\n::save-state name=S::1\n",
        );
        assert_eq!(out, "");
        assert_eq!(
            std::fs::read_to_string(dir.path().join("output")).unwrap(),
            "answer=42\n"
        );
        assert_eq!(
            std::fs::read_to_string(dir.path().join("env")).unwrap(),
            "A=b\n"
        );
        assert_eq!(
            std::fs::read_to_string(dir.path().join("path")).unwrap(),
            "/opt/x\n"
        );
        assert_eq!(
            std::fs::read_to_string(dir.path().join("state")).unwrap(),
            "S=1\n"
        );
        // Deprecation warns queued once per command.
        assert_eq!(l.pending_warnings.lock().unwrap().len(), 4);
    }

    #[test]
    fn commands_split_across_chunks_still_parse() {
        let l = logger();
        assert_eq!(l.interpret_chunk("noise\n::add-"), "noise\n");
        // `rest` has no trailing newline — it stays buffered until one lands.
        assert_eq!(l.interpret_chunk("mask::s3cr3t\nrest"), "");
        assert_eq!(l.extra_masks.lock().unwrap().as_slice(), &["s3cr3t"]);
        assert_eq!(l.interpret_chunk(" done\n"), "rest done\n");
    }

    #[test]
    fn annotation_commands_emit_marker_plus_github_line() {
        let l = logger();
        let out = l.interpret_chunk("::error::boom\nplain text with :: inside\n");
        assert_eq!(
            out,
            "##[ann]{\"level\":\"error\",\"message\":\"boom\"}\n##[error]boom\nplain text with :: inside\n"
        );
    }

    #[test]
    fn annotation_props_decode_into_marker() {
        let l = logger();
        let out = l.interpret_chunk(
            "::warning file=src/main.rs,line=3,endLine=4,col=1,endColumn=9,title=Lint%3Aclean::careful\n",
        );
        let (marker, visible) = out.split_once('\n').unwrap();
        assert_eq!(visible, "##[warning]careful\n");
        let ann: serde_json::Value =
            serde_json::from_str(marker.strip_prefix("##[ann]").unwrap()).unwrap();
        assert_eq!(ann["level"], "warning");
        assert_eq!(ann["message"], "careful");
        assert_eq!(ann["path"], "src/main.rs");
        assert_eq!(ann["line"], 3);
        assert_eq!(ann["end_line"], 4);
        assert_eq!(ann["col"], 1);
        assert_eq!(ann["end_col"], 9);
        // `%3A` is a property escape — `%20` is not (URL encoding, not
        // command encoding); a literal colon would split the prop list.
        assert_eq!(ann["title"], "Lint:clean");
    }

    #[test]
    fn echo_toggle_is_consumed() {
        let l = logger();
        // `echo` is an interpreted no-op — we never echo commands.
        let out = l.interpret_chunk("::echo::on\n::echo::off\n");
        assert_eq!(out, "");
        assert!(l.pending_warnings.lock().unwrap().is_empty());
    }

    #[test]
    fn add_matcher_registers_and_remove_drops() {
        let l = logger();
        let dir = tempfile::tempdir().unwrap();
        l.set_workspace(dir.path().to_path_buf());
        std::fs::write(
            dir.path().join("m.json"),
            r#"{"problemMatcher":[{"owner":"demo","pattern":[{"regexp":"^E (\\d+): (.+)$","line":1,"message":2}]}]}"#,
        )
        .unwrap();
        let out = l.interpret_chunk("::add-matcher::m.json\nE 7: blew up\nplain\n");
        let (marker, rest) = out.split_once('\n').unwrap();
        let ann: serde_json::Value =
            serde_json::from_str(marker.strip_prefix("##[ann]").unwrap()).unwrap();
        assert_eq!(ann["level"], "error");
        assert_eq!(ann["message"], "blew up");
        assert_eq!(ann["line"], 7);
        assert_eq!(rest, "##[error]E 7: blew up\nplain\n");
        // ::remove-matcher owner=demo:: stops matching subsequent lines.
        let out = l.interpret_chunk("::remove-matcher owner=demo::\nE 9: again\n");
        assert_eq!(out, "E 9: again\n");
    }

    #[test]
    fn matcher_severity_and_bad_file_warn() {
        let l = logger();
        let dir = tempfile::tempdir().unwrap();
        l.set_workspace(dir.path().to_path_buf());
        std::fs::write(
            dir.path().join("m.json"),
            r#"{"problemMatcher":[{"owner":"w","pattern":[{"regexp":"^W (warning|error): (.+)$","severity":1,"message":2}]}]}"#,
        )
        .unwrap();
        let out = l.interpret_chunk(
            "::add-matcher::missing.json\n::add-matcher::m.json\nW warning: heads up\n",
        );
        // First add-matcher warns (missing file); the hit annotates as
        // warning via the capture group.
        let (marker, rest) = out.split_once('\n').unwrap();
        let ann: serde_json::Value =
            serde_json::from_str(marker.strip_prefix("##[ann]").unwrap()).unwrap();
        assert_eq!(ann["level"], "warning");
        assert_eq!(ann["message"], "heads up");
        assert_eq!(rest, "##[warning]W warning: heads up\n");
        assert_eq!(l.pending_warnings.lock().unwrap().len(), 1);
    }

    #[test]
    fn matcher_multiline_loop_carries_file() {
        let l = logger();
        let dir = tempfile::tempdir().unwrap();
        l.set_workspace(dir.path().to_path_buf());
        // The `file` group lives in the header pattern; the looped last
        // pattern produces one annotation per violation line.
        std::fs::write(dir.path().join("badFile.js"), "x\n").unwrap();
        std::fs::write(
            dir.path().join("m.json"),
            r#"{"problemMatcher":[{"owner":"stylish","pattern":[
                {"regexp":"^([^\\s].*)$","file":1},
                {"regexp":"^\\s+line\\s(\\d+),\\scol\\s(\\d+),\\s(Error|Warning)\\s-\\s(.+)\\s\\((.+)\\)$","line":1,"column":2,"severity":3,"message":4,"code":5,"loop":true}
            ]}]}"#,
        )
        .unwrap();
        let out = l.interpret_chunk(
            "::add-matcher::m.json\nbadFile.js\n  line 50, col 11, Error - 'myVar' unused. (no-unused-vars)\n  line 60, col 2, Warning - 'x' unused. (no-x)\n",
        );
        let anns: Vec<serde_json::Value> = out
            .lines()
            .filter_map(|l| l.strip_prefix("##[ann]"))
            .map(|j| serde_json::from_str(j).unwrap())
            .collect();
        assert_eq!(anns.len(), 2);
        // File captured by the header pattern carries into both hits and
        // resolves workspace-relative.
        assert_eq!(anns[0]["path"], "badFile.js");
        assert_eq!(anns[0]["level"], "error");
        assert_eq!(anns[0]["line"], 50);
        assert_eq!(anns[0]["col"], 11);
        assert_eq!(anns[0]["title"], "no-unused-vars");
        assert_eq!(anns[1]["level"], "warning");
        assert_eq!(anns[1]["line"], 60);
    }

    #[test]
    fn matcher_cap_is_ten_per_level() {
        let l = logger();
        let dir = tempfile::tempdir().unwrap();
        l.set_workspace(dir.path().to_path_buf());
        std::fs::write(
            dir.path().join("m.json"),
            r#"{"problemMatcher":[{"owner":"flood","pattern":[{"regexp":"^E (.+)$","message":1}]}]}"#,
        )
        .unwrap();
        let mut input = "::add-matcher::m.json\n".to_string();
        for i in 0..12 {
            input.push_str(&format!("E fail {i}\n"));
        }
        let out = l.interpret_chunk(&input);
        assert_eq!(out.matches("##[ann]").count(), 10);
        // Lines 11-12 still forward — just raw, no annotation.
        assert!(out.contains("E fail 11\n"));
        assert_eq!(l.pending_warnings.lock().unwrap().len(), 1);
    }

    #[test]
    fn escaped_command_data_decodes() {
        let l = logger();
        let dir = tempfile::tempdir().unwrap();
        l.set_runner_temp(dir.path().to_path_buf());
        // Property escapes (`%3A` `:` `%2C` `,`) decode in names; data escapes
        // (`%0A`, `%25`) decode in payloads — a multiline value lands in
        // heredoc form like the `$GITHUB_OUTPUT` grammar.
        let out = l.interpret_chunk("::set-output name=a%3Ab%2Cc::x%0Ay%25z\n::group::t%25\n");
        assert_eq!(out, "##[group]t%\n");
        assert_eq!(
            std::fs::read_to_string(dir.path().join("output")).unwrap(),
            "a:b,c<<OXIDEAN_EOF\nx\ny%z\nOXIDEAN_EOF\n"
        );
    }
}
