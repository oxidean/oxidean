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
        let mut hits = glob_walk(senv.workspace, pat);
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

    // Archive: `tar -czf <tgz> -C <workspace> -T <list>` — file list avoids
    // argv limits; entries stay workspace-relative like upstream zips.
    let stage = senv.runner_temp.join("artifacts");
    std::fs::create_dir_all(&stage).map_err(|e| e.to_string())?;
    let rels: Vec<String> = files
        .iter()
        .filter_map(|f| {
            f.strip_prefix(senv.workspace)
                .ok()
                .map(|r| r.to_string_lossy().replace('\\', "/"))
        })
        .collect();
    let list_file = stage.join(format!("{name}.files"));
    std::fs::write(&list_file, rels.join("\n")).map_err(|e| e.to_string())?;
    let tgz = stage.join(format!("{name}.tgz"));
    let out = Command::new("tar")
        .args(["czf"])
        .arg(&tgz)
        .arg("-C")
        .arg(senv.workspace)
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
    let bytes = std::fs::read(&tgz).map_err(|e| e.to_string())?;
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
        let out = Command::new("tar")
            .args(["xzf"])
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

/// Any relative path segment starting with `.` (hidden-file filter).
fn has_hidden_segment(root: &Path, f: &Path) -> bool {
    f.strip_prefix(root)
        .map(|r| {
            r.components()
                .any(|c| c.as_os_str().to_string_lossy().starts_with('.'))
        })
        .unwrap_or(false)
}
