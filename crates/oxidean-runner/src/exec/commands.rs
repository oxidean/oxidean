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
//!
//! File-targeted commands write into the same `runner_temp` files the
//! `$GITHUB_*` env-file protocol uses, so the existing post-step drain
//! applies them with identical ordering. `set-env`/`add-path` are applied
//! (Gitea/`act` parity) — GitHub hard-fails them server-side; the deprecation
//! warning keeps the divergence loud rather than silent.
//!
//! `add-matcher`/`remove-matcher` stay warned-and-passed-through: problem
//! matchers are not implemented.

use std::path::PathBuf;
use std::sync::atomic::Ordering;

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
                    out.push_str(body);
                    out.push_str(nl);
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
            "add-matcher" | "remove-matcher" => {
                self.queue_warn(
                    match cmd {
                        "add-matcher" => "add-matcher",
                        _ => "remove-matcher",
                    },
                    format!(
                        "workflow command '::{cmd}::' is not interpreted — problem matchers are unsupported"
                    ),
                );
                Verdict::Drop
            }
            _ => Verdict::Forward,
        }
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
    fn echo_toggle_and_matchers_are_consumed() {
        let l = logger();
        // `echo` is an interpreted no-op — we never echo commands. Matchers
        // drop with a once-queued warning (problem matchers unsupported).
        let out = l.interpret_chunk(
            "::echo::on\n::add-matcher::m.json\n::remove-matcher owner=x::\n::echo::off\n",
        );
        assert_eq!(out, "");
        assert_eq!(l.pending_warnings.lock().unwrap().len(), 2);
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
