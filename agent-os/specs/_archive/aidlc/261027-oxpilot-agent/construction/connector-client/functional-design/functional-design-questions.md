# Functional Design Questions — connector-client

Unit: `connector-client` (U7, kind `service`) — owns `OxpilotConnector`:
`ox pilot` subcommands in the `oxidean-cli` crate — the pairing flow
(`pair`), credential custody user-side, the claim→execute→report loop
with documented backoff, a local exec/fs sandbox honoring the granted
capability set, cancel handling, and no secret echoing into results.
depends_on: U6 (`ConnectorGateway` + the C3 HTTP routes `/pair`,
`/claim`, `/report`, `/heartbeat`). U5 supplies the action args the
gateway queues; U8 renders pairing-code mint and target status in the
UI.

## Sources

- [unit] `inception/units-generation/unit-of-work.md` §U7 — "owns
  `OxpilotConnector`: `ox pilot connect`/`pair`/`disconnect` subcommands
  in the `ox` crate — pairing flow, credential custody user-side,
  claim→execute→report loop with documented backoff, local exec/fs
  sandbox honoring the granted capability set, cancel handling, no
  secret echoing into results. depends_on: U6."
- [story-map] `unit-of-work-story-map.md` — US4.3 → U7 (U5, U6): "exec/fs
  tools dispatch via U5 → U6 queue → U7 runs them"; US4.6 → U7 (U6):
  "`ox pilot` client + dial-out".
- [contract] `inception/contract-design/contract-summary.md` §C3 —
  connector HTTP protocol: `POST /pair {pairing_code, device_label,
  requested_caps}` → `{connector_id, target_id, verifier}` (shown once,
  stored by connector); `POST /claim {target_id, wait_seconds<=60,
  capabilities}` → `200 {actions:[{action_id, kind, args, grant_id}],
  cancel:[action_id], retry_after}` or `204` + `Retry-After` (clamp 60);
  `POST /report {action_id, status:done|failed, exit_code?, output}` →
  `{ack}` (idempotent by action_id); `POST /heartbeat {target_id}` →
  `{revoked, cancel}`. Verifier-class Bearer auth — never session or
  PAT. Cancel sets MUST be honored before starting and during
  execution.
- [req] FR5.1 (connector dials out — no inbound listener), FR5.3
  (capability bound: exec_shell + fs_read/fs_write only; no GUI), FR5.4
  (verifier hashed server-side; user-side custody is the connector's
  own file — agent-reachable surfaces never return it), FR5.5 (reported
  output is secrets-redacted before write), FR5.6 (revocation immediate
  — heartbeat `revoked:true` exits the loop).
- [stories] US4.6 → AC4.6.1 (authenticates via pairing flow, maintains
  outbound channel, exposes only exec/fs), AC4.6.2 (PAT-class auth with
  hashed verifier material; reconnect/backoff documented), AC4.6.3
  (protocol surface covered by outbound-HTTP fixture tests); US4.3
  contribution → AC4.3.1 (runs actions only with live grant — the
  server enforces; client executes what claim returns), AC4.3.4
  contribution (client surfaces target state), AC4.4.2 contribution
  (cancel set kills in-flight action); US4.1 contribution → AC4.1.1
  (dial-out only), AC4.1.2 client half (device-grant-style code →
  credentials), AC4.1.3 client half (custody of returned verifier).
- [u6-fd] `construction/connector-gateway/functional-design/
  functional-spec.md` — gateway-side semantics U7 must honor: report
  answering 404 → kill the action locally (session-delete close path,
  BR6.5/WF5); heartbeat interval is client-chosen within a documented
  bound; `wait_seconds` clamped server-side; pairing code single-use;
  U7 emits `openapi.yaml` as a code artifact for the C3 surface.
- [shipped] `crates/oxidean-cli/src/` — `args.rs` (hand-rolled parser,
  `enum Command`, `(group, sub, leaf)` dispatch, usage registry),
  `commands.rs` (dispatch impls), `config.rs` (instances + config dir,
  `resolve_instance`, `resolve_token`), `keystore.rs` (per-instance
  token store: keychain/file), `http.rs` (reqwest client wrapper),
  `rpc.rs` (`/api/rpc` envelope client), `device.rs` (device-code grant
  precedent — pairing-adjacent flow), `tests/rpc_http.rs` +
  `self_update_http.rs` (wiremock MockServer fixture pattern —
  AC4.6.3's outbound-HTTP coverage rides this). No `pilot` module or
  command group exists — U7 is greenfield inside oxidean-cli.

## Q1. Module placement — where does the connector live in `oxidean-cli`?

- A. New `src/pilot/` module dir (`mod.rs` + `client.rs` + `exec.rs` +
  `state.rs`): the connector is a multi-part subsystem (HTTP client,
  claim loop, executor, custody) — a single `pilot.rs` file would be
  ~1k lines, against the crate's one-file-per-surface norm where
  `commands.rs`/`args.rs` are already 4–5k lines.
- B. Single `src/pilot.rs`: smallest diff, but the claim loop +
  executor + custody + HTTP types would crowd one file past the
  crate's own readability bar.
- C. New workspace crate `oxidean-oxpilot-connector`: cleanest
  isolation but adds a crate + versioning component for ~1k lines, and
  the unit pins the subcommands *in* the `ox` binary — a crate would
  still have to be consumed by `oxidean-cli`, adding ceremony without
  changing the deployable.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — new `src/pilot/` module dir (mod/client/exec/state): the connector is a multi-part subsystem; a single file would crowd the crate's readability bar. Conductor-decided under standing delegation; ratify at PR review.


## Q2. Subcommand tree — the user-facing verbs?

- A. `ox pilot pair <code>` + `ox pilot run` + `ox pilot disconnect` +
  `ox pilot status`: `pair` exchanges the code for credentials
  (one-shot); `run` is the long-lived foreground claim→execute→report
  loop; `disconnect` forgets local credentials (server-side revoke
  stays owner-RPC territory); `status` prints the stored target/caps
  without connecting. Matches the unit's `connect/pair/disconnect`
  naming with `run` carrying the connect+loop semantics.
- B. `ox pilot connect <code>` as one verb doing pair+run: fewer
  verbs, but pairing once and re-running later (the common case —
  credentials persist) would need a separate verb anyway, and
  conflating exchange with the loop hides the credential lifecycle.
- C. `ox connector *`: names the subsystem not the feature — `ox
  pilot` binds the UX to the product name per the unit's own wording.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — `ox pilot pair|run|disconnect|status`: `pair` exchanges the code one-shot; `run` carries the long-lived claim loop; `disconnect` forgets local creds; `status` inspects stored state. Matches the unit's own verb naming. Conductor-decided under standing delegation; ratify at PR review.


## Q3. Credential custody — where do `connector_id`/`target_id`/
`verifier` live user-side?

- A. A dedicated `pilot-targets.json` in the CLI config dir (0600)
  for metadata, verifier via the existing `keystore` seam under the
  composite instance key `pilot/<instance>/<connector_id>` — the API
  is instance-scoped (`set(instance, token)`), and on `set` Err
  (headless, no Secret Service) a defined file fallback lands in
  `pilot-secrets.json` (0600): secrets ride the established custody
  seam while the non-secret metadata stays inspectable JSON.
  Multiple targets per instance survive because the file is a map
  keyed by target_id.
- B. Everything in `pilot-targets.json` including the verifier (0600):
  simpler — one artifact — but the verifier then sits beside metadata
  in a plaintext file on disk; the keystore seam exists precisely to
  avoid that.
- C. Reuse `config.json` instance entries: the config schema is
  user-edited by hand; mixing machine-minted connector rows into it
  invites corruption and the schema doesn't model target/caps
  metadata.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — `pilot-targets.json` metadata (0600) + verifier via the keystore seam under composite instance key `pilot/<instance>/<connector_id>` with a defined `pilot-secrets.json` (0600) headless fallback: secrets ride the established custody seam; metadata stays inspectable. Conductor-decided under standing delegation; ratify at PR review.


## Q4. Run model — how does `ox pilot run` behave?

- A. Foreground loop, one process, no self-daemonizing: print a
  banner (target, caps, log verbosity) then loop claim→execute→report
  with heartbeat on its own interval; SIGINT/SIGTERM exits cleanly.
  Restart/supervision is the operator's business (systemd, docker
  restart policy) — the unit documents it, doesn't implement it.
- B. `--daemon` flag self-backgrounding: pid files, log redirection,
  and stop semantics balloon scope for v1; the documented way to
  daemonize is the platform's supervisor.
- C. One-shot `--once` mode only (claim once, execute, exit): useful
  for tests but useless as the shipping model — the story is a
  maintained outbound channel.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — foreground loop, one process, clean SIGINT/SIGTERM exit; supervision is the operator's (systemd/docker) — documented, not implemented. Conductor-decided under standing delegation; ratify at PR review.


## Q5. Sandbox boundary — what constrains `fs_read`/`fs_write`/`exec_shell`?

- A. A `--root <dir>` jail (default: cwd at launch): fs_* paths resolve
  inside it (lexical normalize + `..`/symlink-escape refusal);
  exec_shell runs with `cwd=root`; the child env drops the verifier
  and the connector's own secret-pattern vars (`OXIDEAN_*TOKEN*`,
  `OXIDEAN_*KEY*`, `OXIDEAN_*SECRET*`, `OXIDEAN_*PASS*`, plus bare
  `*PAIRING*` — the OXIDEAN-prefixed scope keeps the child's own
  credentials intact, BR4.2) so the connector's secrets can never be
  printed by a child. This is honest sandboxing for v1 — OS-level
  isolation (namespaces/seccomp) is platform-specific and out of
  scope.
- B. No jail — actions run with the connector's ambient fs/exec
  rights: simplest, but a granted `fs_read` could then read the
  connector's own credential file or any path on the host — fails the
  "honors the granted capability set" wording and turns prompt-level
  injection into filesystem exfiltration.
- C. Interactive approval prompt per action in the terminal: breaks
  the unattended-connector story (the consent model lives on the
  server — action grants are answered in the UI; a second terminal
  prompt would double-consent and block headless operation).
- D. Not decided.
- X. Other (please specify)

[Answer]: A — `--root <dir>` jail (default cwd): lexical normalize + `..`/symlink-escape refusal; exec cwd=root; verifier never enters child env. OS-level isolation is out of scope for v1. Conductor-decided under standing delegation; ratify at PR review.


## Q6. Capability enforcement — which set does the client honor?

- A. `min(requested_caps, granted)` — the client sends `requested_caps`
  at pair time, stores the granted set from the pair response, and
  refuses any claimed action kind outside it before executing (a
  defense-in-depth check — the server shouldn't send them, a
  compromised server shouldn't be able to make the client exceed its
  own declared bound).
- B. Trust the server: execute whatever claim returns. Violates
  fail-closed — the client is the last line between a queued action
  and the user's filesystem.
- C. Honor `requested_caps` only: if the server narrowed the grant,
  the client would still act as if the wider set were live —
  misrepresents the effective permission.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — `min(requested_caps, granted)`: refuse claimed action kinds outside the granted set before executing — the client is the last line against a queued/compromised action. Conductor-decided under standing delegation; ratify at PR review.


## Q7. Execution concurrency — how many actions at once?

- A. Serial: execute actions from each claim response in order, one at
  a time; report before starting the next. The claim contract is FIFO;
  parallel execution would reorder `running`/`done` reports and
  complicate cancel handling for zero v1 benefit.
- B. Bounded parallel (N=4): faster but interleaves reports, needs
  per-action cancel routing while children run, and the contract's
  FIFO ordering gives no parallelism guarantee to exploit.
- C. Fully async spawn-per-action: runaway risk on a user's machine,
  cancels can't be honored deterministically.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — serial execution per claim response; report before next. FIFO contract gives no parallelism to exploit; cancel stays deterministic. Conductor-decided under standing delegation; ratify at PR review.


## Q8. Cancel handling — how do cancel sets kill in-flight work?

- A. Check the cancel set before each action starts (drop unstarted
  cancelled ids, report `cancelled`) + keep a live map of running
  `action_id → child` so a heartbeat/claim cancel kills the process
  group (`kill`/`taskkill`) and reports `cancelled` — honoring
  "before starting and during execution" (C3 rule).
- B. Check only between actions: an in-flight `sleep 600` would run to
  completion after revoke — fails AC4.4.2's "immediate" and the C3
  MUST.
- C. Cooperative cancel token read by the executor between output
  chunks, no signal: portable but can't interrupt a blocked child
  read/write — the kill is required anyway.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — pre-start cancel check + live action_id→child map; heartbeat/claim cancel kills the process group and reports `cancelled`. Honors the C3 before-and-during MUST. Conductor-decided under standing delegation; ratify at PR review.


## Q9. Output contract — what goes back in `report.output`?

- A. Bounded UTF-8 tail (cap ~64 KiB — keeps the last N bytes with a
  `truncated` marker in the payload), `exit_code` when the process
  exits, `status: failed` on spawn errors/timeouts/caps-refusal
  carrying the reason in output. Client-side redaction: the verifier
  plus every secret value the client can itself enumerate —
  `*TOKEN*`/`*KEY*`/`*SECRET*`/`*PASS*` env values,
  `config.json` `instances.*.token` values, `pilot-secrets.json`
  values (≥8 chars) — scrubbed before report (defends the "no secret
  echoing" unit line without trying to detect arbitrary secrets —
  that's the server-side redactor's job).
- B. Stream stdout back live via chunked reports: C3's report is
  idempotent-by-action_id — a streaming variant would need a
  contract amendment for sequence numbers; v1 keeps one report per
  action.
- C. No cap: unbounded output can blow the request body and the DB
  row — the server bound exists; the client should be the tighter
  side.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — ~64KiB UTF-8 tail with truncated marker, exit_code, failed-status reasons; client-side scrub of verifier + enumerated secret values (env patterns, config.json tokens, pilot-secrets.json) before report — BR7.2's full set. Conductor-decided under standing delegation; ratify at PR review.


## Q10. Backoff/reconnect — the documented policy (AC4.6.2)?

- A. Claim loop: honor `retry_after`/Retry-After exactly; on transport
  error exponential backoff 1s→2s→…→60s max with ±20% jitter, reset on
  success; wire errors key on the flat `{"error":"<code>"}` CODE —
  `unauthorized`/off-pair `forbidden` → kill in-flight, delete creds,
  exit non-zero; `revoked`/claim-or-heartbeat `not_found` → kill,
  report, delete creds, exit 0; `disabled`/`internal`/unknown →
  backoff with creds preserved; `rate_limited` → Retry-After;
  `bad_input` → exit non-zero, creds preserved (BR2.3 taxonomy —
  supersedes the original blanket 401/403 reading); heartbeat every
  15s (within the server's stale-target
  bound) sharing the same backoff; `revoked:true` → delete local
  credentials + exit 0. All bounds named in `--help` and docs.
- B. Fixed 5s retry forever: simple but hammers a downed server and
  hides auth-death — a revoked target would retry forever instead of
  exiting.
- C. Delegate restart to supervisor — exit nonzero on ANY error:
  too brittle; a transient network blip shouldn't need systemd to
  flap the service.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — honor retry_after/Retry-After; transport-error exponential 1s→60s ±20% jitter reset on success; error-code-keyed terminals (unauthorized→auth death, revoked/not_found→forget+exit 0, disabled→non-terminal backoff, bad_input→exit creds-kept — BR2.3); heartbeat 15s. Conductor-decided under standing delegation; ratify at PR review.


## Q11. Instance/config resolution — how does the connector pick a forge?

- A. Ride the existing seams: `--instance <name|url>` /
  `OXIDEAN_INSTANCE` via `resolve_instance`, config-dir discovery via
  `config_dir()`. Pairing needs no user login (the code IS the
  auth); run/report/heartbeat auth = the stored verifier Bearer —
  deliberately NOT `resolve_token` (connector creds are
  verifier-class, never a user PAT — C3 rule).
- B. Require `--instance` every invocation: consistent but hostile —
  pairing is a one-time act; forcing re-entry on `run` breaks
  systemd units that shouldn't embed the URL twice.
- C. New `ox pilot config` writer subcommand: duplicates
  `resolve_instance`'s job; the stored target row already records
  which instance it paired against.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — `--instance`/`OXIDEAN_INSTANCE` via `resolve_instance`, config-dir discovery via `config_dir()`; connector auth is the stored verifier Bearer, never `resolve_token` PAT material (C3 rule). Conductor-decided under standing delegation; ratify at PR review.


## Q12. Test coverage — what do the AC4.6.3 fixtures pin?

- A. wiremock suites mirroring `tests/rpc_http.rs`: pair happy/4xx,
  claim 200 (actions+cancel parse) /204+Retry-After /clamped wait,
  report idempotent shape, heartbeat revoked/cancel decode, Bearer
  header assertion, backoff timing (injected clock or short bounds),
  + executor unit tests (jail escape refusal, cancel-kill, output
  cap, env scrub) with no network.
- B. E2E against a live forge: heavier than the unit needs and
  duplicates U6's server-side integration coverage — the client's
  contract surface is outbound HTTP.
- C. Mock the transport trait only: tests the loop but not the wire —
  AC4.6.3 pins "outbound-HTTP fixture tests", wiremock is that.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — wiremock suites mirroring tests/rpc_http.rs (pair/claim/report/heartbeat wire shapes, Bearer assertion, backoff bounds) + executor unit tests (jail escape, cancel-kill, output cap, env scrub) with no network. Conductor-decided under standing delegation; ratify at PR review.


## Consolidated Summary Confirmation

Decisions recap:

- Layout: `src/pilot/` module dir in `oxidean-cli` — mod + client +
  exec + state (Q1).
- Verbs: `ox pilot pair` / `run` / `disconnect` / `status` (Q2).
- Custody: verifier via keystore seam (composite instance key
  `pilot/<instance>/<connector_id>`) + `pilot-secrets.json` headless
  fallback; metadata in `pilot-targets.json` 0600 (Q3).
- Run model: foreground claim→execute→report loop, clean signals,
  supervisor documented not implemented (Q4).
- Sandbox: `--root` jail, path-escape refusal, exec cwd=root (Q5).
- Caps: `min(requested, granted)`, refuse outside kinds before exec
  (Q6).
- Concurrency: serial, FIFO, report-before-next (Q7).
- Cancel: pre-start check + kill process group mid-flight, report
  `cancelled` (Q8).
- Output: ~64 KiB exec tail + truncation marker, fs ≤ FS_IO_CAP
  (128 KiB raw), scrubbed with BR7.2's full secret-value set (Q9).
- Backoff: retry_after honored, exponential 1s→60s ±20% jitter,
  error-code-keyed terminals (unauthorized→death, revoked→exit 0,
  disabled→retry, bad_input→exit creds-kept), heartbeat 15s (Q10).
- Config: `resolve_instance`/`config_dir()` seams; verifier Bearer,
  never PAT (Q11).
- Tests: wiremock wire-shape suites + executor unit tests (Q12).

Does this all look correct before I write the functional-design
artifacts?

- Looks correct
- Request changes

[Answer]: Looks correct
