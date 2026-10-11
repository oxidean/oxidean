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

struct Harness {
    /// `--harness <id>` selector.
    id: &'static str,
    label: &'static str,
    /// Repo-relative dir, `None` = global-scope only.
    project: Option<&'static str>,
    global: GlobalDir,
}

/// Curated table matching the vercel-labs/skills registry for the common
/// harnesses; unknown harnesses can still consume `.agents/skills` via the
/// `generic` entry.
const HARNESSES: &[Harness] = &[
    Harness { id: "claude",   label: "Claude Code",      project: Some(".claude/skills"),   global: GlobalDir::Env("CLAUDE_CONFIG_DIR", ".claude/skills") },
    Harness { id: "cursor",   label: "Cursor",           project: Some(".agents/skills"),   global: GlobalDir::Home(".cursor/skills") },
    Harness { id: "codex",    label: "Codex",            project: Some(".agents/skills"),   global: GlobalDir::Home(".agents/skills") },
    Harness { id: "copilot",  label: "GitHub Copilot",   project: Some(".agents/skills"),   global: GlobalDir::Home(".copilot/skills") },
    Harness { id: "gemini",   label: "Gemini CLI",       project: Some(".agents/skills"),   global: GlobalDir::Home(".gemini/skills") },
    Harness { id: "opencode", label: "OpenCode",         project: Some(".agents/skills"),   global: GlobalDir::Xdg("opencode/skills") },
    Harness { id: "amp",      label: "Amp",              project: Some(".agents/skills"),   global: GlobalDir::Xdg("agents/skills") },
    Harness { id: "devin",    label: "Devin",            project: Some(".devin/skills"),    global: GlobalDir::Xdg("devin/skills") },
    Harness { id: "windsurf", label: "Windsurf",         project: Some(".windsurf/skills"), global: GlobalDir::Home(".codeium/windsurf/skills") },
    Harness { id: "kiro",     label: "Kiro CLI",         project: Some(".kiro/skills"),     global: GlobalDir::Home(".kiro/skills") },
    Harness { id: "roo",      label: "Roo Code",         project: Some(".roo/skills"),      global: GlobalDir::Home(".roo/skills") },
    Harness { id: "goose",    label: "Goose",            project: Some(".goose/skills"),    global: GlobalDir::Xdg("goose/skills") },
    Harness { id: "cline",    label: "Cline",            project: Some(".agents/skills"),   global: GlobalDir::Home(".agents/skills") },
    Harness { id: "openclaw", label: "OpenClaw",         project: Some("skills"),           global: GlobalDir::Home(".openclaw/skills") },
    Harness { id: "generic",  label: "Any `.agents/skills` harness", project: Some(".agents/skills"), global: GlobalDir::Home(".agents/skills") },
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
    harness_flags: &[String],
    project: bool,
    global: bool,
) -> Result<(Scope, Vec<&'static Harness>), CliError> {
    let tty = std::io::stdin().is_terminal() && std::io::stdout().is_terminal();

    let selected: Vec<&'static Harness> = if harness_flags.is_empty() {
        if !tty {
            return Err(CliError::Usage(
                "non-interactive: pass --harness <id,...> and --project|--global".into(),
            ));
        }
        prompt_harnesses()?
    } else {
        let mut out = Vec::new();
        for spec in harness_flags {
            for id in spec.split(',').map(str::trim).filter(|s| !s.is_empty()) {
                if id == "all" {
                    out.extend(HARNESSES.iter());
                    continue;
                }
                let h = harness_by_id(id)?;
                if !out.iter().any(|x: &&Harness| x.id == h.id) {
                    out.push(h);
                }
            }
        }
        out
    };
    if selected.is_empty() {
        return Err(CliError::Usage("no harnesses selected".into()));
    }

    let scope = match (project, global) {
        (true, false) => Scope::Project,
        (false, true) => Scope::Global,
        (true, true) => {
            return Err(CliError::Usage("--project and --global are exclusive".into()));
        }
        (false, false) => {
            if !tty {
                return Err(CliError::Usage(
                    "non-interactive: pass --harness <id,...> and --project|--global".into(),
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
        let marker = if h
            .project
            .map(|d| PathBuf::from(d).exists())
            .unwrap_or(false)
        {
            " (detected)"
        } else {
            ""
        };
        eprintln!("  {:2}. {}{}", i + 1, h.label, marker);
    }
    eprintln!("numbers comma-separated, or 'a' for all:");
    let mut line = String::new();
    std::io::stdin()
        .read_line(&mut line)
        .map_err(|e| CliError::Usage(format!("read selection: {e}")))?;
    let line = line.trim();
    if line == "a" || line == "all" {
        return Ok(HARNESSES.iter().collect());
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

pub fn install(
    _global: &Global,
    _rt: &Runtime,
    harness_flags: &[String],
    project: bool,
    global: bool,
) -> Result<Out, CliError> {
    let (scope, harnesses) = resolve_targets(harness_flags, project, global)?;
    let mut written = Vec::new();
    let mut skipped = Vec::new();
    for h in harnesses {
        let base = match scope {
            Scope::Project => match h.project {
                Some(d) => PathBuf::from(d),
                None => {
                    skipped.push(format!("{} (no project dir)", h.label));
                    continue;
                }
            },
            Scope::Global => match global_dir(h) {
                Ok(d) => d,
                Err(e) => {
                    skipped.push(format!("{} ({e})", h.label));
                    continue;
                }
            },
        };
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
        }
    }

    #[test]
    fn flags_parse_and_install() {
        let dir = std::env::temp_dir().join(format!("ox-skill-test-{}", std::process::id()));
        std::env::set_current_dir(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_current_dir(&dir).unwrap();
        let g = crate::args::Global {
            instance: None,
            json: false,
            help: false,
            version: false,
        };
        let rt = Runtime {
            config: crate::config::Config::default(),
            config_path: dir.join("config.json"),
        };
        let out = install(&g, &rt, &["claude".to_string(), "codex".to_string()], true, false)
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
}
