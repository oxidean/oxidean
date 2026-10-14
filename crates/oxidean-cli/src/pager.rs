//! Pager — pipe long output through `OXIDEAN_PAGER` → `PAGER` → `less -FRX`
//! when stdout is a TTY; print directly otherwise (or when spawning fails —
//! the pager is an enhancement, never a failure mode).

use std::io::Write;
use std::process::{Command, Stdio};

use crate::term;

/// Send `text` to the terminal, paging when interactive and `text` is long.
/// Returns `false` if the pager couldn't run — callers should print directly.
fn spawn_pager(text: &str) -> bool {
    let spec = std::env::var("OXIDEAN_PAGER")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| std::env::var("PAGER").ok().filter(|s| !s.trim().is_empty()))
        .unwrap_or_else(|| "less -FRX".to_string());
    if spec.trim() == "cat" {
        return false;
    }
    // Shell-word splitting is deliberate — `PAGER="less -F"` must split.
    let mut words = spec.split_whitespace();
    let Some(prog) = words.next() else {
        return false;
    };
    let mut cmd = Command::new(prog);
    cmd.args(words)
        .stdin(Stdio::piped())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    let Ok(mut child) = cmd.spawn() else {
        return false;
    };
    if let Some(mut stdin) = child.stdin.take() {
        // EPIPE (user quits less early) is normal — ignore write errors.
        let _ = stdin.write_all(text.as_bytes());
        let _ = stdin.write_all(b"\n");
    }
    let _ = child.wait();
    true
}

/// Print `text` — through the pager when it's longer than one screen and
/// stdout is a TTY, directly otherwise.
pub fn emit(text: &str) {
    let lines = text.lines().count();
    if term::stdout_tty() && lines > term_height() && spawn_pager(text) {
        return;
    }
    println!("{text}");
}

/// Always page when interactive (view commands opt into paging even for
/// shorter bodies — `gh pr view` behaves the same).
pub fn emit_paged(text: &str) {
    if term::stdout_tty() && spawn_pager(text) {
        return;
    }
    println!("{text}");
}

fn term_height() -> usize {
    std::env::var("LINES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(24)
}
