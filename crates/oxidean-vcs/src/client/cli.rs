//! `CliSurface` — the 16-command `ox vcs` implementation set (FR12.1, C8).
//!
//! Every command is argv-free: `oxidean-cli` parses flags into [`VcsCommand`]
//! and calls [`run`]; this layer owns request shaping (`{owner, name}` first
//! per C1), the `CliOutcome{envelope, pretty}` result shape `Out::Rpc`
//! mirrors, and pretty renderings. `{ok:false}` envelopes pass through
//! verbatim — server `vcs.*` codes are classified by `error_exit_code`, never
//! rewritten here.

use std::fs;
use std::path::{Path, PathBuf};

use base64::Engine;
use oxidean_core::vcs_types::{
    ManifestEntryKind, VcsChangesetGetResponse, VcsFileGetResponse, VcsFileMode,
};
use oxidean_core::RpcResponse;
use serde_json::{json, Value};

use super::object_cache::ObjectCache;
use super::transport::{CliFault, CliOutcome, VcsTransport};
use super::working_copy::{checkout as wc_checkout, CheckoutResult, PathFilter};

/// Inline content cap — matches the wire's 256 KiB `content_b64` bound
/// (larger files ride the object PUT path, AC2.1.2).
pub const INLINE_CONTENT_CAP: usize = 256 * 1024;

/// `{owner, name}` — every `vcs.*` request opens with this pair (C1).
#[derive(Debug, Clone, PartialEq)]
pub struct RepoRef {
    pub owner: String,
    pub name: String,
}

impl RepoRef {
    /// `owner/name` — the CLI's repo-spec shape.
    pub fn parse(spec: &str) -> Result<Self, CliFault> {
        let (owner, name) = spec
            .split_once('/')
            .ok_or_else(|| CliFault::Usage(format!("expected OWNER/NAME, got {spec:?}")))?;
        if owner.is_empty() || name.is_empty() || name.contains('/') {
            return Err(CliFault::Usage(format!(
                "expected OWNER/NAME, got {spec:?}"
            )));
        }
        Ok(Self {
            owner: owner.to_string(),
            name: name.to_string(),
        })
    }

    fn input(&self) -> serde_json::Map<String, Value> {
        let mut m = serde_json::Map::new();
        m.insert("owner".into(), json!(self.owner));
        m.insert("name".into(), json!(self.name));
        m
    }
}

/// One `--add <vpath>=<local>` / `--modify <vpath>=<local>` staging row.
/// `add` declares a new path, `modify` an update to an existing one — the
/// wire kind is declared client-side, not tree-validated (u4 stores it
/// verbatim).
#[derive(Debug, Clone, PartialEq)]
pub struct FileSpec {
    /// Repo-relative VCS path.
    pub path: String,
    /// Local file to read content/mode from.
    pub local: PathBuf,
    /// `add` or `modify`.
    pub kind: ManifestEntryKind,
    /// `--mode` override (else derived from the local file).
    pub mode: Option<VcsFileMode>,
}

/// One `--rename <old>=<new>` row.
#[derive(Debug, Clone, PartialEq)]
pub struct RenameSpec {
    pub old_path: String,
    pub path: String,
}

/// `ref set` CAS arms — mutually exclusive flags resolved argv-side.
#[derive(Debug, Clone, PartialEq)]
pub struct RefSetArgs {
    /// `--expect <cs>` — CAS on the current value (absent ⇒ unconstrained
    /// unless `--create`).
    pub expect: Option<String>,
    /// `--create` — `expected_old: null` (ref must not exist).
    pub create: bool,
    /// `--to <cs>` — the new head.
    pub to: Option<String>,
    /// `--delete` — `new: null`.
    pub delete: bool,
}

/// The 16 `ox vcs` commands (`ref` groups `list`/`set`).
#[derive(Debug, Clone, PartialEq)]
pub enum VcsCommand {
    Enable {
        repo: RepoRef,
        disable: bool,
    },
    Status {
        repo: RepoRef,
    },
    Commit {
        repo: RepoRef,
        message: String,
        parents: Vec<String>,
        files: Vec<FileSpec>,
        deletes: Vec<String>,
        renames: Vec<RenameSpec>,
    },
    Log {
        repo: RepoRef,
        from: Option<String>,
        limit: Option<i64>,
        cursor: Option<String>,
    },
    Diff {
        repo: RepoRef,
        from: String,
        to: String,
        paths: Vec<String>,
        limit: Option<i64>,
        cursor: Option<String>,
    },
    Show {
        repo: RepoRef,
        id: String,
    },
    Cat {
        repo: RepoRef,
        changeset: String,
        path: String,
    },
    Blame {
        repo: RepoRef,
        path: String,
        at: Option<String>,
        limit: Option<i64>,
        cursor: Option<String>,
    },
    History {
        repo: RepoRef,
        path: String,
        limit: Option<i64>,
        cursor: Option<String>,
    },
    MergeBase {
        repo: RepoRef,
        a: String,
        b: String,
    },
    Merge {
        repo: RepoRef,
        a: String,
        b: String,
        message: Option<String>,
    },
    Rebase {
        repo: RepoRef,
        changeset: String,
        onto: String,
        message: Option<String>,
    },
    Checkout {
        repo: RepoRef,
        changeset: String,
        paths: Vec<String>,
        dir: PathBuf,
    },
    RefList {
        repo: RepoRef,
        limit: Option<i64>,
        cursor: Option<String>,
    },
    RefSet {
        repo: RepoRef,
        name: String,
        args: RefSetArgs,
    },
    Import {
        repo: RepoRef,
        from: Option<String>,
        limit: Option<i64>,
        cursor: Option<String>,
    },
    Export {
        repo: RepoRef,
        head: String,
        branch: String,
        force: bool,
    },
}

/// The wire procedure each command calls (`required_procedures` glue asserts
/// the same rows).
pub fn procedure(cmd: &VcsCommand) -> &'static str {
    match cmd {
        VcsCommand::Enable { .. } => "vcs.repo_enable",
        VcsCommand::Status { .. } => "vcs.status",
        VcsCommand::Commit { .. } => "vcs.changeset_create",
        VcsCommand::Log { .. } => "vcs.log",
        VcsCommand::Diff { .. } => "vcs.diff",
        VcsCommand::Show { .. } => "vcs.changeset_get",
        VcsCommand::Cat { .. } => "vcs.file_get",
        VcsCommand::Blame { .. } => "vcs.blame",
        VcsCommand::History { .. } => "vcs.path_history",
        VcsCommand::MergeBase { .. } => "vcs.merge_base",
        VcsCommand::Merge { .. } => "vcs.merge",
        VcsCommand::Rebase { .. } => "vcs.rebase",
        VcsCommand::Checkout { .. } => "vcs.tree",
        VcsCommand::RefList { .. } => "vcs.ref_list",
        VcsCommand::RefSet { .. } => "vcs.ref_update",
        VcsCommand::Import { .. } => "vcs.git_import",
        VcsCommand::Export { .. } => "vcs.git_export",
    }
}

/// Run one command; `cache` backs checkout (and is unused elsewhere — pass a
/// throwaway `ObjectCache::from_env()`).
pub async fn run(
    transport: &dyn VcsTransport,
    cmd: &VcsCommand,
    cache: &mut ObjectCache,
) -> Result<CliOutcome, CliFault> {
    match cmd {
        VcsCommand::Enable { repo, disable } => {
            let mut input = repo.input();
            input.insert("enabled".into(), json!(!disable));
            let envelope = call(transport, "vcs.repo_enable", input).await?;
            let pretty = ok_pretty(&envelope, |_| {
                format!(
                    "vcs {} for {}/{}",
                    if *disable { "disabled" } else { "enabled" },
                    repo.owner,
                    repo.name
                )
            });
            Ok(CliOutcome {
                envelope,
                pretty,
                bytes: None,
            })
        }
        VcsCommand::Status { repo } => {
            let envelope = call(transport, "vcs.status", repo.input()).await?;
            let pretty = ok_pretty(&envelope, |data| {
                let heads = data["heads"]
                    .as_array()
                    .map(|hs| {
                        hs.iter()
                            .map(|h| {
                                format!(
                                    "  {} {}",
                                    h["name"].as_str().unwrap_or("?"),
                                    h["changeset_id"].as_str().unwrap_or("?")
                                )
                            })
                            .collect::<Vec<_>>()
                            .join("\n")
                    })
                    .unwrap_or_default();
                format!(
                    "enabled: {}\nheads:\n{}\nchangesets: {}  objects: {}  storage: {} bytes",
                    data["enabled"],
                    if heads.is_empty() {
                        "  (none)".to_string()
                    } else {
                        heads
                    },
                    data["changeset_count"],
                    data["object_count"],
                    data["storage_bytes"],
                )
            });
            Ok(CliOutcome {
                envelope,
                pretty,
                bytes: None,
            })
        }
        VcsCommand::Commit {
            repo,
            message,
            parents,
            files,
            deletes,
            renames,
        } => {
            if files.is_empty() && deletes.is_empty() && renames.is_empty() {
                return Err(CliFault::Usage(
                    "vcs commit needs at least one --add/--modify, --delete, or --rename".into(),
                ));
            }
            // The wire grammar requires `mode` on every entry and `blob` on
            // every non-delete — delete/rename resolve both from the source
            // path's record at the first parent via vcs.file_get.
            if (!deletes.is_empty() || !renames.is_empty()) && parents.is_empty() {
                return Err(CliFault::Usage(
                    "--delete/--rename need --parent to resolve the source entry".into(),
                ));
            }
            let mut changes = Vec::new();
            for spec in files {
                changes.push(stage_file(transport, repo, spec).await?);
            }
            for path in deletes {
                let (mode, _) = file_meta(transport, repo, parents.first().unwrap(), path).await?;
                changes.push(json!({ "path": path, "kind": "delete", "mode": mode }));
            }
            for rename in renames {
                let (mode, blob) =
                    file_meta(transport, repo, parents.first().unwrap(), &rename.old_path).await?;
                changes.push(json!({
                    "path": rename.path,
                    "kind": "rename",
                    "old_path": rename.old_path,
                    "mode": mode,
                    "blob": blob,
                }));
            }
            let mut input = repo.input();
            input.insert("parents".into(), json!(parents));
            input.insert("message".into(), json!(message));
            input.insert("changes".into(), Value::Array(changes));
            let envelope = call(transport, "vcs.changeset_create", input).await?;
            let pretty = ok_pretty(&envelope, |data| {
                format!(
                    "changeset {}\nmanifest {}",
                    data["changeset_id"].as_str().unwrap_or("?"),
                    data["manifest_hash"].as_str().unwrap_or("?"),
                )
            });
            Ok(CliOutcome {
                envelope,
                pretty,
                bytes: None,
            })
        }
        VcsCommand::Log {
            repo,
            from,
            limit,
            cursor,
        } => {
            let mut input = repo.input();
            input.insert("from".into(), json!(from.as_deref().unwrap_or("main")));
            opt(&mut input, "limit", limit.map(Value::from));
            opt(&mut input, "cursor", cursor.clone().map(Value::from));
            let envelope = call(transport, "vcs.log", input).await?;
            let pretty = ok_pretty(&envelope, changeset_listing);
            Ok(CliOutcome {
                envelope,
                pretty,
                bytes: None,
            })
        }
        VcsCommand::Diff {
            repo,
            from,
            to,
            paths,
            limit,
            cursor,
        } => {
            let mut input = repo.input();
            input.insert("from".into(), json!(from));
            input.insert("to".into(), json!(to));
            if !paths.is_empty() {
                input.insert("paths".into(), json!(paths));
            }
            opt(&mut input, "limit", limit.map(Value::from));
            opt(&mut input, "cursor", cursor.clone().map(Value::from));
            let envelope = call(transport, "vcs.diff", input).await?;
            let pretty = ok_pretty(&envelope, |data| {
                let entries = data["entries"].as_array().cloned().unwrap_or_default();
                let mut out = String::new();
                for e in &entries {
                    let kind = e["kind"].as_str().unwrap_or("?");
                    let path = e["path"].as_str().unwrap_or("?");
                    out.push_str(&format!("{kind}\t{path}\n"));
                    match &e["patch"] {
                        Value::String(patch) => out.push_str(patch),
                        v if v["truncated"] == json!(true) => {
                            out.push_str("  <patch truncated — fetch blobs to diff>\n");
                        }
                        _ => {}
                    }
                    if !out.ends_with('\n') {
                        out.push('\n');
                    }
                }
                let t = &data["totals"];
                out.push_str(&format!(
                    "totals: {} files, +{} -{}\n",
                    t["files"], t["additions"], t["deletions"]
                ));
                cursor_line(data, &mut out);
                out.trim_end().to_string()
            });
            Ok(CliOutcome {
                envelope,
                pretty,
                bytes: None,
            })
        }
        VcsCommand::Show { repo, id } => {
            let mut input = repo.input();
            input.insert("id".into(), json!(id));
            let envelope = call(transport, "vcs.changeset_get", input).await?;
            let pretty = ok_pretty(&envelope, changeset_detail);
            Ok(CliOutcome {
                envelope,
                pretty,
                bytes: None,
            })
        }
        VcsCommand::Cat {
            repo,
            changeset,
            path,
        } => {
            let mut input = repo.input();
            input.insert("changeset_id".into(), json!(changeset));
            input.insert("path".into(), json!(path));
            let envelope = call(transport, "vcs.file_get", input).await?;
            match &envelope {
                RpcResponse::Err { .. } => Ok(CliOutcome {
                    envelope,
                    pretty: String::new(),
                    bytes: None,
                }),
                RpcResponse::Ok { data, .. } => {
                    let resp: VcsFileGetResponse = serde_json::from_value(data.clone())
                        .map_err(|e| CliFault::Transport(format!("decoding vcs.file_get: {e}")))?;
                    if let Some(b64) = &resp.content_b64 {
                        let bytes = base64::engine::general_purpose::STANDARD
                            .decode(b64)
                            .map_err(|e| {
                                CliFault::Transport(format!("decoding content_b64: {e}"))
                            })?;
                        return Ok(CliOutcome {
                            envelope,
                            pretty: String::new(),
                            bytes: Some(bytes),
                        });
                    }
                    if resp.too_large {
                        let bytes = transport
                            .object_get(&repo.owner, &repo.name, resp.blob.as_str())
                            .await?;
                        return Ok(CliOutcome {
                            envelope,
                            pretty: String::new(),
                            bytes: Some(bytes),
                        });
                    }
                    // Present-but-empty inline content (a legit empty file).
                    Ok(CliOutcome {
                        envelope,
                        pretty: String::new(),
                        bytes: Some(Vec::new()),
                    })
                }
            }
        }
        VcsCommand::Blame {
            repo,
            path,
            at,
            limit,
            cursor,
        } => {
            let mut input = repo.input();
            input.insert("path".into(), json!(path));
            opt(&mut input, "at", at.clone().map(Value::from));
            opt(&mut input, "limit", limit.map(Value::from));
            opt(&mut input, "cursor", cursor.clone().map(Value::from));
            let envelope = call(transport, "vcs.blame", input).await?;
            let pretty = ok_pretty(&envelope, |data| {
                let mut out = String::new();
                for e in data["entries"].as_array().cloned().unwrap_or_default() {
                    out.push_str(&format!(
                        "{}\t{}\t{}\n",
                        e["changeset_id"].as_str().unwrap_or("?"),
                        e["author"]["name"].as_str().unwrap_or("?"),
                        e["content"].as_str().unwrap_or(""),
                    ));
                }
                cursor_line(data, &mut out);
                out.trim_end().to_string()
            });
            Ok(CliOutcome {
                envelope,
                pretty,
                bytes: None,
            })
        }
        VcsCommand::History {
            repo,
            path,
            limit,
            cursor,
        } => {
            let mut input = repo.input();
            input.insert("path".into(), json!(path));
            opt(&mut input, "limit", limit.map(Value::from));
            opt(&mut input, "cursor", cursor.clone().map(Value::from));
            let envelope = call(transport, "vcs.path_history", input).await?;
            let pretty = ok_pretty(&envelope, |data| {
                let mut out = String::new();
                for e in data["entries"].as_array().cloned().unwrap_or_default() {
                    out.push_str(&format!(
                        "{}\t{}\t{}\t{}\n",
                        e["changeset_id"].as_str().unwrap_or("?"),
                        e["kind"].as_str().unwrap_or("?"),
                        e["path"].as_str().unwrap_or("?"),
                        e["committed_at"].as_str().unwrap_or("?"),
                    ));
                }
                cursor_line(data, &mut out);
                out.trim_end().to_string()
            });
            Ok(CliOutcome {
                envelope,
                pretty,
                bytes: None,
            })
        }
        VcsCommand::MergeBase { repo, a, b } => {
            let mut input = repo.input();
            input.insert("a".into(), json!(a));
            input.insert("b".into(), json!(b));
            let envelope = call(transport, "vcs.merge_base", input).await?;
            let pretty = ok_pretty(&envelope, |data| {
                format!("merge base: {}", data["merge_base"].as_str().unwrap_or("?"))
            });
            Ok(CliOutcome {
                envelope,
                pretty,
                bytes: None,
            })
        }
        VcsCommand::Merge {
            repo,
            a,
            b,
            message,
        } => {
            let mut input = repo.input();
            input.insert("a".into(), json!(a));
            input.insert("b".into(), json!(b));
            opt(&mut input, "message", message.clone().map(Value::from));
            let envelope = call(transport, "vcs.merge", input).await?;
            let pretty = merge_pretty(&envelope, "merge changeset");
            Ok(CliOutcome {
                envelope,
                pretty,
                bytes: None,
            })
        }
        VcsCommand::Rebase {
            repo,
            changeset,
            onto,
            message,
        } => {
            let mut input = repo.input();
            input.insert("changeset_id".into(), json!(changeset));
            input.insert("new_parent_id".into(), json!(onto));
            opt(&mut input, "message", message.clone().map(Value::from));
            let envelope = call(transport, "vcs.rebase", input).await?;
            let pretty = merge_pretty(&envelope, "head");
            Ok(CliOutcome {
                envelope,
                pretty,
                bytes: None,
            })
        }
        VcsCommand::Checkout {
            repo,
            changeset,
            paths,
            dir,
        } => {
            let filter = PathFilter::new(paths);
            let result = wc_checkout(
                transport,
                &repo.owner,
                &repo.name,
                changeset,
                &filter,
                dir,
                cache,
            )
            .await;
            match result {
                Ok(CheckoutResult { files_written, blobs_fetched, cache_hits, skipped_outside_filter }) => {
                    Ok(CliOutcome {
                        envelope: RpcResponse::ok(json!({
                            "files_written": files_written,
                            "blobs_fetched": blobs_fetched,
                            "cache_hits": cache_hits,
                            "skipped_outside_filter": skipped_outside_filter,
                            "dir": dir.display().to_string(),
                        })),
                        pretty: format!(
                            "checked out {files_written} files ({blobs_fetched} fetched, {cache_hits} cached, {skipped_outside_filter} skipped)"
                        ),
                        bytes: None,
                    })
                }
                // Server envelopes stay verbatim so `vcs.disabled` etc. classify.
                Err(CliFault::Server(envelope)) => {
                    Ok(CliOutcome { envelope: *envelope, pretty: String::new(), bytes: None })
                }
                Err(fault) => Err(fault),
            }
        }
        VcsCommand::RefList {
            repo,
            limit,
            cursor,
        } => {
            let mut input = repo.input();
            opt(&mut input, "limit", limit.map(Value::from));
            opt(&mut input, "cursor", cursor.clone().map(Value::from));
            let envelope = call(transport, "vcs.ref_list", input).await?;
            let pretty = ok_pretty(&envelope, |data| {
                let mut out = String::new();
                for e in data["entries"].as_array().cloned().unwrap_or_default() {
                    out.push_str(&format!(
                        "{}\t{}\t{}\n",
                        e["name"].as_str().unwrap_or("?"),
                        e["changeset_id"].as_str().unwrap_or("?"),
                        e["updated_at"].as_str().unwrap_or("?"),
                    ));
                }
                cursor_line(data, &mut out);
                out.trim_end().to_string()
            });
            Ok(CliOutcome {
                envelope,
                pretty,
                bytes: None,
            })
        }
        VcsCommand::RefSet { repo, name, args } => {
            validate_ref_set(args)?;
            let mut input = repo.input();
            input.insert("ref".into(), json!(name));
            input.insert(
                "expected_old".into(),
                if args.create {
                    Value::Null
                } else {
                    json!(args.expect)
                },
            );
            input.insert(
                "new".into(),
                if args.delete {
                    Value::Null
                } else {
                    json!(args.to)
                },
            );
            let envelope = call(transport, "vcs.ref_update", input).await?;
            let pretty = ok_pretty(&envelope, |data| match data["changeset_id"].as_str() {
                Some(cs) => format!("ref {} -> {cs}", data["ref"].as_str().unwrap_or(name)),
                None => format!("ref {} deleted", data["ref"].as_str().unwrap_or(name)),
            });
            Ok(CliOutcome {
                envelope,
                pretty,
                bytes: None,
            })
        }
        VcsCommand::Import {
            repo,
            from,
            limit,
            cursor,
        } => {
            let mut input = repo.input();
            opt(&mut input, "from", from.clone().map(Value::from));
            opt(&mut input, "limit", limit.map(Value::from));
            opt(&mut input, "cursor", cursor.clone().map(Value::from));
            let envelope = call(transport, "vcs.git_import", input).await?;
            let pretty = ok_pretty(&envelope, |data| {
                let mut out = format!(
                    "imported {} commit(s) ({} tip(s) skipped)",
                    data["imported"], data["skipped_tips"]
                );
                for h in data["heads"].as_array().cloned().unwrap_or_default() {
                    out.push_str(&format!(
                        "\n  {} {} -> {}",
                        h["name"].as_str().unwrap_or("?"),
                        h["git_oid"].as_str().unwrap_or("?"),
                        h["changeset_id"].as_str().unwrap_or("<pending>")
                    ));
                }
                cursor_line(data, &mut out);
                if data["done"] == json!(true) {
                    out.push_str("\ndone");
                }
                out
            });
            Ok(CliOutcome {
                envelope,
                pretty,
                bytes: None,
            })
        }
        VcsCommand::Export {
            repo,
            head,
            branch,
            force,
        } => {
            let mut input = repo.input();
            input.insert("head".into(), json!(head));
            input.insert("branch".into(), json!(branch));
            if *force {
                input.insert("force".into(), json!(true));
            }
            let envelope = call(transport, "vcs.git_export", input).await?;
            let pretty = ok_pretty(&envelope, |data| {
                format!(
                    "exported {} -> {} (git {})",
                    head,
                    data["branch"].as_str().unwrap_or("?"),
                    data["git_oid"].as_str().unwrap_or("?"),
                )
            });
            Ok(CliOutcome {
                envelope,
                pretty,
                bytes: None,
            })
        }
    }
}

/// Insert when `Some` — wire `Option` fields omit rather than null.
fn opt(map: &mut serde_json::Map<String, Value>, key: &str, value: Option<Value>) {
    if let Some(v) = value {
        map.insert(key.into(), v);
    }
}

async fn call(
    transport: &dyn VcsTransport,
    procedure: &str,
    input: serde_json::Map<String, Value>,
) -> Result<RpcResponse, CliFault> {
    transport
        .call(procedure, Value::Object(input))
        .await
        .map_err(CliFault::from)
}

/// `pretty` renders only on `{ok:true}` — the error arm prints
/// `ox: code: message` (the existing `Out::Rpc` convention).
fn ok_pretty(envelope: &RpcResponse, render: impl Fn(&Value) -> String) -> String {
    match envelope {
        RpcResponse::Ok { data, .. } => render(data),
        RpcResponse::Err { .. } => String::new(),
    }
}

/// Merge/rebase render — `{ok:true}` shows the landed id; `vcs.conflict`
/// envelopes additionally render their typed `data.conflicts[]` list so the
/// human channel sees the same machine-readable paths.
fn merge_pretty(envelope: &RpcResponse, label: &str) -> String {
    match envelope {
        RpcResponse::Ok { data, .. } => {
            let id = data["changeset_id"]
                .as_str()
                .or_else(|| data["head"].as_str());
            let ff = if data["fast_forward"] == json!(true) {
                " (fast-forward)"
            } else {
                ""
            };
            format!("{label} {}{}", id.unwrap_or("?"), ff)
        }
        RpcResponse::Err { error, .. } if error.code == "vcs.conflict" => {
            let mut out = String::from("conflicts:");
            if let Some(list) = error.data.as_ref().and_then(|d| d["conflicts"].as_array()) {
                for c in list {
                    out.push_str(&format!(
                        "\n  {} {}",
                        c["kind"].as_str().unwrap_or("conflict"),
                        c["path"].as_str().unwrap_or("?"),
                    ));
                }
            }
            out
        }
        RpcResponse::Err { .. } => String::new(),
    }
}

/// Shared `changeset_get`/`log` render — one line per entry plus cursor.
fn changeset_listing(data: &Value) -> String {
    let mut out = String::new();
    for e in data["entries"].as_array().cloned().unwrap_or_default() {
        out.push_str(&format!(
            "{}\t{}\t{}\n",
            e["changeset_id"].as_str().unwrap_or("?"),
            e["committed_at"].as_str().unwrap_or("?"),
            e["message"]
                .as_str()
                .unwrap_or("")
                .lines()
                .next()
                .unwrap_or(""),
        ));
    }
    cursor_line(data, &mut out);
    out.trim_end().to_string()
}

/// `ox vcs show` render — header + manifest rows.
fn changeset_detail(data: &Value) -> String {
    let cs: VcsChangesetGetResponse =
        serde_json::from_value(data.clone()).unwrap_or(VcsChangesetGetResponse {
            changeset_id: oxidean_core::vcs_types::ChangesetId::parse(
                "cs_00000000000000000000000000",
            )
            .unwrap(),
            parents: Vec::new(),
            author: oxidean_core::vcs_types::VcsAuthor {
                name: String::new(),
                email: String::new(),
            },
            message: String::new(),
            committed_at: String::new(),
            manifest: Vec::new(),
        });
    let mut out = format!(
        "changeset {}\nparents: {}\nauthor: {} <{}>\ndate: {}\n\n{}\n\nmanifest:\n",
        cs.changeset_id,
        cs.parents
            .iter()
            .map(|p| p.as_str().to_string())
            .collect::<Vec<_>>()
            .join(", "),
        cs.author.name,
        cs.author.email,
        cs.committed_at,
        cs.message.trim_end(),
    );
    for e in &cs.manifest {
        let mut line = format!("  {}\t{}", format!("{:?}", e.kind).to_lowercase(), e.path);
        if let Some(old) = &e.old_path {
            line.push_str(&format!(" (from {old})"));
        }
        if let Some(blob) = &e.blob {
            line.push_str(&format!("  {blob}"));
        }
        if e.content_b64.is_some() {
            line.push_str("  <inline>");
        }
        out.push_str(&line);
        out.push('\n');
    }
    out.trim_end().to_string()
}

fn cursor_line(data: &Value, out: &mut String) {
    if let Some(c) = data["cursor"].as_str() {
        out.push_str(&format!("cursor: {c}\n"));
    }
}

/// `ref set` flag validation — the CAS arms must be fully constrained.
fn validate_ref_set(args: &RefSetArgs) -> Result<(), CliFault> {
    if args.create && args.expect.is_some() {
        return Err(CliFault::Usage(
            "--create and --expect are mutually exclusive".into(),
        ));
    }
    if args.delete && args.to.is_some() {
        return Err(CliFault::Usage(
            "--delete and --to are mutually exclusive".into(),
        ));
    }
    if !args.create && args.expect.is_none() {
        return Err(CliFault::Usage(
            "ref set needs --expect <cs> or --create (CAS is never unconstrained)".into(),
        ));
    }
    if !args.delete && args.to.is_none() {
        return Err(CliFault::Usage(
            "ref set needs --to <cs> or --delete".into(),
        ));
    }
    Ok(())
}

/// Stage one `--add`/`--modify`: read the local file (symlinks contribute
/// their target path as content), choose `content_b64` ≤256 KiB else
/// `object_put` + `blob`, and derive `mode` from fs metadata unless `--mode`
/// overrode it.
async fn stage_file(
    transport: &dyn VcsTransport,
    repo: &RepoRef,
    spec: &FileSpec,
) -> Result<Value, CliFault> {
    let (bytes, derived_mode) = read_local(&spec.local)?;
    let mode = spec.mode.unwrap_or(derived_mode);
    let mut entry = serde_json::Map::new();
    entry.insert("path".into(), json!(spec.path));
    entry.insert("kind".into(), json!(spec.kind));
    entry.insert("mode".into(), json!(mode));
    if bytes.len() <= INLINE_CONTENT_CAP {
        entry.insert(
            "content_b64".into(),
            json!(base64::engine::general_purpose::STANDARD.encode(&bytes)),
        );
    } else {
        let hash = transport
            .object_put(&repo.owner, &repo.name, &bytes)
            .await?;
        entry.insert("blob".into(), json!(hash));
    }
    Ok(Value::Object(entry))
}

/// Resolve `path` at `parent` via `vcs.file_get` — delete/rename manifest
/// entries must carry the source file's `mode` (and rename its `blob`) per
/// the wire grammar, so commit pulls them from the server instead of
/// guessing. A server `Err` (path absent at that parent) propagates as-is.
async fn file_meta(
    transport: &dyn VcsTransport,
    repo: &RepoRef,
    parent: &str,
    path: &str,
) -> Result<(VcsFileMode, String), CliFault> {
    let mut input = repo.input();
    input.insert("changeset_id".into(), json!(parent));
    input.insert("path".into(), json!(path));
    let envelope = call(transport, "vcs.file_get", input).await?;
    match &envelope {
        RpcResponse::Ok { data, .. } => {
            let resp: VcsFileGetResponse = serde_json::from_value(data.clone())
                .map_err(|e| CliFault::Transport(format!("decoding vcs.file_get: {e}")))?;
            Ok((resp.mode, resp.blob.as_str().to_string()))
        }
        RpcResponse::Err { .. } => Err(CliFault::Server(Box::new(envelope))),
    }
}

/// Read a local path for staging — symlinks yield their target string as the
/// blob payload (`mode 120000`); regular files yield bytes + exec-bit mode.
fn read_local(local: &Path) -> Result<(Vec<u8>, VcsFileMode), CliFault> {
    let meta = fs::symlink_metadata(local)
        .map_err(|e| CliFault::Fs(format!("reading {}: {e}", local.display())))?;
    if meta.file_type().is_symlink() {
        let target = fs::read_link(local)
            .map_err(|e| CliFault::Fs(format!("reading link {}: {e}", local.display())))?;
        return Ok((
            target
                .as_os_str()
                .to_string_lossy()
                .into_owned()
                .into_bytes(),
            VcsFileMode::Symlink,
        ));
    }
    let bytes =
        fs::read(local).map_err(|e| CliFault::Fs(format!("reading {}: {e}", local.display())))?;
    Ok((bytes, regular_mode(&meta)))
}

#[cfg(unix)]
fn regular_mode(meta: &fs::Metadata) -> VcsFileMode {
    use std::os::unix::fs::PermissionsExt;
    if meta.permissions().mode() & 0o111 != 0 {
        VcsFileMode::Executable
    } else {
        VcsFileMode::Regular
    }
}

#[cfg(not(unix))]
fn regular_mode(_meta: &fs::Metadata) -> VcsFileMode {
    VcsFileMode::Regular
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::object_cache::object_hash;
    use crate::client::transport::TransportError;
    use oxidean_core::AppError;
    use std::collections::{HashMap, VecDeque};
    use std::sync::Mutex;

    /// Records every `call` input and returns queued envelopes in order;
    /// `object_get`/`object_put` serve/capture the blob map.
    struct Rec {
        calls: Mutex<Vec<(String, Value)>>,
        responses: Mutex<VecDeque<RpcResponse>>,
        objects: Mutex<HashMap<String, Vec<u8>>>,
        puts: Mutex<Vec<Vec<u8>>>,
    }

    impl Rec {
        fn new(responses: Vec<RpcResponse>) -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                responses: Mutex::new(responses.into()),
                objects: Mutex::new(HashMap::new()),
                puts: Mutex::new(Vec::new()),
            }
        }
        fn ok(data: Value) -> RpcResponse {
            RpcResponse::ok(data)
        }
        fn last_input(&self) -> Value {
            self.calls.lock().unwrap().last().unwrap().1.clone()
        }
    }

    #[async_trait::async_trait]
    impl VcsTransport for Rec {
        async fn call(&self, procedure: &str, input: Value) -> Result<RpcResponse, TransportError> {
            self.calls
                .lock()
                .unwrap()
                .push((procedure.to_string(), input));
            self.responses
                .lock()
                .unwrap()
                .pop_front()
                .ok_or_else(|| TransportError::Transport("no scripted response".into()))
        }
        async fn object_get(
            &self,
            _o: &str,
            _n: &str,
            hash: &str,
        ) -> Result<Vec<u8>, TransportError> {
            self.objects
                .lock()
                .unwrap()
                .get(hash)
                .cloned()
                .ok_or_else(|| TransportError::Http {
                    status: 404,
                    body: "vcs.not_found".into(),
                })
        }
        async fn object_put(
            &self,
            _o: &str,
            _n: &str,
            bytes: &[u8],
        ) -> Result<String, TransportError> {
            self.puts.lock().unwrap().push(bytes.to_vec());
            Ok(object_hash(bytes))
        }
    }

    fn repo() -> RepoRef {
        RepoRef {
            owner: "acme".into(),
            name: "widgets".into(),
        }
    }

    fn cache(dir: &Path) -> ObjectCache {
        ObjectCache::new(dir.join("cache"), 1 << 20)
    }

    #[test]
    fn procedure_maps_every_variant() {
        let r = repo();
        let cases: Vec<(VcsCommand, &str)> = vec![
            (
                VcsCommand::Enable {
                    repo: r.clone(),
                    disable: false,
                },
                "vcs.repo_enable",
            ),
            (VcsCommand::Status { repo: r.clone() }, "vcs.status"),
            (
                VcsCommand::Commit {
                    repo: r.clone(),
                    message: "m".into(),
                    parents: vec![],
                    files: vec![],
                    deletes: vec![],
                    renames: vec![],
                },
                "vcs.changeset_create",
            ),
            (
                VcsCommand::Log {
                    repo: r.clone(),
                    from: None,
                    limit: None,
                    cursor: None,
                },
                "vcs.log",
            ),
            (
                VcsCommand::Diff {
                    repo: r.clone(),
                    from: "a".into(),
                    to: "b".into(),
                    paths: vec![],
                    limit: None,
                    cursor: None,
                },
                "vcs.diff",
            ),
            (
                VcsCommand::Show {
                    repo: r.clone(),
                    id: "cs_1".into(),
                },
                "vcs.changeset_get",
            ),
            (
                VcsCommand::Cat {
                    repo: r.clone(),
                    changeset: "cs_1".into(),
                    path: "f".into(),
                },
                "vcs.file_get",
            ),
            (
                VcsCommand::Blame {
                    repo: r.clone(),
                    path: "f".into(),
                    at: None,
                    limit: None,
                    cursor: None,
                },
                "vcs.blame",
            ),
            (
                VcsCommand::History {
                    repo: r.clone(),
                    path: "f".into(),
                    limit: None,
                    cursor: None,
                },
                "vcs.path_history",
            ),
            (
                VcsCommand::MergeBase {
                    repo: r.clone(),
                    a: "a".into(),
                    b: "b".into(),
                },
                "vcs.merge_base",
            ),
            (
                VcsCommand::Merge {
                    repo: r.clone(),
                    a: "a".into(),
                    b: "b".into(),
                    message: None,
                },
                "vcs.merge",
            ),
            (
                VcsCommand::Rebase {
                    repo: r.clone(),
                    changeset: "c".into(),
                    onto: "o".into(),
                    message: None,
                },
                "vcs.rebase",
            ),
            (
                VcsCommand::Checkout {
                    repo: r.clone(),
                    changeset: "cs_1".into(),
                    paths: vec![],
                    dir: PathBuf::from("x"),
                },
                "vcs.tree",
            ),
            (
                VcsCommand::RefList {
                    repo: r.clone(),
                    limit: None,
                    cursor: None,
                },
                "vcs.ref_list",
            ),
            (
                VcsCommand::RefSet {
                    repo: r.clone(),
                    name: "main".into(),
                    args: RefSetArgs {
                        expect: Some("cs_1".into()),
                        create: false,
                        to: Some("cs_2".into()),
                        delete: false,
                    },
                },
                "vcs.ref_update",
            ),
            (
                VcsCommand::Import {
                    repo: r.clone(),
                    from: None,
                    limit: None,
                    cursor: None,
                },
                "vcs.git_import",
            ),
            (
                VcsCommand::Export {
                    repo: r.clone(),
                    head: "cs_1".into(),
                    branch: "main".into(),
                    force: false,
                },
                "vcs.git_export",
            ),
        ];
        assert_eq!(cases.len(), 17);
        for (cmd, want) in &cases {
            assert_eq!(procedure(cmd), *want);
        }
    }

    #[test]
    fn repo_ref_parse_validates_shape() {
        let r = RepoRef::parse("acme/widgets").unwrap();
        assert_eq!((r.owner.as_str(), r.name.as_str()), ("acme", "widgets"));
        for bad in ["noSlash", "/name", "owner/", "a/b/c", ""] {
            assert!(RepoRef::parse(bad).is_err(), "{bad} must fail");
        }
    }

    #[tokio::test]
    async fn enable_sends_flag_and_renders() {
        let t = Rec::new(vec![Rec::ok(json!({ "enabled": true }))]);
        let dir = tempfile::tempdir().unwrap();
        let out = run(
            &t,
            &VcsCommand::Enable {
                repo: repo(),
                disable: false,
            },
            &mut cache(dir.path()),
        )
        .await
        .unwrap();
        let input = t.last_input();
        assert_eq!(input["enabled"], json!(true));
        assert_eq!(input["owner"], json!("acme"));
        assert!(out.pretty.contains("enabled for acme/widgets"));
    }

    #[tokio::test]
    async fn err_envelopes_pass_verbatim() {
        let t = Rec::new(vec![RpcResponse::err(AppError::new(
            "vcs.disabled",
            "vcs is not enabled",
        ))]);
        let dir = tempfile::tempdir().unwrap();
        let out = run(
            &t,
            &VcsCommand::Status { repo: repo() },
            &mut cache(dir.path()),
        )
        .await
        .unwrap();
        match out.envelope {
            RpcResponse::Err { error, .. } => assert_eq!(error.code, "vcs.disabled"),
            other => panic!("expected err envelope, got {other:?}"),
        }
        assert!(out.pretty.is_empty());
    }

    #[tokio::test]
    async fn commit_builds_manifest_with_inline_and_rename() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("new.txt"), b"hello").unwrap();
        fs::write(dir.path().join("upd.txt"), b"v2").unwrap();
        let t = Rec::new(vec![
            // delete old.txt → file_get at parent; rename a.txt → file_get.
            Rec::ok(json!({
                "mode": "100644",
                "blob": "sha256:1111111111111111111111111111111111111111111111111111111111111111",
                "size": 3,
                "too_large": false,
                "content_b64": null
            })),
            Rec::ok(json!({
                "mode": "100755",
                "blob": "sha256:2222222222222222222222222222222222222222222222222222222222222222",
                "size": 3,
                "too_large": false,
                "content_b64": null
            })),
            Rec::ok(json!({
                "changeset_id": "cs_01",
                "manifest_hash": "sha256:xx"
            })),
        ]);
        let cmd = VcsCommand::Commit {
            repo: repo(),
            message: "test commit".into(),
            parents: vec!["cs_parent".into()],
            files: vec![
                FileSpec {
                    path: "src/new.txt".into(),
                    local: dir.path().join("new.txt"),
                    kind: ManifestEntryKind::Add,
                    mode: None,
                },
                FileSpec {
                    path: "src/upd.txt".into(),
                    local: dir.path().join("upd.txt"),
                    kind: ManifestEntryKind::Modify,
                    mode: Some(VcsFileMode::Executable),
                },
            ],
            deletes: vec!["old.txt".into()],
            renames: vec![RenameSpec {
                old_path: "a.txt".into(),
                path: "b.txt".into(),
            }],
        };
        let out = run(&t, &cmd, &mut cache(dir.path())).await.unwrap();
        let input = t.last_input();
        assert_eq!(input["parents"], json!(["cs_parent"]));
        assert_eq!(input["message"], json!("test commit"));
        let changes = input["changes"].as_array().unwrap();
        assert_eq!(changes.len(), 4);
        assert_eq!(changes[0]["kind"], json!("add"));
        assert_eq!(changes[0]["mode"], json!("100644"));
        assert_eq!(
            changes[0]["content_b64"],
            json!(base64::engine::general_purpose::STANDARD.encode(b"hello"))
        );
        assert_eq!(changes[1]["kind"], json!("modify"));
        assert_eq!(changes[1]["mode"], json!("100755"), "--mode override wins");
        assert_eq!(
            changes[2],
            json!({ "path": "old.txt", "kind": "delete", "mode": "100644" })
        );
        assert_eq!(
            changes[3],
            json!({
                "path": "b.txt",
                "kind": "rename",
                "old_path": "a.txt",
                "mode": "100755",
                "blob": "sha256:2222222222222222222222222222222222222222222222222222222222222222"
            })
        );
        assert!(out.pretty.contains("cs_01"));
    }

    #[tokio::test]
    async fn commit_large_file_uploads_object_first() {
        let dir = tempfile::tempdir().unwrap();
        let big = vec![7u8; INLINE_CONTENT_CAP + 1];
        fs::write(dir.path().join("big.bin"), &big).unwrap();
        let t = Rec::new(vec![Rec::ok(json!({
            "changeset_id": "cs_01",
            "manifest_hash": "sha256:xx"
        }))]);
        let cmd = VcsCommand::Commit {
            repo: repo(),
            message: "big".into(),
            parents: vec![],
            files: vec![FileSpec {
                path: "big.bin".into(),
                local: dir.path().join("big.bin"),
                kind: ManifestEntryKind::Add,
                mode: None,
            }],
            deletes: vec![],
            renames: vec![],
        };
        run(&t, &cmd, &mut cache(dir.path())).await.unwrap();
        // The object went out the raw route; the manifest row carries `blob`.
        assert_eq!(t.puts.lock().unwrap().len(), 1);
        assert_eq!(t.puts.lock().unwrap()[0].len(), INLINE_CONTENT_CAP + 1);
        let changes = t.last_input()["changes"].as_array().unwrap().clone();
        assert_eq!(changes[0]["blob"], json!(object_hash(&big)));
        assert!(changes[0].get("content_b64").is_none());
    }

    #[tokio::test]
    async fn commit_requires_at_least_one_change() {
        let t = Rec::new(vec![]);
        let dir = tempfile::tempdir().unwrap();
        let err = run(
            &t,
            &VcsCommand::Commit {
                repo: repo(),
                message: "empty".into(),
                parents: vec![],
                files: vec![],
                deletes: vec![],
                renames: vec![],
            },
            &mut cache(dir.path()),
        )
        .await
        .expect_err("empty commit must be refused");
        assert_eq!(err.code(), "ox.usage");
        assert!(t.calls.lock().unwrap().is_empty(), "no RPC must fire");
    }

    #[tokio::test]
    async fn ref_set_cas_payloads() {
        // --create --to cs_2 → expected_old:null, new:"cs_2"
        let t = Rec::new(vec![Rec::ok(
            json!({ "ref": "main", "changeset_id": "cs_2" }),
        )]);
        let dir = tempfile::tempdir().unwrap();
        run(
            &t,
            &VcsCommand::RefSet {
                repo: repo(),
                name: "main".into(),
                args: RefSetArgs {
                    expect: None,
                    create: true,
                    to: Some("cs_2".into()),
                    delete: false,
                },
            },
            &mut cache(dir.path()),
        )
        .await
        .unwrap();
        let input = t.last_input();
        // The wire field is `ref` — VcsRefUpdateRequest.ref_name carries
        // #[serde(rename = "ref")]; sending `ref_name` is rpc.bad_input.
        assert_eq!(input["ref"], json!("main"));
        assert_eq!(input["expected_old"], Value::Null);
        assert_eq!(input["new"], json!("cs_2"));

        // --expect cs_1 --delete → expected_old:"cs_1", new:null
        let t = Rec::new(vec![Rec::ok(json!({ "ref": "main" }))]);
        let out = run(
            &t,
            &VcsCommand::RefSet {
                repo: repo(),
                name: "main".into(),
                args: RefSetArgs {
                    expect: Some("cs_1".into()),
                    create: false,
                    to: None,
                    delete: true,
                },
            },
            &mut cache(dir.path()),
        )
        .await
        .unwrap();
        let input = t.last_input();
        assert_eq!(input["expected_old"], json!("cs_1"));
        assert_eq!(input["new"], Value::Null);
        assert!(out.pretty.contains("deleted"));
    }

    #[tokio::test]
    async fn commit_resolves_delete_rename_from_parent() {
        // The wire grammar requires `mode` on every entry and `blob` on
        // rename — both resolve via vcs.file_get against the first parent.
        let t = Rec::new(vec![
            Rec::ok(json!({
                "mode": "100755",
                "blob": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "size": 4,
                "too_large": false,
                "content_b64": null
            })),
            Rec::ok(json!({
                "mode": "100644",
                "blob": "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                "size": 4,
                "too_large": false,
                "content_b64": null
            })),
            Rec::ok(json!({ "changeset_id": "cs_new", "manifest_hash": "h" })),
        ]);
        let dir = tempfile::tempdir().unwrap();
        run(
            &t,
            &VcsCommand::Commit {
                repo: repo(),
                message: "m".into(),
                parents: vec!["cs_p".into()],
                files: vec![],
                deletes: vec!["old.txt".into()],
                renames: vec![RenameSpec {
                    old_path: "a.txt".into(),
                    path: "b.txt".into(),
                }],
            },
            &mut cache(dir.path()),
        )
        .await
        .unwrap();
        let calls = t.calls.lock().unwrap();
        assert_eq!(calls[0].0, "vcs.file_get");
        assert_eq!(calls[0].1["changeset_id"], json!("cs_p"));
        assert_eq!(calls[0].1["path"], json!("old.txt"));
        assert_eq!(calls[1].0, "vcs.file_get");
        assert_eq!(calls[1].1["path"], json!("a.txt"));
        assert_eq!(calls[2].0, "vcs.changeset_create");
        let changes = calls[2].1["changes"].as_array().unwrap();
        assert_eq!(
            changes[0],
            json!({ "path": "old.txt", "kind": "delete", "mode": "100755" })
        );
        assert_eq!(
            changes[1],
            json!({
                "path": "b.txt",
                "kind": "rename",
                "old_path": "a.txt",
                "mode": "100644",
                "blob": "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
            })
        );
    }

    #[tokio::test]
    async fn commit_delete_without_parent_is_usage() {
        let t = Rec::new(vec![]);
        let dir = tempfile::tempdir().unwrap();
        let err = run(
            &t,
            &VcsCommand::Commit {
                repo: repo(),
                message: "m".into(),
                parents: vec![],
                files: vec![],
                deletes: vec!["x.txt".into()],
                renames: vec![],
            },
            &mut cache(dir.path()),
        )
        .await
        .expect_err("delete without a parent must refuse");
        assert_eq!(err.code(), "ox.usage");
        assert!(t.calls.lock().unwrap().is_empty(), "no RPC must fire");
    }

    #[test]
    fn ref_set_flag_validation() {
        let base = RefSetArgs {
            expect: None,
            create: false,
            to: None,
            delete: false,
        };
        assert!(validate_ref_set(&base).is_err(), "unconstrained refused");
        assert!(validate_ref_set(&RefSetArgs {
            expect: Some("a".into()),
            create: true,
            ..base.clone()
        })
        .is_err());
        assert!(validate_ref_set(&RefSetArgs {
            to: Some("b".into()),
            delete: true,
            expect: Some("a".into()),
            ..base.clone()
        })
        .is_err());
        assert!(
            validate_ref_set(&RefSetArgs {
                expect: Some("a".into()),
                ..base.clone()
            })
            .is_err(),
            "no new side refused"
        );
        assert!(validate_ref_set(&RefSetArgs {
            create: true,
            to: Some("b".into()),
            ..base
        })
        .is_ok());
    }

    #[tokio::test]
    async fn cat_inline_and_too_large_paths() {
        // Inline content → decoded bytes.
        let b64 = base64::engine::general_purpose::STANDARD.encode(b"file body");
        let t = Rec::new(vec![Rec::ok(json!({
            "blob": object_hash(b"file body"), "mode": "100644", "size": 9,
            "content_b64": b64, "too_large": false
        }))]);
        let dir = tempfile::tempdir().unwrap();
        let cmd = VcsCommand::Cat {
            repo: repo(),
            changeset: "cs_1".into(),
            path: "f.txt".into(),
        };
        let out = run(&t, &cmd, &mut cache(dir.path())).await.unwrap();
        assert_eq!(out.bytes.as_deref(), Some(b"file body".as_slice()));

        // too_large → raw object route.
        let blob = object_hash(b"large body");
        let t = Rec::new(vec![Rec::ok(json!({
            "blob": blob, "mode": "100644", "size": 10,
            "content_b64": null, "too_large": true
        }))]);
        t.objects
            .lock()
            .unwrap()
            .insert(blob.clone(), b"large body".to_vec());
        let out = run(&t, &cmd, &mut cache(dir.path())).await.unwrap();
        assert_eq!(out.bytes.as_deref(), Some(b"large body".as_slice()));
    }

    #[test]
    fn merge_pretty_renders_conflict_list() {
        let env = RpcResponse::err(AppError::new("vcs.conflict", "merge conflicts"));
        let mut env = env;
        if let RpcResponse::Err { error, .. } = &mut env {
            error.data = Some(json!({
                "conflicts": [
                    { "kind": "both_modified", "path": "src/a.rs" },
                    { "kind": "delete_vs_modify", "path": "b.txt" },
                ]
            }));
        }
        let pretty = merge_pretty(&env, "merge changeset");
        assert!(pretty.contains("conflicts:"));
        assert!(pretty.contains("both_modified src/a.rs"));
        assert!(pretty.contains("delete_vs_modify b.txt"));

        let ok = RpcResponse::ok(json!({ "changeset_id": "cs_m", "fast_forward": true }));
        assert!(merge_pretty(&ok, "merge changeset").contains("cs_m (fast-forward)"));
    }
}
