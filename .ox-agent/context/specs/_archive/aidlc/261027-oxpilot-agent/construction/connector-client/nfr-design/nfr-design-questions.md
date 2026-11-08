# NFR Design Questions — connector-client

Unit: `connector-client` (U7, kind `service` — greenfield) —
`OxpilotConnector`: `ox pilot pair`/`run`/`disconnect`/`status`, the
claim→execute→report loop, custody seam, jail executor, scrub, and
client-side C3 surface.

## Sources

- [nfr-req] construction/connector-client/nfr-requirements/*.md —
  NFR1.x–NFR6.x/NFR-Sx rows the designs must satisfy.
- [design] functional-design/{entities,rules,functional-spec}.md —
  BR1–BR9 entity/state ground truth (READY iter-3).
- [contract] contract-summary.md — C3 wire shapes, C8 ordering.
- [code] crates/oxidean-cli/src/{args,commands,config,keystore,
  device,http}.rs, crates/oxidean-cli/tests/rpc_http.rs — the shipped
  seams U7 plugs into.

## Q1. Module layout inside `oxidean-cli`?

- A. `src/pilot/` dir: `mod.rs` (dispatch arms + `Command` enum
  variants via args.rs's hand-rolled pattern), `client.rs` (typed C3 calls: pair/claim/
  report/heartbeat + flat ConnectorError decode + REQ_TIMEOUT split),
  `custody.rs` (keystore + pilot-secrets.json two-backend seam +
  pilot-targets.json I/O), `exec.rs` (spawn/jail/cancel/output/scrub),
  `state.rs` (RunState: running map + settled set + backoff), plus
  `src/pilot/openapi.yaml` checked in — rides every existing seam,
  zero new crates.
- B. Single `src/pilot.rs` file — the loop + custody + executor +
  jail + scrub is ~1.5k lines of distinct concerns; one file hides
  the RunState/executor seam the tests need.
- C. New `oxidean-connector` crate — re-implements instance
  resolution/keystore/http/config_dir or drags oxidean-cli in as a
  dep; wrong direction.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — src/pilot/{mod,client,custody,exec,state}.rs + checked-in openapi.yaml; zero new crates, all seams reused

## Q2. Custody seam mechanics — how do the two backends behave end-to-end?

- A. `custody.rs`: `write_verifier(instance, connector_id, verifier)`
  tries `keystore::set("pilot/{instance}/{connector_id}", v)`; on Err
  writes `config_dir()/pilot-secrets.json` (0600, create-or-update
  map entry, tmp + fsync + rename — strictly stronger than
  config.rs::save's non-atomic write; the tmp+rename precedent is
  self_update.rs's binary swap) — Ok(Backend::Keyring|File). `read_verifier`
  tries keyring first, then the file (read order mirrors write
  order). `delete_verifier` attempts BOTH backends (a credential in
  either must die). `pilot-targets.json` is a separate 0600 map —
  custody NEVER reads/writes verifier there; `status` reports which
  backend holds the row's verifier (or `missing`) without touching
  secret bytes.
- B. Config-backed secrets via `config.json` `instances.*.token` —
  pollutes the user-edited file with machine creds and couples
  connector identity to the instance entry's lifetime; Q3-C
  rejected it.
- C. Try keyring, else in-memory only (run-scoped) — a `run` that
  can't persist survives the process but pairing burns the
  single-use code on a machine that can't keep the verifier; BR1.2's
  both-fail abort exists precisely for this.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — custody.rs two-backend write/read/delete with Backend enum; secrets-map atomic write; status reports backend-not-bytes; targets file stays secret-free

## Q3. Run-loop structure — serial claim → execute → report + heartbeat + signals?

- A. `run` loads row+verifier once (WF2.1), canonicalizes `--root`,
  builds RunState; the loop: heartbeat tick (15s tokio interval —
  the crate already links tokio via reqwest) OR claim response,
  whichever arrives first — a `tokio::select!` arms
  `heartbeat.tick()` vs the claim future so cancels arrive on
  either channel; apply `cancel` set (kill running child via
  process-group, report pre-start ids once); execute claimed
  actions SERIALLY (exec/fs) with output cap+scrub — and each
  action's child wait is ITSELF a nested select! {child completion
  \| EXEC_TIMEOUT deadline \| heartbeat tick \| signal} so mid-flight
  cancels and liveness touches land during execution; report
  idempotently; honor retry_after with the 1s cancel-only floor;
  backoff state lives in RunState. SIGINT/SIGTERM via
  `tokio::signal` → same kill+report+exit-0 close-out.
- B. Two threads (claim thread + heartbeat thread) — shared
  RunState needs a mutex around every cancel application; the
  select! loop serializes the same semantics with zero lock
  surface.
- C. Blocking sync loop with a heartbeat inside claim wait — a
  60s claim parks the heartbeat past its cadence; the server
  would read the connector stale while it's actually alive.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — tokio select! loop: heartbeat interval vs claim future; serial exec with a nested per-action select! {child | EXEC_TIMEOUT | heartbeat | signal}; RunState holds running/settled/backoff/root; signals ride the same close-out

## Q4. Error classification and exit-path shape?

- A. `client.rs` returns a typed `C3Error` enum mirroring the flat
  vocabulary (Unauthorized, Forbidden, Revoked, Disabled, NotFound,
  BadInput, Internal, RateLimited{retry_after}, PairingExpired,
  PairingUsed, Transport) — the loop's classifier maps enum →
  action (backoff/kill+delete+exit/drop-continue/honor-hint) per
  BR2.3's table; exit paths funnel through ONE `terminate(reason)`
  that kills in-flight children (process-group), decides whether a
  close-out report is even possible (none on dead creds), runs the
  custody deletes the taxonomy requires, and returns the pinned
  exit code — no scattered process::exit calls.
- B. Match on HTTP status codes at each call site — the exact
  defect R-01 was filed against (several codes share a status).
- C. String error codes matched ad hoc — drifts from the
  enumerated ConnectorError vocabulary; the enum IS the pin.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — typed C3Error enum + single terminate(reason) funnel: kill-first, report-or-not per cred liveness, custody deletes, pinned exit codes

## Q5. Executor mechanics — spawn, cancel, output, scrub?

- A. `exec.rs`: `exec_shell` spawns `sh -c`/`cmd /C` with cwd=root +
  env-clear-filtered (drop OXIDEAN_*TOKEN*/KEY*/SECRET*/PASS* +
  *PAIRING* + verifier), process-group detached (own pgid so
  killpg/taskkill /T hits the whole tree); output collected to a
  bounded tail buffer (64 KiB) via pump threads; EXEC_TIMEOUT via
  kill-after deadline. `fs_*`: lexical resolve → escape/symlink/
  non-regular/config_dir refusals → FS_IO_CAP size-check → read or
  atomic temp+rename write. Scrub pass runs enumerated-value
  substring replace over the final output string before report.
  Cancel consults RunState.running — action_id → ChildHandle.
- B. Reap output after exit without a tail bound — a `yes`-class
  child OOMs the connector before the cap applies; bound the
  buffer, not the readback.
- C. Spawn via `std::process::Command` without a process group —
  `child.kill()` hits the shell only; grandchildren (piped
  pipelines) orphan on cancel — BR5.2's group kill is the pin.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — pgid-detached spawn + bounded tail buffer + deadline kill + jail-resolve + enumerated scrub; cancel via RunState.running map

## Q6. Jail + canonicalization mechanics?

- A. `--root` canonicalized once at launch (must exist + be a dir +
  NOT contain `config_dir()` as itself-or-descendant — refuse at
  startup, not per-op); per-op: `root.join(path)` lexical normalize
  (`.`/`..` fold, reject escapes) → `fs::canonicalize` the resolved
  parent to catch symlink escapes → check canonical result stays
  under canonical root AND outside canonical `config_dir()` →
  `symlink_metadata` on the leaf for the non-regular refusal. Root
  itself may be inside config_dir only if config_dir IS the root —
  still refuse: BR6.3's exclusion is absolute.
- B. String-prefix check on `root/path` — `..` and symlinks defeat
  it; lexical+canonical is the established two-step.
- C. Canonicalize `--root` per-op — TOCTOU window between ops is
  fine to leave (the op-level canonicalize catches escapes), but
  re-canonicalizing the root lets a swapped-out root dir follow a
  moved tree mid-run; pin once.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — root canonicalized once at launch + per-op lexical→canonical→boundary→symlink_metadata chain; config_dir exclusion absolute

## Q7. Test + docs wiring?

- A. `tests/pilot_connector.rs` (wiremock suite: all four routes ×
  wire shapes × each error code → classifier asserts; retry_after
  honored; cancel applied) + `src/pilot/*` unit tests (jail chains,
  env filter, tail buffer, scrub set, settled dedupe, custody
  backends with temp dirs) + `docs/CONNECTOR.md` (or the
  CONFIGURATION section) + `--help` text carrying the exit-code
  contract + `src/pilot/openapi.yaml`; CI rides the existing
  nextest lane — no new job.
- B. Integration tests hitting a spawned oxidean-api — a connector
  CI lane can't assume the forge; fixtures are NFR9's client half.
- C. Defer tests to the build-and-test stage — the wire taxonomy
  is exactly what rots silently; Q7-A pinned fixtures at
  nfr-requirements.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — wiremock C3 suite + no-network pilot unit tests + docs/help carrying the contract + openapi.yaml; existing CI lanes

## Consolidated Summary Confirmation

- Layout: `src/pilot/{mod,client,custody,exec,state}.rs` +
  openapi.yaml; all seams reused (Q1).
- Custody: two-backend write/read/delete, Backend enum, atomic
  secrets-map, secret-free targets file (Q2).
- Loop: tokio select! heartbeat-vs-claim; serial exec; RunState;
  signal close-out (Q3).
- Errors: typed C3Error enum → classifier → single terminate()
  funnel with kill-first + cred-liveness-aware close-out (Q4).
- Executor: pgid-detached spawn, bounded tail, deadline kill,
  jail-resolve chain, enumerated scrub, cancel map (Q5).
- Jail: root canonicalized once; per-op lexical→canonical→
  boundary→non-regular chain; config_dir exclusion absolute (Q6).
- Tests/docs: wiremock suite + unit tests + help/docs contract +
  existing CI lanes (Q7).

[Answer]: Looks correct
