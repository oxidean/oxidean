//! Builtin `actions/upload-artifact` / `actions/download-artifact` —
//! GitHub artifact v4 semantics over the runner-proto artifact endpoints.
//! Blobs on the wire are tar.gz (proto-internal); `with:` inputs follow the
//! upstream actions (`name`, `path`, `if-no-files-found`, `pattern`,
//! `merge-multiple`, `include-hidden-files`, `retention-days`).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use tokio::process::Command;

use super::glob::{glob_match, glob_walk};
use super::StepEnv;

type With<'a> = &'a serde_json::Map<String, serde_json::Value>;

fn input<'a>(with: With<'a>, key: &str) -> Option<&'a str> {
    with.get(key).and_then(|v| v.as_str())
}

fn bool_input(with: With<'_>, key: &str) -> bool {
    with.get(key)
        .map(|v| v.as_bool().unwrap_or_else(|| v.as_str() == Some("true")))
        .unwrap_or(false)
}

/// `uses: actions/upload-artifact` — glob `path`, tar.gz, upload.
pub(crate) async fn upload(
    senv: &StepEnv<'_>,
    with: With<'_>,
) -> Result<HashMap<String, String>, String> {
    let name = input(with, "name").unwrap_or("artifact").to_string();
    let path_spec =
        input(with, "path").ok_or_else(|| "upload-artifact requires a 'path' input".to_string())?;
    let include_hidden = bool_input(with, "include-hidden-files");
    // `compression-level` 0–9: 0 stores an uncompressed tar, 1–9 tune the
    // gzip effort (GitHub default 6 — `tar czf` compresses at gzip's own
    // default, so only non-default levels take the explicit path).
    let compression_level = match input(with, "compression-level") {
        Some(s) => match s.parse::<u8>() {
            Ok(n) if n <= 9 => Some(n),
            _ => {
                senv.logger
                    .line(&format!(
                        "::warning::invalid compression-level '{s}' — expected 0-9, using the default"
                    ))
                    .await;
                None
            }
        },
        None => None,
    };

    // Multiline path list; `!` negates like hashFiles.
    let mut files: Vec<PathBuf> = Vec::new();
    for raw in path_spec.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        let (neg, pat) = match line.strip_prefix('!') {
            Some(p) => (true, p),
            None => (false, line),
        };
        // A magic-free pattern that names a directory contributes every file
        // beneath it — upstream walks the search path itself.
        let mut hits = if !pat.contains(['*', '?', '[', '{'])
            && senv.workspace.join(pat.trim_start_matches('/')).is_dir()
        {
            let base = pat.trim_end_matches('/');
            let dir_glob = if base == "." {
                "**".to_string()
            } else {
                format!("{base}/**")
            };
            glob_walk(senv.workspace, &dir_glob)
        } else {
            glob_walk(senv.workspace, pat)
        };
        if !include_hidden {
            hits.retain(|f| !has_hidden_segment(senv.workspace, f));
        }
        hits.sort();
        hits.dedup();
        if neg {
            let hits: std::collections::HashSet<PathBuf> = hits.into_iter().collect();
            files.retain(|f| !hits.contains(f));
        } else {
            files.extend(hits);
        }
    }
    files.sort();
    files.dedup();

    if files.is_empty() {
        let msg = format!(
            "no files were found with the provided path: {}",
            path_spec.lines().collect::<Vec<_>>().join(" ")
        );
        return match input(with, "if-no-files-found").unwrap_or("warn") {
            "error" => Err(msg),
            "ignore" => Ok(HashMap::new()),
            _ => {
                senv.logger.line(&format!("::warning::{msg}")).await;
                Ok(HashMap::new())
            }
        };
    }

    // Archive entries are relative to the upload root — computed from the
    // *search paths* (non-wildcard pattern prefixes), not the matched files:
    // `build/` → `build`, `out/**/*.txt` → `out`, a lone `dist/payload.txt`
    // → `dist`. Upstream findFilesToUpload semantics.
    let root = upload_archive_root(senv.workspace, path_spec, &files);
    let stage = senv.runner_temp.join("artifacts");
    std::fs::create_dir_all(&stage).map_err(|e| e.to_string())?;
    let rels: Vec<String> = files
        .iter()
        .filter_map(|f| {
            f.strip_prefix(&root)
                .ok()
                .map(|r| r.to_string_lossy().replace('\\', "/"))
        })
        .collect();
    let list_file = stage.join(format!("{name}.files"));
    std::fs::write(&list_file, rels.join("\n")).map_err(|e| e.to_string())?;
    let archive = match compression_level {
        // Level 0 stores the tar uncompressed; 1–9 drive `gzip -N`.
        // The download side auto-detects (`tar xf`), so both shapes extract.
        Some(level) => {
            let tar = stage.join(format!("{name}.tar"));
            let out = Command::new("tar")
                .args(["cf"])
                .arg(&tar)
                .arg("-C")
                .arg(&root)
                .arg("-T")
                .arg(&list_file)
                .output()
                .await
                .map_err(|e| format!("tar unavailable: {e}"))?;
            if !out.status.success() {
                return Err(format!(
                    "artifact archive failed: {}",
                    String::from_utf8_lossy(&out.stderr)
                ));
            }
            if level == 0 {
                tar
            } else {
                let out = Command::new("gzip")
                    .arg(format!("-{level}"))
                    .arg(&tar)
                    .output()
                    .await
                    .map_err(|e| format!("gzip unavailable: {e}"))?;
                if !out.status.success() {
                    return Err(format!(
                        "artifact compression failed: {}",
                        String::from_utf8_lossy(&out.stderr)
                    ));
                }
                stage.join(format!("{name}.tar.gz"))
            }
        }
        None => {
            let tgz = stage.join(format!("{name}.tgz"));
            let out = Command::new("tar")
                .args(["czf"])
                .arg(&tgz)
                .arg("-C")
                .arg(&root)
                .arg("-T")
                .arg(&list_file)
                .output()
                .await
                .map_err(|e| format!("tar unavailable: {e}"))?;
            if !out.status.success() {
                return Err(format!(
                    "artifact archive failed: {}",
                    String::from_utf8_lossy(&out.stderr)
                ));
            }
            tgz
        }
    };
    let bytes = std::fs::read(&archive).map_err(|e| e.to_string())?;
    let job_id = senv
        .task
        .job_id
        .as_deref()
        .ok_or_else(|| "upload-artifact: task has no job_id".to_string())?;
    let retention_days = input(with, "retention-days").and_then(|s| s.parse().ok());
    let meta = senv
        .logger
        .client
        .upload_artifact(job_id, &name, retention_days, bytes)
        .await?;
    senv.logger
        .line(&format!(
            "Uploaded artifact '{name}' ({} bytes, {})",
            meta.size_bytes,
            &meta.digest_sha256[..12.min(meta.digest_sha256.len())]
        ))
        .await;

    let mut outs = HashMap::new();
    outs.insert("artifact-id".into(), meta.id);
    outs.insert(
        "artifact-digest".into(),
        format!("sha256:{}", meta.digest_sha256),
    );
    if let Some(server) = senv.task.server_url.as_deref() {
        outs.insert(
            "artifact-url".into(),
            format!(
                "{}/api/actions/artifacts/{}/{}",
                server.trim_end_matches('/'),
                meta.run_id,
                name
            ),
        );
    }
    Ok(outs)
}

/// `uses: actions/download-artifact` — fetch + extract into `path`.
pub(crate) async fn download(
    senv: &StepEnv<'_>,
    with: With<'_>,
) -> Result<HashMap<String, String>, String> {
    let dest = match input(with, "path") {
        Some(p) if Path::new(p).is_absolute() => PathBuf::from(p),
        Some(p) => senv.workspace.join(p),
        None => senv.workspace.to_path_buf(),
    };
    let merge = bool_input(with, "merge-multiple");
    // `run-id`/`repository` select another run (GitHub v4 cross-run path);
    // `github-token` supplies the credential, defaulting to `github.token`
    // (the job token) exactly like the upstream action.
    let task_run = senv
        .task
        .run_id
        .as_deref()
        .ok_or_else(|| "download-artifact: task has no run_id".to_string())?;
    let repository = input(with, "repository");
    let supplied_run = input(with, "run-id");
    let cross = supplied_run.is_some() || repository.is_some();
    let owned_client;
    let (client, run_id): (&crate::client::Client, &str) = if cross {
        let token = input(with, "github-token")
            .or(senv.task.job_token.as_deref())
            .filter(|t| !t.is_empty())
            .ok_or_else(|| {
                "download-artifact: 'github-token' is required to download across \
                 repositories or runs"
                    .to_string()
            })?;
        owned_client = crate::client::Client::new(&senv.config.origin, Some(token.to_string()));
        (&owned_client, supplied_run.unwrap_or(task_run))
    } else {
        (senv.logger.client, task_run)
    };

    // `name` → one artifact into `path`; `pattern`/empty → list, filter by
    // name glob, each into `path/<name>` unless merge-multiple.
    let targets: Vec<(String, PathBuf)> = match input(with, "name") {
        Some(n) => vec![(n.to_string(), dest.clone())],
        None => {
            let pattern = input(with, "pattern");
            let metas = client.list_artifacts(run_id, repository).await?;
            metas
                .into_iter()
                .filter(|m| pattern.map(|p| glob_match(p, &m.name)).unwrap_or(true))
                .map(|m| {
                    let dir = if merge {
                        dest.clone()
                    } else {
                        dest.join(&m.name)
                    };
                    (m.name, dir)
                })
                .collect()
        }
    };
    if targets.is_empty() {
        senv.logger
            .line("::warning::no artifacts matched the download filter")
            .await;
        return Ok(HashMap::new());
    }
    let stage = senv.runner_temp.join("artifacts");
    std::fs::create_dir_all(&stage).map_err(|e| e.to_string())?;
    for (name, dir) in &targets {
        let bytes = client.download_artifact(run_id, name, repository).await?;
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let tgz = stage.join(format!("dl-{name}.tgz"));
        std::fs::write(&tgz, &bytes).map_err(|e| e.to_string())?;
        // `xf` auto-detects compression — `compression-level: 0` uploads are
        // plain tar, anything else is gzipped.
        let out = Command::new("tar")
            .args(["xf"])
            .arg(&tgz)
            .arg("-C")
            .arg(dir)
            .output()
            .await
            .map_err(|e| format!("tar unavailable: {e}"))?;
        let _ = std::fs::remove_file(&tgz);
        if !out.status.success() {
            return Err(format!(
                "artifact '{name}' extract failed: {}",
                String::from_utf8_lossy(&out.stderr)
            ));
        }
        senv.logger
            .line(&format!("Downloaded artifact '{name}' → {}", dir.display()))
            .await;
    }
    let mut outs = HashMap::new();
    outs.insert("download-path".into(), dest.to_string_lossy().to_string());
    Ok(outs)
}

/// The non-wildcard prefix of one `path:` pattern — upstream
/// `globber.getSearchPaths()` for a single pattern. A magic-free pattern
/// contributes its own path (literal file or directory); `.`/`..` collapse
/// lexically and `..` cannot climb above the workspace.
fn pattern_search_path(workspace: &Path, pattern: &str) -> PathBuf {
    let rel = pattern.trim().trim_start_matches('/');
    let mut root = workspace.to_path_buf();
    for seg in rel.split('/') {
        if seg.is_empty() || seg == "." {
            continue;
        }
        if seg.contains(['*', '?', '[', '{']) {
            break;
        }
        if seg == ".." {
            if root != workspace {
                root.pop();
            }
            continue;
        }
        root.push(seg);
    }
    root
}

/// Upload archive root — upstream `findFilesToUpload` semantics:
/// - search paths come only from non-negated `path:` lines, de-duplicated,
///   with any path already under another search path dropped;
/// - >1 search path → `getMultiPathLCA(searchPaths)`;
/// - exactly 1 → that path, except when the single matched file *is* the
///   search path (a literal `path: f.txt`), where the root is its parent.
fn upload_archive_root(workspace: &Path, path_spec: &str, files: &[PathBuf]) -> PathBuf {
    let mut search_paths: Vec<PathBuf> = Vec::new();
    for raw in path_spec.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('!') {
            continue;
        }
        let sp = pattern_search_path(workspace, line);
        if !search_paths.contains(&sp) {
            search_paths.push(sp);
        }
    }
    search_paths = search_paths
        .iter()
        .filter(|sp| !search_paths.iter().any(|o| o != *sp && sp.starts_with(o)))
        .cloned()
        .collect();
    match search_paths.len() {
        0 => workspace.to_path_buf(),
        1 => {
            let sp = &search_paths[0];
            if files.len() == 1 && files[0] == *sp {
                sp.parent()
                    .map(|p| p.to_path_buf())
                    .unwrap_or_else(|| workspace.to_path_buf())
            } else {
                sp.clone()
            }
        }
        _ => multi_path_lca(&search_paths, workspace),
    }
}

/// Least common ancestor of a set of paths — `getMultiPathLCA`.
fn multi_path_lca(paths: &[PathBuf], fallback: &Path) -> PathBuf {
    let mut it = paths.iter();
    let mut root = match it.next() {
        Some(p) => p.clone(),
        None => return fallback.to_path_buf(),
    };
    for p in it {
        while !p.starts_with(&root) {
            if !root.pop() {
                return fallback.to_path_buf();
            }
        }
    }
    if root.as_os_str().is_empty() {
        fallback.to_path_buf()
    } else {
        root
    }
}

/// Any relative path segment starting with `.` (hidden-file filter).
fn has_hidden_segment(root: &Path, f: &Path) -> bool {
    f.strip_prefix(root)
        .map(|r| {
            r.components()
                .any(|c| c.as_os_str().to_string_lossy().starts_with('.'))
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::upload_archive_root;
    use std::path::{Path, PathBuf};

    /// `download-artifact` request shape over a stub API: same-run reads keep
    /// the runner client; `run-id`/`repository`/`github-token` build a fresh
    /// client carrying the supplied credential, falling back to the job token.
    #[tokio::test]
    async fn download_cross_run_sends_supplied_credentials_and_ids() {
        use crate::client::{Client, Task};
        use crate::config::{Config, ExecMode};
        use crate::exec::{Logger, PendingPost, StepEnv};
        use std::collections::HashSet;

        // Tarball fixture — the blob the stub serves back for artifact GETs.
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("msg.txt"), "from-upstream").unwrap();
        let tgz = dir.path().join("blob.tgz");
        let st = std::process::Command::new("tar")
            .arg("czf")
            .arg(&tgz)
            .arg("-C")
            .arg(&src)
            .arg("msg.txt")
            .status()
            .unwrap();
        assert!(st.success());
        let blob = std::fs::read(&tgz).unwrap();

        // Stub API: record (request line, Authorization) per hit; artifact
        // GETs get the tarball, everything else a benign ack.
        let hits = std::sync::Arc::new(std::sync::Mutex::new(Vec::<(String, String)>::new()));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        {
            let hits = hits.clone();
            tokio::spawn(async move {
                use tokio::io::{AsyncReadExt, AsyncWriteExt};
                while let Ok((mut s, _)) = listener.accept().await {
                    let hits = hits.clone();
                    let blob = blob.clone();
                    tokio::spawn(async move {
                        let mut buf = vec![0u8; 8192];
                        let n = s.read(&mut buf).await.unwrap_or(0);
                        let head = String::from_utf8_lossy(&buf[..n]).to_string();
                        let line = head.lines().next().unwrap_or("").to_string();
                        let auth = head
                            .lines()
                            .find(|l| l.to_ascii_lowercase().starts_with("authorization:"))
                            .map(|l| l.trim().to_string())
                            .unwrap_or_default();
                        hits.lock().unwrap().push((line.clone(), auth));
                        let body = if line.starts_with("GET /api/actions/artifacts/") {
                            blob
                        } else {
                            br#"{"ok":true,"log_index":0}"#.to_vec()
                        };
                        let resp =
                            format!("HTTP/1.1 200 OK\r\ncontent-length: {}\r\n\r\n", body.len());
                        let _ = s.write_all(resp.as_bytes()).await;
                        let _ = s.write_all(&body).await;
                    });
                }
            });
        }

        let runner_temp = dir.path().join("rt");
        let workspace = dir.path().join("ws");
        std::fs::create_dir_all(&workspace).unwrap();
        let config = Config {
            origin: format!("http://127.0.0.1:{port}"),
            registration_token: None,
            name: "t".into(),
            labels: vec![],
            state_path: PathBuf::from("/tmp/x"),
            work_dir: workspace.clone(),
            poll_ms: 1,
            wait_ms: 0,
            git_token: None,
            job_timeout_secs: 60,
            actions_mirror: None,
        };
        let client = Client::new(&config.origin, Some("runner-tok".into()));
        let mask_values: Vec<String> = vec![];
        let logger = Logger {
            client: &client,
            run_id: "run-local".into(),
            job_id: "job-local".into(),
            mask_values: &mask_values,
            extra_masks: std::sync::Mutex::new(Vec::new()),
            stop_token: std::sync::Mutex::new(None),
            partial_line: std::sync::Mutex::new(String::new()),
            pending_warnings: std::sync::Mutex::new(Vec::new()),
            cancelled: std::sync::atomic::AtomicBool::new(false),
            log_cursor: std::sync::atomic::AtomicI64::new(0),
            seen_cmd_warns: std::sync::Mutex::new(std::collections::BTreeSet::new()),
            runner_temp: std::sync::OnceLock::new(),
            step_debug: std::sync::atomic::AtomicBool::new(false),
            matchers: std::sync::Mutex::new(Vec::new()),
            workspace: std::sync::OnceLock::new(),
            annotation_counts: std::sync::Mutex::new(std::collections::BTreeMap::new()),
        };
        let mode = ExecMode::Host;
        let mut env: Vec<(String, String)> = vec![];
        let reserved: HashSet<String> = HashSet::new();
        let mut posts: Vec<PendingPost> = vec![];
        let task: Task = serde_json::from_value(serde_json::json!({
            "run_id": "run-local",
            "job_id": "job-local",
            "job_token": "job-tok-x"
        }))
        .unwrap();
        let senv = StepEnv {
            task: &task,
            config: &config,
            mode: &mode,
            workspace: &workspace,
            runner_temp: &runner_temp,
            env: &mut env,
            reserved: &reserved,
            logger: &logger,
            posts: &mut posts,
        };

        let with = |pairs: &[(&str, &str)]| {
            pairs
                .iter()
                .map(|(k, v)| (k.to_string(), serde_json::Value::String(v.to_string())))
                .collect::<serde_json::Map<String, serde_json::Value>>()
        };

        // Same-run: no cross inputs → the runner client (Bearer runner-tok)
        // hits the task's own run id.
        super::download(&senv, &with(&[("name", "dist"), ("path", "dl1")]))
            .await
            .unwrap();

        // Cross-run: github-token + run-id + repository → a fresh client
        // carrying the supplied token hits the supplied run id.
        super::download(
            &senv,
            &with(&[
                ("name", "dist"),
                ("path", "dl2"),
                ("run-id", "run-upstream"),
                ("repository", "o/r"),
                ("github-token", "tok-supplied"),
            ]),
        )
        .await
        .unwrap();

        // github-token absent → github.token (the job token) carries the
        // cross-run call, exactly like the upstream action.
        super::download(
            &senv,
            &with(&[
                ("name", "dist"),
                ("path", "dl3"),
                ("run-id", "run-upstream"),
            ]),
        )
        .await
        .unwrap();

        let seen = hits.lock().unwrap().clone();
        let artifact_gets: Vec<&(String, String)> =
            seen.iter().filter(|(l, _)| l.starts_with("GET ")).collect();
        assert_eq!(artifact_gets.len(), 3, "three artifact downloads: {seen:?}");
        assert!(
            artifact_gets[0]
                .0
                .starts_with("GET /api/actions/artifacts/run-local/dist"),
            "same-run hits the task run id — {}",
            artifact_gets[0].0
        );
        assert_eq!(artifact_gets[0].1, "authorization: Bearer runner-tok");
        assert!(
            artifact_gets[1]
                .0
                .starts_with("GET /api/actions/artifacts/run-upstream/dist?repository=o%2Fr"),
            "cross-run carries supplied run-id + repository — {}",
            artifact_gets[1].0
        );
        assert_eq!(artifact_gets[1].1, "authorization: Bearer tok-supplied");
        assert!(
            artifact_gets[2]
                .0
                .starts_with("GET /api/actions/artifacts/run-upstream/dist"),
            "cross-run without repository omits the guard — {}",
            artifact_gets[2].0
        );
        assert_eq!(artifact_gets[2].1, "authorization: Bearer job-tok-x");

        assert_eq!(
            std::fs::read_to_string(workspace.join("dl2/msg.txt")).unwrap(),
            "from-upstream"
        );
    }

    /// Cross-run inputs without any usable credential must fail fast with a
    /// clear message — not silently downgrade to the runner token.
    #[tokio::test]
    async fn download_cross_run_requires_a_token() {
        use crate::client::{Client, Task};
        use crate::config::{Config, ExecMode};
        use crate::exec::{Logger, PendingPost, StepEnv};
        use std::collections::HashSet;

        let dir = tempfile::tempdir().unwrap();
        let runner_temp = dir.path().join("rt");
        let workspace = dir.path().join("ws");
        std::fs::create_dir_all(&workspace).unwrap();
        let config = Config {
            origin: "http://127.0.0.1:1".into(),
            registration_token: None,
            name: "t".into(),
            labels: vec![],
            state_path: PathBuf::from("/tmp/x"),
            work_dir: workspace.clone(),
            poll_ms: 1,
            wait_ms: 0,
            git_token: None,
            job_timeout_secs: 60,
            actions_mirror: None,
        };
        let client = Client::new(&config.origin, None);
        let mask_values: Vec<String> = vec![];
        let logger = Logger {
            client: &client,
            run_id: "run-local".into(),
            job_id: "job-local".into(),
            mask_values: &mask_values,
            extra_masks: std::sync::Mutex::new(Vec::new()),
            stop_token: std::sync::Mutex::new(None),
            partial_line: std::sync::Mutex::new(String::new()),
            pending_warnings: std::sync::Mutex::new(Vec::new()),
            cancelled: std::sync::atomic::AtomicBool::new(false),
            log_cursor: std::sync::atomic::AtomicI64::new(0),
            seen_cmd_warns: std::sync::Mutex::new(std::collections::BTreeSet::new()),
            runner_temp: std::sync::OnceLock::new(),
            step_debug: std::sync::atomic::AtomicBool::new(false),
            matchers: std::sync::Mutex::new(Vec::new()),
            workspace: std::sync::OnceLock::new(),
            annotation_counts: std::sync::Mutex::new(std::collections::BTreeMap::new()),
        };
        let mode = ExecMode::Host;
        let mut env: Vec<(String, String)> = vec![];
        let reserved: HashSet<String> = HashSet::new();
        let mut posts: Vec<PendingPost> = vec![];
        // No job token minted for this task.
        let task: Task = serde_json::from_value(serde_json::json!({
            "run_id": "run-local",
            "job_id": "job-local"
        }))
        .unwrap();
        let senv = StepEnv {
            task: &task,
            config: &config,
            mode: &mode,
            workspace: &workspace,
            runner_temp: &runner_temp,
            env: &mut env,
            reserved: &reserved,
            logger: &logger,
            posts: &mut posts,
        };
        let with: serde_json::Map<String, serde_json::Value> = [
            ("name".to_string(), serde_json::json!("dist")),
            ("run-id".to_string(), serde_json::json!("run-upstream")),
        ]
        .into_iter()
        .collect();
        let err = super::download(&senv, &with).await.unwrap_err();
        assert!(
            err.contains("github-token"),
            "must name the missing input — {err}"
        );
    }

    fn ws() -> &'static Path {
        Path::new("/w")
    }

    /// The archive root follows the *search paths*, not the matched files —
    /// `findFilesToUpload` semantics. A lone literal file ships at the
    /// artifact root; a directory pattern keeps its interior layout even
    /// when only one deep file matched.
    #[test]
    fn archive_root_follows_search_paths() {
        // Single literal file → parent dir → entry `payload.txt`.
        let files = vec![PathBuf::from("/w/dist/payload.txt")];
        assert_eq!(
            upload_archive_root(ws(), "dist/payload.txt", &files),
            PathBuf::from("/w/dist")
        );

        // Directory pattern whose only match sits in a subdir → root is the
        // directory → entry `libs/app.jar` (not `app.jar`).
        let files = vec![PathBuf::from("/w/build/libs/app.jar")];
        assert_eq!(
            upload_archive_root(ws(), "build/", &files),
            PathBuf::from("/w/build")
        );

        // `**` glob with a deep single match → root is the literal prefix
        // → entry `sub/b.txt` (not `b.txt`).
        let files = vec![PathBuf::from("/w/out/sub/b.txt")];
        assert_eq!(
            upload_archive_root(ws(), "out/**/*.txt", &files),
            PathBuf::from("/w/out")
        );

        // Two literal files in one dir → LCA is the dir.
        let files = vec![PathBuf::from("/w/a/f1.txt"), PathBuf::from("/w/a/f2.txt")];
        assert_eq!(
            upload_archive_root(ws(), "a/f1.txt\na/f2.txt", &files),
            PathBuf::from("/w/a")
        );

        // Disjoint patterns → LCA = workspace.
        let files = vec![PathBuf::from("/w/a/f.txt"), PathBuf::from("/w/b/f.txt")];
        assert_eq!(
            upload_archive_root(ws(), "a/**\nb/**", &files),
            ws().to_path_buf()
        );

        // A `!` exclusion contributes no search path — the `a/` root stands.
        let files = vec![PathBuf::from("/w/a/keep.txt")];
        assert_eq!(
            upload_archive_root(ws(), "a/**\n!a/drop.txt", &files),
            PathBuf::from("/w/a")
        );

        // A search path already covered by an ancestor drops out — the
        // ancestor is the root, not the LCA of the two.
        let files = vec![
            PathBuf::from("/w/src/app/f.js"),
            PathBuf::from("/w/src/util/g.js"),
        ];
        assert_eq!(
            upload_archive_root(ws(), "src/\nsrc/app", &files),
            PathBuf::from("/w/src")
        );
    }
}
