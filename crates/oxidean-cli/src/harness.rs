//! Shared agent-harness detection + selection machinery.
//!
//! `ox skill` (skills dirs) and `ox mcp` (MCP config files) both pick rows out
//! of a per-command harness registry through the same knobs —
//! `--harness <id,…>` | `--harness all`, `--detected`, `--project | --global` —
//! and the same TTY prompts. The per-command modules own their registry rows;
//! this module owns everything row-agnostic: home/XDG resolution, installed
//! markers, flag parsing, and interactive selection.

use std::io::IsTerminal;
use std::path::PathBuf;

use crate::commands::CliError;

/// Resolved home/XDG roots — carried through detection + target resolution
/// so tests can point global scope at a tempdir instead of the real `$HOME`.
pub(crate) struct Dirs {
    pub home: Option<PathBuf>,
    pub xdg: PathBuf,
}

impl Dirs {
    pub fn env() -> Self {
        Dirs {
            home: home(),
            xdg: xdg_config(),
        }
    }
}

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
}

fn xdg_config() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .or_else(|| home().map(|h| h.join(".config")))
        .unwrap_or_else(|| PathBuf::from(".config"))
}

/// Evidence that a harness is installed/used — `--detected` and the prompt's
/// `d` select harnesses where any marker hits.
pub(crate) enum Detect {
    /// Repo-relative marker (dir or file) exists.
    Project(&'static str),
    /// `$HOME`-relative marker exists.
    Home(&'static str),
    /// `$XDG_CONFIG_HOME`-relative marker exists.
    Xdg(&'static str),
    /// Executable found on `PATH`.
    Bin(&'static str),
}

/// One row of a per-command harness registry — the pieces detection and
/// selection need. Registry structs (`skill::Harness`, `mcp::McpHarness`)
/// carry their command-specific fields alongside.
pub(crate) trait HarnessRow {
    fn id(&self) -> &'static str;
    fn label(&self) -> &'static str;
    fn markers(&self) -> &'static [Detect];
}

/// Does this row look installed/used? Any marker hit counts.
pub(crate) fn detected_at(dirs: &Dirs, root: &std::path::Path, markers: &[Detect]) -> bool {
    markers.iter().any(|d| match d {
        Detect::Project(p) => {
            if root.as_os_str().is_empty() {
                PathBuf::from(p).exists()
            } else {
                root.join(p).exists()
            }
        }
        Detect::Home(p) => dirs
            .home
            .as_ref()
            .map(|h| h.join(p).exists())
            .unwrap_or(false),
        Detect::Xdg(p) => dirs.xdg.join(p).exists(),
        Detect::Bin(b) => on_path(b),
    })
}

/// Executable search on `PATH` — existence plus exec bit (PATHEXT-style
/// extensions on Windows), no process spawning.
pub(crate) fn on_path(bin: &str) -> bool {
    let Some(paths) = std::env::var_os("PATH") else {
        return false;
    };
    #[cfg(windows)]
    let names: Vec<String> = std::iter::once(bin.to_string())
        .chain(
            ["exe", "cmd", "bat", "ps1"]
                .iter()
                .map(|e| format!("{bin}.{e}")),
        )
        .collect();
    #[cfg(not(windows))]
    let names = [bin];
    std::env::split_paths(&paths).any(|dir| names.iter().any(|n| executable(&dir.join(n))))
}

#[cfg(unix)]
fn executable(p: &std::path::Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    p.metadata()
        .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn executable(p: &std::path::Path) -> bool {
    p.is_file()
}

pub(crate) fn by_id<'a, T: HarnessRow>(table: &'a [T], id: &str) -> Result<&'a T, CliError> {
    table.iter().find(|h| h.id() == id).ok_or_else(|| {
        CliError::Usage(format!(
            "unknown harness {id:?} — valid: {}",
            table.iter().map(|h| h.id()).collect::<Vec<_>>().join(",")
        ))
    })
}

/// `--harness <id,...>` / `all` → row list (deduped, table order).
pub(crate) fn parse_harness_flags<'a, T: HarnessRow>(
    table: &'a [T],
    flags: &[String],
) -> Result<Vec<&'a T>, CliError> {
    let mut out = Vec::new();
    for spec in flags {
        for id in spec.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            if id == "all" {
                for h in table {
                    if !out.iter().any(|x: &&T| x.id() == h.id()) {
                        out.push(h);
                    }
                }
                continue;
            }
            let h = by_id(table, id)?;
            if !out.iter().any(|x: &&T| x.id() == h.id()) {
                out.push(h);
            }
        }
    }
    Ok(out)
}

/// Row list from `--harness`/`--detected`; `detected_only` intersects the
/// flagged set, or means "all detected" when no flags were given.
pub(crate) fn select_harnesses<'a, T: HarnessRow>(
    table: &'a [T],
    dirs: &Dirs,
    root: &std::path::Path,
    harness_flags: &[String],
    detected_only: bool,
) -> Result<Vec<&'a T>, CliError> {
    let selected: Vec<&'a T> = if harness_flags.is_empty() {
        if detected_only {
            table
                .iter()
                .filter(|h| detected_at(dirs, root, h.markers()))
                .collect()
        } else {
            Vec::new()
        }
    } else {
        let mut out = parse_harness_flags(table, harness_flags)?;
        if detected_only {
            out.retain(|h| detected_at(dirs, root, h.markers()));
        }
        out
    };
    if selected.is_empty() {
        return Err(CliError::Usage(if detected_only {
            "no detected harnesses".into()
        } else {
            "no harnesses selected".into()
        }));
    }
    Ok(selected)
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Scope {
    Project,
    Global,
}

/// Resolve scope + row list from flags, prompting on a TTY when unset.
/// `prompt_title` is the picker header (`"Install the ox skill for which
/// harness(es)?"` / the MCP equivalent).
#[allow(clippy::too_many_arguments)]
pub(crate) fn resolve_targets<'a, T: HarnessRow>(
    table: &'a [T],
    dirs: &Dirs,
    root: &std::path::Path,
    harness_flags: &[String],
    detected_only: bool,
    project: bool,
    global: bool,
    prompt_title: &str,
) -> Result<(Scope, Vec<&'a T>), CliError> {
    let tty = std::io::stdin().is_terminal() && std::io::stdout().is_terminal();

    let selected: Vec<&'a T> = if harness_flags.is_empty() && !detected_only {
        if !tty {
            return Err(CliError::Usage(
                "non-interactive: pass --harness <id,...> or --detected, plus --project|--global"
                    .into(),
            ));
        }
        prompt_harnesses(table, dirs, prompt_title)?
    } else {
        select_harnesses(table, dirs, root, harness_flags, detected_only)?
    };

    let scope = match (project, global) {
        (true, false) => Scope::Project,
        (false, true) => Scope::Global,
        (true, true) => {
            return Err(CliError::Usage(
                "--project and --global are exclusive".into(),
            ));
        }
        (false, false) => {
            if !tty {
                return Err(CliError::Usage(
                    "non-interactive: pass --harness <id,...> or --detected, plus --project|--global"
                        .into(),
                ));
            }
            prompt_scope()?
        }
    };
    Ok((scope, selected))
}

fn prompt_scope() -> Result<Scope, CliError> {
    eprintln!("Install scope: [1] project  [2] global (user-level)");
    let mut line = String::new();
    std::io::stdin()
        .read_line(&mut line)
        .map_err(|e| CliError::Usage(format!("read scope: {e}")))?;
    match line.trim() {
        "1" | "" | "p" | "project" => Ok(Scope::Project),
        "2" | "g" | "global" => Ok(Scope::Global),
        other => Err(CliError::Usage(format!("invalid scope {other:?}"))),
    }
}

fn prompt_harnesses<'a, T: HarnessRow>(
    table: &'a [T],
    dirs: &Dirs,
    prompt_title: &str,
) -> Result<Vec<&'a T>, CliError> {
    let empty = std::path::Path::new("");
    let mut detected = Vec::new();
    let mut rest = Vec::new();
    for h in table {
        if detected_at(dirs, empty, h.markers()) {
            detected.push(h);
        } else {
            rest.push(h);
        }
    }
    eprintln!("{prompt_title}");
    if !detected.is_empty() {
        eprintln!("  detected:");
    }
    for (i, h) in detected.iter().chain(&rest).enumerate() {
        eprintln!("  {:2}. {}", i + 1, h.label());
    }
    eprintln!("numbers comma-separated, 'd' for detected, or 'a' for all — `--harness <id>` skips this prompt:");
    let mut line = String::new();
    std::io::stdin()
        .read_line(&mut line)
        .map_err(|e| CliError::Usage(format!("read selection: {e}")))?;
    let line = line.trim();
    if line == "a" || line == "all" {
        return Ok(table.iter().collect());
    }
    if line == "d" || line == "detected" {
        return select_harnesses(table, dirs, std::path::Path::new(""), &[], true);
    }
    let mut out = Vec::new();
    for tok in line.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        let n: usize = tok
            .parse()
            .map_err(|_| CliError::Usage(format!("invalid selection {tok:?}")))?;
        let h = table
            .get(n.wrapping_sub(1))
            .ok_or_else(|| CliError::Usage(format!("selection {n} out of range")))?;
        if !out.iter().any(|x: &&T| x.id() == h.id()) {
            out.push(h);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn on_path_misses_unknown_binary() {
        assert!(!on_path("ox-harness-test-not-a-real-binary"));
    }
}
