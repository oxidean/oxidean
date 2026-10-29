//! Action execution — `exec_shell` / `fs_read` / `fs_write`.
//!
//! `exec_shell` spawns with cwd = canonical root and is ambient — NOT
//! path-jailed. The fs ops run the jail chain (BR6.2/BR6.3,
//! security-design Q6):
//!   lexical normalize → reject `..` escape → canonicalize (deepest
//!   existing ancestor for write-create) → reject escape past canonical
//!   root → reject inside canonical `config_dir()` → `symlink_metadata`
//!   leaf check → regular files only.
//!
//! Output pipeline: exec tail buffer keeps the LAST 64 KiB of the
//! interleaved stdout/stderr stream; fs payloads cap at 128 KiB raw
//! (pre-base64); every report output scrubs the enumerated secret set and
//! must stay under the server's 256 KiB bound.

use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use base64::Engine as _;
use serde::Deserialize;

use super::custody::SecretString;

/// Server-side report bound (routes.rs `OUTPUT_MAX`); our outputs are far
/// smaller — this is the assertion ceiling, never a target.
pub const OUTPUT_MAX: usize = 256 * 1024;
/// Raw byte cap for fs_read/fs_write payloads — BEFORE base64 encoding.
pub const FS_IO_CAP: u64 = 128 * 1024;
/// Combined stdout+stderr tail kept for exec_shell reports.
pub const EXEC_TAIL_CAP: usize = 64 * 1024;
/// Default exec timeout (`--exec-timeout` flag default, BR7.3).
pub const EXEC_TIMEOUT_DEFAULT: u64 = 300;

// ---------------------------------------------------------------------------
// Tail buffer — keeps the LAST EXEC_TAIL_CAP bytes, notes dropped count.
// ---------------------------------------------------------------------------

#[derive(Default)]
pub struct TailBuffer {
    buf: Vec<u8>,
    dropped: u64,
}

impl TailBuffer {
    pub fn push(&mut self, chunk: &[u8]) {
        self.buf.extend_from_slice(chunk);
        if self.buf.len() > EXEC_TAIL_CAP {
            let excess = self.buf.len() - EXEC_TAIL_CAP;
            self.buf.drain(..excess);
            self.dropped += excess as u64;
        }
    }

    pub fn is_empty(&self) -> bool {
        self.buf.is_empty() && self.dropped == 0
    }

    /// Drain to a String: char-boundary-safe (leading partial UTF-8 run
    /// counts as dropped), `…<n> bytes truncated` marker prepended when
    /// the stream overflowed the cap.
    pub fn render(&self) -> String {
        let mut start = 0;
        while start < self.buf.len() && (self.buf[start] & 0xC0) == 0x80 {
            start += 1;
        }
        let text = String::from_utf8_lossy(&self.buf[start..]);
        let dropped = self.dropped + start as u64;
        if dropped > 0 {
            format!("…{dropped} bytes truncated\n{text}")
        } else {
            text.into_owned()
        }
    }
}

// ---------------------------------------------------------------------------
// Environment — child env strip vs output-scrub set are different lists.
// ---------------------------------------------------------------------------

/// Names removed from a child's environment (security-design Q5):
/// `OXIDEAN_*TOKEN*`/`OXIDEAN_*KEY*`/`OXIDEAN_*SECRET*`/`OXIDEAN_*PASS*`
/// and bare `*PAIRING*` — case-insensitive.
pub fn is_stripped_env(name: &str) -> bool {
    let n = name.to_uppercase();
    if n.contains("PAIRING") {
        return true;
    }
    n.starts_with("OXIDEAN_")
        && (n.contains("TOKEN") || n.contains("KEY") || n.contains("SECRET") || n.contains("PASS"))
}

/// The env names to drop when spawning the shell.
fn stripped_env_names() -> Vec<std::ffi::OsString> {
    std::env::vars_os()
        .filter_map(|(k, _)| k.to_str().filter(|s| is_stripped_env(s)).map(|_| k.clone()))
        .collect()
}

/// Secret VALUES replaced with `\[redacted\]` in every reported output:
/// the verifier, any env value under a token/key/secret/pass-ish name,
/// stored `config.json` instance tokens, and every value already in
/// `pilot-secrets.json` (scrub set ≠ env-strip set — a generic
/// `MY_API_TOKEN` still needs its value scrubbed from `env` output).
pub fn scrub_secrets(
    cfg_dir: &Path,
    verifier: &SecretString,
    cfg: &crate::config::Config,
) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut push = |v: String| {
        if v.len() >= 8 && !out.contains(&v) {
            out.push(v);
        }
    };
    push(verifier.expose().to_string());
    for (k, v) in std::env::vars_os() {
        let ku = k.to_string_lossy().to_uppercase();
        if ku.contains("TOKEN")
            || ku.contains("KEY")
            || ku.contains("SECRET")
            || ku.contains("PASS")
        {
            if let Ok(v) = v.into_string() {
                push(v);
            }
        }
    }
    for inst in cfg.instances.values() {
        if let Some(t) = &inst.token {
            push(t.clone());
        }
    }
    if let Ok(map) = super::custody::load_map_for_scrub(cfg_dir) {
        for v in map.into_values() {
            push(v);
        }
    }
    out
}

/// Replace every ≥8-char secret value with `\[redacted\]` (NFR5.4).
pub fn scrub(text: &str, secrets: &[String]) -> String {
    let mut out = text.to_string();
    for s in secrets {
        if s.len() >= 8 {
            out = out.replace(s.as_str(), "[redacted]");
        }
    }
    out
}

/// Byte-level [`scrub`] for binary payloads — replaces each ≥8-char
/// secret's raw bytes BEFORE base64 encoding. A text scrub of the
/// encoded string only catches secrets that survive base64's 3-byte
/// alignment; the encoded wire form must never round-trip back to the
/// secret. Replacements run right-to-left so earlier hits' offsets
/// stay valid.
pub fn scrub_bytes(bytes: &mut Vec<u8>, secrets: &[String]) {
    for s in secrets {
        if s.len() < 8 {
            continue;
        }
        let needle = s.as_bytes();
        let mut hits: Vec<usize> = bytes
            .windows(needle.len())
            .enumerate()
            .filter_map(|(i, w)| (w == needle).then_some(i))
            .collect();
        for i in hits.drain(..).rev() {
            bytes.splice(i..i + needle.len(), b"[redacted]".iter().copied());
        }
    }
}

// ---------------------------------------------------------------------------
// Jail — lexical + canonical resolution under canonical_root, config-dir
// exclusion in BOTH directions at launch, per-op inward check on resolve.
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum JailError {
    /// `..` / symlink escape past the root (or write-create under one).
    Escape,
    /// Resolved path lands inside `config_dir()` (BR6.3).
    ConfigDir,
    /// Leaf isn't a regular file (dir, fifo, socket, device, symlink-on-write).
    NotRegular,
    /// Read target doesn't exist.
    NotFound,
    Io(String),
}

impl JailError {
    /// The `failed` report reason — names match rules.md's named refusals.
    pub fn report_reason(&self, input: &str) -> String {
        match self {
            Self::Escape => format!("jail_escape:{input}"),
            Self::ConfigDir => format!("jail_config:{input}"),
            Self::NotRegular => format!("not_regular_file:{input}"),
            Self::NotFound => format!("not_found:{input}"),
            Self::Io(e) => format!("io:{e}"),
        }
    }
}

/// `is_within(path, dir)` — component-wise prefix; `/root2` is NOT within
/// `/root`, `/` contains everything.
pub fn is_within(path: &Path, dir: &Path) -> bool {
    path == dir || path.starts_with(dir)
}

/// Collapse `.` and `..` lexically. A `..` that would pop above the
/// volume/root is dropped (it can never descend below `/`), so escapes
/// always surface as `!starts_with(root)` afterwards.
fn normalize_lexical(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    // At "/" (or a bare relative start) the parent is a
                    // no-op — Path::join keeps us absolute anyway.
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Canonicalize `p` if it exists; otherwise canonicalize the deepest
/// existing ancestor and re-append the unborn tail (write-create flow).
/// The returned path is still under the same lexical tree but has all
/// EXISTING symlinks resolved.
fn canonicalize_deepest(p: &Path) -> Result<PathBuf, JailError> {
    let mut tail: Vec<std::ffi::OsString> = Vec::new();
    let mut cur: &Path = p;
    loop {
        match std::fs::canonicalize(cur) {
            Ok(c) => {
                let mut out = c;
                for seg in tail.iter().rev() {
                    out.push(seg);
                }
                return Ok(out);
            }
            Err(_) => match cur.file_name() {
                Some(name) => {
                    tail.push(name.to_os_string());
                    match cur.parent() {
                        Some(par) => cur = par,
                        None => {
                            return Err(JailError::Io(format!("cannot resolve {}", p.display())));
                        }
                    }
                }
                None => return Err(JailError::Io(format!("cannot resolve {}", p.display()))),
            },
        }
    }
}

/// Canonicalize config_dir leniently (it may not exist yet — then the
/// deepest-existing-ancestor view still gives a canonical comparison base).
pub fn canonicalize_lenient(p: &Path) -> PathBuf {
    canonicalize_deepest(p).unwrap_or_else(|_| normalize_lexical(p))
}

/// Launch-time `--root` validation (Q6-A): canonicalize once; must exist
/// and be a directory; refuse BOTH `root ⊆ config_dir` and
/// `config_dir ⊆ root` — the custody dir can never shadow the jail, and a
/// home-directory root can never smuggle it in.
pub fn launch_root(flag: Option<&str>, cfg_dir: &Path) -> Result<PathBuf, String> {
    let raw = match flag {
        Some(r) => PathBuf::from(r),
        None => std::env::current_dir()
            .map_err(|e| format!("cannot determine cwd for default --root: {e}"))?,
    };
    let canonical_root = std::fs::canonicalize(&raw).map_err(|e| {
        format!(
            "jail root {} must exist and be readable: {e}",
            raw.display()
        )
    })?;
    if !canonical_root.is_dir() {
        return Err(format!(
            "jail root {} is not a directory",
            canonical_root.display()
        ));
    }
    let canonical_cfg = canonicalize_lenient(cfg_dir);
    if is_within(&canonical_root, &canonical_cfg) || is_within(&canonical_cfg, &canonical_root) {
        return Err(format!(
            "jail root {} must not be, contain, or sit inside the config dir {}",
            canonical_root.display(),
            canonical_cfg.display()
        ));
    }
    Ok(canonical_root)
}

/// Shared resolution: `root.join(input)` — `join` on an ABSOLUTE input
/// yields the absolute path itself, so both forms funnel through the same
/// lexical normalize → containment → canonicalize-deepest → containment →
/// config-dir chain. `Ok` returns the LEXICAL path (leaf ops still want
/// `symlink_metadata` to see link names).
fn resolve_jailed(
    canonical_root: &Path,
    canonical_cfg: &Path,
    input: &str,
) -> Result<PathBuf, JailError> {
    let p = normalize_lexical(&canonical_root.join(Path::new(input)));
    if !is_within(&p, canonical_root) {
        return Err(JailError::Escape);
    }
    let c = canonicalize_deepest(&p)?;
    if !is_within(&c, canonical_root) {
        return Err(JailError::Escape);
    }
    if is_within(&c, canonical_cfg) {
        return Err(JailError::ConfigDir);
    }
    Ok(p)
}

/// Read-path resolution — full canonicalize (must exist), then the leaf
/// must be a regular file BY `symlink_metadata` (a symlink leaf whose
/// canonical target is in-jail is still refused — no link-following reads).
pub fn resolve_read(
    canonical_root: &Path,
    canonical_cfg: &Path,
    input: &str,
) -> Result<PathBuf, JailError> {
    let p = resolve_jailed(canonical_root, canonical_cfg, input)?;
    let meta = match std::fs::symlink_metadata(&p) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(JailError::NotFound),
        Err(e) => return Err(JailError::Io(e.to_string())),
    };
    if !meta.file_type().is_file() {
        return Err(JailError::NotRegular);
    }
    Ok(p)
}

/// Write-path resolution — the leaf may not exist (write-create); an
/// existing leaf must be a regular file (no symlink/dir/fifo overwrite).
pub fn resolve_write(
    canonical_root: &Path,
    canonical_cfg: &Path,
    input: &str,
) -> Result<PathBuf, JailError> {
    let p = resolve_jailed(canonical_root, canonical_cfg, input)?;
    match std::fs::symlink_metadata(&p) {
        Ok(m) => {
            if !m.file_type().is_file() {
                return Err(JailError::NotRegular);
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(JailError::Io(e.to_string())),
    }
    Ok(p)
}

// ---------------------------------------------------------------------------
// Per-kind args — `ActionClaim.args` is a JSON STRING (double-encoded).
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ExecShellArgs {
    pub command: String,
}

#[derive(Debug, Deserialize)]
pub struct FsReadArgs {
    pub path: String,
}

#[derive(Debug, Deserialize)]
pub struct FsWriteArgs {
    pub path: String,
    /// Base64 standard-alphabet payload (server encodes; cap applies to
    /// the DECODED length).
    pub content_b64: String,
}

/// Double-parse: the outer claim envelope already gave `args` as a string;
/// the inner JSON is per-kind. `Err` → `invalid_args:…` report reason.
pub fn parse_args<'a, T: Deserialize<'a>>(raw: &'a str) -> Result<T, String> {
    serde_json::from_str(raw).map_err(|e| format!("invalid_args:{e}"))
}

// ---------------------------------------------------------------------------
// Action outcomes — the (status, exit_code, output) report triple.
// ---------------------------------------------------------------------------

pub struct ActionOutcome {
    pub status: super::client::ReportStatus,
    pub exit_code: Option<i64>,
    pub output: String,
}

impl ActionOutcome {
    /// fs_* completions carry no exit_code (no process ran) — only
    /// exec_shell reports one.
    pub fn done(output: String) -> Self {
        Self {
            status: super::client::ReportStatus::Done,
            exit_code: None,
            output,
        }
    }
    pub fn failed(reason: String) -> Self {
        Self {
            status: super::client::ReportStatus::Failed,
            exit_code: None,
            output: reason,
        }
    }
}

/// `fs_read` — jailed path → ≤128 KiB → base64 → scrubbed. Runs inside
/// `spawn_blocking` (caller's responsibility).
pub fn fs_read(
    canonical_root: &Path,
    canonical_cfg: &Path,
    input: &str,
    secrets: &[String],
) -> ActionOutcome {
    match resolve_read(canonical_root, canonical_cfg, input) {
        Err(j) => ActionOutcome::failed(j.report_reason(input)),
        Ok(p) => {
            let meta = match std::fs::metadata(&p) {
                Ok(m) => m,
                Err(e) => return ActionOutcome::failed(format!("io:{e}")),
            };
            if meta.len() > FS_IO_CAP {
                return ActionOutcome::failed(format!(
                    "too_large:{} bytes — cap {FS_IO_CAP}",
                    meta.len()
                ));
            }
            // Bound the READ, not just the pre-check: a file that grows
            // between metadata and open is cut at cap+1 and refused.
            // O_NOFOLLOW closes the leaf-swap TOCTOU between the earlier
            // symlink_metadata check and this open.
            let mut oo = std::fs::OpenOptions::new();
            oo.read(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                oo.custom_flags(libc::O_NOFOLLOW);
            }
            match oo.open(&p) {
                Ok(f) => {
                    use std::io::Read;
                    let mut bytes = Vec::new();
                    match f.take(FS_IO_CAP + 1).read_to_end(&mut bytes) {
                        Err(e) => ActionOutcome::failed(format!("io:{e}")),
                        Ok(_) if bytes.len() as u64 > FS_IO_CAP => ActionOutcome::failed(format!(
                            "too_large:{} bytes — cap {FS_IO_CAP}",
                            bytes.len()
                        )),
                        Ok(_) => {
                            // Scrub the RAW bytes first — a secret
                            // inside the file must not survive the
                            // base64 round-trip. The post-encode text
                            // scrub stays as the second layer.
                            scrub_bytes(&mut bytes, secrets);
                            let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
                            ActionOutcome::done(scrub(&b64, secrets))
                        }
                    }
                }
                Err(e) => ActionOutcome::failed(format!("io:{e}")),
            }
        }
    }
}

/// `fs_write` — decode (≤128 KiB decoded) → jailed resolve → tmp+rename
/// atomic write. Missing parent dirs are NOT auto-created (BR4.4); the
/// file mode is `0666 & ~umask` (server-authored content).
pub fn fs_write(
    canonical_root: &Path,
    canonical_cfg: &Path,
    input: &str,
    content_b64: &str,
    secrets: &[String],
) -> ActionOutcome {
    use std::io::Write;
    let bytes = match base64::engine::general_purpose::STANDARD.decode(content_b64) {
        Ok(b) => b,
        Err(e) => return ActionOutcome::failed(format!("invalid_args:content_b64 {e}")),
    };
    if bytes.len() as u64 > FS_IO_CAP {
        return ActionOutcome::failed(format!("too_large:{} bytes — cap {FS_IO_CAP}", bytes.len()));
    }
    match resolve_write(canonical_root, canonical_cfg, input) {
        Err(j) => ActionOutcome::failed(j.report_reason(input)),
        Ok(p) => {
            let parent = match p.parent() {
                Some(par) => par.to_path_buf(),
                None => return ActionOutcome::failed("io:no parent".into()),
            };
            // create_new (O_CREAT|O_EXCL): a pre-planted file OR symlink at
            // the tmp name fails instead of being followed — the pid in
            // the name is observable to a jailed exec_shell ($PPID), so a
            // predictable tmp path must never open for write.
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.subsec_nanos())
                .unwrap_or(0);
            let tmp = parent.join(format!(".oxpilot-tmp-{}-{nanos}", std::process::id()));
            let result = (|| -> Result<(), std::io::Error> {
                let mut f = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&tmp)?;
                f.write_all(&bytes)?;
                f.sync_all()?;
                std::fs::rename(&tmp, &p)?;
                Ok(())
            })();
            if result.is_err() {
                let _ = std::fs::remove_file(&tmp);
            }
            match result {
                Ok(()) => ActionOutcome::done(scrub(
                    &format!("wrote {} bytes to {input}", bytes.len()),
                    secrets,
                )),
                Err(e) => ActionOutcome::failed(format!("io:{e}")),
            }
        }
    }
}

// ---------------------------------------------------------------------------
// exec_shell — spawn in own process group, pump stdout+stderr into the
// shared tail buffer, wait inside spawn_blocking; kill = group kill.
// ---------------------------------------------------------------------------

/// The live half of a spawned shell — `wait` runs on the blocking pool so
/// the async executor never stalls; `buf` accumulates the interleaved tail.
pub struct ShellChild {
    pid: u32,
    buf: Arc<Mutex<TailBuffer>>,
    wait: tokio::task::JoinHandle<(Option<i32>, String)>,
}

impl ShellChild {
    pub fn pid(&self) -> u32 {
        self.pid
    }
    /// Current (possibly still-growing) tail — used for timeout reports.
    pub fn tail_snapshot(&self) -> String {
        self.buf
            .lock()
            .map(|b| b.render())
            .unwrap_or_else(|e| e.into_inner().render())
    }

    /// The spawn_blocking wait handle — polled inside `tokio::select!`.
    pub fn wait_mut(&mut self) -> &mut tokio::task::JoinHandle<(Option<i32>, String)> {
        &mut self.wait
    }
}

#[cfg(unix)]
fn shell_command(command: &str) -> Command {
    let mut c = Command::new("/bin/sh");
    c.arg("-c").arg(command);
    c
}

#[cfg(windows)]
fn shell_command(command: &str) -> Command {
    let mut c = Command::new("cmd");
    c.args(["/C", command]);
    c
}

/// Spawn `sh -c <command>` with cwd=canonical_root in its OWN process
/// group; pump both streams into a TailBuffer; return the child + a
/// spawn_blocking wait handle yielding `(exit_code, tail)`.
pub fn spawn_shell(command: &str, canonical_root: &Path) -> std::io::Result<ShellChild> {
    let mut cmd = shell_command(command);
    cmd.current_dir(canonical_root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0); // own pgid → killpg reaches the whole tree
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0000_0200); // CREATE_NEW_PROCESS_GROUP
    }
    for name in stripped_env_names() {
        cmd.env_remove(&name);
    }
    let mut child: Child = cmd.spawn()?;
    let pid = child.id();
    let buf: Arc<Mutex<TailBuffer>> = Arc::new(Mutex::new(TailBuffer::default()));
    let mut pumps: Vec<JoinHandle<()>> = Vec::new();
    let mut streams: Vec<Box<dyn Read + Send>> = Vec::new();
    if let Some(s) = child.stdout.take() {
        streams.push(Box::new(s));
    }
    if let Some(s) = child.stderr.take() {
        streams.push(Box::new(s));
    }
    for stream in streams {
        let buf = Arc::clone(&buf);
        let mut stream = stream;
        pumps.push(std::thread::spawn(move || {
            let mut chunk = [0u8; 8192];
            loop {
                match stream.read(&mut chunk) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        let mut g = buf.lock().unwrap_or_else(|e| e.into_inner());
                        g.push(&chunk[..n]);
                    }
                }
            }
        }));
    }
    let buf2 = Arc::clone(&buf);
    let wait = tokio::task::spawn_blocking(move || {
        let status = child.wait().ok();
        for p in pumps {
            let _ = p.join();
        }
        let tail = buf2
            .lock()
            .map(|b| b.render())
            .unwrap_or_else(|e| e.into_inner().render());
        (status.and_then(|s| s.code()), tail)
    });
    Ok(ShellChild { pid, buf, wait })
}

/// Kill the child's whole process group (unix `killpg`) / process tree
/// (windows `taskkill /T`). Best-effort — a dead pid is a silent no-op.
#[cfg(unix)]
pub fn kill_process_tree(pid: u32) {
    // process_group(0) made pgid == pid.
    unsafe {
        libc::kill(-(pid as i32), libc::SIGKILL);
    }
}

#[cfg(windows)]
pub fn kill_process_tree(pid: u32) {
    let _ = Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

/// Universal output-bound guard — every reported output MUST be <
/// OUTPUT_MAX; our real bounds are 64 KiB tail / ~170 KiB base64, so this
/// assertion is the defense-in-depth ceiling.
pub fn enforce_output_bound(mut output: String) -> String {
    if output.len() >= OUTPUT_MAX {
        // Degrade to a bounded report, never a panic: truncate under the
        // ceiling with room for the marker, on a UTF-8 boundary.
        let keep = OUTPUT_MAX - 64;
        let mut end = keep.min(output.len());
        while !output.is_char_boundary(end) {
            end -= 1;
        }
        output.truncate(end);
        output.push_str("…<output bound applied>");
    }
    output
}

/// Hostname default for `--label` (BR1.1: label defaults to hostname).
pub fn hostname_label() -> String {
    std::env::var("HOSTNAME")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| {
            #[cfg(unix)]
            {
                let mut buf = [0u8; 256];
                let rc =
                    unsafe { libc::gethostname(buf.as_mut_ptr() as *mut libc::c_char, buf.len()) };
                if rc == 0 {
                    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
                    Some(String::from_utf8_lossy(&buf[..end]).into_owned())
                } else {
                    Some(String::new())
                }
            }
            #[cfg(windows)]
            {
                Some(std::env::var("COMPUTERNAME").unwrap_or_default())
            }
        })
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "connector".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Jail {
        _tmp: tempfile::TempDir,
        root: PathBuf,
        cfg: PathBuf,
    }

    fn jail() -> Jail {
        let tmp = tempfile::tempdir().unwrap();
        let jail_dir = tmp.path().join("jail");
        let cfg_dir = tmp.path().join("cfg");
        std::fs::create_dir_all(&jail_dir).unwrap();
        std::fs::create_dir_all(&cfg_dir).unwrap();
        Jail {
            _tmp: tmp,
            root: std::fs::canonicalize(&jail_dir).unwrap(),
            cfg: std::fs::canonicalize(&cfg_dir).unwrap(),
        }
    }

    #[test]
    fn lexical_escape_refused() {
        let j = jail();
        assert!(matches!(
            resolve_read(&j.root, &j.cfg, "../../etc/passwd"),
            Err(JailError::Escape)
        ));
        assert!(matches!(
            resolve_write(&j.root, &j.cfg, "../outside.txt"),
            Err(JailError::Escape)
        ));
    }

    #[test]
    fn in_jail_resolves_and_reads() {
        let j = jail();
        std::fs::write(j.root.join("f.txt"), "hello").unwrap();
        let p = resolve_read(&j.root, &j.cfg, "sub/../f.txt").unwrap();
        assert!(p.ends_with("f.txt"));
        let out = fs_read(&j.root, &j.cfg, "f.txt", &[]);
        assert!(matches!(
            out.status,
            super::super::client::ReportStatus::Done
        ));
        assert_eq!(
            base64::engine::general_purpose::STANDARD
                .decode(&out.output)
                .unwrap(),
            b"hello"
        );
    }

    #[test]
    fn fs_read_scrubs_secret_bytes_before_encoding() {
        let j = jail();
        let secret = "sk-supersecretvalue123".to_string();
        std::fs::write(
            j.root.join("leak.txt"),
            format!("prefix {secret} suffix\n\x00\x01{secret}"),
        )
        .unwrap();
        let out = fs_read(&j.root, &j.cfg, "leak.txt", std::slice::from_ref(&secret));
        assert!(matches!(
            out.status,
            super::super::client::ReportStatus::Done
        ));
        // The decoded payload must not round-trip the secret — the
        // byte-level scrub ran before base64 (both the text and the
        // binary trailing copy are gone).
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(&out.output)
            .unwrap();
        let text = String::from_utf8_lossy(&decoded);
        assert!(!text.contains(&secret));
        assert_eq!(text.matches("[redacted]").count(), 2);
    }

    #[cfg(unix)]
    #[test]
    fn symlink_escape_refused() {
        use std::os::unix::fs::symlink;
        let j = jail();
        std::fs::write("/tmp/ox-pilot-escape-target", "x").unwrap();
        symlink("/tmp/ox-pilot-escape-target", j.root.join("link")).unwrap();
        assert!(matches!(
            resolve_read(&j.root, &j.cfg, "link"),
            Err(JailError::Escape) | Err(JailError::NotRegular)
        ));
    }

    #[cfg(unix)]
    #[test]
    fn in_jail_symlink_refused_as_non_regular() {
        use std::os::unix::fs::symlink;
        let j = jail();
        std::fs::write(j.root.join("real"), "x").unwrap();
        symlink(j.root.join("real"), j.root.join("ln")).unwrap();
        // canonical stays in-jail; symlink_metadata leaf refuses the read.
        assert!(matches!(
            resolve_read(&j.root, &j.cfg, "ln"),
            Err(JailError::NotRegular)
        ));
    }

    #[test]
    fn non_regular_dir_refused() {
        let j = jail();
        std::fs::create_dir_all(j.root.join("d")).unwrap();
        assert!(matches!(
            resolve_read(&j.root, &j.cfg, "d"),
            Err(JailError::NotRegular)
        ));
    }

    #[test]
    fn config_dir_inward_refused() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg_raw = tmp.path().join("cfg");
        std::fs::create_dir_all(&cfg_raw).unwrap();
        let cfg = std::fs::canonicalize(&cfg_raw).unwrap();
        // Root that contains the config dir → a file inside cfg refuses.
        let root = std::fs::canonicalize(tmp.path()).unwrap();
        std::fs::write(cfg.join("pilot-secrets.json"), "{}").unwrap();
        assert!(matches!(
            resolve_read(&root, &cfg, "cfg/pilot-secrets.json"),
            Err(JailError::ConfigDir)
        ));
    }

    #[test]
    fn launch_root_refuses_both_directions() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = tmp.path().join("cfg");
        std::fs::create_dir_all(&cfg).unwrap();
        // root == cfg parent → cfg ⊆ root → refused.
        assert!(launch_root(Some(tmp.path().to_str().unwrap()), &cfg).is_err());
        // root inside cfg → refused.
        let inner = cfg.join("inner");
        std::fs::create_dir_all(&inner).unwrap();
        assert!(launch_root(Some(inner.to_str().unwrap()), &cfg).is_err());
        // sibling root → fine.
        let sib = tmp.path().join("sib");
        std::fs::create_dir_all(&sib).unwrap();
        assert!(launch_root(Some(sib.to_str().unwrap()), &cfg).is_ok());
    }

    #[test]
    fn env_strip_set() {
        assert!(is_stripped_env("OXIDEAN_TOKEN"));
        assert!(is_stripped_env("OXIDEAN_API_KEY"));
        assert!(is_stripped_env("OXIDEAN_SOME_SECRET"));
        assert!(is_stripped_env("OXIDEAN_PASS"));
        assert!(is_stripped_env("MY_PAIRING_CODE"));
        assert!(!is_stripped_env("PATH"));
        assert!(!is_stripped_env("MY_API_TOKEN")); // not OXIDEAN_-prefixed
    }

    #[test]
    fn tail_buffer_bounds_and_marks() {
        let mut b = TailBuffer::default();
        let chunk = vec![b'x'; EXEC_TAIL_CAP];
        b.push(&chunk);
        b.push(&vec![b'y'; 4096]);
        let s = b.render();
        assert!(s.starts_with('…'), "{s}");
        assert!(s.contains("4096 bytes truncated"), "{s}");
        assert_eq!(s.len(), "…4096 bytes truncated\n".len() + EXEC_TAIL_CAP);
    }

    #[test]
    fn tail_buffer_char_boundary() {
        let mut b = TailBuffer::default();
        // Fill the buffer, then push a multibyte char split across the cut.
        b.push(&vec![b'a'; EXEC_TAIL_CAP - 1]);
        b.push("é".as_bytes()); // 2 bytes — overflow by 1, drops first 'a'
        let s = b.render();
        assert!(s.contains("1 bytes truncated"));
    }

    #[test]
    fn scrub_replaces_only_long_values() {
        let secrets = vec!["short".into(), "a-very-long-secret-value".into()];
        let s = scrub("token a-very-long-secret-value and short stay", &secrets);
        // "short" is 5 chars — under the ≥8 floor, stays verbatim.
        assert_eq!(s, "token [redacted] and short stay");
    }

    #[test]
    fn fs_write_read_roundtrip() {
        let j = jail();
        std::fs::create_dir_all(j.root.join("sub/dir")).unwrap();
        let b64 = base64::engine::general_purpose::STANDARD.encode(b"payload-bytes");
        let out = fs_write(&j.root, &j.cfg, "sub/dir/new.bin", &b64, &[]);
        assert!(matches!(
            out.status,
            super::super::client::ReportStatus::Done
        ));
        assert_eq!(
            std::fs::read(j.root.join("sub/dir/new.bin")).unwrap(),
            b"payload-bytes"
        );
    }

    #[test]
    fn fs_write_refuses_missing_parent() {
        let j = jail();
        let b64 = base64::engine::general_purpose::STANDARD.encode(b"payload-bytes");
        let out = fs_write(&j.root, &j.cfg, "no/such/dir/new.bin", &b64, &[]);
        // BR4.4: missing parents are NOT auto-created — the tmp create fails.
        assert!(matches!(
            out.status,
            super::super::client::ReportStatus::Failed
        ));
        assert!(!j.root.join("no").exists());
    }

    #[test]
    fn fs_write_caps_decoded() {
        let j = jail();
        let big =
            base64::engine::general_purpose::STANDARD.encode(vec![0u8; FS_IO_CAP as usize + 1]);
        let out = fs_write(&j.root, &j.cfg, "big.bin", &big, &[]);
        assert!(matches!(
            out.status,
            super::super::client::ReportStatus::Failed
        ));
        assert!(out.output.contains("too_large"), "{}", out.output);
    }

    #[test]
    fn fs_read_caps_file() {
        let j = jail();
        std::fs::write(j.root.join("big"), vec![0u8; FS_IO_CAP as usize + 1]).unwrap();
        let out = fs_read(&j.root, &j.cfg, "big", &[]);
        assert!(out.output.contains("too_large"), "{}", out.output);
    }
}
