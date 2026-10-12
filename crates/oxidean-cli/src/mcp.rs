//! `ox mcp` — wire the instance MCP endpoint (`POST /api/mcp`) into
//! agent-harness MCP client configs.
//!
//! `install` merges an `oxidean` server entry into each harness's config
//! file — `.mcp.json` (Claude Code), `.cursor/mcp.json` (Cursor),
//! `.vscode/mcp.json` (VS Code), `mcp_config.json` (Windsurf),
//! `settings.json` (Gemini, Zed), `config.toml` (Codex), `opencode.json`,
//! `mcp.json` (Kiro). `status` reports what's configured and pings the
//! endpoint; `remove` strips the entry again; `print` emits the JSON block
//! for manual pasting. Harness/scope selection reuses the `ox skill`
//! machinery (`--harness`/`--detected`/`--project`/`--global`, same prompts).
//!
//! Auth: the entry carries `Authorization: Bearer <pat>` resolved from the
//! stored `ox` login unless `--token`/`--token-stdin`/`--web`/`--anonymous`
//! says otherwise. Codex is the exception — its `bearer_token_env_var` keeps
//! the secret out of `config.toml`.

use std::collections::BTreeSet;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{json, Map, Value};

use oxidean_core::RpcResponse;

use crate::args::Global;
use crate::commands::{resolve_instance, CliError, Out, Runtime};
use crate::config::{self, InstanceEntry};
use crate::harness::{self, Detect, Dirs, HarnessRow, Scope};
use crate::rpc::Client;

/// Key the entry is written under in every JSON config.
const SERVER_NAME: &str = "oxidean";

/// Env var Codex reads its bearer from (`bearer_token_env_var`) — its
/// `config.toml` never carries the literal token.
const CODEX_TOKEN_ENV: &str = "OXIDEAN_TOKEN";

/// Scopes the `--web` device grant asks for — repo domain + registries, the
/// same set `ox auth login` requests (the MCP tools span both).
const DEVICE_SCOPES: &str = "repo package:read package:write";

/// How the `oxidean` server entry is serialized into a config file.
enum Format {
    /// JSON file — `{"<top_key>": {"oxidean": <entry>}}` merged over whatever
    /// else the file holds.
    Json {
        top_key: &'static str,
        kind: EntryKind,
    },
    /// Codex `config.toml` — a `[mcp_servers.oxidean]` table.
    CodexToml,
}

/// Server-entry shape per harness — remote-HTTP MCP config isn't uniform yet.
enum EntryKind {
    /// `{"url": U, "headers": {…}}` — cursor, kiro, generic.
    UrlHeaders,
    /// `{"type": "http", "url": U, "headers": {…}}` — claude, vscode.
    TypedHttp,
    /// `{"httpUrl": U, "headers": {…}}` — gemini (its `url` key means SSE).
    HttpUrl,
    /// `{"serverUrl": U, "headers": {…}}` — windsurf.
    ServerUrl,
    /// `{"type": "remote", "url": U, "enabled": true, "headers": {…}}` — opencode.
    OpencodeRemote,
    /// `{"command": {"path": "npx", "args": ["mcp-remote", U, …]}}` — zed
    /// (stdio bridge; `npx mcp-remote` proxies the streamable-HTTP endpoint).
    ZedBridge,
}

/// Global-scope config file location.
enum GlobalFile {
    /// `$HOME/<path>`
    Home(&'static str),
    /// `$XDG_CONFIG_HOME/<path>` or `~/.config/<path>`
    Xdg(&'static str),
}

struct McpHarness {
    /// `--harness <id>` selector.
    id: &'static str,
    label: &'static str,
    /// Repo-relative config file, `None` = global-scope only.
    project: Option<&'static str>,
    /// Global-scope config files.
    global: &'static [GlobalFile],
    format: Format,
    /// Installed-harness markers for `--detected` / prompt ordering.
    detect: &'static [Detect],
}

impl HarnessRow for McpHarness {
    fn id(&self) -> &'static str {
        self.id
    }
    fn label(&self) -> &'static str {
        self.label
    }
    fn markers(&self) -> &'static [Detect] {
        self.detect
    }
}

/// MCP-capable harnesses and their config files. Subset of the `ox skill`
/// registry — remote-HTTP MCP support is younger than skill dirs. `generic`
/// is print-only (any client with a `mcpServers`-shaped config).
const HARNESSES: &[McpHarness] = &[
    McpHarness {
        id: "claude",
        label: "Claude Code",
        project: Some(".mcp.json"),
        global: &[GlobalFile::Home(".claude.json")],
        format: Format::Json {
            top_key: "mcpServers",
            kind: EntryKind::TypedHttp,
        },
        detect: &[
            Detect::Project(".claude"),
            Detect::Home(".claude"),
            Detect::Bin("claude"),
        ],
    },
    McpHarness {
        id: "cursor",
        label: "Cursor",
        project: Some(".cursor/mcp.json"),
        global: &[GlobalFile::Home(".cursor/mcp.json")],
        format: Format::Json {
            top_key: "mcpServers",
            kind: EntryKind::UrlHeaders,
        },
        detect: &[
            Detect::Project(".cursor"),
            Detect::Home(".cursor"),
            Detect::Bin("cursor-agent"),
            Detect::Bin("agent"),
        ],
    },
    McpHarness {
        id: "vscode",
        label: "VS Code",
        project: Some(".vscode/mcp.json"),
        global: &[],
        format: Format::Json {
            top_key: "servers",
            kind: EntryKind::TypedHttp,
        },
        detect: &[Detect::Project(".vscode"), Detect::Bin("code")],
    },
    McpHarness {
        id: "windsurf",
        label: "Windsurf",
        project: None,
        global: &[GlobalFile::Home(".codeium/windsurf/mcp_config.json")],
        format: Format::Json {
            top_key: "mcpServers",
            kind: EntryKind::ServerUrl,
        },
        detect: &[
            Detect::Project(".windsurf"),
            Detect::Home(".codeium/windsurf"),
            Detect::Bin("windsurf"),
        ],
    },
    McpHarness {
        id: "gemini",
        label: "Gemini CLI",
        project: Some(".gemini/settings.json"),
        global: &[GlobalFile::Home(".gemini/settings.json")],
        format: Format::Json {
            top_key: "mcpServers",
            kind: EntryKind::HttpUrl,
        },
        detect: &[
            Detect::Project(".gemini"),
            Detect::Home(".gemini"),
            Detect::Bin("gemini"),
        ],
    },
    McpHarness {
        id: "codex",
        label: "Codex",
        project: Some(".codex/config.toml"),
        global: &[GlobalFile::Home(".codex/config.toml")],
        format: Format::CodexToml,
        detect: &[
            Detect::Project(".codex"),
            Detect::Home(".codex"),
            Detect::Bin("codex"),
        ],
    },
    McpHarness {
        id: "kiro",
        label: "Kiro",
        project: Some(".kiro/settings/mcp.json"),
        global: &[GlobalFile::Home(".kiro/settings/mcp.json")],
        format: Format::Json {
            top_key: "mcpServers",
            kind: EntryKind::UrlHeaders,
        },
        detect: &[
            Detect::Project(".kiro"),
            Detect::Home(".kiro"),
            Detect::Bin("kiro"),
            Detect::Bin("kiro-cli"),
        ],
    },
    McpHarness {
        id: "opencode",
        label: "OpenCode",
        project: Some("opencode.json"),
        global: &[GlobalFile::Xdg("opencode/opencode.json")],
        format: Format::Json {
            top_key: "mcp",
            kind: EntryKind::OpencodeRemote,
        },
        detect: &[
            Detect::Project(".opencode"),
            Detect::Xdg("opencode"),
            Detect::Bin("opencode"),
        ],
    },
    McpHarness {
        id: "zed",
        label: "Zed",
        project: Some(".zed/settings.json"),
        global: &[GlobalFile::Xdg("zed/settings.json")],
        format: Format::Json {
            top_key: "context_servers",
            kind: EntryKind::ZedBridge,
        },
        detect: &[Detect::Xdg("zed"), Detect::Bin("zed")],
    },
    McpHarness {
        id: "generic",
        label: "Any `mcpServers` client",
        project: None,
        global: &[],
        format: Format::Json {
            top_key: "mcpServers",
            kind: EntryKind::UrlHeaders,
        },
        detect: &[],
    },
];

fn endpoint_url(instance: &str) -> String {
    format!("{}/api/mcp", instance.trim_end_matches('/'))
}

/// The server-entry object for a JSON config — `token` becomes the
/// `Authorization: Bearer` header when present.
fn entry_json(kind: &EntryKind, endpoint: &str, token: Option<&str>) -> Value {
    let mut entry = match kind {
        EntryKind::UrlHeaders => json!({"url": endpoint}),
        EntryKind::TypedHttp => json!({"type": "http", "url": endpoint}),
        EntryKind::HttpUrl => json!({"httpUrl": endpoint}),
        EntryKind::ServerUrl => json!({"serverUrl": endpoint}),
        EntryKind::OpencodeRemote => {
            json!({"type": "remote", "url": endpoint, "enabled": true})
        }
        EntryKind::ZedBridge => {
            let mut args = vec![json!("mcp-remote"), json!(endpoint)];
            if let Some(t) = token {
                args.push(json!("--header"));
                args.push(json!(format!("Authorization: Bearer {t}")));
            }
            return json!({"command": {"path": "npx", "args": args}});
        }
    };
    if let Some(t) = token {
        entry["headers"] = json!({"Authorization": format!("Bearer {t}")});
    }
    entry
}

/// `[mcp_servers.oxidean]` section for Codex `config.toml`. With a token the
/// bearer rides an env var (`OXIDEAN_TOKEN` by default), so the file itself
/// stays secret-free.
fn codex_section(endpoint: &str, env_var: Option<&str>) -> String {
    let mut s = format!("[mcp_servers.{SERVER_NAME}]\nurl = \"{endpoint}\"\n");
    if let Some(var) = env_var {
        s.push_str(&format!("bearer_token_env_var = \"{var}\"\n"));
    }
    s
}

/// The `bearer_token_env_var` value inside a codex section body.
fn codex_env_var(body: &str) -> Option<String> {
    body.lines().find_map(|l| {
        let l = l.trim();
        l.strip_prefix("bearer_token_env_var")
            .and_then(|r| r.trim().strip_prefix('='))
            .map(|v| v.trim().trim_matches('"').to_string())
    })
}

/// Remove every `[mcp_servers.oxidean…]` table (our section plus nested
/// sub-tables) from `config.toml` text, preserving everything else.
fn strip_codex_section(content: &str) -> String {
    let header = format!("[mcp_servers.{SERVER_NAME}]");
    let nested = format!("[mcp_servers.{SERVER_NAME}.");
    let mut out = String::with_capacity(content.len());
    let mut skip = false;
    for line in content.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            skip = t == header || t.starts_with(&nested);
        }
        if !skip {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

/// The `[mcp_servers.oxidean]` body (lines after the header until the next
/// table) for status inspection — `None` when absent.
fn codex_entry_body(content: &str) -> Option<String> {
    let header = format!("[mcp_servers.{SERVER_NAME}]");
    let nested = format!("[mcp_servers.{SERVER_NAME}.");
    let mut inside = false;
    let mut body = String::new();
    for line in content.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            if inside && !(t == header || t.starts_with(&nested)) {
                break;
            }
            inside = t == header || t.starts_with(&nested);
            continue;
        }
        if inside {
            body.push_str(line);
            body.push('\n');
        }
    }
    inside.then_some(body)
}

/// Config files a harness writes at `scope` — `Err(reason)` when it has none
/// there (reported as skipped, mirroring `target_bases` in skill.rs).
fn target_files(
    dirs: &Dirs,
    root: &Path,
    h: &McpHarness,
    scope: Scope,
) -> Result<Vec<PathBuf>, String> {
    match scope {
        Scope::Project => h
            .project
            .map(|p| {
                vec![if root.as_os_str().is_empty() {
                    PathBuf::from(p)
                } else {
                    root.join(p)
                }]
            })
            .ok_or_else(|| "no project config file".to_string()),
        Scope::Global => {
            if h.global.is_empty() {
                return Err("no global config file".to_string());
            }
            h.global
                .iter()
                .map(|g| match g {
                    GlobalFile::Home(p) => dirs
                        .home
                        .as_ref()
                        .map(|h| h.join(p))
                        .ok_or_else(|| "no home directory".to_string()),
                    GlobalFile::Xdg(p) => Ok(dirs.xdg.join(p)),
                })
                .collect()
        }
    }
}

/// Existing JSON doc at `path` (missing/empty file → empty object, `existed =
/// false`). Malformed or non-object files are usage errors — we never clobber
/// a config we can't merge.
fn load_json(path: &Path) -> Result<(Map<String, Value>, bool), CliError> {
    match std::fs::read_to_string(path) {
        Ok(raw) => {
            if raw.trim().is_empty() {
                return Ok((Map::new(), false));
            }
            let v: Value = serde_json::from_str(&raw).map_err(|e| {
                CliError::Usage(format!(
                    "{} exists but isn't valid JSON ({e}) — refusing to overwrite",
                    path.display()
                ))
            })?;
            match v {
                Value::Object(m) => Ok((m, true)),
                _ => Err(CliError::Usage(format!(
                    "{} isn't a JSON object — refusing to overwrite",
                    path.display()
                ))),
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok((Map::new(), false)),
        Err(e) => Err(CliError::Usage(format!("{}: {e}", path.display()))),
    }
}

/// Write `text` to `path`, creating parents; 0600 on unix since MCP configs
/// may embed a bearer token.
fn write_text(path: &Path, text: &str) -> Result<(), CliError> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .map_err(|e| CliError::Usage(format!("create {}: {e}", parent.display())))?;
        }
    }
    std::fs::write(path, text)
        .map_err(|e| CliError::Usage(format!("write {}: {e}", path.display())))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

fn write_json(path: &Path, doc: &Map<String, Value>) -> Result<(), CliError> {
    let mut text = serde_json::to_string_pretty(&Value::Object(doc.clone()))
        .map_err(|e| CliError::Usage(format!("serialize {}: {e}", path.display())))?;
    text.push('\n');
    write_text(path, &text)
}

/// Merge (or replace) the `oxidean` entry into one config file.
fn install_entry(
    path: &Path,
    format: &Format,
    endpoint: &str,
    token: Option<&str>,
) -> Result<(), CliError> {
    match format {
        Format::Json { top_key, kind } => {
            let (mut doc, _) = load_json(path)?;
            let top = doc
                .entry((*top_key).to_string())
                .or_insert_with(|| json!({}));
            let servers = top.as_object_mut().ok_or_else(|| {
                CliError::Usage(format!(
                    "{}: `{top_key}` isn't an object — refusing to overwrite",
                    path.display()
                ))
            })?;
            servers.insert(SERVER_NAME.to_string(), entry_json(kind, endpoint, token));
            write_json(path, &doc)
        }
        Format::CodexToml => {
            let raw = std::fs::read_to_string(path).unwrap_or_default();
            let mut content = strip_codex_section(&raw).trim_end().to_string();
            if !content.is_empty() {
                content.push_str("\n\n");
            }
            content.push_str(&codex_section(endpoint, token.map(|_| CODEX_TOKEN_ENV)));
            write_text(path, &content)
        }
    }
}

/// Drop the `oxidean` entry from one config file.
/// Returns true when something was removed.
fn remove_entry(path: &Path, format: &Format) -> Result<bool, CliError> {
    match format {
        Format::Json { top_key, .. } => {
            let Ok((mut doc, existed)) = load_json(path) else {
                return Err(CliError::Usage(format!(
                    "{} exists but isn't valid JSON — refusing to edit",
                    path.display()
                )));
            };
            if !existed {
                return Ok(false);
            }
            let removed = doc
                .get_mut(*top_key)
                .and_then(Value::as_object_mut)
                .map(|servers| servers.remove(SERVER_NAME).is_some())
                .unwrap_or(false);
            if !removed {
                return Ok(false);
            }
            // Tidy: drop the top key when it went empty (`{}` left behind is
            // legal but noise).
            if doc
                .get(*top_key)
                .and_then(Value::as_object)
                .map(|o| o.is_empty())
                .unwrap_or(false)
            {
                doc.remove(*top_key);
            }
            write_json(path, &doc)?;
            Ok(true)
        }
        Format::CodexToml => {
            let Ok(raw) = std::fs::read_to_string(path) else {
                return Ok(false);
            };
            if codex_entry_body(&raw).is_none() {
                return Ok(false);
            }
            write_text(path, strip_codex_section(&raw).trim_end())?;
            Ok(true)
        }
    }
}

/// `ox mcp print` — the config block for manual pasting. The token is always
/// a `<token>` placeholder; real credentials never hit stdout here.
pub fn print_cfg(g: &Global, rt: &Runtime, harness_id: Option<&str>) -> Result<Out, CliError> {
    let instance = resolve_instance(g, &rt.config)?;
    print_block(harness_id, &endpoint_url(&instance))
}

fn print_block(harness_id: Option<&str>, endpoint: &str) -> Result<Out, CliError> {
    let h = match harness_id {
        Some(id) => Some(harness::by_id(HARNESSES, id)?),
        None => None,
    };
    let (text, entry) = match h.map(|h| &h.format) {
        Some(Format::CodexToml) => {
            let text = codex_section(endpoint, Some(CODEX_TOKEN_ENV));
            (text.clone(), json!({"format": "toml", "section": text}))
        }
        other => {
            let (top_key, kind) = match other {
                Some(Format::Json { top_key, kind }) => (*top_key, kind),
                None => ("mcpServers", &EntryKind::UrlHeaders),
                _ => unreachable!(),
            };
            let block =
                json!({ top_key: { SERVER_NAME: entry_json(kind, endpoint, Some("<token>")) } });
            let text = serde_json::to_string_pretty(&block).unwrap_or_else(|_| "{}".into());
            (
                text,
                json!({"format": "json", "top_key": top_key, "entry": block[top_key][SERVER_NAME].clone()}),
            )
        }
    };
    Ok(Out::Report {
        text,
        json: json!({
            "ok": true,
            "data": {"endpoint": endpoint, "harness": h.map(|h| h.id), "block": entry}
        }),
    })
}

/// Resolve the credential for `install`: explicit flag → device grant →
/// stored login → anonymous (with a hint). Returns the token plus a short
/// provenance label for the report.
async fn resolve_install_token(
    g: &Global,
    rt: &Runtime,
    instance: &str,
    token: Option<String>,
    token_stdin: bool,
    web: bool,
    anonymous: bool,
) -> Result<(Option<String>, &'static str), CliError> {
    if let Some(t) = token {
        let t = t.trim().to_string();
        if t.is_empty() {
            return Err(CliError::Usage("--token: empty token".into()));
        }
        return Ok((Some(t), "--token"));
    }
    if token_stdin {
        let mut buf = String::new();
        std::io::stdin()
            .read_to_string(&mut buf)
            .map_err(|e| CliError::Usage(format!("--token-stdin: read stdin: {e}")))?;
        let t = buf.trim().to_string();
        if t.is_empty() {
            return Err(CliError::Usage("--token-stdin: empty token".into()));
        }
        return Ok((Some(t), "--token-stdin"));
    }
    if web {
        return Ok((
            Some(device_grant_login(rt, instance).await?),
            "device grant",
        ));
    }
    if anonymous {
        return Ok((None, "anonymous"));
    }
    let _ = g; // --instance already folded into `instance` by the caller.
    match config::resolve_token(config::env_token().as_deref(), &rt.config, instance) {
        Some(t) => Ok((Some(t), "stored login")),
        None => {
            eprintln!(
                "ox mcp: no credentials for {instance} — installing without auth \
                 (public repos only).\n  Run `ox auth login` first, or pass \
                 --token/--token-stdin/--web for private-repo access."
            );
            Ok((None, "anonymous"))
        }
    }
}

/// Run the RFC 8628 device grant and store the minted PAT like `ox auth
/// login`, so the CLI and every configured harness share one credential.
async fn device_grant_login(rt: &Runtime, instance: &str) -> Result<String, CliError> {
    let token = crate::device::run_grant(instance, DEVICE_SCOPES).await?;
    let authed = Client::new(instance, Some(token.clone()));
    if let Ok(RpcResponse::Ok { data, .. }) = authed.call("auth.me", json!({})).await {
        if let Some(user) = data.get("username").and_then(Value::as_str) {
            eprintln!("Logged in to {instance} as {user}");
        }
    }
    let mut cfg = rt.config.clone();
    cfg.instances.insert(
        instance.to_string(),
        InstanceEntry {
            token: Some(token.clone()),
        },
    );
    cfg.default_instance = Some(instance.to_string());
    config::save(&rt.config_path, &cfg).map_err(CliError::Config)?;
    Ok(token)
}

/// `--upgrade` credential handling — existing entries keep their auth unless
/// an explicit flag replaces it (`--anonymous` strips it). The stored `ox`
/// login is never consulted: an upgrade shouldn't change who a file speaks as.
enum AuthOverride {
    Preserve,
    Set(Option<String>),
}

async fn resolve_upgrade_override(
    rt: &Runtime,
    instance: &str,
    token: Option<String>,
    token_stdin: bool,
    web: bool,
    anonymous: bool,
) -> Result<AuthOverride, CliError> {
    if let Some(t) = token {
        let t = t.trim().to_string();
        if t.is_empty() {
            return Err(CliError::Usage("--token: empty token".into()));
        }
        return Ok(AuthOverride::Set(Some(t)));
    }
    if token_stdin {
        let mut buf = String::new();
        std::io::stdin()
            .read_to_string(&mut buf)
            .map_err(|e| CliError::Usage(format!("--token-stdin: read stdin: {e}")))?;
        let t = buf.trim().to_string();
        if t.is_empty() {
            return Err(CliError::Usage("--token-stdin: empty token".into()));
        }
        return Ok(AuthOverride::Set(Some(t)));
    }
    if web {
        return Ok(AuthOverride::Set(Some(
            device_grant_login(rt, instance).await?,
        )));
    }
    if anonymous {
        return Ok(AuthOverride::Set(None));
    }
    Ok(AuthOverride::Preserve)
}

/// `ox mcp install` — select harnesses + scope, resolve credentials, merge
/// the entry into each config file.
#[allow(clippy::too_many_arguments)]
pub async fn install(
    g: &Global,
    rt: &Runtime,
    harness_flags: &[String],
    detected: bool,
    project: bool,
    global: bool,
    token: Option<String>,
    token_stdin: bool,
    web: bool,
    anonymous: bool,
    upgrade: bool,
) -> Result<Out, CliError> {
    let instance = resolve_instance(g, &rt.config)?;
    if upgrade {
        let auth =
            resolve_upgrade_override(rt, &instance, token, token_stdin, web, anonymous).await?;
        return upgrade_at(
            &Dirs::env(),
            Path::new(""),
            harness_flags,
            detected,
            project,
            global,
            Some(&endpoint_url(&instance)),
            &auth,
        );
    }
    let (token, auth_src) =
        resolve_install_token(g, rt, &instance, token, token_stdin, web, anonymous).await?;
    install_at(
        &Dirs::env(),
        Path::new(""),
        harness_flags,
        detected,
        project,
        global,
        &endpoint_url(&instance),
        token.as_deref(),
        auth_src,
    )
}

#[allow(clippy::too_many_arguments)]
fn install_at(
    dirs: &Dirs,
    root: &Path,
    harness_flags: &[String],
    detected_only: bool,
    project: bool,
    global: bool,
    endpoint: &str,
    token: Option<&str>,
    auth_src: &str,
) -> Result<Out, CliError> {
    let (scope, harnesses) = harness::resolve_targets(
        HARNESSES,
        dirs,
        root,
        harness_flags,
        detected_only,
        project,
        global,
        "Configure the instance MCP endpoint for which harness(es)?",
    )?;

    let mut written = Vec::new();
    let mut skipped = Vec::new();
    let mut codex_written = false;
    let mut seen = BTreeSet::new();
    for h in harnesses {
        let files = match target_files(dirs, root, h, scope) {
            Ok(f) => f,
            Err(reason) => {
                skipped.push(format!("{} ({reason})", h.label));
                continue;
            }
        };
        for path in files {
            if !seen.insert(path.clone()) {
                continue;
            }
            install_entry(&path, &h.format, endpoint, token)?;
            if matches!(h.format, Format::CodexToml) {
                codex_written = true;
            }
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
    if codex_written && token.is_some() {
        text.push_str(&format!(
            "\nnote: Codex reads the token from ${CODEX_TOKEN_ENV} — \
             e.g. `export {CODEX_TOKEN_ENV}=$(ox auth token)`.\n"
        ));
    }
    if !written.is_empty() {
        text.push_str(&format!(
            "\nendpoint {endpoint} · auth: {auth_src} — restart the harness to pick it up.\n"
        ));
    }
    let written_v: Vec<Value> = written.iter().map(|w| json!(w)).collect();
    Ok(Out::Report {
        text: text.trim_end().to_string(),
        json: json!({"ok": true, "data": {
            "endpoint": endpoint, "auth": auth_src,
            "installed": written_v, "skipped": skipped,
        }}),
    })
}

/// `ox mcp remove` — strip the `oxidean` entry from selected configs.
pub fn remove(
    g: &Global,
    _rt: &Runtime,
    harness_flags: &[String],
    detected: bool,
    project: bool,
    global: bool,
) -> Result<Out, CliError> {
    let _ = g;
    remove_at(
        &Dirs::env(),
        Path::new(""),
        harness_flags,
        detected,
        project,
        global,
    )
}

fn remove_at(
    dirs: &Dirs,
    root: &Path,
    harness_flags: &[String],
    detected_only: bool,
    project: bool,
    global: bool,
) -> Result<Out, CliError> {
    let (scope, harnesses) = harness::resolve_targets(
        HARNESSES,
        dirs,
        root,
        harness_flags,
        detected_only,
        project,
        global,
        "Remove the Oxidean MCP entry from which harness(es)?",
    )?;

    let mut removed = Vec::new();
    let mut skipped = Vec::new();
    let mut seen = BTreeSet::new();
    for h in harnesses {
        let files = match target_files(dirs, root, h, scope) {
            Ok(f) => f,
            Err(reason) => {
                skipped.push(format!("{} ({reason})", h.label));
                continue;
            }
        };
        for path in files {
            if !seen.insert(path.clone()) {
                continue;
            }
            if remove_entry(&path, &h.format)? {
                removed.push(path.display().to_string());
            } else {
                skipped.push(format!("{} (no entry)", path.display()));
            }
        }
    }

    let mut text = String::new();
    for r in &removed {
        text.push_str(&format!("removed {r}\n"));
    }
    for s in &skipped {
        text.push_str(&format!("skipped {s}\n"));
    }
    let removed_v: Vec<Value> = removed.iter().map(|w| json!(w)).collect();
    Ok(Out::Report {
        text: text.trim_end().to_string(),
        json: json!({"ok": true, "data": {"removed": removed_v, "skipped": skipped}}),
    })
}

/// Endpoint URL a JSON entry points at (`url`/`httpUrl`/`serverUrl`, or
/// `args[1]` for the zed mcp-remote bridge).
fn entry_url(entry: &Value) -> Option<String> {
    entry
        .get("url")
        .or_else(|| entry.get("httpUrl"))
        .or_else(|| entry.get("serverUrl"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            entry
                .pointer("/command/args/1")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
}

/// Bearer token a JSON entry carries — `headers.Authorization` for HTTP
/// entries, the arg after `--header` for the zed bridge.
fn entry_token(entry: &Value) -> Option<String> {
    entry
        .pointer("/headers/Authorization")
        .and_then(Value::as_str)
        .and_then(|s| s.strip_prefix("Bearer "))
        .map(str::to_string)
        .or_else(|| {
            let args = entry.pointer("/command/args")?.as_array()?;
            let i = args.iter().position(|a| a.as_str() == Some("--header"))?;
            args.get(i + 1)?
                .as_str()?
                .strip_prefix("Authorization: Bearer ")
                .map(str::to_string)
        })
}

/// Rebuild an existing `oxidean` entry in canonical shape — `endpoint`
/// replaces the URL when `Some` (the `None` case is the self-update refresh,
/// which keeps whatever instance the entry already targets). Auth comes from
/// `auth`: preserved verbatim or replaced by an explicit flag. `None` when
/// the entry isn't an object or carries no URL we can keep.
fn upgraded_entry(
    existing: &Value,
    kind: &EntryKind,
    endpoint: Option<&str>,
    auth: &AuthOverride,
) -> Option<Value> {
    if !existing.is_object() {
        return None;
    }
    let url = endpoint
        .map(str::to_string)
        .or_else(|| entry_url(existing))?;
    Some(match kind {
        EntryKind::ZedBridge => {
            let header = match auth {
                AuthOverride::Preserve => existing
                    .pointer("/command/args")
                    .and_then(Value::as_array)
                    .and_then(|a| {
                        a.iter()
                            .position(|v| v.as_str() == Some("--header"))
                            .and_then(|i| a.get(i + 1))
                            .and_then(Value::as_str)
                    })
                    .map(str::to_string),
                AuthOverride::Set(t) => t.as_ref().map(|t| format!("Authorization: Bearer {t}")),
            };
            let mut args = vec![json!("mcp-remote"), json!(url)];
            if let Some(h) = header {
                args.push(json!("--header"));
                args.push(json!(h));
            }
            json!({"command": {"path": "npx", "args": args}})
        }
        _ => {
            let mut e = match auth {
                AuthOverride::Preserve => entry_json(kind, &url, None),
                AuthOverride::Set(t) => entry_json(kind, &url, t.as_deref()),
            };
            if let AuthOverride::Preserve = auth {
                // Keep the headers object verbatim — extra custom headers
                // ride along untouched.
                if let Some(h) = existing.get("headers").filter(|h| h.is_object()) {
                    e["headers"] = h.clone();
                }
            }
            e
        }
    })
}

/// Outcome of a single-file upgrade sweep.
enum Upgrade {
    /// No `oxidean` entry — file untouched.
    NoEntry,
    /// Entry exists but isn't a shape we generated, or the file doesn't
    /// parse — untouched, reported as skipped.
    Unrecognized,
    /// Already canonical.
    UpToDate,
    /// Rewritten.
    Upgraded,
}

fn upgrade_entry(
    path: &Path,
    format: &Format,
    endpoint: Option<&str>,
    auth: &AuthOverride,
) -> Result<Upgrade, CliError> {
    match format {
        Format::Json { top_key, kind } => {
            // A file we can't parse is skipped, never clobbered — same house
            // rule as install, but a sweep skips instead of aborting.
            let Ok((mut doc, existed)) = load_json(path) else {
                return Ok(Upgrade::Unrecognized);
            };
            if !existed {
                return Ok(Upgrade::NoEntry);
            }
            let Some(existing) = doc.get(*top_key).and_then(|t| t.get(SERVER_NAME)).cloned() else {
                return Ok(Upgrade::NoEntry);
            };
            let Some(new_entry) = upgraded_entry(&existing, kind, endpoint, auth) else {
                return Ok(Upgrade::Unrecognized);
            };
            if existing == new_entry {
                return Ok(Upgrade::UpToDate);
            }
            doc[*top_key][SERVER_NAME] = new_entry;
            write_json(path, &doc)?;
            Ok(Upgrade::Upgraded)
        }
        Format::CodexToml => {
            let Ok(raw) = std::fs::read_to_string(path) else {
                return Ok(Upgrade::NoEntry);
            };
            let Some(body) = codex_entry_body(&raw) else {
                return Ok(Upgrade::NoEntry);
            };
            let url = endpoint
                .map(str::to_string)
                .or_else(|| codex_entry_url(&body));
            let Some(url) = url else {
                return Ok(Upgrade::Unrecognized);
            };
            let env_var = match auth {
                AuthOverride::Preserve => codex_env_var(&body),
                AuthOverride::Set(Some(_)) => Some(CODEX_TOKEN_ENV.to_string()),
                AuthOverride::Set(None) => None,
            };
            let mut content = strip_codex_section(&raw).trim_end().to_string();
            if !content.is_empty() {
                content.push_str("\n\n");
            }
            content.push_str(&codex_section(&url, env_var.as_deref()));
            if content.trim_end() == raw.trim_end() {
                return Ok(Upgrade::UpToDate);
            }
            write_text(path, &content)?;
            Ok(Upgrade::Upgraded)
        }
    }
}

/// The `url = "…"` value inside a codex section body.
fn codex_entry_url(body: &str) -> Option<String> {
    body.lines().find_map(|l| {
        let l = l.trim();
        l.strip_prefix("url")
            .and_then(|r| r.trim().strip_prefix('='))
            .map(|v| v.trim().trim_matches('"').to_string())
    })
}

/// `--upgrade` — rewrite the `oxidean` entry only where it already exists.
/// Never prompts, never creates files; bare invocation sweeps every harness
/// at both scopes, `--harness`/`--detected`/scope flags narrow it. `endpoint`
/// `None` preserves each entry's URL (`ox self-update` refresh).
#[allow(clippy::too_many_arguments)]
fn upgrade_at(
    dirs: &Dirs,
    root: &Path,
    harness_flags: &[String],
    detected_only: bool,
    project: bool,
    global: bool,
    endpoint: Option<&str>,
    auth: &AuthOverride,
) -> Result<Out, CliError> {
    let scopes: &[Scope] = match (project, global) {
        (true, false) => &[Scope::Project],
        (false, true) => &[Scope::Global],
        (true, true) => {
            return Err(CliError::Usage(
                "--project and --global are exclusive".into(),
            ));
        }
        (false, false) => &[Scope::Project, Scope::Global],
    };
    let harnesses: Vec<&McpHarness> = if harness_flags.is_empty() && !detected_only {
        HARNESSES.iter().filter(|h| h.id != "generic").collect()
    } else {
        harness::select_harnesses(HARNESSES, dirs, root, harness_flags, detected_only)?
    };

    let mut upgraded = Vec::new();
    let mut current = Vec::new();
    let mut skipped = Vec::new();
    let mut seen = BTreeSet::new();
    for h in harnesses {
        for &scope in scopes {
            let Ok(files) = target_files(dirs, root, h, scope) else {
                continue;
            };
            for path in files {
                if !seen.insert(path.clone()) {
                    continue;
                }
                match upgrade_entry(&path, &h.format, endpoint, auth)? {
                    Upgrade::Upgraded => upgraded.push(path.display().to_string()),
                    Upgrade::UpToDate => current.push(path.display().to_string()),
                    Upgrade::Unrecognized => skipped.push(path.display().to_string()),
                    Upgrade::NoEntry => {}
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
    for s in &skipped {
        text.push_str(&format!("skipped {s}\n"));
    }
    if upgraded.is_empty() && current.is_empty() && skipped.is_empty() {
        text.push_str("no MCP server entries found\n");
    }
    let upgraded_v: Vec<Value> = upgraded.iter().map(|w| json!(w)).collect();
    let current_v: Vec<Value> = current.iter().map(|w| json!(w)).collect();
    Ok(Out::Report {
        text: text.trim_end().to_string(),
        json: json!({"ok": true, "data": {
            "endpoint": endpoint,
            "upgraded": upgraded_v, "current": current_v, "skipped": skipped,
        }}),
    })
}

/// `ox self-update` hook — re-render every existing `oxidean` entry into this
/// binary's canonical shape, keeping each entry's endpoint and credential.
/// Returns the number of files rewritten. Never prompts or creates files.
pub(crate) fn refresh_installed() -> Result<usize, CliError> {
    let out = upgrade_at(
        &Dirs::env(),
        Path::new(""),
        &[],
        false,
        false,
        false,
        None,
        &AuthOverride::Preserve,
    )?;
    Ok(match &out {
        Out::Report { json, .. } => json
            .pointer("/data/upgraded")
            .and_then(Value::as_array)
            .map(|a| a.len())
            .unwrap_or(0),
        _ => 0,
    })
}

/// One discovered config file and what its `oxidean` entry says.
struct StatusRow {
    harness: &'static str,
    scope: &'static str,
    path: PathBuf,
    /// `Some(entry state)` when the file parses — url + auth kind.
    state: Result<Option<(String, String)>, ()>,
}

/// Read the `oxidean` entry out of a config file.
/// `Ok(Some((url, auth)))` configured · `Ok(None)` file parses, no entry ·
/// `Err(())` exists but doesn't parse as our format expects.
fn inspect_file(path: &Path, format: &Format) -> Result<Option<(String, String)>, ()> {
    match format {
        Format::Json { top_key, .. } => {
            let (doc, _) = load_json(path).map_err(|_| ())?;
            let Some(entry) = doc.get(*top_key).and_then(|t| t.get(SERVER_NAME)) else {
                return Ok(None);
            };
            Ok(Some((
                entry_url(entry).unwrap_or_default(),
                if entry_token(entry).is_some() {
                    "bearer"
                } else {
                    "none"
                }
                .to_string(),
            )))
        }
        Format::CodexToml => {
            let raw = std::fs::read_to_string(path).map_err(|_| ())?;
            let Some(body) = codex_entry_body(&raw) else {
                return Ok(None);
            };
            let url = codex_entry_url(&body).unwrap_or_default();
            let auth = codex_env_var(&body)
                .map(|v| format!("env {v}"))
                .unwrap_or_else(|| "none".to_string());
            Ok(Some((url, auth)))
        }
    }
}

/// Scan every harness config path; files that exist get a row.
fn status_at(dirs: &Dirs, root: &Path, current_endpoint: Option<&str>) -> Vec<StatusRow> {
    let mut rows = Vec::new();
    let mut seen = BTreeSet::new();
    for h in HARNESSES {
        if h.id == "generic" {
            continue;
        }
        for (scope, files) in [
            ("project", h.project.map(|p| vec![root.join(p)])),
            (
                "global",
                h.global
                    .iter()
                    .map(|g| match g {
                        GlobalFile::Home(p) => dirs.home.as_ref().map(|h| h.join(p)),
                        GlobalFile::Xdg(p) => Some(dirs.xdg.join(p)),
                    })
                    .collect::<Option<Vec<_>>>(),
            ),
        ] {
            let Some(files) = files else { continue };
            for path in files {
                if !path.exists() || !seen.insert(path.clone()) {
                    continue;
                }
                let _ = current_endpoint; // url comparison is the caller's
                rows.push(StatusRow {
                    harness: h.id,
                    scope,
                    state: inspect_file(&path, &h.format),
                    path,
                });
            }
        }
    }
    rows
}

/// Result of POSTing `initialize` (+ `tools/list`) at the endpoint.
enum EndpointState {
    /// Answered — server name/version plus the advertised tool count.
    Reachable {
        server: String,
        tools: Option<usize>,
    },
    /// 404 — the instance admin disabled MCP (or the proxy has no such route).
    Disabled,
    /// 401 — the stored credential was rejected.
    AuthRejected,
    /// Couldn't get a usable response at all.
    Unreachable(String),
}

async fn ping_endpoint(endpoint: &str, token: Option<&str>) -> EndpointState {
    let Ok(client) = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
    else {
        return EndpointState::Unreachable("http client init failed".into());
    };
    let send = |body: Value| {
        let client = client.clone();
        let token = token.map(str::to_string);
        let endpoint = endpoint.to_string();
        async move {
            let mut req = client.post(endpoint).json(&body);
            if let Some(t) = token {
                req = req.bearer_auth(t);
            }
            req.send().await
        }
    };
    let init = json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": {
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": {"name": "ox", "version": env!("CARGO_PKG_VERSION")},
        },
    });
    let resp = match send(init).await {
        Ok(r) => r,
        Err(e) => return EndpointState::Unreachable(e.to_string()),
    };
    match resp.status().as_u16() {
        401 => return EndpointState::AuthRejected,
        404 => return EndpointState::Disabled,
        s if !(200..300).contains(&s) => {
            return EndpointState::Unreachable(format!("HTTP {s}"));
        }
        _ => {}
    }
    let body: Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => return EndpointState::Unreachable(format!("bad JSON: {e}")),
    };
    let server = body
        .pointer("/result/serverInfo/name")
        .and_then(Value::as_str)
        .unwrap_or("mcp")
        .to_string();
    // Tool count — second call; failure just drops the count.
    let tools = {
        let list = json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}});
        match send(list).await {
            Ok(r) if r.status().is_success() => r.json::<Value>().await.ok().and_then(|v| {
                v.pointer("/result/tools")
                    .and_then(Value::as_array)
                    .map(|a| a.len())
            }),
            _ => None,
        }
    };
    EndpointState::Reachable { server, tools }
}

/// `ox mcp status` — what's configured on this machine plus a live probe of
/// the instance endpoint.
pub async fn status(g: &Global, rt: &Runtime) -> Result<Out, CliError> {
    let instance = config::resolve_instance(
        g.instance.as_deref(),
        config::env_instance().as_deref(),
        &rt.config,
    )
    .map_err(CliError::Usage)?;
    let endpoint = instance.as_deref().map(endpoint_url);
    let rows = status_at(&Dirs::env(), Path::new(""), endpoint.as_deref());

    let token = instance
        .as_deref()
        .and_then(|i| config::resolve_token(config::env_token().as_deref(), &rt.config, i));
    let probe = match &endpoint {
        Some(ep) => Some(ping_endpoint(ep, token.as_deref()).await),
        None => None,
    };

    let mut text = String::new();
    if rows.is_empty() {
        text.push_str("no harness MCP configs found — `ox mcp install` writes them\n");
    }
    for row in &rows {
        let state = match &row.state {
            Ok(Some((url, auth))) => {
                let note = match &endpoint {
                    Some(ep) if url != ep => " (other instance)",
                    _ => "",
                };
                format!("{url}{note} [{auth}]")
            }
            Ok(None) => "no oxidean entry".to_string(),
            Err(()) => "unparseable".to_string(),
        };
        text.push_str(&format!(
            "{:<10} {:<8} {}  {}\n",
            row.harness,
            row.scope,
            row.path.display(),
            state
        ));
    }
    match (&endpoint, &probe) {
        (Some(ep), Some(EndpointState::Reachable { server, tools })) => {
            let count = tools.map(|n| format!(", {n} tools")).unwrap_or_default();
            text.push_str(&format!("\nendpoint {ep}: reachable — {server}{count}\n"));
        }
        (Some(ep), Some(EndpointState::Disabled)) => {
            text.push_str(&format!(
                "\nendpoint {ep}: MCP is disabled on the instance \
                 (admin: `ox admin mcp set --enable`)\n"
            ));
        }
        (Some(ep), Some(EndpointState::AuthRejected)) => {
            text.push_str(&format!(
                "\nendpoint {ep}: credential rejected (401) — `ox auth login` to refresh\n"
            ));
        }
        (Some(ep), Some(EndpointState::Unreachable(why))) => {
            text.push_str(&format!("\nendpoint {ep}: unreachable — {why}\n"));
        }
        (None, _) => {
            text.push_str(
                "\nno instance configured — the file scan above is all there is. \
                 `--instance <url>` (or `ox auth login`) adds a live probe.\n",
            );
        }
        _ => {}
    }

    let rows_json: Vec<Value> = rows
        .iter()
        .map(|r| {
            let (state, url, auth) = match &r.state {
                Ok(Some((u, a))) => ("configured", json!(u), json!(a)),
                Ok(None) => ("no-entry", Value::Null, Value::Null),
                Err(()) => ("unparseable", Value::Null, Value::Null),
            };
            json!({
                "harness": r.harness, "scope": r.scope,
                "path": r.path.display().to_string(),
                "state": state, "url": url, "auth": auth,
            })
        })
        .collect();
    let probe_json = match &probe {
        Some(EndpointState::Reachable { server, tools }) => {
            json!({"state": "reachable", "server": server, "tools": tools})
        }
        Some(EndpointState::Disabled) => json!({"state": "disabled"}),
        Some(EndpointState::AuthRejected) => json!({"state": "auth-rejected"}),
        Some(EndpointState::Unreachable(w)) => json!({"state": "unreachable", "error": w}),
        None => json!({"state": "skipped", "reason": "no instance configured"}),
    };
    Ok(Out::Report {
        text: text.trim_end().to_string(),
        json: json!({"ok": true, "data": {
            "configured": rows_json, "endpoint": endpoint, "probe": probe_json,
        }}),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ox-mcp-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Global scope rooted at `dir` — tests never touch the real $HOME/XDG.
    fn test_dirs(dir: &Path) -> Dirs {
        Dirs {
            home: Some(dir.to_path_buf()),
            xdg: dir.join("xdg"),
        }
    }

    #[test]
    fn harness_ids_unique() {
        let mut seen = std::collections::BTreeSet::new();
        for h in HARNESSES {
            assert!(seen.insert(h.id), "dup {}", h.id);
            assert!(!h.id.contains(' '));
        }
    }

    #[test]
    fn entry_json_shapes() {
        let url = "https://forge.example.com/api/mcp";
        let e = entry_json(&EntryKind::TypedHttp, url, Some("pat"));
        assert_eq!(e["type"], "http");
        assert_eq!(e["url"], url);
        assert_eq!(e["headers"]["Authorization"], "Bearer pat");
        let e = entry_json(&EntryKind::HttpUrl, url, None);
        assert_eq!(e["httpUrl"], url);
        assert!(e.get("headers").is_none());
        let e = entry_json(&EntryKind::ZedBridge, url, Some("pat"));
        assert_eq!(e["command"]["args"][0], "mcp-remote");
        assert_eq!(e["command"]["args"][1], url);
        assert_eq!(e["command"]["args"][2], "--header");
    }

    #[test]
    fn install_writes_and_merges_json() {
        let dir = test_dir("install-json");
        // Pre-existing config — other server + unknown top-level key survive.
        let cursor = dir.join(".cursor");
        std::fs::create_dir_all(&cursor).unwrap();
        std::fs::write(
            cursor.join("mcp.json"),
            r#"{"mcpServers": {"other": {"url": "https://x/mcp"}}, "editor": {"x": 1}}"#,
        )
        .unwrap();
        let out = install_at(
            &test_dirs(&dir),
            &dir,
            &["cursor".to_string()],
            false,
            true,
            false,
            "https://forge.example.com/api/mcp",
            Some("pat_123"),
            "test",
        )
        .unwrap();
        match &out {
            Out::Report { text, .. } => assert!(text.contains(".cursor/mcp.json")),
            _ => panic!(),
        }
        let doc: Value =
            serde_json::from_str(&std::fs::read_to_string(cursor.join("mcp.json")).unwrap())
                .unwrap();
        assert_eq!(doc["mcpServers"]["other"]["url"], "https://x/mcp");
        assert_eq!(doc["editor"]["x"], 1);
        let e = &doc["mcpServers"]["oxidean"];
        assert_eq!(e["url"], "https://forge.example.com/api/mcp");
        assert_eq!(e["headers"]["Authorization"], "Bearer pat_123");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn install_anonymous_writes_no_headers() {
        let dir = test_dir("install-anon");
        install_at(
            &test_dirs(&dir),
            &dir,
            &["claude".to_string()],
            false,
            true,
            false,
            "https://forge.example.com/api/mcp",
            None,
            "test",
        )
        .unwrap();
        let doc: Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join(".mcp.json")).unwrap()).unwrap();
        let e = &doc["mcpServers"]["oxidean"];
        assert_eq!(e["type"], "http");
        assert!(e.get("headers").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn install_is_idempotent() {
        let dir = test_dir("install-twice");
        let dirs = test_dirs(&dir);
        for _ in 0..2 {
            install_at(
                &dirs,
                &dir,
                &["cursor".to_string()],
                false,
                true,
                false,
                "https://forge.example.com/api/mcp",
                Some("pat"),
                "test",
            )
            .unwrap();
        }
        let doc: Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join(".cursor/mcp.json")).unwrap())
                .unwrap();
        assert_eq!(doc["mcpServers"].as_object().unwrap().len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn install_refuses_malformed_json() {
        let dir = test_dir("install-badjson");
        std::fs::create_dir_all(dir.join(".cursor")).unwrap();
        std::fs::write(dir.join(".cursor/mcp.json"), "{not json").unwrap();
        let err = install_at(
            &test_dirs(&dir),
            &dir,
            &["cursor".to_string()],
            false,
            true,
            false,
            "https://x/api/mcp",
            None,
            "test",
        )
        .err()
        .unwrap();
        assert!(format!("{err}").contains("refusing to overwrite"), "{err}");
        // …and the file is untouched.
        assert_eq!(
            std::fs::read_to_string(dir.join(".cursor/mcp.json")).unwrap(),
            "{not json"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn install_refuses_non_object_top_key() {
        let dir = test_dir("install-badkey");
        std::fs::create_dir_all(dir.join(".vscode")).unwrap();
        std::fs::write(dir.join(".vscode/mcp.json"), r#"{"servers": "nope"}"#).unwrap();
        let err = install_at(
            &test_dirs(&dir),
            &dir,
            &["vscode".to_string()],
            false,
            true,
            false,
            "https://x/api/mcp",
            None,
            "test",
        )
        .err()
        .unwrap();
        assert!(
            format!("{err}").contains("`servers` isn't an object"),
            "{err}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn codex_install_and_reinstall() {
        let dir = test_dir("codex");
        let dirs = test_dirs(&dir);
        install_at(
            &dirs,
            &dir,
            &["codex".to_string()],
            false,
            true,
            false,
            "https://forge.example.com/api/mcp",
            Some("pat"),
            "test",
        )
        .unwrap();
        let raw = std::fs::read_to_string(dir.join(".codex/config.toml")).unwrap();
        assert!(raw.contains("[mcp_servers.oxidean]"));
        assert!(raw.contains("url = \"https://forge.example.com/api/mcp\""));
        assert!(raw.contains("bearer_token_env_var = \"OXIDEAN_TOKEN\""));
        // Reinstall over a foreign section — replaced, foreign stays.
        std::fs::write(
            dir.join(".codex/config.toml"),
            "[mcp_servers.other]\nurl = \"https://o/x\"\n\n[mcp_servers.oxidean]\nurl = \"https://stale/x\"\n",
        )
        .unwrap();
        install_at(
            &dirs,
            &dir,
            &["codex".to_string()],
            false,
            true,
            false,
            "https://forge.example.com/api/mcp",
            Some("pat"),
            "test",
        )
        .unwrap();
        let raw = std::fs::read_to_string(dir.join(".codex/config.toml")).unwrap();
        assert_eq!(raw.matches("[mcp_servers.oxidean]").count(), 1);
        assert!(raw.contains("[mcp_servers.other]"));
        assert!(raw.contains("https://forge.example.com/api/mcp"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn remove_strips_only_our_entry() {
        let dir = test_dir("remove");
        std::fs::create_dir_all(dir.join(".cursor")).unwrap();
        std::fs::write(
            dir.join(".cursor/mcp.json"),
            r#"{"mcpServers": {"oxidean": {"url": "x"}, "other": {"url": "y"}}}"#,
        )
        .unwrap();
        let out = remove_at(
            &test_dirs(&dir),
            &dir,
            &["cursor".to_string()],
            false,
            true,
            false,
        )
        .unwrap();
        match &out {
            Out::Report { text, .. } => assert!(text.contains("removed ")),
            _ => panic!(),
        }
        let doc: Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join(".cursor/mcp.json")).unwrap())
                .unwrap();
        assert!(doc["mcpServers"].get("oxidean").is_none());
        assert_eq!(doc["mcpServers"]["other"]["url"], "y");
        // Second removal reports no entry.
        let out = remove_at(
            &test_dirs(&dir),
            &dir,
            &["cursor".to_string()],
            false,
            true,
            false,
        )
        .unwrap();
        match &out {
            Out::Report { text, .. } => assert!(text.contains("(no entry)")),
            _ => panic!(),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn status_reports_configured_and_noentry() {
        let dir = test_dir("status");
        std::fs::create_dir_all(dir.join(".cursor")).unwrap();
        std::fs::write(
            dir.join(".cursor/mcp.json"),
            r#"{"mcpServers": {"oxidean": {"url": "https://f/api/mcp", "headers": {"Authorization": "Bearer x"}}}}"#,
        )
        .unwrap();
        // zed file exists but has no entry.
        std::fs::create_dir_all(dir.join(".zed")).unwrap();
        std::fs::write(dir.join(".zed/settings.json"), "{}").unwrap();
        let rows = status_at(&test_dirs(&dir), &dir, None);
        let cursor = rows.iter().find(|r| r.harness == "cursor").unwrap();
        assert!(
            matches!(&cursor.state, Ok(Some((u, a))) if u == "https://f/api/mcp" && a == "bearer")
        );
        let zed = rows.iter().find(|r| r.harness == "zed").unwrap();
        assert!(matches!(&zed.state, Ok(None)));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn status_reads_codex_env_auth() {
        let dir = test_dir("status-codex");
        std::fs::create_dir_all(dir.join(".codex")).unwrap();
        std::fs::write(
            dir.join(".codex/config.toml"),
            "[mcp_servers.oxidean]\nurl = \"https://f/api/mcp\"\nbearer_token_env_var = \"OXIDEAN_TOKEN\"\n\n[other]\nx = 1\n",
        )
        .unwrap();
        let rows = status_at(&test_dirs(&dir), &dir, None);
        let codex = rows.iter().find(|r| r.harness == "codex").unwrap();
        assert!(
            matches!(&codex.state, Ok(Some((u, a))) if u == "https://f/api/mcp" && a == "env OXIDEAN_TOKEN")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn global_scope_writes_all_files() {
        let dir = test_dir("global-scope");
        let out = install_at(
            &test_dirs(&dir),
            &dir,
            &["claude".to_string()],
            false,
            false,
            true,
            "https://f/api/mcp",
            None,
            "test",
        )
        .unwrap();
        match &out {
            Out::Report { text, .. } => assert!(text.contains(".claude.json")),
            _ => panic!(),
        }
        assert!(dir.join(".claude.json").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn windsurf_skips_project_scope() {
        let dir = test_dir("windsurf-project");
        let out = install_at(
            &test_dirs(&dir),
            &dir,
            &["windsurf".to_string()],
            false,
            true,
            false,
            "https://f/api/mcp",
            None,
            "test",
        )
        .unwrap();
        match &out {
            Out::Report { text, .. } => {
                assert!(text.contains("skipped Windsurf (no project config file)"))
            }
            _ => panic!(),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn print_block_generic_and_codex() {
        let out = print_block(None, "https://f/api/mcp").unwrap();
        match &out {
            Out::Report { text, .. } => {
                assert!(text.contains("\"mcpServers\""));
                assert!(text.contains("oxidean"));
                assert!(text.contains("<token>"));
                assert!(!text.contains("pat_"));
            }
            _ => panic!(),
        }
        let out = print_block(Some("codex"), "https://f/api/mcp").unwrap();
        match &out {
            Out::Report { text, .. } => {
                assert!(text.contains("[mcp_servers.oxidean]"));
                assert!(text.contains("bearer_token_env_var"));
            }
            _ => panic!(),
        }
        assert!(print_block(Some("nope"), "https://f/api/mcp").is_err());
    }

    #[test]
    fn strip_codex_preserves_surrounding() {
        let raw = "# head\n[mcp_servers.a]\nurl = \"u\"\n\n[mcp_servers.oxidean]\nurl = \"x\"\nbearer_token_env_var = \"T\"\n\n[mcp_servers.oxidean.sub]\nk = 1\n\n[settings]\ny = 2\n";
        let out = strip_codex_section(raw);
        assert!(!out.contains("oxidean"));
        assert!(out.contains("[mcp_servers.a]"));
        assert!(out.contains("[settings]"));
        assert!(out.contains("# head"));
    }

    /// Seed a cursor config that carries an `oxidean` entry.
    fn seed_cursor(dir: &Path, url: &str, token: Option<&str>) -> PathBuf {
        let cursor = dir.join(".cursor");
        std::fs::create_dir_all(&cursor).unwrap();
        let mut e = json!({"url": url});
        if let Some(t) = token {
            e["headers"] = json!({"Authorization": format!("Bearer {t}")});
        }
        let path = cursor.join("mcp.json");
        std::fs::write(
            &path,
            serde_json::to_string_pretty(&json!({"mcpServers": {"oxidean": e}})).unwrap(),
        )
        .unwrap();
        path
    }

    #[test]
    fn upgrade_rewrites_only_existing() {
        let dir = test_dir("upgrade-only-existing");
        // cursor has an entry; vscode's file exists without one; claude
        // has no file at all.
        seed_cursor(&dir, "https://old.example/api/mcp", Some("pat_keep"));
        std::fs::create_dir_all(dir.join(".vscode")).unwrap();
        std::fs::write(
            dir.join(".vscode/mcp.json"),
            r#"{"servers": {"other": {}}}"#,
        )
        .unwrap();

        let out = upgrade_at(
            &test_dirs(&dir),
            &dir,
            &[],
            false,
            false,
            false,
            Some("https://new.example/api/mcp"),
            &AuthOverride::Preserve,
        )
        .unwrap();
        match &out {
            Out::Report { text, json } => {
                assert!(text.contains("upgraded"), "{text}");
                assert!(text.contains(".cursor/mcp.json"), "{text}");
                assert_eq!(json["data"]["upgraded"].as_array().unwrap().len(), 1);
            }
            _ => panic!(),
        }
        // Entry rewritten to the new endpoint, bearer preserved verbatim.
        let doc: Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join(".cursor/mcp.json")).unwrap())
                .unwrap();
        let e = &doc["mcpServers"]["oxidean"];
        assert_eq!(e["url"], "https://new.example/api/mcp");
        assert_eq!(e["headers"]["Authorization"], "Bearer pat_keep");
        // vscode's entry-less file and claude's absent file are untouched.
        assert_eq!(
            std::fs::read_to_string(dir.join(".vscode/mcp.json")).unwrap(),
            r#"{"servers": {"other": {}}}"#
        );
        assert!(!dir.join(".mcp.json").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn upgrade_reports_up_to_date_and_none() {
        let dir = test_dir("upgrade-current");
        seed_cursor(&dir, "https://f/api/mcp", Some("pat"));
        // Canonical entry already → up-to-date, file bytes unchanged.
        let out = upgrade_at(
            &test_dirs(&dir),
            &dir,
            &[],
            false,
            false,
            false,
            Some("https://f/api/mcp"),
            &AuthOverride::Preserve,
        )
        .unwrap();
        match &out {
            Out::Report { text, json } => {
                assert!(text.contains("up-to-date"), "{text}");
                assert_eq!(json["data"]["upgraded"].as_array().unwrap().len(), 0);
                assert_eq!(json["data"]["current"].as_array().unwrap().len(), 1);
            }
            _ => panic!(),
        }

        let empty = test_dir("upgrade-none");
        let out = upgrade_at(
            &test_dirs(&empty),
            &empty,
            &[],
            false,
            false,
            false,
            Some("https://f/api/mcp"),
            &AuthOverride::Preserve,
        )
        .unwrap();
        match &out {
            Out::Report { text, .. } => {
                assert!(text.contains("no MCP server entries found"), "{text}")
            }
            _ => panic!(),
        }
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&empty);
    }

    #[test]
    fn upgrade_auth_override_rotates_and_strips() {
        let dir = test_dir("upgrade-auth");
        seed_cursor(&dir, "https://f/api/mcp", Some("pat_old"));
        // Explicit token rotates the bearer everywhere it exists.
        upgrade_at(
            &test_dirs(&dir),
            &dir,
            &["cursor".to_string()],
            false,
            false,
            false,
            Some("https://f/api/mcp"),
            &AuthOverride::Set(Some("pat_new".into())),
        )
        .unwrap();
        let doc: Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join(".cursor/mcp.json")).unwrap())
                .unwrap();
        assert_eq!(
            doc["mcpServers"]["oxidean"]["headers"]["Authorization"],
            "Bearer pat_new"
        );
        // --anonymous strips auth.
        upgrade_at(
            &test_dirs(&dir),
            &dir,
            &["cursor".to_string()],
            false,
            false,
            false,
            Some("https://f/api/mcp"),
            &AuthOverride::Set(None),
        )
        .unwrap();
        let doc: Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join(".cursor/mcp.json")).unwrap())
                .unwrap();
        assert!(doc["mcpServers"]["oxidean"].get("headers").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn upgrade_skips_malformed_and_non_object() {
        let dir = test_dir("upgrade-skip");
        std::fs::create_dir_all(dir.join(".cursor")).unwrap();
        std::fs::write(dir.join(".cursor/mcp.json"), "{not json").unwrap();
        std::fs::create_dir_all(dir.join(".kiro/settings")).unwrap();
        std::fs::write(
            dir.join(".kiro/settings/mcp.json"),
            r#"{"mcpServers": {"oxidean": "https://f/api/mcp"}}"#,
        )
        .unwrap();
        let out = upgrade_at(
            &test_dirs(&dir),
            &dir,
            &[],
            false,
            false,
            false,
            Some("https://f/api/mcp"),
            &AuthOverride::Preserve,
        )
        .unwrap();
        match &out {
            Out::Report { text, json } => {
                assert!(text.contains("skipped"), "{text}");
                assert_eq!(
                    json["data"]["skipped"].as_array().unwrap().len(),
                    2,
                    "{json}"
                );
            }
            _ => panic!(),
        }
        // Neither file was touched.
        assert_eq!(
            std::fs::read_to_string(dir.join(".cursor/mcp.json")).unwrap(),
            "{not json"
        );
        assert!(std::fs::read_to_string(dir.join(".kiro/settings/mcp.json"))
            .unwrap()
            .contains("\"oxidean\": \"https://f/api/mcp\""));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn upgrade_codex_preserves_env_var() {
        let dir = test_dir("upgrade-codex");
        std::fs::create_dir_all(dir.join(".codex")).unwrap();
        std::fs::write(
            dir.join(".codex/config.toml"),
            "[mcp_servers.oxidean]\nurl = \"https://old/api/mcp\"\nbearer_token_env_var = \"MY_TOKEN\"\n\n[settings]\nx = 1\n",
        )
        .unwrap();
        upgrade_at(
            &test_dirs(&dir),
            &dir,
            &["codex".to_string()],
            false,
            false,
            false,
            Some("https://new/api/mcp"),
            &AuthOverride::Preserve,
        )
        .unwrap();
        let raw = std::fs::read_to_string(dir.join(".codex/config.toml")).unwrap();
        // Custom env var name survives; endpoint updates; sibling table stays.
        assert!(raw.contains("url = \"https://new/api/mcp\""), "{raw}");
        assert!(raw.contains("bearer_token_env_var = \"MY_TOKEN\""), "{raw}");
        assert!(raw.contains("[settings]"), "{raw}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn upgrade_zed_preserves_header() {
        let dir = test_dir("upgrade-zed");
        let zed = dir.join("xdg/zed");
        std::fs::create_dir_all(&zed).unwrap();
        std::fs::write(
            zed.join("settings.json"),
            serde_json::to_string_pretty(&json!({
                "context_servers": {"oxidean": {"command": {"path": "npx", "args": [
                    "mcp-remote", "https://old/api/mcp", "--header",
                    "Authorization: Bearer pat_zed"]}}}
            }))
            .unwrap(),
        )
        .unwrap();
        upgrade_at(
            &test_dirs(&dir),
            &dir,
            &["zed".to_string()],
            false,
            false,
            true,
            Some("https://new/api/mcp"),
            &AuthOverride::Preserve,
        )
        .unwrap();
        let doc: Value =
            serde_json::from_str(&std::fs::read_to_string(zed.join("settings.json")).unwrap())
                .unwrap();
        let args = doc["context_servers"]["oxidean"]["command"]["args"]
            .as_array()
            .unwrap();
        assert_eq!(args[1], "https://new/api/mcp");
        assert!(args
            .iter()
            .any(|a| a.as_str() == Some("Authorization: Bearer pat_zed")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn refresh_installed_preserves_url_and_auth() {
        let dir = test_dir("refresh");
        // An entry aimed at a different instance keeps its URL — self-update
        // refresh never repoints configs.
        seed_cursor(&dir, "https://other.example/api/mcp", Some("pat_x"));
        let n = upgrade_at(
            &test_dirs(&dir),
            &dir,
            &[],
            false,
            false,
            false,
            None,
            &AuthOverride::Preserve,
        )
        .unwrap();
        match &n {
            Out::Report { json, .. } => {
                // Canonical already → nothing to rewrite, nothing repointed.
                assert_eq!(json["data"]["upgraded"].as_array().unwrap().len(), 0);
            }
            _ => panic!(),
        }
        let doc: Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join(".cursor/mcp.json")).unwrap())
                .unwrap();
        let e = &doc["mcpServers"]["oxidean"];
        assert_eq!(e["url"], "https://other.example/api/mcp");
        assert_eq!(e["headers"]["Authorization"], "Bearer pat_x");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
