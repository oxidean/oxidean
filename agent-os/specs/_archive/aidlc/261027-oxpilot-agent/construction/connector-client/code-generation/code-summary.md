# Code Summary — connector-client (U7)

The `ox pilot` connector agent inside the existing `oxidean-cli` binary:
pair a machine against Oxidean's C3 connector gateway, custody the verifier
in the OS keyring (0600 file fallback), and run a claim/heartbeat loop that
executes server-dispatched `exec_shell`/`fs_read`/`fs_write` actions inside
a canonicalized filesystem jail — with capability checks, config-directory
exclusion, process-group cancellation, bounded/scrubbed output, and the
`{0,1}` exit contract. No new crate, no codegen, no RPC procedures.

## New module `crates/oxidean-cli/src/pilot/`

- **`client.rs`** — typed `C3Client` over the four flat-envelope routes
  (`pair` unauthenticated; `claim`/`report`/`heartbeat` bearer-auth).
  Flat `{"error":"<code>"}` decode into a `C3Error` taxonomy
  (unauthorized/forbidden/revoked/disabled/not_found/bad_input/internal/
  rate_limited/pairing_expired/pairing_used/unknown), `Retry-After`
  (204 header) + `retry_after` (200 body) parsing, route-specific timeouts
  (claim = wait+15s, others 30s per device.rs precedent), verbose stderr
  logging that redacts codes/verifiers/bearer material.
- **`custody.rs`** — two-backend verifier seam: keyring composite key
  `pilot/<instance>/<connector_id>` via the existing `keystore` service,
  then `pilot-secrets.json` (0600, atomic tmp→fsync→rename). Zeroize-backed
  secret wrapper; `Debug` prints `[redacted]`. Separate
  `pilot-targets.json` metadata rows (0600, atomic, no verifier fields);
  live `status` probe (keyring/file/missing) without exposing material;
  `load_map_for_scrub` feeds the output-scrub secret set; corrupt files
  refuse loudly on every reader.
- **`exec.rs`** — the action executor + jail. `exec_shell` spawns via the
  platform shell with a stripped environment (`*TOKEN*`/`*KEY*`/
  `*SECRET*`/`*PASS*`/`*PAIRING*` names), its own Unix process group, and
  concurrent stdout/stderr readers folding into a shared 64 KiB tail
  buffer (`…<n> bytes truncated` marker); timeout/cancel kills the whole
  pgid. `fs_read`/`fs_write` run the jail chain — lexical `..` collapse →
  containment → deepest-existing canonicalization → symlink escape refusal
  → config-dir exclusion → regular-file leaf — with a shared 128 KiB raw
  cap and base64 payloads. All reported output passes a value scrub
  (verifier + env-secret values + config tokens + secret-map values,
  ≥8 chars → `[redacted]`). Jail-root relationship checks run at `run`
  launch: root must not be/contain/sit-inside the config dir.
- **`state.rs`** — `RunState` + the nested `tokio::select!` loop: claim
  long-poll, 15s heartbeat timer, in-flight child completion, and OS
  signals all funnels through one termination path. Cancel ids are applied
  before start decisions (`cancelled:pre-start`), kill the running pgid
  (`cancelled:mid-flight`), and dedupe through a settled-id set so nothing
  reports twice. `classify` maps each C3 error per route: auth death →
  kill in-flight + delete creds+row + exit 1; `revoked`/claim-`not_found`
  → creds+row deleted, exit 0; `bad_input` → report-cancel + exit 1;
  `rate_limited` → `Retry-After` verbatim; disabled/internal/transport/
  unknown → `min(60, 2^n×(1±0.2))` jittered backoff. SIGINT/SIGTERM →
  kill in-flight, `cancelled:<sig>`, creds kept, exit 0.
- **`mod.rs`** — the four verbs. `pair` (default label = hostname, default
  caps = the three kinds, one `rate_limited` retry honoring the hint,
  verifier store then metadata row with delete-compensation on row
  failure, never prints either secret). `run` (target resolution — only
  row or `--target`, root resolution + jail checks at launch, zombie
  verifier refusal, loop). `disconnect` (both custody backends + row,
  no server call, idempotent). `status` (metadata rows + live verifier
  probe; `--json` prints a self-describing object, never secrets).
- **`openapi.yaml`** — local C3 contract reference (paths, flat-error
  envelope, auth scheme, action kinds, caps) for readers and review.

## Wiring into the existing CLI

- **`args.rs`** — `PilotPair/PilotRun/PilotDisconnect/PilotStatus` variants,
  parse arms (`--label/--caps/--verbose`, `--target/--root/--wait-seconds/
  --exec-timeout/--verbose`, `--target`, none), HELP entries including the
  `{0,1}` contract, `pilot` in the top-level group list.
- **`commands.rs`** — dispatch arms calling `pilot::{pair,run,disconnect,
  status}`; all four variants return `vec![]` from `required_procedures`
  (C3 routes, not `/api/rpc` — keeps `skill-sync` green).
- **`completion.rs`** — `COMMAND_TREE["pilot"] = pair|run|disconnect|status`.
- **`lib.rs`** — `pub mod pilot`. (`config.rs` gained no new helpers —
  custody derives both paths internally; two candidate helpers were
  added then deleted in review as dead.)
- **`Cargo.toml`/`Cargo.lock`** — `base64 0.23` (fs payloads), `zeroize 1`
  (verifier + secret-map memory hygiene — no `secrecy` crate in the lock),
  unix-only `libc 0.2` (pgid kill + `gethostname`). No other dependency
  changes; `tempfile`/`wiremock` were already present for tests.

## Tests

- **`tests/pilot_connector.rs`** — wiremock suite over all four routes:
  pair wire shape + no-secret-leak, flat-error taxonomy, `Retry-After`
  honored, bearer auth + bodies on claim/report/heartbeat, cancel-set
  application, revocation → clean exit + creds deleted, reconnect/backoff,
  loop termination on auth death.
- **Colocated `#[cfg(test)]`** — jail lexical/canonical escape + symlink +
  config-dir refusals, leaf regular-file enforcement, env-name filter,
  tail-buffer bounds + marker, scrub-set construction, capability refusal,
  backoff math bounds, dispatch-table totality, settled-id dedupe,
  pre-start cancel report, Unix live-child group kill, custody round-trips
  over `tempfile`.
- **Not run locally** — this intent's verification command is the
  CI-deferral pair `git diff --check && git status --porcelain`; cargo
  commands are recorded in `unit-test-instructions.md` as the CI shapes.

## Docs + CI

- **`docs/CONNECTOR.md`** — new runbook: pair→run, jail semantics, bounds
  table, `{0,1}` exit table, custody model, disconnect-is-local-forget.
- **`docs/CLI.md`** — pilot block under Maintenance & local.
- **`docs/CLI-PARITY.md`** — `pilot …` row marked `implemented` (kept out
  of `ox-only` so `check-cli-parity.sh` exercises `ox pilot --help`).
- **`docs/TESTING.md`** — rpc-sync job-table row now names the parity check.
- **`.github/workflows/ci.yml`** — `check-cli-parity` step in the existing
  `rpc-sync` job: `OX_BIN=target/debug/ox ./scripts/check-cli-parity.sh`
  against the rust-bins artifact.

## Deviations

- `secrecy` was unavailable in the lockfile — a `zeroize`-backed wrapper
  with a redacting `Debug` fills the same role (zeroize was already the
  planned dep).
- `status --json` emits a self-describing document rather than routing
  through the RPC envelope, preserving the "no secrets through `--json`"
  rule while honoring the global flag.

## Sources

- `construction/connector-client/functional-design/` (functional-spec,
  rules, entities)
- `construction/connector-client/nfr-design/` (security, reliability,
  performance, logical-components)
- `construction/connector-client/infrastructure-design/`
  (infrastructure-specification)
- `inception/contract-design/contract-summary.md` §C3
- `inception/units-generation/unit-of-work.md` (U7 connector-client
  scope, dependencies, owns-list)
- `inception/requirements-analysis/` + `inception/user-stories/`
  (AC/US/NFR upstream ids)
- shipped consumers: `crates/oxidean-api-oxpilot/src/connector/`,
  `crates/oxidean-cli/src/{args,commands,device,keystore,config,
  completion}.rs`, `crates/oxidean-cli/tests/rpc_http.rs`

## Post-review hardening (iteration-2 READY + supplementary pass)

- `fs_write` tmp creation is `create_new` (`O_CREAT|O_EXCL`) with a
  nanosecond-suffixed name — a jailed child pre-planting a file or
  symlink at the predictable `<pid>` tmp name now fails closed instead
  of being followed out of the jail.
- `fs_read` opens with `O_NOFOLLOW` (unix), closing the leaf-swap TOCTOU
  between `symlink_metadata` and open; the read is bounded by
  `take(FS_IO_CAP+1)` rather than the metadata pre-check.
- The exec `End::Timeout` arm now consults `settled` (no double report
  after a mid-flight cancel) and scrubs the tail before report — same
  as `End::Done`.
- All `Retry-After`-driven sleeps clamp to 1h (loop hints, report
  retries, pair retry-once) — a hostile hint can't hang or panic.
- `fs_write` never auto-creates missing parents (BR4.4); the roundtrip
  test pre-creates and a refusal test pins the behavior.
- `settled` is capped at 4096 with eviction — dedupe survives while the
  server re-announces (≤50/batch), bounded for lifetime connectors.
- `enforce_output_bound` degrades to a bounded truncation instead of
  `assert!` — no reachable panic on a report path.
- Custody chmod failures now abort the write; the containing dir is
  fsynced after rename so the atomic entry survives a crash.
