# Functional Spec — connector-client (U7)

## Sources

- [unit] `inception/units-generation/unit-of-work.md` §U7.
- [story-map] `unit-of-work-story-map.md` — US4.3 → U7 (U5, U6);
  US4.6 → U7 (U6).
- [contract] `inception/contract-design/contract-summary.md` §C3 —
  the connector HTTP protocol U7 is the client of.
- [req] `inception/requirements-analysis/requirements.md` — FR5.1,
  FR5.3, FR5.4 (client-half custody), FR5.5 (client-half output
  redaction), FR5.6 (revocation honored client-side).
- [domain] `inception/domain-design/components.md` §OxpilotConnector —
  "registers via pairing code, then runs a claim→execute→report loop
  (Actions-runner precedent) with reconnect/backoff. Executes only
  the granted capability set (shell exec, fs read/write — no GUI);
  refuses anything else at parse time. Credential custody stays
  user-side; the client never echoes secrets into results." — U7
  lands this component inside `oxidean-cli`, not as a new process
  kind.
- [stories] US4.6 (AC4.6.1–4.6.3 — full ownership); contributing
  US4.1 (AC4.1.1, AC4.1.2/4.1.3 client halves), US4.3 (AC4.3.1/
  AC4.3.4 execution-half), US4.4 (AC4.4.2 in-flight cancel).
- [u6-fd] `construction/connector-gateway/functional-design/
  functional-spec.md` — server-side semantics the client honors.
- [shipped] `crates/oxidean-cli/src/` — `args.rs` (parser, `Command`
  enum, `(group,sub,leaf)` dispatch, usage registry), `commands.rs`,
  `config.rs` (`resolve_instance`, `config_dir`, `Config`),
  `keystore.rs` (`set`/`get`/`delete` per-instance tokens),
  `http.rs` (reqwest wrapper), `device.rs` (device-grant precedent),
  `tests/rpc_http.rs` + `tests/self_update_http.rs` (wiremock
  fixture pattern — AC4.6.3). No `pilot` code ships — U7 is
  greenfield in `oxidean-cli`.

## Scope

U7 builds `OxpilotConnector` inside the existing `ox` binary: a new
`src/pilot/` module (`mod.rs` — types + custody, `client.rs` — the
C3 HTTP client, `exec.rs` — sandboxed executor, `state.rs` — the
claim loop/backoff/heartbeat), the `ox pilot` command group
(`pair`/`run`/`disconnect`/`status`) wired into `args.rs`'s registry
+ `commands.rs` dispatch, the verifier-in-keystore +
metadata-in-`pilot-targets.json` custody split, and the wiremock
outbound-HTTP fixture suite (AC4.6.3) plus executor unit tests. It
consumes U6's C3 routes; it never invents server behavior. It does
NOT build: action-arg semantics (U5's job — the client validates
only what it needs to execute), consent UI (U8), or any new
deployable (the connector IS `ox`).

## Workflows

### WF1 — Pair (BR1)

1. User mints a code via U8's UI (`oxpilot.pairing_create`) — code
   shown once, 10-min TTL (U6-BR1.1).
2. User runs `ox pilot pair <code>` on the machine to connect
   (optional `--instance`/`--label`/`--caps` — no `--root`: the
   jail root is a `run`-time option, BR1.1).
3. Client resolves instance (Q11-A) → `POST /connector/pair` → on
   `200`: verifier→keystore first, then `pilot-targets.json` row,
   then prints the confirmation WITHOUT the verifier (BR1.2).
   On 4xx: verbatim reason → exit non-zero (BR1.3).
4. The target now exists server-side in `pending_consent` — the
   UI's connect-consent dialog (U8) flips it `connected`; the
   client does nothing further at pair time.

### WF2 — Run loop (BR2, BR4, BR5, BR7, BR8)

1. `ox pilot run [--target <id>] [--root <dir>]` loads the identity
   row + verifier (missing → re-pair hint, exit non-zero — BR9.3),
   prints banner (instance, label, target, caps, root).
2. Loop: `POST /claim{target_id, wait_seconds, capabilities}` —
   `200` → honor `retry_after`; apply `cancel` set (BR5.1); execute
   each action serially (BR4.1) → `POST /report` per action;
   `204` → wait `Retry-After` → re-poll.
3. Parallel timer: `POST /heartbeat{target_id}` every 15s (BR8.5) —
   apply `cancel`/`revoked` like a claim's.
4. Transport failure → BR8.2 backoff. Wire errors key on the
   `{"error":"<code>"}` code (BR2.3): `unauthorized` → BR8.3 death;
   `revoked`/`not_found` (claim/heartbeat) or heartbeat
   `revoked:true` → BR8.4 terminal; `disabled` → backoff, creds
   preserved; `rate_limited` → honor `Retry-After`. SIGINT/SIGTERM →
   BR4.5 close-out (kill + report `cancelled:<signal>`) → exit 0.
5. During an action: a cancel arriving on either channel kills the
   child process group and reports `cancelled:mid-flight` (BR5.2).

### WF3 — Execute one action (BR3–BR7)

1. `kind` outside `granted_caps` → `failed:capability_refused` report
   (BR3.2). Args invalid → `failed:<validation>` (BR3.3).
2. `exec_shell{command}`: spawn under root-cwd + scrubbed env
   (BR4.2); register `action_id → child` in the running map;
   collect output to the 64 KiB tail cap; on exit → report
   `done`/`failed` + exit_code + scrubbed output.
3. `fs_read{path}`: jail-resolve (BR6.1 — escapes + the config_dir
   hard exclusion refused; non-regular files refused per BR4.3),
   then size-check: raw > `FS_IO_CAP` (128 KiB) → report `failed`
   with an explicit too-large message (never a silent head-cut —
   a truncated config read back as `done` is a corruption vector,
   N-02); within cap → `done` with base64 content (≈171 KiB wire,
   under the server's 256 KiB `OUTPUT_MAX`).
4. `fs_write{path, content_b64}`: same resolve rules; decoded >
   `FS_IO_CAP` → `failed` too-large; else decode → atomic write
   (temp + rename) → `done`.
5. Report posts idempotently (BR7.4); a `404` from report = the
   action/session row is gone server-side (session-delete close
   path, U6-BR6.5) → the client drops the action and moves on —
   a dead row needs no retry.

### WF4 — Disconnect & status (BR9)

1. `ox pilot disconnect [--target]` → delete verifier + row →
   confirm. (Local forgetting; server revoke is U8's button.)
2. `ox pilot status [--json]` → print rows, no network, no secret
   fields in any output mode.

## Contract deltas this unit carries

- **C3 — status vocabulary on report**: contract pins
  `status: done|failed`; cancelled rides `failed` + a
  `cancelled:*` output marker (BR5.3). Not an amendment — the
  server's own row transitions on its authority.
- **C3 — `connector_id` vs `target_id`**: server emits one value for
  both names in v1 (U6 fd C3 delta); the client stores both fields
  keyed on `target_id`.
- **C3 — error vocabulary (R-01/R-07)**: the shipped routes answer
  FLAT `{"error":"<code>"}` envelopes with a fuller code set than the
  sketch showed (`unauthorized`, `forbidden`, `revoked`, `disabled`,
  `not_found`, `bad_input`, `internal`, `rate_limited`+Retry-After,
  `pairing_expired`, `pairing_used`); the client's taxonomy keys on
  the code (BR2.3), and the emitted `openapi.yaml` must enumerate
  the full set.
- **C3 — pair 200 carries no caps field (R-03)**: `granted_caps`
  derives as `requested_caps` the server accepted (200 ⇒ subset
  accepted); `claim.capabilities` sends that stored set back.
- **C3 — `args` is string-carrying-JSON (R-04)**: shipped
  `ClaimAction.args: String` (the stored redacted column verbatim) —
  the client double-parses envelope-then-inner; not an amendment,
  the wire reality the sketch's `args` field elides.
- **`openapi.yaml`**: U7's code generation emits the C3 surface as a
  real `openapi.yaml` artifact (the contract sketch's "U7's code
  generation emits the real handlers plus a valid openapi.yaml" —
  the client ships the document, U6 shipped the handlers).
- **Heartbeat cadence**: client-chosen 15s (U6 fd leaves the
  interval to the client within the stale-bound — pinned here).

## Procedure/command deltas

- `args.rs`: new `pilot` group — `pair <code> [--instance]
  [--label] [--caps a,b,c]`, `run [--target] [--root]
  [--wait-seconds] [--exec-timeout <s>]`, `disconnect [--target]`,
  `status` — plus a per-command `--verbose` on `pair`/`run` (request
  logging with `pairing_code`/`verifier`/`Authorization` redacted;
  the Global set has no verbose flag today, `args.rs:244-255`) +
  usage-registry rows. `--json` rides the existing Global flag.
- `commands.rs`: dispatch arms for the four verbs.
- `config.rs`: `pilot_targets_path()` + `pilot_secrets_path()`
  helpers beside `config_path()`.
- `keystore.rs`: no change to the API — the custody seam is
  `keystore::set("pilot/<instance>/<connector_id>", verifier)`
  (composite instance key — the API is instance-scoped), with a
  defined headless fallback: `pilot-secrets.json` in config_dir()
  (0600) when `set` errs (BR2.4). NOT "generic key store" — there
  is none.
- Docs: `ox pilot --help` text (incl. disconnect = local-forget
  wording, R-14) + CONFIGURATION/TESTING rows per the repo's
  command-registration norms.

## Negative paths

- Pair errors by code: `pairing_expired`/`pairing_used`/`forbidden`
  → verbatim reason, exit non-zero; `disabled` → operator-disabled
  message; `rate_limited` → honor Retry-After (BR1.3).
- `run` with no identity → re-pair hint, exit non-zero (BR9.3).
- `run` with >1 identity, no `--target` → list + exit non-zero.
- Caps-violating claimed action → refused + reported, never run
  (BR3.2).
- Jail escape attempts (`..`, symlink) → refused + reported failed;
  `config_dir()` is a hard exclusion regardless of root (BR6.3).
- `unauthorized`/`forbidden` (off-pair) → kill in-flight, delete
  creds, exit non-zero with re-pair hint (BR8.3); `revoked` on
  claim/report or `not_found` on claim/heartbeat → BR8.4 kill +
  delete creds + exit 0; `disabled` → backoff, creds preserved;
  `bad_input` → exit non-zero, creds kept (BR2.3).
- Report-404 → drop the action, continue (WF3.5 — the server's
  session-delete close path means retries are pointless).
- Transport failures → bounded backoff, loop stays alive (BR8.2);
  `internal`/unknown codes ride the same backoff.
- Non-regular fs targets (fifo/device/socket) → refused at resolve,
  reported `failed:not_regular_file` (BR4.3).

## Observability

- `ox pilot run` logs to stderr: banner, claim waits, action
  start/finish (action_id + kind + duration, NEVER args/output/
  verifier), cancel kills, backoff entries, terminal reasons.
- `--verbose` (new per-command flag, declared in the deltas) adds
  request/response envelopes with `pairing_code`, `verifier`, and
  `Authorization` redacted.
- No metrics surface in v1 — the CLI is a leaf process; the server
  side owns audit (U6-BR7.2).

## Pins (constants)

- `CLAIM_WAIT_SECONDS` default 30, client-clamped [1, 60] sent —
  the server clamps the top (`CLAIM_WAIT_MAX_SECS=60`) but a
  `--wait-seconds 0` answers an instant `204 + Retry-After: 0`
  that would spin re-polls at RTT speed (N-06); the client floors
  the flag at 1 (BR2.2).
- `HEARTBEAT_SECONDS` = 15.
- `BACKOFF` 1s → ×2 → 60s, ±20% jitter.
- `CANCEL_ONLY_RE_POLL_FLOOR` = 1s (BR5.4 — the server's
  cancel-pending claim answer is instant, retry_after:0).
- `EXEC_TIMEOUT` = 300s default, `--exec-timeout` overridable (BR7.3's
  "timeouts" reason — R-10).
- `OUTPUT_CAP` = 64 KiB tail on exec combined stdout+stderr.
- `OUTPUT_MAX` = 256 KiB — the server's hard report reject bound
  (`connector/routes.rs:47`); ANY report `output` must stay under
  it post-encode or the server answers `bad_input` (N-02).
- `FS_IO_CAP` = 128 KiB — single bound both directions (N-02/N-07a):
  fs_read raw bytes and fs_write decoded bytes. 128 KiB → base64
  ≈ 171 KiB < `OUTPUT_MAX`; a larger file is a defined
  `failed` outcome, never a truncated `done`.
- `REQ_TIMEOUT`: `pair`/`report`/`heartbeat` = 30s (device.rs
  precedent); `claim` = `wait_seconds + 15s` — the long-poll must
  outlive its own wait or every cycle fake-times-out (R-13).
- keystore key shape `pilot/<instance>/<connector_id>`; metadata
  file `pilot-targets.json` mode 0600; secrets fallback
  `pilot-secrets.json` mode 0600.
