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
    let run_id = senv
        .task
        .run_id
        .as_deref()
        .ok_or_else(|| "download-artifact: task has no run_id".to_string())?;
    let merge = bool_input(with, "merge-multiple");
    for ignored in ["github-token", "repository", "run-id"] {
        if input(with, ignored).is_some() {
            senv.logger
                .line(&format!(
                    "::warning::input '{ignored}' is ignored — artifact download is same-run only"
                ))
                .await;
        }
    }

    // `name` → one artifact into `path`; `pattern`/empty → list, filter by
    // name glob, each into `path/<name>` unless merge-multiple.
    let targets: Vec<(String, PathBuf)> = match input(with, "name") {
        Some(n) => vec![(n.to_string(), dest.clone())],
        None => {
            let pattern = input(with, "pattern");
            let metas = senv.logger.client.list_artifacts(run_id).await?;
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
        let bytes = senv.logger.client.download_artifact(run_id, name).await?;
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
