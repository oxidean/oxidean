//! `ox skill` — ship the consumer-facing agent SKILL.md.
//!
//! The skill source is single-authored at `crates/oxidean-cli/skill/SKILL.md`
//! and embedded into the binary, so `ox skill print`/`install` always match
//! the `ox` release in hand. The same file is served by the instance at
//! `GET /cli/skill.md` (see `oxidean-api/src/routes/cli_dist.rs`).
//!
//! `ox skill install` writes `<harness-skills-dir>/ox/SKILL.md` for each
//! selected harness. Harness dirs follow the vercel-labs/skills registry
//! convention: most harnesses load `.agents/skills` at project scope but keep
//! a harness-private dir at global scope.

use std::io::IsTerminal;
use std::path::PathBuf;

use crate::args::Global;
use crate::commands::{CliError, Out, Runtime};
use serde_json::{Value, json};

/// The skill document, verbatim.
pub const SKILL_MD: &str = include_str!("../skill/SKILL.md");

/// Where a harness looks for skills at global (user) scope.
#[derive(Clone)]
enum GlobalDir {
    /// `$HOME/<suffix>`
    Home(&'static str),
    /// `$XDG_CONFIG_HOME/<suffix>` or `~/.config/<suffix>`
    Xdg(&'static str),
    /// `$<env>/skills` or `~/<fallback>`
    Env(&'static str, &'static str),
}

/// Evidence that a harness is installed/used — `--detected` and the prompt's
/// `d` select harnesses where any marker hits.
enum Detect {
    /// Repo-relative marker (dir or file) exists.
    Project(&'static str),
    /// `$HOME`-relative marker exists.
    Home(&'static str),
    /// `$XDG_CONFIG_HOME`-relative marker exists.
    Xdg(&'static str),
    /// Executable found on `PATH`.
    Bin(&'static str),
}

struct Harness {
    /// `--harness <id>` selector.
    id: &'static str,
    label: &'static str,
    /// Repo-relative dir, `None` = global-scope only.
    project: Option<&'static str>,
    global: GlobalDir,
    /// Installed-harness markers; first hit wins.
    detect: &'static [Detect],
}

/// Curated table matching the vercel-labs/skills registry for the common
/// harnesses; unknown harnesses can still consume `.agents/skills` via the
/// `generic` entry.
const HARNESSES: &[Harness] = &[
    Harness { id: "claude",   label: "Claude Code",      project: Some(".claude/skills"),   global: GlobalDir::Env("CLAUDE_CONFIG_DIR", ".claude/skills"),
        detect: &[Detect::Project(".claude"), Detect::Home(".claude"), Detect::Bin("claude")] },
    Harness { id: "cursor",   label: "Cursor",           project: Some(".agents/skills"),   global: GlobalDir::Home(".cursor/skills"),
        detect: &[Detect::Project(".cursor"), Detect::Home(".cursor"), Detect::Bin("cursor-agent"), Detect::Bin("agent")] },
    Harness { id: "codex",    label: "Codex",            project: Some(".agents/skills"),   global: GlobalDir::Home(".agents/skills"),
        detect: &[Detect::Project(".codex"), Detect::Home(".codex"), Detect::Bin("codex")] },
    Harness { id: "copilot",  label: "GitHub Copilot",   project: Some(".agents/skills"),   global: GlobalDir::Home(".copilot/skills"),
        detect: &[Detect::Project(".copilot"), Detect::Home(".copilot"), Detect::Bin("copilot")] },
    Harness { id: "gemini",   label: "Gemini CLI",       project: Some(".agents/skills"),   global: GlobalDir::Home(".gemini/skills"),
        detect: &[Detect::Project(".gemini"), Detect::Home(".gemini"), Detect::Bin("gemini")] },
    Harness { id: "opencode", label: "OpenCode",         project: Some(".agents/skills"),   global: GlobalDir::Xdg("opencode/skills"),
        detect: &[Detect::Project(".opencode"), Detect::Xdg("opencode"), Detect::Bin("opencode")] },
    Harness { id: "amp",      label: "Amp",              project: Some(".agents/skills"),   global: GlobalDir::Xdg("agents/skills"),
        detect: &[Detect::Project(".amp"), Detect::Xdg("amp"), Detect::Bin("amp")] },
    Harness { id: "devin",    label: "Devin",            project: Some(".devin/skills"),    global: GlobalDir::Xdg("devin/skills"),
        detect: &[Detect::Project(".devin"), Detect::Xdg("devin"), Detect::Bin("devin")] },
    Harness { id: "windsurf", label: "Windsurf",         project: Some(".windsurf/skills"), global: GlobalDir::Home(".codeium/windsurf/skills"),
        detect: &[Detect::Project(".windsurf"), Detect::Home(".codeium/windsurf"), Detect::Bin("windsurf")] },
    Harness { id: "kiro",     label: "Kiro CLI",         project: Some(".kiro/skills"),     global: GlobalDir::Home(".kiro/skills"),
        detect: &[Detect::Project(".kiro"), Detect::Home(".kiro"), Detect::Bin("kiro"), Detect::Bin("kiro-cli")] },
    Harness { id: "roo",      label: "Roo Code",         project: Some(".roo/skills"),      global: GlobalDir::Home(".roo/skills"),
        detect: &[Detect::Project(".roo"), Detect::Home(".roo")] },
    Harness { id: "goose",    label: "Goose",            project: Some(".goose/skills"),    global: GlobalDir::Xdg("goose/skills"),
        detect: &[Detect::Project(".goose"), Detect::Xdg("goose"), Detect::Bin("goose")] },
    Harness { id: "cline",    label: "Cline",            project: Some(".agents/skills"),   global: GlobalDir::Home(".agents/skills"),
        detect: &[Detect::Project(".cline"), Detect::Home(".cline"), Detect::Bin("cline")] },
    Harness { id: "openclaw", label: "OpenClaw",         project: Some("skills"),           global: GlobalDir::Home(".openclaw/skills"),
        detect: &[Detect::Project(".openclaw"), Detect::Home(".openclaw"), Detect::Bin("openclaw")] },
    Harness { id: "generic",  label: "Any `.agents/skills` harness", project: Some(".agents/skills"), global: GlobalDir::Home(".agents/skills"),
        detect: &[Detect::Project(".agents"), Detect::Home(".agents")] },
];

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

fn global_dir(h: &Harness) -> Result<PathBuf, CliError> {
    match h.global {
        GlobalDir::Home(suffix) => home()
            .map(|h| h.join(suffix))
            .ok_or_else(|| CliError::Usage("no home directory — cannot resolve global scope".into())),
        GlobalDir::Xdg(suffix) => Ok(xdg_config().join(suffix)),
        GlobalDir::Env(var, fallback) => std::env::var_os(var)
            .map(PathBuf::from)
            .filter(|p| !p.as_os_str().is_empty())
            .map(|p| p.join("skills"))
            .or_else(|| home().map(|h| h.join(fallback)))
            .ok_or_else(|| CliError::Usage(
                "no home directory — cannot resolve global scope".into(),
            )),
    }
}

fn harness_by_id(id: &str) -> Result<&'static Harness, CliError> {
    HARNESSES
        .iter()
        .find(|h| h.id == id)
        .ok_or_else(|| {
            CliError::Usage(format!(
                "unknown harness {id:?} — valid: {}",
                HARNESSES.iter().map(|h| h.id).collect::<Vec<_>>().join(",")
            ))
        })
}

/// Does this harness look installed/used? Any marker hit counts.
fn detected_at(root: &std::path::Path, h: &Harness) -> bool {
    h.detect.iter().any(|d| match d {
        Detect::Project(p) => {
            if root.as_os_str().is_empty() {
                PathBuf::from(p).exists()
            } else {
                root.join(p).exists()
            }
        }
        Detect::Home(p) => home().map(|h| h.join(p).exists()).unwrap_or(false),
        Detect::Xdg(p) => xdg_config().join(p).exists(),
        Detect::Bin(b) => on_path(b),
    })
}

/// Executable search on `PATH` — existence plus exec bit (PATHEXT-style
/// extensions on Windows), no process spawning.
fn on_path(bin: &str) -> bool {
    let Some(paths) = std::env::var_os("PATH") else {
        return false;
    };
    #[cfg(windows)]
    let names: Vec<String> = std::iter::once(bin.to_string())
        .chain(["exe", "cmd", "bat", "ps1"].iter().map(|e| format!("{bin}.{e}")))
        .collect();
    #[cfg(not(windows))]
    let names = [bin];
    std::env::split_paths(&paths)
        .any(|dir| names.iter().any(|n| executable(&dir.join(n))))
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

/// `--harness <id,...>` / `all` → harness list (deduped, table order).
fn parse_harness_flags(flags: &[String]) -> Result<Vec<&'static Harness>, CliError> {
    let mut out = Vec::new();
    for spec in flags {
        for id in spec.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            if id == "all" {
                for h in HARNESSES {
                    if !out.iter().any(|x: &&Harness| x.id == h.id) {
                        out.push(h);
                    }
                }
                continue;
            }
            let h = harness_by_id(id)?;
            if !out.iter().any(|x: &&Harness| x.id == h.id) {
                out.push(h);
            }
        }
    }
    Ok(out)
}

/// Harness list from `--harness`/`--detected`; `detected_only` intersects the
/// flagged set, or means "all detected" when no flags were given.
fn select_harnesses(
    root: &std::path::Path,
    harness_flags: &[String],
    detected_only: bool,
) -> Result<Vec<&'static Harness>, CliError> {
    let selected: Vec<&'static Harness> = if harness_flags.is_empty() {
        if detected_only {
            HARNESSES
                .iter()
                .filter(|h| detected_at(root, h))
                .collect()
        } else {
            Vec::new()
        }
    } else {
        let mut out = parse_harness_flags(harness_flags)?;
        if detected_only {
            out.retain(|h| detected_at(root, h));
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
enum Scope {
    Project,
    Global,
}

pub fn print_skill() -> Out {
    Out::Report {
        text: SKILL_MD.to_string(),
        json: json!({"ok": true, "data": {"skill": SKILL_MD}}),
    }
}

/// Resolve scope + harness list from flags, prompting on a TTY when unset.
fn resolve_targets(
    root: &std::path::Path,
    harness_flags: &[String],
    detected_only: bool,
    project: bool,
    global: bool,
) -> Result<(Scope, Vec<&'static Harness>), CliError> {
    let tty = std::io::stdin().is_terminal() && std::io::stdout().is_terminal();

    let selected: Vec<&'static Harness> = if harness_flags.is_empty() && !detected_only {
        if !tty {
            return Err(CliError::Usage(
                "non-interactive: pass --harness <id,...> or --detected, plus --project|--global"
                    .into(),
            ));
        }
        prompt_harnesses()?
    } else {
        select_harnesses(root, harness_flags, detected_only)?
    };

    let scope = match (project, global) {
        (true, false) => Scope::Project,
        (false, true) => Scope::Global,
        (true, true) => {
            return Err(CliError::Usage("--project and --global are exclusive".into()));
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

fn prompt_harnesses() -> Result<Vec<&'static Harness>, CliError> {
    eprintln!("Install the ox skill for which harness(es)?");
    for (i, h) in HARNESSES.iter().enumerate() {
        let marker = if detected_at(std::path::Path::new(""), h) {
            " (detected)"
        } else {
            ""
        };
        eprintln!("  {:2}. {}{}", i + 1, h.label, marker);
    }
    eprintln!("numbers comma-separated, 'd' for detected, or 'a' for all:");
    let mut line = String::new();
    std::io::stdin()
        .read_line(&mut line)
        .map_err(|e| CliError::Usage(format!("read selection: {e}")))?;
    let line = line.trim();
    if line == "a" || line == "all" {
        return Ok(HARNESSES.iter().collect());
    }
    if line == "d" || line == "detected" {
        return select_harnesses(std::path::Path::new(""), &[], true);
    }
    let mut out = Vec::new();
    for tok in line.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        let n: usize = tok
            .parse()
            .map_err(|_| CliError::Usage(format!("invalid selection {tok:?}")))?;
        let h = HARNESSES
            .get(n.wrapping_sub(1))
            .ok_or_else(|| CliError::Usage(format!("selection {n} out of range")))?;
        if !out.iter().any(|x: &&Harness| x.id == h.id) {
            out.push(h);
        }
    }
    Ok(out)
}

/// Where `<dir>` lands for a harness at a given scope — `Err(reason)` when
/// the harness has no project dir or the global dir can't resolve (skipped).
fn target_base(root: &std::path::Path, h: &Harness, scope: Scope) -> Result<PathBuf, String> {
    match scope {
        Scope::Project => h
            .project
            .map(|d| {
                if root.as_os_str().is_empty() {
                    PathBuf::from(d)
                } else {
                    root.join(d)
                }
            })
            .ok_or_else(|| "no project dir".to_string()),
        Scope::Global => global_dir(h).map_err(|e| e.to_string()),
    }
}

pub fn install(
    _global: &Global,
    _rt: &Runtime,
    harness_flags: &[String],
    project: bool,
    global: bool,
    upgrade: bool,
    detected: bool,
) -> Result<Out, CliError> {
    let root = std::path::Path::new("");
    if upgrade {
        return upgrade_at(root, harness_flags, detected, project, global);
    }
    install_at(root, harness_flags, detected, project, global)
}

fn install_at(
    root: &std::path::Path,
    harness_flags: &[String],
    detected_only: bool,
    project: bool,
    global: bool,
) -> Result<Out, CliError> {
    let (scope, harnesses) = resolve_targets(root, harness_flags, detected_only, project, global)?;
    let mut written = Vec::new();
    let mut skipped = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for h in harnesses {
        let base = match target_base(root, h, scope) {
            Ok(b) => b,
            Err(reason) => {
                skipped.push(format!("{} ({reason})", h.label));
                continue;
            }
        };
        if !seen.insert(base.clone()) {
            continue;
        }
        let dir = base.join("ox");
        std::fs::create_dir_all(&dir)
            .map_err(|e| CliError::Usage(format!("create {}: {e}", dir.display())))?;
        let path = dir.join("SKILL.md");
        std::fs::write(&path, SKILL_MD)
            .map_err(|e| CliError::Usage(format!("write {}: {e}", path.display())))?;
        written.push(path.display().to_string());
    }

    let mut text = String::new();
    for w in &written {
        text.push_str(&format!("installed {w}\n"));
    }
    for s in &skipped {
        text.push_str(&format!("skipped {s}\n"));
    }
    if scope == Scope::Project && !written.is_empty() {
        text.push_str(
            "\nWiring hint — for harnesses that don't auto-load skill dirs, add to AGENTS.md:\n  \
             \"Use the `ox` CLI for Oxidean instance operations; see the `ox` skill in this repo's skills dir.\"\n",
        );
    }
    let written_v: Vec<Value> = written.iter().map(|w| json!(w)).collect();
    Ok(Out::Report {
        text: text.trim_end().to_string(),
        json: json!({"ok": true, "data": {"installed": written_v, "skipped": skipped}}),
    })
}

/// `--upgrade` — rewrite `ox/SKILL.md` only where it already exists. Never
/// prompts, never creates dirs; bare invocation sweeps all harnesses at both
/// scopes, `--harness`/`--detected`/scope flags narrow it.
fn upgrade_at(
    root: &std::path::Path,
    harness_flags: &[String],
    detected_only: bool,
    project: bool,
    global: bool,
) -> Result<Out, CliError> {
    let scopes: &[Scope] = match (project, global) {
        (true, false) => &[Scope::Project],
        (false, true) => &[Scope::Global],
        (true, true) => {
            return Err(CliError::Usage("--project and --global are exclusive".into()));
        }
        (false, false) => &[Scope::Project, Scope::Global],
    };
    let harnesses = if harness_flags.is_empty() && !detected_only {
        HARNESSES.iter().collect::<Vec<_>>()
    } else {
        select_harnesses(root, harness_flags, detected_only)?
    };

    let mut upgraded = Vec::new();
    let mut current = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for h in harnesses {
        for &scope in scopes {
            let Ok(base) = target_base(root, h, scope) else {
                continue;
            };
            let path = base.join("ox").join("SKILL.md");
            if !path.exists() || !seen.insert(path.clone()) {
                continue;
            }
            let stale = std::fs::read_to_string(&path)
                .map(|s| s != SKILL_MD)
                .unwrap_or(true);
            if stale {
                std::fs::write(&path, SKILL_MD)
                    .map_err(|e| CliError::Usage(format!("write {}: {e}", path.display())))?;
                upgraded.push(path.display().to_string());
            } else {
                current.push(path.display().to_string());
            }
        }
    }

    let mut text = String::new();
    for w in &upgraded {
        text.push_str(&format!("upgraded {w}\n"));
    }
    for c in &current {
        text.push_str(&format!("up-to-date {c}\n"));
    }
    if upgraded.is_empty() && current.is_empty() {
        text.push_str("no ox skill installs found\n");
    }
    let upgraded_v: Vec<Value> = upgraded.iter().map(|w| json!(w)).collect();
    let current_v: Vec<Value> = current.iter().map(|w| json!(w)).collect();
    Ok(Out::Report {
        text: text.trim_end().to_string(),
        json: json!({"ok": true, "data": {"upgraded": upgraded_v, "current": current_v}}),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_skill_has_frontmatter() {
        assert!(SKILL_MD.starts_with("---\nname: ox"));
        assert!(SKILL_MD.contains("ox api"));
        assert!(SKILL_MD.contains("--json"));
    }

    #[test]
    fn harness_ids_unique_and_nonflag() {
        let mut seen = std::collections::BTreeSet::new();
        for h in HARNESSES {
            assert!(seen.insert(h.id), "dup {}" , h.id);
            assert!(!h.id.contains(' '));
            assert!(!h.detect.is_empty());
        }
    }

    fn test_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ox-skill-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn flags_parse_and_install() {
        let dir = test_dir("install");
        let out = install_at(
            &dir,
            &["claude".to_string(), "codex".to_string()],
            false,
            true,
            false,
        )
        .unwrap();
        match out {
            Out::Report { text, .. } => assert!(text.contains(".claude/skills/ox/SKILL.md")),
            _ => panic!(),
        }
        assert_eq!(
            std::fs::read_to_string(dir.join(".claude/skills/ox/SKILL.md")).unwrap(),
            SKILL_MD
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn upgrade_rewrites_only_existing() {
        let dir = test_dir("upgrade-stale");
        let stale = dir.join(".claude/skills/ox");
        std::fs::create_dir_all(&stale).unwrap();
        std::fs::write(stale.join("SKILL.md"), "stale").unwrap();
        let out = upgrade_at(&dir, &[], false, true, false).unwrap();
        match out {
            Out::Report { text, .. } => assert!(text.contains("upgraded ")),
            _ => panic!(),
        }
        assert_eq!(
            std::fs::read_to_string(stale.join("SKILL.md")).unwrap(),
            SKILL_MD
        );
        assert!(!dir.join(".kiro/skills/ox").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn upgrade_reports_up_to_date() {
        let dir = test_dir("upgrade-current");
        let fresh = dir.join(".agents/skills/ox");
        std::fs::create_dir_all(&fresh).unwrap();
        std::fs::write(fresh.join("SKILL.md"), SKILL_MD).unwrap();
        let out = upgrade_at(&dir, &["codex".to_string()], false, true, false).unwrap();
        match out {
            Out::Report { text, .. } => {
                assert!(text.contains("up-to-date "));
                assert!(!text.contains("upgraded "));
            }
            _ => panic!(),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn upgrade_empty_reports_none() {
        let dir = test_dir("upgrade-empty");
        let out = upgrade_at(&dir, &[], false, true, false).unwrap();
        match out {
            Out::Report { text, .. } => assert!(text.contains("no ox skill installs")),
            _ => panic!(),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn detected_marks_installed_harness() {
        let dir = test_dir("detect");
        std::fs::create_dir_all(dir.join(".claude")).unwrap();
        let claude = harness_by_id("claude").unwrap();
        assert!(detected_at(&dir, claude));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn detected_selection_subset() {
        let dir = test_dir("detect-subset");
        for h in select_harnesses(&dir, &[], true).unwrap_or_default() {
            assert!(detected_at(&dir, h), "{} not detected", h.id);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn detected_intersects_flagged_harnesses() {
        let dir = test_dir("detect-intersect");
        std::fs::create_dir_all(dir.join(".claude")).unwrap();
        let flags = vec!["claude".to_string(), "roo".to_string(), "goose".to_string()];
        let sel = select_harnesses(&dir, &flags, true).unwrap();
        assert!(sel.iter().any(|h| h.id == "claude"));
        for h in &sel {
            assert!(detected_at(&dir, h), "{} not detected", h.id);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn harness_flags_all_dedup_and_unknown() {
        let all = parse_harness_flags(&["all".to_string(), "claude".to_string()]).unwrap();
        assert_eq!(all.len(), HARNESSES.len());
        let one = parse_harness_flags(&["claude, claude ,cursor".to_string()]).unwrap();
        assert_eq!(one.iter().map(|h| h.id).collect::<Vec<_>>(), ["claude", "cursor"]);
        assert!(parse_harness_flags(&["nope".to_string()]).is_err());
    }

    #[test]
    fn install_dedupes_shared_agents_dir() {
        let dir = test_dir("install-dedup");
        let out = install_at(
            &dir,
            &["codex".to_string(), "copilot".to_string(), "generic".to_string()],
            false,
            true,
            false,
        )
        .unwrap();
        match out {
            Out::Report { text, .. } => {
                assert_eq!(text.matches(".agents/skills/ox/SKILL.md").count(), 1);
            }
            _ => panic!(),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn upgrade_rejects_both_scopes() {
        let dir = test_dir("upgrade-excl");
        assert!(upgrade_at(&dir, &[], false, true, true).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn on_path_misses_unknown_binary() {
        assert!(!on_path("ox-skill-test-not-a-real-binary"));
    }
}
