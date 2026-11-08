# Business Rules — connector-client (U7)

## Rule set

### BR1 — Pairing (client half)

- **BR1.1** `ox pilot pair <code> [--instance <i>] [--label <l>]
  [--caps <csv>]` resolves the instance
  (`--instance`/`OXIDEAN_INSTANCE`/`config` via `resolve_instance`
  — Q11-A), POSTs `/api/oxpilot/connector/pair{pairing_code: code,
  device_label: label ?? hostname, requested_caps: caps ??
  [exec_shell, fs_read, fs_write]}` — UNAUTHENTICATED (the code is
  the credential — BR1.2 server side). `--caps` always sends a
  non-empty set (the empty degenerate can't occur — and must not:
  it would starve the claim caps filter + leave granted_caps
  unknowable, R-03's fix).
- **BR1.2** On `200 {connector_id, target_id, verifier}`:
  `granted_caps := requested_caps` — the pair 200 carries no caps
  field (shipped `RedeemedPairing` is `{connector_id, target_id,
  verifier}`); a 200 means the server accepted the request as a
  subset of the minted caps (`forbidden` rejects supersets), so the
  accepted set IS the requested set. Custody write order: verifier
  to its backend FIRST, then upsert
  `pilot-targets.json[target_id]` (metadata only — recording
  granted_caps = requested_caps), then print
  `paired <device_label> on <instance> — target <target_id>,
  caps <granted_caps>`. The verifier is NEVER echoed, logged, or
  returned in any output — including `--json`/`--verbose` (AC4.1.3
  client half; FR5.4). The abort condition is "no custody backend
  available" — keyring Err falls through to the file backend (BR2.4);
  pairing aborts only when BOTH fail (a target row without its
  verifier is a zombie).
- **BR1.3** Pairing errors map by CODE from the flat
  `{"error":"<code>"}` envelope (BR2.3's taxonomy):
  `pairing_expired` → "code expired — mint a fresh one in the UI";
  `pairing_used` → "code already redeemed";
  `forbidden` → "requested caps beyond the pairing's minted bound —
  narrow `--caps` and retry (the SAME code still works — the
  caps check returns before the pairing is claimed, so it stays
  `pending` until TTL; `gateway.rs:1310-1320` vs 1360, N-09e)" —
  an unknown/malformed code maps to `pairing_used`, not `forbidden`
  (`gateway.rs:1268-1279`; N-07d); `disabled` → "oxpilot
  is disabled on this forge — ask
  the operator"; `rate_limited` → honor `Retry-After` then retry
  once, else report the wait; `internal`/transport → retry guidance.
  No retry loop on the dead-code trio — a dead code stays dead.
- **BR1.4** The pairing code itself is a CLI argument — it must never
  be written to any file, echoed in logs, or included in report
  output. `--verbose` request logging redacts the `pairing_code`
  field (same class as verifier).

### BR2 — Wire auth

- **BR2.1** `claim`/`report`/`heartbeat` authenticate with
  `Authorization: Bearer <verifier>` — never `resolve_token`, never a
  user PAT, never a session cookie (C3's rule — verifier-class only).
- **BR2.2** Every request carries the JSON body C3 pins verbatim;
  `wait_seconds` sent client-clamped to [1, 60]: the server clamps
  the top (`CLAIM_WAIT_MAX_SECS=60`) but `0` parks an instant
  `204 + Retry-After: 0` that would spin re-polls at RTT speed — the
  same spin class BR5.4 floors, so the client floors the flag at 1
  (N-06).
- **BR2.3** Wire errors key on the `{"error":"<code>"}` CODE, never
  the HTTP status alone (the shipped routes map several codes to the
  same status — `routes.rs:64,92-99`). The full per-route vocabulary:
  - `unauthorized` (401 — dead/unknown verifier) → BR8.3 auth death.
  - `revoked` (403 — target revoked/disconnected) → BR8.4 terminal:
    kill in-flight, report `cancelled:revoked`, delete creds, exit 0.
  - `disabled` (403 — feature flag/provider off on the forge) →
    NON-terminal: treat as transport-class (BR8.2 backoff, creds
    preserved) — a transient operator toggle must never burn the
    credential fleet.
  - `rate_limited` (429 + `Retry-After`) → honor the header exactly,
    then continue — never counted as a transport failure.
  - `bad_input` (400) → exit non-zero WITHOUT deleting creds: the
    client sent a malformed request (a client bug, not a dead
    credential).
  - `forbidden` (403 on /pair — code rejected) → pairing path only;
    on claim/report/heartbeat it maps like `unauthorized`.
  - `not_found` (404) on claim/heartbeat → the target row is gone:
    BR8.4 terminal semantics (creds dead — delete, exit 0). On
    report → WF3.5 (drop the action, continue).
  - `internal` (500) and unknown codes → BR8.2 transport backoff.
    `oxpilot.*` wire error envelopes are FLAT `{"error":"<code>"}`
    (routes.rs `err()`), NOT the `/api/rpc` `{error:{code,message}}`
    shape — the client parses the flat form exactly.
- **BR2.4** Custody backends, in order: (a) the existing keystore
  seam via the composite instance key —
  `keystore::set("pilot/<instance>/<connector_id>", verifier)`
  (the API is instance-scoped — `set(instance, token)` builds service
  `ox/{instance}` user `"token"`; the `pilot/…` prefix namespaces
  connector creds from user PATs); (b) on `set` Err (headless Linux,
  no Secret Service — the PRIMARY connector deployment), a defined
  file fallback: `pilot-secrets.json` in `config_dir()`, mode 0600,
  map `{ "<instance>/<connector_id>": "<verifier>" }` — a dedicated
  secrets file, NOT config.json (keep machine secrets out of the
  user-edited file). `status`/`run`/`disconnect` read through the
  same fallback order. BR1.2's abort fires only when BOTH backends
  fail. The instance segment in keys is the normalized instance
  string — a URL-ish value, fine as a JSON map key (the file backend
  never uses it as a filename).

### BR3 — Capability enforcement

- **BR3.1** The client executes ONLY kinds inside `granted_caps` from
  its stored identity row — `min(requested, granted)` already applied
  server-side; the client re-checks (Q6-A: the client is the last
  line between a queued action and the user's filesystem).
- **BR3.2** A claimed action whose `kind` is outside `granted_caps`
  is refused WITHOUT executing and reported `failed` with output
  `capability_refused:<kind>` — the refusal is visible, never a
  silent drop (AC4.3.4 spirit).
- **BR3.3** An action whose `kind` is inside caps but whose args fail
  client-side validation (e.g. fs_write content not valid base64) is
  reported `failed` with the validation reason — never executed. The
  wire `args` is a STRING carrying JSON (`ClaimAction.args: String` —
  the stored redacted column verbatim, shipped `oxpilot.rs:683`),
  so validation is a DOUBLE parse: deserialize the claim envelope,
  then parse `args`' inner text to the per-kind shape
  (`{command}` / `{path}` / `{path, content_b64}`). A malformed inner
  string is a validation failure, not a crash.

### BR4 — Execution model

- **BR4.1** Actions execute SERIALLY in claim-response order; a
  report posts before the next action starts (Q7-A). The FIFO claim
  contract gives no parallelism to exploit.
- **BR4.2** `exec_shell` runs `command` via the platform shell
  (`sh -c` unix / `cmd /C` windows) with `cwd = root`, a fresh env
  inheriting the connector's environment MINUS the verifier and minus
  variables matching `OXIDEAN_*TOKEN*`, `OXIDEAN_*KEY*`,
  `OXIDEAN_*SECRET*`, `OXIDEAN_*PASS*`, `*PAIRING*` — the `OXIDEAN_`
  prefix scope is pinned: a bare `*KEY*` glob would strip
  `AWS_SECRET_ACCESS_KEY`-class credentials the CHILD may need,
  over-scrubbing the user's own environment (Q5-A; N-07c — child
  processes can never print the connector's own secrets).
- **BR4.3** `fs_read{path}` resolves `root/path` lexically
  (normalize `.`/`..`, refuse escape or symlink pointing outside
  root — BR6.1), refuses NON-REGULAR files at resolve time
  (fifo/device/socket → `failed:not_regular_file` — a FIFO read can
  block forever and is uncancellable, so the during-execution cancel
  MUST can't be honored on it — R-09), then SIZE-CHECKS before
  reading: raw size > `FS_IO_CAP` (128 KiB — BR6.2) →
  `failed:too_large` with the byte count, NEVER a silently
  head-truncated `done` (a truncated config read back as success is
  a corruption vector — the agent may write it back believing it
  complete, N-02). Within cap → `done` with the content base64'd
  into `output` (≈171 KiB wire — under the server's 256 KiB
  `OUTPUT_MAX`, BR6.2). fs_* cancel granularity is between ops —
  the read/write syscalls aren't mid-op interruptible, which is
  sound because the refusal above bounds every op to a regular file
  of known size.
- **BR4.4** `fs_write{path, content_b64}` resolves the same way
  (including the non-regular refusal — a device/FIFO write has the
  same block-forever hazard) and size-checks the same way: decoded
  > `FS_IO_CAP` → `failed:too_large` (BR6.2, N-09f), then writes
  atomically (temp-in-root + rename) — a torn write never leaves a
  half file. Missing parent dirs are NOT auto-created.
- **BR4.5** `ox pilot run` exits non-zero on auth death, zero on
  `revoked:true`, and NEVER silently on repeated
  transport failures (backoff is bounded but infinite — a dead server
  keeps the loop alive at the 60s ceiling; supervisors see the
  process alive. If the operator wants failure-exit they wrap with a
  watchdog — documented). SIGINT/SIGTERM is a clean terminal WITH
  close-out: kill the in-flight child (process-group kill like
  BR5.2), report it `cancelled:<sigint|sigterm>` so no `running`
  row strands server-side, THEN exit 0 (R-08).

### BR5 — Cancel honoring

- **BR5.1** The `cancel` set from EACH claim/heartbeat response is
  applied before the next action starts, and every cancel id is
  closed by a report EXACTLY ONCE: an unstarted id — including one
  the client NEVER claimed (a claim response lost in transit strands
  a `cancel_requested` row only the client's report can close — the
  server re-announces cancel ids until their rows close, U6-BR5.3) —
  is reported `status:failed`, output `cancelled:pre-start` without
  executing. A `settled` dedupe set in RunState tracks reported ids
  so re-announced cancels aren't re-reported.
- **BR5.2** A cancel for a RUNNING action kills its process group
  (`killpg` unix / `taskkill /T` windows), then reports
  `status:failed`, `output: cancelled:mid-flight` (id enters
  `settled`) — the C3 MUST is honored during execution, not just
  between actions. fs_* kinds can't be killed mid-op — their
  granularity floor is BR4.3's regular-file bound, which makes a
  mid-op block impossible rather than merely unlikely.
- **BR5.4** A CLAIM response carrying ONLY re-announced cancel ids
  (no new work) and `retry_after: 0` still honors the hint — but
  the client floors the re-poll at 1s in the cancel-only case so an
  unclosed strand can't spin the loop at network round-trip speed
  (the server answers instantly whenever a cancel set is pending —
  `CLAIM_RETRY_AFTER_SECS = 0`, `gateway.rs:49,1503-1516`).
  Claim-scoped: `HeartbeatOutcome` carries no `retry_after`
  (`{revoked, cancel}` only — `oxpilot.rs:703-709`); heartbeat
  cadence is BR8.5's fixed 15s and needs no floor (N-07b).
- **BR5.3** Note on status vocabulary: C3's report pins
  `status: done|failed` — cancelled rides `failed` + a `cancelled:*`
  output marker in v1 (the server's own state machine already
  records `cancelled` when the close-out lands; the wire enum stays
  the contract's).

### BR6 — Sandbox

- **BR6.1** All fs_* paths resolve under `--root` (default: cwd at
  launch, canonicalized): lexical normalization + `..` escape refusal
  + symlink-realpath recheck (a `root/link → /etc` symlink is an
  escape, refused). Relative args in `exec_shell` commands run under
  cwd=root but the SHELL is not path-jailed (documented — v1 exec is
  the user's own machine; the jail covers fs_* deterministically and
  exec's cwd).
- **BR6.2** `FS_IO_CAP` = 128 KiB — ONE constant both directions
  (the old `READ_CAP`/`FS_IO_CAP` dual names are unified, N-07a):
  fs_read raw bytes and fs_write decoded bytes. The bound is
  wire-derived, not arbitrary: the server hard-rejects report
  `output` > `OUTPUT_MAX` = 256 KiB (`connector/routes.rs:47`),
  base64 inflates ~4/3, so 128 KiB raw ≈ 171 KiB wire sits safely
  under it — a `done` can never provoke the `bad_input` → non-zero
  exit that would strand the action row (N-02). Order pinned: cap
  on RAW bytes BEFORE base64 encode. Oversized reads/writes are a
  defined `failed:too_large` outcome carrying the byte count —
  chunked reads are a U5 concern, not the client's.
- **BR6.3** The verifier never enters a child env. The jail has a
  HARD exclusion regardless of `--root`: `config_dir()` and
  everything under it — `pilot-targets.json`, `pilot-secrets.json`,
  `config.json` (which holds user PATs under `instances.*.token` via
  the keystore file fallback — `config.rs:33-42`), the OS keystore
  file if it lives there. A `fs_*` path resolving inside the config
  dir is refused like an escape (`failed:jail_config`). Stored
  Oxidean secrets must never reach report output by ANY path
  (FR5.4's mandated rule); exec_shell's cwd isn't path-jailed so a
  `cat config.json` is a disclosed residual — mitigated by BR7.2's
  value scrub (the values themselves get scrubbed wherever they
  appear), not by refusing the path.

### BR7 — Output contract

- **BR7.1** `output` on exec_shell is the last ≤ 64 KiB of combined
  stdout+stderr (UTF-8, lossy-trimmed to a char boundary) with a
  `…<n> bytes truncated` marker prepended when capped. On fs_* the
  field carries the base64 payload (BR4.3/BR4.4) or a failure
  reason — never partial content. Universal bound: whatever the
  path, the final `output` string stays under the server's
  `OUTPUT_MAX` (256 KiB) — a report the server can't accept is a
  stranded row (N-02).
- **BR7.2** Before send, the output is value-scrubbed: the verifier,
  every secret value the CLIENT itself can enumerate — child-env
  values matching `*TOKEN*`/`*KEY*`/`*SECRET*`/`*PASS*`, stored
  `config.json` `instances.*.token` values, `pilot-secrets.json`
  values — is substring-replaced with `[redacted]` (values ≥ 8 chars
  only, so a common word like "token" doesn't shred output). Cheap,
  honest, catches the enumerated leak paths including exec_shell's
  `cat config.json` residual (R-05); the server-side redactor is the
  second pass — FR5.5.
- **BR7.3** `exit_code` present only when a process exited; spawn
  errors, timeouts, jail refusals, caps refusals, and cancels carry
  their reason in `output` with `status:failed`.
- **BR7.4** No streaming output in v1 — one report per action,
  idempotent by action_id (retries repost the identical body).

### BR8 — Backoff & lifecycle

- **BR8.1** `retry_after` (200 body) and `Retry-After` (204 header)
  are honored EXACTLY — the server is the pacing authority; absent a
  hint the client re-polls after 1s.
- **BR8.2** Transport errors (connect refused, reset, TLS, 5xx
  without a hint): exponential backoff 1s → ×2 → 60s ceiling, ±20%
  jitter, reset to 1s on the first success — documented in `--help`
  and CONFIGURATION (AC4.6.2).
- **BR8.3** `unauthorized` (and `forbidden` off-pair, per BR2.3) →
  auth death: kill any in-flight child FIRST (process-group kill,
  BR5.2) — no orphan survives the process exit — and NO report is
  attempted for it (the verifier is already dead server-side; a
  doomed report just delays the exit, N-04). Then delete verifier +
  identity row, print `verifier rejected — re-pair with a fresh
  code`, exit non-zero. The `bad_input` non-zero exit (BR2.3) kills
  in-flight children the same way but PRESERVES creds — the pinned
  asymmetry is deliberate: `bad_input` is a client-side wire bug
  (credential still valid), `unauthorized`/`forbidden` means the
  credential itself is dead (N-07e). Revocation is NOT auth death —
  it routes through BR8.4 (a `403 {error:"revoked"}` on claim/report
  means the target is revoked/disconnected, not that the credential
  is malformed).
- **BR8.4** Terminal-on-revocation: heartbeat `revoked:true`, a
  `revoked` wire error on claim/report, or a `not_found` wire error
  on claim/heartbeat (the claim 200 shape has NO revoked field —
  revocation reaches the client via the heartbeat field or an error
  code; route-scoped per BR2.3: report's `not_found` is WF3.5's
  drop-and-continue — the ACTION row is gone but the identity
  resolved, so creds stay live, N-01) → kill any running action
  first (report it `cancelled:revoked`), delete verifier + identity
  row, exit 0 — revocation is success, not failure (FR5.6 honored
  client-side). Exit-0 matters: a supervisor's `Restart=on-failure`
  would crash-loop a non-zero exit into BR9.3's dead-creds path.
- **BR8.5** Heartbeat fires every 15s on its own timer between claim
  cycles (the server's stale-bound is longer; 15s keeps last_seen
  fresh without chattiness) — a heartbeat result's `cancel`/`revoked`
  set is applied exactly like a claim's.

### BR9 — Local management

- **BR9.1** `ox pilot disconnect [--target <id>]`: deletes the
  verifier + identity row for the (single or named) target, prints
  confirmation. One row → deletes it; multiple rows + no `--target`
  → "multiple targets — pass --target", listing them (same shape as
  BR9.3's run guard, N-07f). Help text and docs say it plainly:
  disconnect is LOCAL FORGET ONLY — the verifier hash stays valid
  server-side until the owner revokes/disconnects the target in the
  UI, so a recovered credential backup still authenticates (R-14;
  server-side severing is `oxpilot.target_disconnect`, an owner
  proc). Symmetric consequence: running `disconnect` while a `run`
  process is up does NOT affect the live process — credentials were
  loaded once at launch (WF2.1) and the server still honors the
  verifier; the forget takes effect at the NEXT launch (N-05).
- **BR9.2** `ox pilot status [--json]`: prints the stored identity
  rows (instance, label, target_id, caps, paired_at) — no network, no
  verifier material even in `--json` (the JSON shape simply omits the
  secret field).
- **BR9.3** Missing/empty `pilot-targets.json` on `run` →
  "no paired target — run `ox pilot pair <code>` first", exit
  non-zero. Multiple rows and no `--target` → "multiple targets —
  pass --target", listing them. Row present but verifier missing
  from BOTH custody backends (keyring externally cleared and no
  `pilot-secrets.json` entry — N-07g) → the row is a zombie:
  `credentials missing for <target_id> — re-pair or
  ox pilot disconnect --target <id>`, exit non-zero, identity row
  PRESERVED (the server-side grant may still be live — local
  deletion would strand it; the user picks forget or re-pair).

## Notes

- BR numbering is U7-local (BR1–BR9) — the gateway's BR1–BR7 are the
  server side of the same contract; cross-references say "U6-BRx.y".
- Cancel-vocabulary note (BR5.3) is a deliberate contract-local
  reading, not an amendment: the server's action row transitions on
  its own authority; the wire enum stays `done|failed`.
