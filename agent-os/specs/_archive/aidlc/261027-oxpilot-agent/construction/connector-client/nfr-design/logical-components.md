# Logical Components — connector-client (U7)

Consumes: `nfr-requirements/*.md`, `functional-design/{entities,
rules,functional-spec}.md`, `nfr-design-questions.md` Q1-A,
`tech-stack-decisions.md` TS-1..TS-8, shipped
`crates/oxidean-cli/src/{args,commands,config,keystore,device,http}.rs`.

## Module map (`crates/oxidean-cli/src/pilot/`)

| module | owns | consumes |
|--------|------|----------|
| `mod.rs` | hand-rolled `Command` enum variants + usage-registry entries + dispatch arms for `pair`/`run`/`disconnect`/`status`; help text carrying the contract | `args.rs` `Command`/`Flags`/usage registry (no clap — the crate's hand-rolled pattern), `commands.rs` dispatch |
| `client.rs` | typed C3 calls (`pair`/`claim`/`report`/`heartbeat`), `ConnectorError` flat-envelope decode → `C3Error` enum, REQ_TIMEOUT split, Bearer header construction | `http.rs` client conventions, `device.rs` timeout precedent |
| `custody.rs` | `write_verifier`/`read_verifier`/`delete_verifier` two-backend seam, `Backend` enum, `pilot-secrets.json` + `pilot-targets.json` 0600 atomic I/O | `keystore::{set,get,delete}`, `config::config_dir()` |
| `exec.rs` | `exec_shell` spawn (pgid-detached, cwd=root, env-filtered), tail-buffer output, deadline kill, `fs_read`/`fs_write` jail-resolve + atomic write, enumerated-value scrub | `std::process`, platform shells, `config_dir()` |
| `state.rs` | `RunState` (running map, settled set, backoff, canonical root, secret set), `classify(C3Error,route)→Disposition`, `terminate(reason)` exit funnel | all of the above |
| `openapi.yaml` | checked-in client-consumed C3 surface — full flat error vocabulary | contract C3 |

## Wiring

- `args.rs` gains the `pilot` command group (pair/run/disconnect/
  status + per-command `--verbose` on pair/run, `--json` on status
  via the existing Global flag); `commands.rs` gains the dispatch
  arms calling `pilot::mod` entry points.
- `resolve_instance()`/`config_dir()`/`keystore::*`/`http` client —
  consumed unmodified; the connector adds NO new external config.
- `RunState` is the single mutable owner — no locks: the select!
  loop serializes every access (heartbeat arm, claim arm, signal
  arm, exec completion all run on the same task).
- `terminate(reason)` is the only exit path — kill-first →
  report-or-not per cred liveness → custody deletes per the
  terminal table → pinned exit code.

## Data flow (run)

```text
launch: row+verifier load (custody) → canonical root → RunState
loop:   select! { heartbeat.tick | claim_future | signal }
        → apply cancel set (kill/pg + report once via settled)
        → serial exec/fs per claimed action (exec.rs), each action's
          child wait itself a select! {child | EXEC_TIMEOUT |
          heartbeat.tick | signal} so cancels and liveness touches
          land mid-flight
        → report idempotently (client.rs)
        → classify any C3Error → disposition (state.rs)
exit:   terminate(reason) — the funnel
```

## Reused seams (unchanged)

- `keystore::{set,get,delete}(instance, token)` — composite instance
  key is data, not an API change.
- `config::{resolve_instance, config_dir}` — read-only use.
- `device.rs`'s reqwest client/30s-timeout conventions — client.rs
  mirrors them. `Retry-After` header parsing is NEW to the crate
  (device.rs paces via RFC 8628 `interval`/`slow_down` body fields,
  never the header) — client.rs implements it for the 429/204 arms;
  no shared refactor needed.
- `tests/rpc_http.rs` wiremock pattern — `tests/pilot_connector.rs`
  mirrors it.

## Expected new dependencies (TS-8 genuine-gap clause)

| facility | candidate | why existing deps don't cover |
|----------|-----------|-------------------------------|
| SecretString/zeroize-on-drop | `secrecy` (or `zeroize` on a newtype) | no secrecy/zeroize dep anywhere in the workspace |
| fs_* base64 | `base64` | not an oxidean-cli dep today (TS-8 anticipated "e.g. base64") |
| process-group kill | `rustix`/`nix`/`libc` killpg, or `kill -<pgid>` shellout | `Command::process_group` creates the pgid but nothing linked can signal it |
| JSON (metadata/action args) | `serde_json` | already the workspace JSON dep — listed for completeness, not a gap |

No clap/arg-parser dep — pilot rides the crate's hand-rolled
`Command` enum + `Flags` + usage registry.
