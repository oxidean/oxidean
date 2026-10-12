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

/// Where a harness looks for skills at global (user) scope. A harness can
/// have several (antigravity/trae/qoder ship `-cn`/`-cli` siblings) or none
/// (eve, promptscript are project-only).
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
    /// Global-scope dirs — several for harnesses with sibling products
    /// (antigravity/trae/qoder), empty when the harness is project-only.
    global: &'static [GlobalDir],
    /// Installed-harness markers; first hit wins. Best-effort: the shared
    /// `.agents/skills` project dir is *not* a marker on individual harnesses
    /// (it would mark ~50 of them at once — `generic` owns that marker), so
    /// rows with no dedicated dir or known binary simply can't auto-detect;
    /// `--harness <id>` is the explicit path.
    detect: &'static [Detect],
}

/// Harness registry matching the vercel-labs/skills `src/agents.ts` table —
/// the de-facto standard for "install a skill into N agents". `generic`
/// covers anything else that loads `.agents/skills`.
const HARNESSES: &[Harness] = &[
    // ── first-class: dedicated dirs and/or binaries ─────────────────────
    Harness { id: "claude",   label: "Claude Code",      project: Some(".claude/skills"),   global: &[GlobalDir::Env("CLAUDE_CONFIG_DIR", ".claude/skills")],
        detect: &[Detect::Project(".claude"), Detect::Home(".claude"), Detect::Bin("claude")] },
    Harness { id: "cursor",   label: "Cursor",           project: Some(".agents/skills"),   global: &[GlobalDir::Home(".cursor/skills")],
        detect: &[Detect::Project(".cursor"), Detect::Home(".cursor"), Detect::Bin("cursor-agent"), Detect::Bin("agent")] },
    Harness { id: "codex",    label: "Codex",            project: Some(".agents/skills"),   global: &[GlobalDir::Home(".agents/skills")],
        detect: &[Detect::Project(".codex"), Detect::Home(".codex"), Detect::Bin("codex")] },
    Harness { id: "copilot",  label: "GitHub Copilot",   project: Some(".agents/skills"),   global: &[GlobalDir::Home(".copilot/skills")],
        detect: &[Detect::Project(".copilot"), Detect::Home(".copilot"), Detect::Bin("copilot")] },
    Harness { id: "gemini",   label: "Gemini CLI",       project: Some(".agents/skills"),   global: &[GlobalDir::Home(".gemini/skills")],
        detect: &[Detect::Project(".gemini"), Detect::Home(".gemini"), Detect::Bin("gemini")] },
    Harness { id: "antigravity", label: "Antigravity",   project: Some(".agents/skills"),   global: &[GlobalDir::Home(".gemini/antigravity/skills"), GlobalDir::Home(".gemini/antigravity-cli/skills")],
        detect: &[Detect::Home(".gemini/antigravity"), Detect::Home(".gemini/antigravity-cli"), Detect::Bin("antigravity"), Detect::Bin("agy")] },
    Harness { id: "opencode", label: "OpenCode",         project: Some(".agents/skills"),   global: &[GlobalDir::Xdg("opencode/skills")],
        detect: &[Detect::Project(".opencode"), Detect::Xdg("opencode"), Detect::Bin("opencode")] },
    Harness { id: "amp",      label: "Amp",              project: Some(".agents/skills"),   global: &[GlobalDir::Xdg("agents/skills")],
        detect: &[Detect::Project(".amp"), Detect::Xdg("amp"), Detect::Bin("amp")] },
    Harness { id: "replit",   label: "Replit / Universal", project: Some(".agents/skills"), global: &[GlobalDir::Xdg("agents/skills")],
        detect: &[Detect::Bin("repl")] },
    Harness { id: "devin",    label: "Devin for Terminal", project: Some(".devin/skills"),  global: &[GlobalDir::Xdg("devin/skills")],
        detect: &[Detect::Project(".devin"), Detect::Xdg("devin"), Detect::Bin("devin")] },
    Harness { id: "windsurf", label: "Windsurf",         project: Some(".windsurf/skills"), global: &[GlobalDir::Home(".codeium/windsurf/skills")],
        detect: &[Detect::Project(".windsurf"), Detect::Home(".codeium/windsurf"), Detect::Bin("windsurf")] },
    Harness { id: "kiro",     label: "Kiro CLI",         project: Some(".kiro/skills"),     global: &[GlobalDir::Home(".kiro/skills")],
        detect: &[Detect::Project(".kiro"), Detect::Home(".kiro"), Detect::Bin("kiro"), Detect::Bin("kiro-cli")] },
    Harness { id: "roo",      label: "Roo Code",         project: Some(".roo/skills"),      global: &[GlobalDir::Home(".roo/skills")],
        detect: &[Detect::Project(".roo"), Detect::Home(".roo")] },
    Harness { id: "goose",    label: "Goose",            project: Some(".goose/skills"),    global: &[GlobalDir::Xdg("goose/skills")],
        detect: &[Detect::Project(".goose"), Detect::Xdg("goose"), Detect::Bin("goose")] },
    Harness { id: "cline",    label: "Cline",            project: Some(".agents/skills"),   global: &[GlobalDir::Home(".agents/skills")],
        detect: &[Detect::Project(".cline"), Detect::Home(".cline"), Detect::Bin("cline")] },
    Harness { id: "openclaw", label: "OpenClaw",         project: Some("skills"),           global: &[GlobalDir::Home(".openclaw/skills")],
        detect: &[Detect::Project(".openclaw"), Detect::Home(".openclaw"), Detect::Bin("openclaw")] },
    // ── dedicated dirs ──────────────────────────────────────────────────
    Harness { id: "continue",  label: "Continue",        project: Some(".continue/skills"), global: &[GlobalDir::Home(".continue/skills")],
        detect: &[Detect::Project(".continue"), Detect::Home(".continue"), Detect::Bin("cn"), Detect::Bin("continue")] },
    Harness { id: "openhands", label: "OpenHands",       project: Some(".openhands/skills"), global: &[GlobalDir::Home(".openhands/skills")],
        detect: &[Detect::Project(".openhands"), Detect::Home(".openhands"), Detect::Bin("openhands")] },
    Harness { id: "crush",     label: "Crush",           project: Some(".crush/skills"),    global: &[GlobalDir::Xdg("crush/skills")],
        detect: &[Detect::Project(".crush"), Detect::Xdg("crush"), Detect::Bin("crush")] },
    Harness { id: "trae",      label: "Trae / Trae CN",  project: Some(".trae/skills"),     global: &[GlobalDir::Home(".trae/skills"), GlobalDir::Home(".trae-cn/skills")],
        detect: &[Detect::Project(".trae"), Detect::Home(".trae"), Detect::Home(".trae-cn"), Detect::Bin("trae")] },
    Harness { id: "qoder",     label: "Qoder / Qoder CN", project: Some(".qoder/skills"),   global: &[GlobalDir::Home(".qoder/skills"), GlobalDir::Home(".qoder-cn/skills")],
        detect: &[Detect::Project(".qoder"), Detect::Home(".qoder"), Detect::Home(".qoder-cn"), Detect::Bin("qoder")] },
    Harness { id: "qwen",      label: "Qwen Code",       project: Some(".qwen/skills"),     global: &[GlobalDir::Home(".qwen/skills")],
        detect: &[Detect::Project(".qwen"), Detect::Home(".qwen"), Detect::Bin("qwen-code"), Detect::Bin("qwen")] },
    Harness { id: "grok",      label: "Grok Build",      project: Some(".grok/skills"),     global: &[GlobalDir::Env("GROK_HOME", ".grok/skills")],
        detect: &[Detect::Project(".grok"), Detect::Home(".grok"), Detect::Bin("grok")] },
    Harness { id: "vibe",      label: "Mistral Vibe",    project: Some(".vibe/skills"),     global: &[GlobalDir::Env("VIBE_HOME", ".vibe/skills")],
        detect: &[Detect::Project(".vibe"), Detect::Home(".vibe"), Detect::Bin("vibe")] },
    Harness { id: "junie",     label: "Junie",           project: Some(".junie/skills"),    global: &[GlobalDir::Home(".junie/skills")],
        detect: &[Detect::Project(".junie"), Detect::Home(".junie")] },
    Harness { id: "augment",   label: "Augment",         project: Some(".augment/skills"),  global: &[GlobalDir::Home(".augment/skills")],
        detect: &[Detect::Project(".augment"), Detect::Home(".augment"), Detect::Bin("auggie")] },
    Harness { id: "bob",       label: "IBM Bob",         project: Some(".bob/skills"),      global: &[GlobalDir::Home(".bob/skills")],
        detect: &[Detect::Project(".bob"), Detect::Home(".bob"), Detect::Bin("bob")] },
    Harness { id: "cortex",    label: "Cortex Code",     project: Some(".cortex/skills"),   global: &[GlobalDir::Home(".snowflake/cortex/skills")],
        detect: &[Detect::Project(".cortex"), Detect::Home(".snowflake/cortex"), Detect::Bin("cortex")] },
    Harness { id: "posit",     label: "Posit Assistant", project: Some(".posit/assistant/skills"), global: &[GlobalDir::Home(".posit/assistant/skills")],
        detect: &[Detect::Project(".posit"), Detect::Home(".posit")] },
    Harness { id: "tabnine",   label: "Tabnine CLI",     project: Some(".tabnine/agent/skills"), global: &[GlobalDir::Home(".tabnine/agent/skills")],
        detect: &[Detect::Project(".tabnine"), Detect::Home(".tabnine"), Detect::Bin("tabnine")] },
    Harness { id: "aider-desk", label: "AiderDesk",      project: Some(".aider-desk/skills"), global: &[GlobalDir::Home(".aider-desk/skills")],
        detect: &[Detect::Project(".aider-desk"), Detect::Home(".aider-desk")] },
    Harness { id: "astrbot",   label: "AstrBot",         project: Some("data/skills"),      global: &[GlobalDir::Home(".astrbot/data/skills")],
        detect: &[Detect::Home(".astrbot"), Detect::Bin("astrbot")] },
    Harness { id: "autohand",  label: "Autohand Code CLI", project: Some(".autohand/skills"), global: &[GlobalDir::Env("AUTOHAND_HOME", ".autohand/skills")],
        detect: &[Detect::Project(".autohand"), Detect::Home(".autohand"), Detect::Bin("autohand")] },
    Harness { id: "codearts",  label: "CodeArts Agent",  project: Some(".codeartsdoer/skills"), global: &[GlobalDir::Home(".codeartsdoer/skills")],
        detect: &[Detect::Project(".codeartsdoer"), Detect::Home(".codeartsdoer")] },
    Harness { id: "forgecode", label: "ForgeCode",       project: Some(".forge/skills"),    global: &[GlobalDir::Home(".forge/skills")],
        detect: &[Detect::Project(".forge"), Detect::Home(".forge"), Detect::Bin("forgecode"), Detect::Bin("forge")] },
    Harness { id: "fx",        label: "fx",              project: Some(".fx/skills"),       global: &[GlobalDir::Home(".fx/skills")],
        detect: &[Detect::Project(".fx"), Detect::Home(".fx")] },
    Harness { id: "hermes",    label: "Hermes Agent",    project: Some(".hermes/skills"),   global: &[GlobalDir::Env("HERMES_HOME", ".hermes/skills")],
        detect: &[Detect::Project(".hermes"), Detect::Home(".hermes"), Detect::Bin("hermes")] },
    Harness { id: "inferencesh", label: "inference.sh",  project: Some(".inferencesh/skills"), global: &[GlobalDir::Home(".inferencesh/skills")],
        detect: &[Detect::Project(".inferencesh"), Detect::Home(".inferencesh"), Detect::Bin("inference"), Detect::Bin("infsh")] },
    Harness { id: "iflow",     label: "iFlow CLI",       project: Some(".iflow/skills"),    global: &[GlobalDir::Home(".iflow/skills")],
        detect: &[Detect::Project(".iflow"), Detect::Home(".iflow"), Detect::Bin("iflow")] },
    Harness { id: "jazz",      label: "Jazz",            project: Some(".jazz/skills"),     global: &[GlobalDir::Home(".jazz/skills")],
        detect: &[Detect::Project(".jazz"), Detect::Home(".jazz"), Detect::Bin("jazz")] },
    Harness { id: "kimchi",    label: "Kimchi",          project: Some(".kimchi/skills"),   global: &[GlobalDir::Xdg("kimchi/harness/skills")],
        detect: &[Detect::Project(".kimchi"), Detect::Xdg("kimchi"), Detect::Bin("kimchi")] },
    Harness { id: "kode",      label: "Kode",            project: Some(".kode/skills"),     global: &[GlobalDir::Home(".kode/skills")],
        detect: &[Detect::Project(".kode"), Detect::Home(".kode"), Detect::Bin("kode")] },
    Harness { id: "lingma",    label: "Lingma",          project: Some(".lingma/skills"),   global: &[GlobalDir::Home(".lingma/skills")],
        detect: &[Detect::Project(".lingma"), Detect::Home(".lingma")] },
    Harness { id: "mcpjam",    label: "MCPJam",          project: Some(".mcpjam/skills"),   global: &[GlobalDir::Home(".mcpjam/skills")],
        detect: &[Detect::Project(".mcpjam"), Detect::Home(".mcpjam"), Detect::Bin("mcpjam")] },
    Harness { id: "minimax",   label: "MiniMax Code",    project: Some(".minimax/skills"),  global: &[GlobalDir::Home(".minimax/skills")],
        detect: &[Detect::Project(".minimax"), Detect::Home(".minimax"), Detect::Bin("minimax")] },
    Harness { id: "moxby",     label: "Moxby",           project: Some(".moxby/skills"),    global: &[GlobalDir::Home(".moxby/skills")],
        detect: &[Detect::Project(".moxby"), Detect::Home(".moxby")] },
    Harness { id: "mux",       label: "Mux",             project: Some(".mux/skills"),      global: &[GlobalDir::Home(".mux/skills")],
        detect: &[Detect::Project(".mux"), Detect::Home(".mux"), Detect::Bin("mux")] },
    Harness { id: "ona",       label: "Ona",             project: Some(".ona/skills"),      global: &[GlobalDir::Home(".ona/skills")],
        detect: &[Detect::Project(".ona"), Detect::Home(".ona")] },
    Harness { id: "reasonix",  label: "Reasonix",        project: Some(".reasonix/skills"), global: &[GlobalDir::Home(".reasonix/skills")],
        detect: &[Detect::Project(".reasonix"), Detect::Home(".reasonix")] },
    Harness { id: "rovodev",   label: "Rovo Dev",        project: Some(".rovodev/skills"),  global: &[GlobalDir::Home(".rovodev/skills")],
        detect: &[Detect::Project(".rovodev"), Detect::Home(".rovodev"), Detect::Bin("rovodev"), Detect::Bin("acli")] },
    Harness { id: "terramind", label: "Terramind",       project: Some(".terramind/skills"), global: &[GlobalDir::Home(".terramind/skills")],
        detect: &[Detect::Project(".terramind"), Detect::Home(".terramind")] },
    Harness { id: "tinycloud", label: "Tinycloud",       project: Some(".tinycloud/skills"), global: &[GlobalDir::Home(".tinycloud/skills")],
        detect: &[Detect::Project(".tinycloud"), Detect::Home(".tinycloud")] },
    Harness { id: "zcode",     label: "ZCode",           project: Some(".zcode/skills"),    global: &[GlobalDir::Home(".zcode/skills")],
        detect: &[Detect::Project(".zcode"), Detect::Home(".zcode")] },
    Harness { id: "zencoder",  label: "Zencoder / Zenflow", project: Some(".zencoder/skills"), global: &[GlobalDir::Home(".zencoder/skills")],
        detect: &[Detect::Project(".zencoder"), Detect::Home(".zencoder")] },
    Harness { id: "neovate",   label: "Neovate",         project: Some(".neovate/skills"),  global: &[GlobalDir::Home(".neovate/skills")],
        detect: &[Detect::Project(".neovate"), Detect::Home(".neovate")] },
    Harness { id: "pochi",     label: "Pochi",           project: Some(".pochi/skills"),    global: &[GlobalDir::Home(".pochi/skills")],
        detect: &[Detect::Project(".pochi"), Detect::Home(".pochi"), Detect::Bin("pochi")] },
    Harness { id: "adal",      label: "AdaL",            project: Some(".adal/skills"),     global: &[GlobalDir::Home(".adal/skills")],
        detect: &[Detect::Project(".adal"), Detect::Home(".adal")] },
    Harness { id: "codebuddy", label: "CodeBuddy",       project: Some(".codebuddy/skills"), global: &[GlobalDir::Home(".codebuddy/skills")],
        detect: &[Detect::Project(".codebuddy"), Detect::Home(".codebuddy"), Detect::Bin("codebuddy")] },
    Harness { id: "codemaker", label: "Codemaker",       project: Some(".codemaker/skills"), global: &[GlobalDir::Home(".codemaker/skills")],
        detect: &[Detect::Project(".codemaker"), Detect::Home(".codemaker")] },
    Harness { id: "codestudio", label: "Code Studio",    project: Some(".codestudio/skills"), global: &[GlobalDir::Home(".codestudio/skills")],
        detect: &[Detect::Project(".codestudio"), Detect::Home(".codestudio")] },
    Harness { id: "commandcode", label: "Command Code",  project: Some(".commandcode/skills"), global: &[GlobalDir::Home(".commandcode/skills")],
        detect: &[Detect::Project(".commandcode"), Detect::Home(".commandcode")] },
    Harness { id: "droid",     label: "Droid (Factory)", project: Some(".agents/skills"),   global: &[GlobalDir::Home(".factory/skills")],
        detect: &[Detect::Home(".factory"), Detect::Bin("droid")] },
    Harness { id: "firebender", label: "Firebender",     project: Some(".agents/skills"),   global: &[GlobalDir::Home(".firebender/skills")],
        detect: &[Detect::Home(".firebender"), Detect::Bin("firebender")] },
    Harness { id: "deepagents", label: "Deep Agents",    project: Some(".agents/skills"),   global: &[GlobalDir::Home(".deepagents/agent/skills")],
        detect: &[Detect::Home(".deepagents")] },
    Harness { id: "kilo",      label: "Kilo Code",       project: Some(".agents/skills"),   global: &[GlobalDir::Home(".kilo/skills")],
        detect: &[Detect::Project(".kilocode"), Detect::Home(".kilo"), Detect::Bin("kilocode")] },
    // ── `.agents` at both scopes — no dedicated marker; named rows so the
    //    harnesses can be picked explicitly ───────────────────────────────
    Harness { id: "dexto",     label: "Dexto",           project: Some(".agents/skills"),   global: &[GlobalDir::Home(".agents/skills")],
        detect: &[Detect::Bin("dexto")] },
    Harness { id: "kimi",      label: "Kimi Code CLI",   project: Some(".agents/skills"),   global: &[GlobalDir::Home(".agents/skills")],
        detect: &[Detect::Bin("kimi")] },
    Harness { id: "loaf",      label: "Loaf",            project: Some(".agents/skills"),   global: &[GlobalDir::Home(".agents/skills")],
        detect: &[Detect::Bin("loaf")] },
    Harness { id: "pi",        label: "Pi",              project: Some(".agents/skills"),   global: &[GlobalDir::Home(".agents/skills")],
        detect: &[] },
    Harness { id: "sarvam",    label: "Sarvam Code",     project: Some(".agents/skills"),   global: &[GlobalDir::Home(".agents/skills")],
        detect: &[Detect::Bin("sarvam")] },
    Harness { id: "warp",      label: "Warp",            project: Some(".agents/skills"),   global: &[GlobalDir::Home(".agents/skills")],
        detect: &[Detect::Bin("warp-terminal"), Detect::Bin("warp")] },
    Harness { id: "zed",       label: "Zed",             project: Some(".agents/skills"),   global: &[GlobalDir::Home(".agents/skills")],
        detect: &[Detect::Xdg("zed"), Detect::Bin("zed")] },
    // ── project-only ────────────────────────────────────────────────────
    Harness { id: "eve",       label: "Eve",             project: Some("agent/skills"),     global: &[],
        detect: &[Detect::Project("agent/skills"), Detect::Bin("eve")] },
    Harness { id: "promptscript", label: "PromptScript", project: Some(".agents/skills"),   global: &[],
        detect: &[] },
    Harness { id: "generic",  label: "Any `.agents/skills` harness", project: Some(".agents/skills"), global: &[GlobalDir::Home(".agents/skills")],
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

/// Resolved home/XDG roots — carried through detection + target resolution
/// so tests can point global scope at a tempdir instead of the real `$HOME`.
struct Dirs {
    home: Option<PathBuf>,
    xdg: PathBuf,
}

impl Dirs {
    fn env() -> Self {
        Dirs {
            home: home(),
            xdg: xdg_config(),
        }
    }
}

fn resolve_global(dirs: &Dirs, g: &GlobalDir) -> Result<PathBuf, CliError> {
    match g {
        GlobalDir::Home(suffix) => dirs
            .home
            .as_ref()
            .map(|h| h.join(suffix))
            .ok_or_else(|| CliError::Usage("no home directory — cannot resolve global scope".into())),
        GlobalDir::Xdg(suffix) => Ok(dirs.xdg.join(suffix)),
        GlobalDir::Env(var, fallback) => std::env::var_os(var)
            .map(PathBuf::from)
            .filter(|p| !p.as_os_str().is_empty())
            .map(|p| p.join("skills"))
            .or_else(|| dirs.home.as_ref().map(|h| h.join(fallback)))
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
fn detected_at(dirs: &Dirs, root: &std::path::Path, h: &Harness) -> bool {
    h.detect.iter().any(|d| match d {
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
    dirs: &Dirs,
    root: &std::path::Path,
    harness_flags: &[String],
    detected_only: bool,
) -> Result<Vec<&'static Harness>, CliError> {
    let selected: Vec<&'static Harness> = if harness_flags.is_empty() {
        if detected_only {
            HARNESSES
                .iter()
                .filter(|h| detected_at(dirs, root, h))
                .collect()
        } else {
            Vec::new()
        }
    } else {
        let mut out = parse_harness_flags(harness_flags)?;
        if detected_only {
            out.retain(|h| detected_at(dirs, root, h));
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
    dirs: &Dirs,
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
        prompt_harnesses(dirs)?
    } else {
        select_harnesses(dirs, root, harness_flags, detected_only)?
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

fn prompt_harnesses(dirs: &Dirs) -> Result<Vec<&'static Harness>, CliError> {
    let empty = std::path::Path::new("");
    let mut detected = Vec::new();
    let mut rest = Vec::new();
    for h in HARNESSES {
        if detected_at(dirs, empty, h) {
            detected.push(h);
        } else {
            rest.push(h);
        }
    }
    eprintln!("Install the ox skill for which harness(es)?");
    if !detected.is_empty() {
        eprintln!("  detected:");
    }
    for (i, h) in detected.iter().chain(&rest).enumerate() {
        eprintln!("  {:2}. {}", i + 1, h.label);
    }
    eprintln!("numbers comma-separated, 'd' for detected, or 'a' for all — `--harness <id>` skips this prompt:");
    let mut line = String::new();
    std::io::stdin()
        .read_line(&mut line)
        .map_err(|e| CliError::Usage(format!("read selection: {e}")))?;
    let line = line.trim();
    if line == "a" || line == "all" {
        return Ok(HARNESSES.iter().collect());
    }
    if line == "d" || line == "detected" {
        return select_harnesses(dirs, std::path::Path::new(""), &[], true);
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
/// the harness has no dir at that scope or it can't resolve (skipped).
/// Global scope can yield several bases (antigravity/trae/qoder siblings).
fn target_bases(
    dirs: &Dirs,
    root: &std::path::Path,
    h: &Harness,
    scope: Scope,
) -> Result<Vec<PathBuf>, String> {
    match scope {
        Scope::Project => h
            .project
            .map(|d| {
                vec![if root.as_os_str().is_empty() {
                    PathBuf::from(d)
                } else {
                    root.join(d)
                }]
            })
            .ok_or_else(|| "no project dir".to_string()),
        Scope::Global => {
            if h.global.is_empty() {
                return Err("no global dir".to_string());
            }
            h.global
                .iter()
                .map(|g| resolve_global(dirs, g).map_err(|e| e.to_string()))
                .collect()
        }
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
    let dirs = Dirs::env();
    let root = std::path::Path::new("");
    if upgrade {
        return upgrade_at(&dirs, root, harness_flags, detected, project, global);
    }
    install_at(&dirs, root, harness_flags, detected, project, global)
}

/// `ox self-update` hook — sweep every harness at both scopes, rewriting
/// only existing `ox/SKILL.md` files. Returns the number of files refreshed.
pub(crate) fn refresh_installed() -> Result<usize, CliError> {
    let dirs = Dirs::env();
    match upgrade_at(&dirs, std::path::Path::new(""), &[], false, false, false)? {
        Out::Report { json, .. } => Ok(json
            .pointer("/data/upgraded")
            .and_then(Value::as_array)
            .map(|a| a.len())
            .unwrap_or(0)),
        _ => Ok(0),
    }
}

fn install_at(
    dirs: &Dirs,
    root: &std::path::Path,
    harness_flags: &[String],
    detected_only: bool,
    project: bool,
    global: bool,
) -> Result<Out, CliError> {
    let (scope, harnesses) =
        resolve_targets(dirs, root, harness_flags, detected_only, project, global)?;
    let mut written = Vec::new();
    let mut skipped = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for h in harnesses {
        let bases = match target_bases(dirs, root, h, scope) {
            Ok(b) => b,
            Err(reason) => {
                skipped.push(format!("{} ({reason})", h.label));
                continue;
            }
        };
        for base in bases {
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
    dirs: &Dirs,
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
        select_harnesses(dirs, root, harness_flags, detected_only)?
    };

    let mut upgraded = Vec::new();
    let mut current = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for h in harnesses {
        for &scope in scopes {
            let Ok(bases) = target_bases(dirs, root, h, scope) else {
                continue;
            };
            for base in bases {
                let path = base.join("ox").join("SKILL.md");
                if !path.exists() || !seen.insert(path.clone()) {
                    continue;
                }
                let stale = std::fs::read_to_string(&path)
                    .map(|s| s != SKILL_MD)
                    .unwrap_or(true);
                if stale {
                    std::fs::write(&path, SKILL_MD).map_err(|e| {
                        CliError::Usage(format!("write {}: {e}", path.display()))
                    })?;
                    upgraded.push(path.display().to_string());
                } else {
                    current.push(path.display().to_string());
                }
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
            // Detection is best-effort — only `.agents`-only harnesses (which
            // can't use the shared dir as a marker) may have no markers.
            assert!(
                !h.detect.is_empty() || h.project == Some(".agents/skills"),
                "{} has no detection markers",
                h.id
            );
        }
    }

    /// Every row in the issue-117 vercel-labs/skills reference table is
    /// reachable via `--harness <id>` — guards against silently dropping a
    /// row when the registry is refreshed.
    #[test]
    fn covers_spec_harness_table() {
        const SPEC: &[&str] = &[
            "claude", "cursor", "codex", "copilot", "gemini", "antigravity",
            "opencode", "amp", "replit", "devin", "cline", "dexto", "kimi",
            "loaf", "pi", "sarvam", "warp", "zed", "droid", "firebender",
            "deepagents", "kilo", "windsurf", "kiro", "roo", "goose",
            "continue", "openhands", "crush", "trae", "qoder", "qwen", "grok",
            "vibe", "junie", "augment", "bob", "cortex", "posit", "tabnine",
            "openclaw", "aider-desk", "astrbot", "autohand", "codearts",
            "codebuddy", "codemaker", "codestudio", "commandcode", "forgecode",
            "fx", "hermes", "inferencesh", "iflow", "jazz", "kimchi", "kode",
            "lingma", "mcpjam", "minimax", "moxby", "mux", "ona", "reasonix",
            "rovodev", "terramind", "tinycloud", "zcode", "zencoder",
            "neovate", "pochi", "adal", "eve", "promptscript",
        ];
        for id in SPEC {
            assert!(
                HARNESSES.iter().any(|h| h.id == *id),
                "spec harness {id} missing from HARNESSES"
            );
        }
    }

    #[test]
    fn multi_global_writes_all_dirs() {
        let dir = test_dir("multi-global");
        let out = install_at(&test_dirs(&dir), &dir, &["antigravity".to_string()], false, false, true).unwrap();
        match out {
            Out::Report { text, .. } => {
                assert!(text.contains(".gemini/antigravity/skills/ox/SKILL.md"));
                assert!(text.contains(".gemini/antigravity-cli/skills/ox/SKILL.md"));
            }
            _ => panic!(),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn project_only_harness_skips_global() {
        let dir = test_dir("no-global");
        let out = install_at(&test_dirs(&dir), &dir, &["eve".to_string()], false, false, true).unwrap();
        match out {
            Out::Report { text, .. } => assert!(text.contains("skipped Eve (no global dir)")),
            _ => panic!(),
        }
        // …but installs fine at project scope.
        let out = install_at(&test_dirs(&dir), &dir, &["eve".to_string()], false, true, false).unwrap();
        match out {
            Out::Report { text, .. } => assert!(text.contains("agent/skills/ox/SKILL.md")),
            _ => panic!(),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn env_global_falls_back_to_home() {
        // Var is never set in the test env — exercises the ~/<fallback> arm.
        let dir = resolve_global(
            &test_dirs(std::path::Path::new("/ox-test-home")),
            &GlobalDir::Env("OX_SKILL_TEST_UNSET_VAR", ".autohand/skills"),
        )
        .unwrap();
        assert_eq!(dir, PathBuf::from("/ox-test-home/.autohand/skills"));
    }

    #[test]
    fn shared_agents_marker_only_detects_generic() {
        let dir = test_dir("agents-marker");
        std::fs::create_dir_all(dir.join(".agents")).unwrap();
        let generic = harness_by_id("generic").unwrap();
        let pi = harness_by_id("pi").unwrap();
        assert!(detected_at(&test_dirs(&dir), &dir, generic));
        assert!(!detected_at(&test_dirs(&dir), &dir, pi), "shared .agents dir must not mark a named harness");
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn test_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ox-skill-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Global scope rooted at `dir` — tests never touch the real $HOME/XDG.
    fn test_dirs(dir: &std::path::Path) -> Dirs {
        Dirs {
            home: Some(dir.to_path_buf()),
            xdg: dir.join("xdg"),
        }
    }

    #[test]
    fn flags_parse_and_install() {
        let dir = test_dir("install");
        let out = install_at(
            &test_dirs(&dir),
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
        let out = upgrade_at(&test_dirs(&dir), &dir, &[], false, true, false).unwrap();
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
        let out = upgrade_at(&test_dirs(&dir), &dir, &["codex".to_string()], false, true, false).unwrap();
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
        let out = upgrade_at(&test_dirs(&dir), &dir, &[], false, true, false).unwrap();
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
        assert!(detected_at(&test_dirs(&dir), &dir, claude));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn detected_selection_subset() {
        let dir = test_dir("detect-subset");
        for h in select_harnesses(&test_dirs(&dir), &dir, &[], true).unwrap_or_default() {
            assert!(detected_at(&test_dirs(&dir), &dir, h), "{} not detected", h.id);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn detected_intersects_flagged_harnesses() {
        let dir = test_dir("detect-intersect");
        std::fs::create_dir_all(dir.join(".claude")).unwrap();
        let flags = vec!["claude".to_string(), "roo".to_string(), "goose".to_string()];
        let sel = select_harnesses(&test_dirs(&dir), &dir, &flags, true).unwrap();
        assert!(sel.iter().any(|h| h.id == "claude"));
        for h in &sel {
            assert!(detected_at(&test_dirs(&dir), &dir, h), "{} not detected", h.id);
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
            &test_dirs(&dir),
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
        assert!(upgrade_at(&test_dirs(&dir), &dir, &[], false, true, true).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn on_path_misses_unknown_binary() {
        assert!(!on_path("ox-skill-test-not-a-real-binary"));
    }
}
