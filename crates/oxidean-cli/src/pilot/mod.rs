//! `ox pilot` — the C3 connector surface.
//!
//! `pair` redeems a UI-minted pairing code for `{connector_id, target_id,
//! verifier}` and custodies the verifier (keyring → `pilot-secrets.json`
//! fallback) plus a `pilot-targets.json` metadata row (verifier first —
//! a row write failure compensates by deleting the verifier).
//!
//! `run` selects one row, probes custody, validates the `--root` jail, then
//! multiplexes claim long-polls against 15s heartbeats and signals
//! (`state::run_loop`). `disconnect` is local-forget ONLY — no server
//! call, the server-side target/grant lives until revoked in the UI.
//! `status` lists rows with a live verifier probe (never secret material).
//!
//! The wire contract is C3 (`pilot/openapi.yaml`): four flat-envelope
//! routes, `Authorization: Bearer <verifier>`, the 10-code error
//! vocabulary — NOT the standard `/api/rpc` envelope.

pub mod client;
pub mod custody;
pub mod exec;
pub mod state;

use std::path::PathBuf;

use serde_json::json;

use crate::args::Global;
use crate::commands::{resolve_instance, CliError, Out, Runtime};
use crate::config;
use client::{C3Error, ConnectorClient};
use custody::{SecretString, TargetRow};
use state::{classify, Disposition, Route};

/// The closed capability set (C3) — pair defaults to the full set and a
/// claim batch is refused per-kind outside `granted_caps`.
pub const DEFAULT_CAPS: &[&str] = &["exec_shell", "fs_read", "fs_write"];

fn cfg_dir() -> Result<PathBuf, CliError> {
    config::config_dir().map_err(CliError::Config)
}

/// `ox pilot pair <code>` — redeem → custody → row. Custody write happens
/// BEFORE the row; a row-write failure deletes the verifier so no zombie
/// is ever created by THIS command.
pub async fn pair(
    g: &Global,
    rt: &Runtime,
    code: &str,
    label: Option<&str>,
    caps: Option<Vec<String>>,
    verbose: bool,
) -> Result<Out, CliError> {
    // The pilot surface pins {0,1} — remap Usage-class errors (2) into
    // the failure code instead of leaking the global usage contract.
    let instance =
        resolve_instance(g, &rt.config).map_err(|e| CliError::Transport(e.to_string()))?;
    let caps: Vec<String> = match caps {
        Some(c) if c.is_empty() => {
            return Err(CliError::Transport(
                "--caps must name at least one capability".to_string(),
            ));
        }
        Some(c) => c,
        None => DEFAULT_CAPS.iter().map(|s| s.to_string()).collect(),
    };
    let label = label
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .unwrap_or_else(exec::hostname_label);
    let client = ConnectorClient::new(&instance, verbose);
    let redeemed = match client.pair(code, &label, &caps).await {
        Ok(r) => r,
        Err(e) => return pair_error(&client, &instance, code, &label, &caps, e).await,
    };
    finish_pair(&instance, redeemed, &label, caps)
}

/// Retry-once for `rate_limited` (hint honored verbatim), then the same
/// outcome mapping — a second rate-limit is the hint to retry later.
async fn pair_error(
    client: &ConnectorClient,
    instance: &str,
    code: &str,
    label: &str,
    caps: &[String],
    e: C3Error,
) -> Result<Out, CliError> {
    if let Disposition::PairRetryOnce(secs) = classify(&e, Route::Pair) {
        tokio::time::sleep(std::time::Duration::from_secs(secs.clamp(1, 3600))).await;
        match client.pair(code, label, caps).await {
            Ok(r) => return finish_pair(instance, r, label, caps.to_vec()),
            Err(e2) => return Err(pair_fail(&e2)),
        }
    }
    Err(pair_fail(&e))
}

fn pair_fail(e: &C3Error) -> CliError {
    match classify(e, Route::Pair) {
        Disposition::PairReject(msg) => CliError::Transport(msg.to_string()),
        Disposition::PairRetryOnce(secs) => {
            CliError::Transport(format!("rate limited again — retry after {secs}s"))
        }
        Disposition::PairFail(msg) => CliError::Transport(msg),
        _ => CliError::Transport(format!("pairing failed ({e})")),
    }
}

/// Custody + row write after a successful redeem (shared by the initial
/// attempt and the rate-limit retry).
fn finish_pair(
    instance: &str,
    redeemed: client::RedeemedPairing,
    label: &str,
    caps: Vec<String>,
) -> Result<Out, CliError> {
    let dir = cfg_dir()?;
    let verifier = SecretString::new(redeemed.verifier);
    let backend = custody::write_verifier(&dir, instance, &redeemed.connector_id, &verifier)
        .map_err(|e| {
            CliError::Transport(format!(
                "{e} — pairing aborted; the server grant expires on its own (re-pair to retry)"
            ))
        })?;
    let row = TargetRow {
        instance: instance.to_string(),
        connector_id: redeemed.connector_id,
        target_id: redeemed.target_id,
        device_label: label.to_string(),
        granted_caps: caps.clone(),
        requested_caps: caps,
        paired_at: custody::now_rfc3339(),
    };
    if let Err(e) = custody::upsert_target(&dir, &row) {
        // Compensation (BR1.2): the verifier dies with its missing row —
        // never a self-created zombie.
        custody::delete_verifier(&dir, &row.instance, &row.connector_id);
        return Err(CliError::Transport(format!(
            "could not write {}: {e} — verifier deleted; re-pair to retry",
            custody::targets_path(&dir).display()
        )));
    }
    // NEVER print the verifier or the pairing code.
    Ok(Out::Report {
        text: format!(
            "paired \"{}\" with {instance}\n  target_id: {}\n  capabilities: {}\n  verifier custody: {}\n\
             run `ox pilot run` to start the connector",
            row.device_label,
            row.target_id,
            row.granted_caps.join(", "),
            backend.label(),
        ),
        json: json!({
            "paired": true,
            "target_id": row.target_id,
            "connector_id": row.connector_id,
            "device_label": row.device_label,
            "granted_caps": row.granted_caps,
            "custody": backend.label(),
        }),
    })
}

/// Row selection for `run`/`disconnect` — `--target` picks one; one row
/// selects itself; several require the flag.
fn pick_row<'m>(
    map: &'m std::collections::BTreeMap<String, TargetRow>,
    target: Option<&str>,
) -> Result<&'m TargetRow, CliError> {
    match target {
        Some(t) => match map.get(t) {
            Some(row) => Ok(row),
            // `--target` accepts a device label per the help text — resolve
            // it only when exactly one row carries it.
            None => {
                let by_label: Vec<&TargetRow> =
                    map.values().filter(|r| r.device_label == t).collect();
                match by_label.as_slice() {
                    [row] => Ok(*row),
                    [] => Err(CliError::Transport(format!(
                        "no paired target \"{t}\" — `ox pilot status` lists known targets"
                    ))),
                    _ => Err(CliError::Transport(format!(
                        "label \"{t}\" matches {} targets — pass the target id instead",
                        by_label.len()
                    ))),
                }
            }
        },
        None if map.len() == 1 => map
            .values()
            .next()
            .ok_or_else(|| CliError::Transport("empty target map".to_string())),
        None if map.is_empty() => Err(CliError::Transport(
            "no paired target — run `ox pilot pair <code>` first".to_string(),
        )),
        None => Err(CliError::Transport(format!(
            "multiple paired targets — pass --target <id> ({})",
            map.keys().cloned().collect::<Vec<_>>().join(", ")
        ))),
    }
}

/// `ox pilot run` — probe custody, validate the jail, then the loop.
pub async fn run(
    rt: &Runtime,
    target: Option<&str>,
    root: Option<&str>,
    wait_seconds: Option<u64>,
    exec_timeout: Option<u64>,
    verbose: bool,
) -> Result<Out, CliError> {
    let dir = cfg_dir()?;
    let map = custody::load_targets(&dir).map_err(|e| CliError::Transport(e.to_string()))?;
    let row = pick_row(&map, target)?.clone();
    // Live probe — the named zombie: row preserved, verifier absent.
    let (_backend, verifier) = custody::read_verifier(&dir, &row.instance, &row.connector_id)
        .map_err(|e| CliError::Transport(e.to_string()))?
        .ok_or_else(|| {
            CliError::Transport(format!(
                "target {} is paired but its verifier is missing — the row is preserved; \
                 `ox pilot disconnect --target {}` clears it, or re-pair",
                row.target_id, row.target_id
            ))
        })?;
    let canonical_root = exec::launch_root(root, &dir).map_err(CliError::Transport)?;
    let canonical_cfg = exec::canonicalize_lenient(&dir);
    let secrets = exec::scrub_secrets(&dir, &verifier, &rt.config);
    let client = ConnectorClient::new(&row.instance, verbose);
    eprintln!(
        "ox pilot: running \"{}\" (target {}) on {}\n  caps: {}\n  jail root: {}\n  ctrl-c to stop",
        row.device_label,
        row.target_id,
        row.instance,
        row.granted_caps.join(", "),
        canonical_root.display(),
    );
    let st = state::RunState::new(state::RunOpts {
        client,
        verifier,
        row,
        cfg_dir: dir,
        canonical_root,
        canonical_cfg,
        secrets,
        wait_seconds: wait_seconds.unwrap_or(state::WAIT_SECONDS_DEFAULT),
        exec_timeout_secs: exec_timeout.unwrap_or(exec::EXEC_TIMEOUT_DEFAULT).max(1),
        verbose,
    })?;
    st.run_loop().await
}

/// `ox pilot disconnect` — LOCAL FORGET only. No server call: the
/// server-side target and connect grant live until revoked in the UI
/// (BR9.1–9.3). Zombie rows (missing verifier) clear the same way.
pub fn disconnect(rt: &Runtime, target: Option<&str>) -> Result<Out, CliError> {
    let _ = rt;
    let dir = cfg_dir()?;
    let map = custody::load_targets(&dir).map_err(|e| CliError::Transport(e.to_string()))?;
    let row = pick_row(&map, target)?.clone();
    custody::delete_verifier(&dir, &row.instance, &row.connector_id);
    custody::delete_target(&dir, &row.target_id).map_err(|e| CliError::Transport(e.to_string()))?;
    Ok(Out::Report {
        text: format!(
            "disconnected \"{}\" (target {}) — local credentials removed\n\
             the server-side grant stays until revoked in the Oxidean UI",
            row.device_label, row.target_id
        ),
        json: json!({"disconnected": true, "target_id": row.target_id}),
    })
}

/// `ox pilot status` — every row + a LIVE probe (present|missing) and the
/// backend label — never secret material, never a stored attribute.
pub fn status(rt: &Runtime) -> Result<Out, CliError> {
    let _ = rt;
    let dir = cfg_dir()?;
    let map = custody::load_targets(&dir).map_err(|e| CliError::Transport(e.to_string()))?;
    if map.is_empty() {
        return Ok(Out::Report {
            text: "no paired targets — `ox pilot pair <code>` to connect this machine".to_string(),
            json: json!({"targets": []}),
        });
    }
    let mut lines = vec![format!(
        "{:<38} {:<24} {:<10} {:<10} {}",
        "TARGET", "LABEL", "VERIFIER", "BACKEND", "INSTANCE"
    )];
    let mut json_rows = Vec::new();
    for row in map.values() {
        let probe = custody::read_verifier(&dir, &row.instance, &row.connector_id)
            .map_err(|e| CliError::Transport(e.to_string()))?;
        let (verifier, backend) = match &probe {
            Some((b, _)) => ("present", b.label()),
            None => ("missing", "-"),
        };
        lines.push(format!(
            "{:<38} {:<24} {:<10} {:<10} {}",
            row.target_id, row.device_label, verifier, backend, row.instance
        ));
        json_rows.push(json!({
            "target_id": row.target_id,
            "connector_id": row.connector_id,
            "instance": row.instance,
            "device_label": row.device_label,
            "granted_caps": row.granted_caps,
            "paired_at": row.paired_at,
            "verifier": verifier,
            "backend": backend,
        }));
    }
    lines.push(
        "\n`missing` = zombie row (verifier gone) — `ox pilot disconnect --target <id>` clears it"
            .to_string(),
    );
    Ok(Out::Report {
        text: lines.join("\n"),
        json: json!({"targets": json_rows}),
    })
}
