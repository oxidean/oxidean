### Architecture Review — code-generation (connector-client)

### Review

**Reviewer:** aidlc-architecture-reviewer-agent
**Verdict:** NOT-READY
**Iteration:** 1

Adversarial review of the uncommitted `ox pilot` (U7) connector output
against the approved functional/NFR/infrastructure designs, the C3
contract sketch, and the shipped U6 server ground truth
(`crates/oxidean-api-oxpilot/src/connector/{routes,gateway}.rs`).
Review-only; no builds run, no implementation files touched.

### Review coverage

| Dimension | Result |
|---|---|
| Wire correctness (flat-envelope decode, 10 codes, Retry-After 429+204, `retry_after` body, Bearer-only, timeouts, `[1,60]` clamp, args double-parse) | PASS — `client.rs` matches shipped C3 |
| Dispatch totality (10 codes + Transport + Unknown × 4 routes) | PASS — `state.rs:73-121` + `dispatch_table_totality` test |
| Custody (keyring key shape, 0600 atomic fallback, read shape, dual delete, verifier-then-row + compensation, corrupt-file named errors, no verifier in rows/logs/json/env) | PASS with one Minor (`set_permissions` result discarded) |
| Run loop (outer select hb|claim|signal; nested select child|timeout|hb|signal; `running`≤1 insert-before-spawn; `settled` edges; cancel exactly-once; `terminate` sole funnel; {0,1} exits) | FAIL — `claim_fut` await does not compile (R-02); SIGINT receiver is transient (R-05); `settled` never evicts (R-06) |
| Jail & execution (both-direction launch check, lexical+canonical chain, `symlink_metadata` leaf, env strip, 64 KiB tail, 128 KiB cap, atomic write, ≥8-char scrub, killpg/taskkill, spawn_blocking) | FAIL — `hostname_label` does not compile (R-01); `fs_write` auto-creates parents vs BR4.4 (R-03); `fs_read` metadata-then-read race (R-07/R-08) |
| Caps & bounds (256 KiB / 64 KiB / 128 KiB / 300s / 15s / jitter-before-clamp / ≤50 dequeue / 600s TTL) | PASS — constants match server (`gateway.rs`) and spec |
| Registration (exhaustive parse/help, completion arms, `required_procedures`→`vec![]`, honest help) | PASS |
| Docs/CI honesty (CONNECTOR.md, CLI-PARITY `implemented`, one step inside `rpc-sync`, `openapi.yaml` vs shipped C3) | FAIL — parity row invisible to the probe's awk (R-04); minor doc nits |
| Compiles-by-inspection | FAIL — R-01 + R-02 are each fatal |

### New findings

| ID | Severity | Location | Finding | Required action |
|----|----------|----------|---------|-----------------|
| R-01 | Critical | `crates/oxidean-cli/src/pilot/exec.rs:629-645` | `hostname_label`'s `Option::or_else` closure returns `String`, not `Option<String>` — on `cfg(unix)` the `return` statements yield `String` from the closure; on `cfg(windows)` the block tail is `String`. `or_else` requires `FnOnce() -> Option<String>` → E0308 — the crate does not compile on any target. | Wrap the branch results in `Some(...)` (or restructure so the or_else produces `Option<String>`), keeping the final `.filter(...).unwrap_or_else("connector")` shape. |
| R-02 | Critical | `crates/oxidean-cli/src/pilot/state.rs:353` | `claim_fut.as_mut().expect("armed").await` awaits `&mut Pin<Box<dyn Future + Send>>`. `Option::as_mut` yields `&mut Pin<Box<…>>`; `&mut F: Future` requires `F: Unpin`, and `Pin<Box<dyn Future + Send>>` is `!Unpin` (`dyn Future + Send` is not `Unpin`) → E0277 — the crate does not compile. The `claim_fut = None` clear-None fix at :366 exists textually but the arm can't compile to reach it. | Project the pin before awaiting — `claim_fut.as_mut().unwrap().as_mut().await` (`Pin<&mut dyn Future>` is a `Future`) — or `claim_fut.take().unwrap().await`, which also supersedes the manual `None` clear. |
| R-03 | Major | `crates/oxidean-cli/src/pilot/exec.rs:458` | `fs_write` runs `std::fs::create_dir_all(&parent)` — auto-creating missing parent dirs, which BR4.4 explicitly forbids ("Missing parent dirs are NOT auto-created"). The colocated test `fs_write_read_roundtrip` (:816-828) writes `sub/dir/new.bin` and asserts `Done`, ratifying the violation. | Delete the `create_dir_all`; let the tmp-file `File::create` surface `io:`/`not_found:` when the parent is absent. Fix the test to pre-create the parent (and add a refusal case). |
| R-04 | Major | `docs/CLI-PARITY.md:132` vs `scripts/check-cli-parity.sh:31-38` | The pilot row's ox cell `` `pilot pair\|run\|disconnect\|status` `` embeds `|` inside the cell, so `awk -F'\|'` splits it into `$4="run"`, `$5="disconnect"`, `$6="status`"` — `$4 ~ /implemented/` never matches → `ox pilot --help` is never probed. The row's justification ("kept `implemented` so the parity check covers the surface") and code-summary's claim are false in effect. | Rephrase the ox cell without inner pipes (e.g. `` `pilot` `` or comma-separated verbs) so the awk's `$4` sees `implemented` and the probe actually exercises the group. |
| R-05 | Major | `crates/oxidean-cli/src/pilot/state.rs:204-209`, used at :354 and :654 | `next_signal` constructs a fresh `tokio::signal::ctrl_c()` per `select!` iteration; the receiver is dropped when another arm wins. Once tokio's OS-level SIGINT handler is armed (first registration), SIGINT delivered while no `ctrl_c` receiver exists is broadcast to zero subscribers — silently eaten, not fatal. That gap covers every handler body: `on_heartbeat`, `on_claim`'s exec/report path, `post_report` retries+sleeps (up to ~2 min), `apply_cancels`, `exec_fs` awaits, and the nested-select's own `on_heartbeat`/post-`End` report. A first Ctrl-C can be swallowed outright; BR4.5's kill/report/exit-0 close-out is probabilistically lost (SIGTERM is unaffected — `self.sigterm` is a persistent stream). | Keep one SIGINT receiver alive across iterations like `sigterm` (`tokio::signal::unix::signal(SignalKind::interrupt())` on unix; a once-created pinned `ctrl_c` future on non-unix), or hold `next_signal`'s future pinned between selects. |
| R-06 | Minor | `crates/oxidean-cli/src/pilot/state.rs:250` (`settled` field; inserts at :532, :544, :554) | `settled` never evicts — scalability-design §"State sizing" (:16-18) and the codegen plan invariant (:147) pin "evict when the server stops re-announcing"; a long-lived connector grows the set by lifetime action throughput. | Evict ids absent from subsequent cancel re-announcements (or cap the set); if lifetime-bounded is intended, amend the design note. |
| R-07 | Minor | `crates/oxidean-cli/src/pilot/exec.rs:614-621` | `enforce_output_bound` uses `assert!` — a report output ≥ 256 KiB panics the whole connector mid-run. Reachable via the `fs_read` race in R-08 (file grows past `FS_IO_CAP` between check and read → base64 > 171 KiB → can exceed 256 KiB). | Replace the assert with a bounded truncation or a `failed` outcome — a hostile/racy input must degrade to a report, never a panic. |
| R-08 | Minor | `crates/oxidean-cli/src/pilot/exec.rs:413-426` | `fs_read` checks `metadata.len() > FS_IO_CAP` then `std::fs::read`s the whole file — TOCTOU lets a growing file exceed the pinned 128 KiB raw bound (and feeds R-07's panic). | `File::open` + `Read::take(FS_IO_CAP + 1)` and fail `too_large` when the read hits the sentinel — bound the read, not just the pre-check. |
| R-09 | Minor | `crates/oxidean-cli/src/pilot/mod.rs:217-223` | `run`'s startup banner omits `caps` — functional-spec WF2.1 pins "banner (instance, label, target, caps, root)"; the effective capability set is invisible at launch. | Add the caps line to the banner (or amend the spec text). |
| R-10 | Minor | `crates/oxidean-cli/src/pilot/custody.rs:137-148` | `let _ = f.set_permissions(0o600)` silently discards failure — on chmod error the verifier file lands at `0666 & ~umask` (e.g. 0644 under umask 022), quietly weakening the pinned 0600. Also no parent-dir fsync after rename, so the atomic entry isn't durable across a crash (row exists, verifier lost → self-made zombie). | Propagate the chmod error (abort the write) and `fsync` the containing directory after the rename. |
| R-11 | Nit | `crates/oxidean-cli/src/pilot/state.rs:151` | `TermReason::NotFound` reports drained in-flight actions as `cancelled:not_found`; the reliability matrix names the kill-report marker `cancelled:revoked` for that close-out (BR8.4). The marker is arguably more honest but drifts the pinned vocabulary. | Align the marker or amend the design note to bless `cancelled:not_found`. |
| R-12 | Nit | `crates/oxidean-cli/src/pilot/openapi.yaml:39,96,118,147,19` | `"4xx"` is not a valid OpenAPI 3.0 patterned response name (spec: `1XX`–`5XX` or `default`); and pair's requestBody marks `device_label`/`requested_caps` required while the shipped `PairBody` has `#[serde(default)]` on both (`routes.rs:146-149`). | Use `default`/`4XX` and relax the two pair fields to match shipped semantics. |
| R-13 | Nit | `crates/oxidean-cli/src/config.rs:111-121` | `pilot_targets_path()`/`pilot_secrets_path()` are dead duplicates — custody derives both paths internally from `dir`; no caller uses the config helpers. | Route custody through them or delete them. |
| R-14 | Nit | `docs/CONNECTOR.md:57` vs `crates/oxidean-cli/src/pilot/state.rs:672` | Doc lists refusal `timeout:<secs>`; impl emits `timeout:300s` (literal `s` inside the value). Same shape class for `too_large:<n>` ("`… bytes — cap …`"). | Align one side's vocabulary. |
| R-15 | Nit | `crates/oxidean-cli/src/pilot/exec.rs:435` | Comment claims "The file mode is 0644" — nothing sets it; the tmp file inherits `0666 & ~umask` (0600 under umask 077). | Set the mode explicitly or correct the comment. |
| R-16 | Nit | `crates/oxidean-cli/src/pilot/state.rs:288` | `tokio::time::interval` default `MissedTickBehavior::Burst`: ticks accumulated during unpolled windows (`post_report` retry chains, `exec_fs`, terminate-path reports) fire in an immediate burst — a short heartbeat storm and gratuitous rate-limit exposure. | `hb.set_missed_tick_behavior(MissedTickBehavior::Delay)` at construction. |
| R-17 | Nit | `crates/oxidean-cli/src/pilot/state.rs:584-610` | `exec_fs` never registers the op in `running` — consistent with the between-ops cancel granularity, but a later cancel for an fs action whose outcome report never landed is reported `cancelled:pre-start` even though it ran. | Acceptable as-is; add a comment or a `fs-in-flight` marker if marker honesty matters to the operator. |
| R-18 | Nit | `aidlc/…/code-generation/source-manifest.json`; `.github/workflows/ci.yml:885`; `docs/TESTING.md:110-118,314` | The unit's recorded writes include hunks belonging to the connector-gateway (U6) work — `dialect_oxpilot` added to the `db-dialects` nextest filter and the "Oxpilot connector-gateway scoped runs (U6)" doc section — interleaved with this unit's `check-cli-parity` step and `rpc-sync` row updates in the same uncommitted diff. | Confirm the U6 hunks belong to the gateway unit's write set (they read as such); the manifest/provenance for this unit should not claim them. |
| R-19 | Nit | `crates/oxidean-cli/src/pilot/state.rs:401,417,439` | `next_poll = Instant::now() + Duration::from_secs(hint)` — a hostile/malformed `Retry-After` (e.g. `u64::MAX`) overflows `Instant + Duration` → panic, crashing the loop where `rate_limited` is meant to be survivable. | Clamp the hint before adding (e.g. `secs.min(3600)` after the `max(1)` floor) or use `checked_add`. |
| R-20 | Nit | `crates/oxidean-cli/src/pilot/mod.rs:233`; `crates/oxidean-cli/src/args.rs:1334-1343` | `--exec-timeout 0` is accepted → `Duration::from_secs(0)` deadline fires instantly; every exec action reports `timeout:0s`. No floor on the operator flag. | Clamp `--exec-timeout` to ≥1 (or document 0 as "instant timeout"). |

### Verified-OK spot confirmations (checked against code, not claims)

- `client.rs` — flat `{"error":"<code>"}` decoded before status on all four routes; all 10 codes + Transport + Unknown; `Retry-After` read on 429 and 204; `retry_after` required in the 200 body (server always sends — `routes.rs:238`); `requested_caps`/`capabilities`/`grant_id` wire names correct (`grant_id` `Option` handles absent/null); `wait_seconds` clamped `[1,60]` and timeout `wait+15s`; Bearer only on claim/report/heartbeat; pair unauthenticated.
- `state.rs` — `classify` is total: pair-only codes → `Backoff` on loop routes, loop-only codes → `PairFail` on pair; `rate_limited` on pair retries once honoring the hint; `running` ≤1 with `Armed`→`Live` insert-before-spawn; `settled` consulted at exec-entry (:562) and completion (:684), inserted only after a landed report; cancels exactly-once; `terminate` is the sole funnel (best-effort raw `client.report` — no classify recursion); nested exec select multiplexes child|deadline|hb|signal; backoff is jitter-before-clamp `min(60, 2^n × (1±0.2))`.
- `mod.rs` — verifier write precedes row; row-failure compensates with `delete_verifier`; `disconnect` is local-forget only; `status` probes live (no stored backend); only `CliError::Transport`/`Config` are constructed anywhere in `pilot/` → runtime exits are {0,1}; `resolve_instance`'s `Usage` is remapped to `Transport`; parse-time `UsageError`→2 is honestly scoped out in `docs/CONNECTOR.md` ("for run-loop terminals").
- `custody.rs` — keyring key `pilot/<instance>/<connector_id>`; file map `<instance>/<connector_id>`; 0600 tmp+fsync+rename; `read_verifier` shape `Result<Option<(Backend, SecretString)>>`; delete hits both backends; corrupt files → named `Parse` errors; `TargetRow` carries no verifier; `SecretString` zeroizes + redacts `Debug`.
- `exec.rs` — jail chain correct: lexical normalize → `..` escape refusal → deepest-existing canonicalize → root containment (component-wise) → config-dir exclusion → `symlink_metadata` leaf regular-file (symlink leaf refused on read AND write-overwrite); env strip matches the pinned name patterns; tail buffer keeps last 64 KiB with UTF-8 continuation-skip + `…<n> bytes truncated`; `process_group(0)` + `killpg`/`taskkill /T`; fs ops under `spawn_blocking`; scrub set ≥8 chars (verifier + env token/key/secret/pass values + config tokens + secrets-map values).
- Registration — `args.rs` parse arms + GROUPS + HELP entries (honest `{0,1}`/local-forget/TTL claims); `completion.rs` tree; `required_procedures` returns `vec![]` for all four pilot arms (no `oxpilot.*` literals → skill-sync safe); `Cargo.toml` adds `base64`/`zeroize`/unix-`libc` only; CI gains exactly ONE step inside `rpc-sync` (no new job → no ruleset edit needed).

### Prior findings

| ID | Severity | Location | Finding | Required action | Status |
|----|----------|----------|---------|-----------------|--------|
| — | — | — | None — first review iteration for this unit. | — | — |

### Notes for the implementer

- R-01/R-02 are each individually compile-fatal: `cargo check -p oxidean-cli` cannot pass with either present, so nothing else in this review was load-bearing for the verdict — but R-03/R-04/R-05 would have held the gate regardless.
- For R-02 the minimal correct shape is projecting to `Pin<&mut dyn Future>` before `.await`; `claim_fut.take().unwrap().await` also folds the `None` clear away cleanly.
- For R-03 the spec's text is unambiguous; the fix must update the ratifying unit test too or the suite will keep blessing the violation.
- For R-04 verify locally with `OX_BIN=<built ox> ./scripts/check-cli-parity.sh` after rephrasing the cell — the probe only sees rows whose `implemented` lands in `awk`'s `$4`.

### Findings

| ID | Severity | Location | Finding | Required action | Status |
|---|---|---|---|---|---|
| - | - | - | No findings | No action required | Resolved |
