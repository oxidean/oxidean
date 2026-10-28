## Review

**Reviewer:** aidlc-architecture-reviewer-agent
**Verdict:** READY
**Iteration:** 2

**Scope:** verification of the 20 iteration-1 fixes in the current working
tree, plus a full regression sweep of `crates/oxidean-cli/src/pilot/*.rs`
for defects introduced by those fixes. Review-only; no builds run, no
implementation files touched.

### Review coverage

| Dimension | Result |
|---|---|
| R-01/R-02 compile-fatal fixes (`or_else` Option shape, `Pin<&mut dyn Future>` projection) | PASS — both arms compile by inspection; async-block capture borrows `&mut claim_fut`, never moves the future |
| R-05 signal persistence (unix `Signal` streams, non-unix once-pinned `ctrl_c`) | PASS — `ctrl_c()` occurs exactly once (`state.rs:283`, `cfg(not(unix))`); both select sites use `next_signal` |
| R-03/R-07/R-08 execution fixes (no auto-mkdir, bounded read, non-panic bound) | PASS — refusal test added; `take(FS_IO_CAP+1)` sentinel; truncation at char boundary |
| Docs/CI honesty (parity probe, CONNECTOR.md, openapi.yaml, code-summary) | PASS with one Nit — the awk probe now emits `pilot` (run locally); code-summary still describes the deleted helpers (V-03) |
| Regression sweep (cfg coherence, `settle` borrows, dispatch totality, custody ordering, select! arm structure) | PASS with one Minor — an unclamped `Retry-After` sleep survives on two paths (V-01) |

### New findings

| ID | Severity | Location | Finding | Required action |
|----|----------|----------|---------|-----------------|
| V-01 | Minor | `crates/oxidean-cli/src/pilot/state.rs:513`; `crates/oxidean-cli/src/pilot/mod.rs:90` | `post_report`'s `Disposition::Sleep` arm and `pair_error`'s retry still sleep a server-supplied hint with only a `max(1)` floor — no ceiling. Inside the live loop an absurd `Retry-After` parks the connector inside the handler; the now-persistent SIGINT/SIGTERM receivers buffer the signal but nothing polls `recv()` until the sleep ends — graceful shutdown is dead, only SIGKILL works. Same threat class as R-19 but hang-not-panic. | Clamp both sleeps to the same `.min(3600)` ceiling used at `:422/:439/:461`. (Applied post-review.) |
| V-02 | Nit | `crates/oxidean-cli/src/pilot/state.rs:134`, `:195` | Stale comments left behind by the fixes: `TermReason::NotFound`'s doc still says "marker `cancelled:not_found`" while `report_marker()` emits `cancelled:revoked`; the "Signals" section header still claims "ctrl_c everywhere" though unix SIGINT now arrives via `signal(SignalKind::interrupt())`. | Update both comments to match the code. (Applied post-review.) |
| V-03 | Nit | `aidlc/…/connector-client/code-generation/code-summary.md:73` | `code-summary.md` still claimed `config.rs` gained `pilot_targets_path()`/`pilot_secrets_path()` — both were deleted under R-13, and the deletion was not recorded under Deviations. | Correct the summary line and note the deviation. (Applied post-review.) |

### Verified-OK spot confirmations (iteration-2 checks against code, not claims)

- `state.rs:364-376` — destructure borrows `hb`/`next_poll`/`sigint`/`sigterm`
  from `&mut self`; `claim_fut` stays a disjoint local; select! preconditions
  (`is_none`/`is_some`) are evaluated before the branch futures capture
  `claim_fut` — no aliasing.
- `state.rs:374` — `claim_fut.as_mut().expect("armed").as_mut().await`:
  `Option::as_mut` → `&mut Pin<Box<dyn Future+Send>>` → `Pin::as_mut` →
  `Pin<&mut dyn Future>` which itself implements `Future` (`&mut F` is
  `Unpin`, `Target: Future`). The async block captures `claim_fut` by `&mut`
  (required by `.as_mut()`) — nothing is moved out of the `Option`; the
  completed future is dropped by `claim_fut = None` at `:387` before the
  next `Ev::ArmClaim` re-arms — no re-poll of a finished future.
- `state.rs:198-224` — cfg coherence: on unix `SigInt`/`SigStream` are both
  `tokio::signal::unix::Signal` and `next_signal` selects `recv()` on each;
  on non-unix `SigInt` is the pinned `ctrl_c` future (Output
  `io::Result<()>` — matches) and `SigStream = ()` passed as `_: &mut
  SigStream`. `std::future::Future` is fully qualified in the alias — no
  import needed. Signal registration in `new()` (:275-283) maps failures
  to `CliError::Transport` → exit 1.
- `state.rs:564-572` — `settle` caps at 4096 with one arbitrary eviction;
  `iter().next().cloned()` ends the immutable borrow before `remove(&ev)`
  mutably borrows — clean. All three insert sites (:554, :580, :590)
  route through it; the `settled.insert` calls at :913/:984 are direct
  test setup only.
- `exec.rs:625-638` — `enforce_output_bound` truncates at a char boundary
  under `OUTPUT_MAX - 64`, then appends a ~24-byte marker: worst case
  262,104 B < 256 KiB. No panic path remains.
- `exec.rs:413-442` — `fs_read` keeps the metadata pre-check (better
  `too_large` message on stable files) but bounds the actual read with
  `File::open` + `take(FS_IO_CAP + 1)`; the sentinel overshoot fails
  `too_large`. Growth TOCTOU closed.
- `exec.rs:465-491` — `fs_write` has no `create_dir_all`; a missing parent
  surfaces through the tmp `File::create` as `io:…` and the refusal test
  (:848-859) asserts `Failed` + `!j.root.join("no").exists()`. The doc
  comment (:447-449) now states `0666 & ~umask` + no-auto-create.
- `exec.rs:645-664` — `hostname_label`'s `or_else` closure yields
  `Option<String>` on both cfg branches (`Some` at :653, :655, :660);
  `.filter`/`.unwrap_or_else("connector")` shape preserved.
- `custody.rs:134-158` — `set_permissions(0o600)` propagates via `map_err`
  inside the unix cfg block; `#[cfg(unix)]` directory `File::open(dir)` +
  `sync_all` runs after the rename (best-effort `let _`, correct —
  dir-open failure must not fail the write).
- `mod.rs:217-224` — banner now prints `caps: {granted_caps}`.
- `mod.rs:234` — `exec_timeout … .max(1)` floors the operator flag.
- `docs/CLI-PARITY.md:132` — ox cell is `` `pilot` `` (verbs moved to the
  prose column, comma-separated). Running the probe's awk verbatim emits
  `pilot` — `ox pilot --help` is now exercised.
- `openapi.yaml` — `"4XX"` at :39/:96/:118/:147 (valid 3.0 patterned
  response name); pair `required: [pairing_code]` only (:19).
- `config.rs` — zero hits for `pilot_targets_path`/`pilot_secrets_path`
  anywhere under `crates/`; custody keeps its own `targets_path`/
  `secrets_path` (`custody.rs:103-109`).
- `CONNECTOR.md:57` — `timeout:<n>s` now matches `format!("timeout:{}s")`
  (`state.rs:716`).
- `state.rs:284-287` — `hb.set_missed_tick_behavior(MissedTickBehavior::
  Delay)` at construction.
- `state.rs:618-622` — `exec_fs` doc comment now documents the
  `cancelled:pre-start` understatement explicitly.
- Source manifest — file-level writes legitimately include `ci.yml` and
  `TESTING.md` (U7 added the `check-cli-parity` step inside `rpc-sync` and
  the TESTING row). The U6 hunks in the same files
  (`dialect_oxpilot` nextest filter) are a shared-diff granularity
  artifact; U6's own manifest also claims `TESTING.md`. Rationale
  accepted.
- Invariants re-checked unchanged: dispatch totality (`classify` +
  `loop_error` + `dispatch_table_totality` test), `running`≤1
  insert-before-spawn, cancels exactly-once, `terminate` sole funnel,
  custody verifier-then-row + compensation, jail chain order, both
  `select!` sites' arm structure.

### Prior findings

| ID | Severity | Location | Finding | Required action | Status |
|----|----------|----------|---------|-----------------|--------|
| R-01 | Critical | `exec.rs:645-662` | `hostname_label` `or_else` returned `String` | Wrap branches in `Some(...)` | Resolved |
| R-02 | Critical | `state.rs:374` | `claim_fut` `&mut Pin<Box<dyn Future+Send>>` await E0277 | `Pin::as_mut` projection to `Pin<&mut dyn Future>` | Resolved |
| R-03 | Major | `exec.rs:465-491` | `fs_write` auto-created parents vs BR4.4 | Removed `create_dir_all`; refusal test added | Resolved |
| R-04 | Major | `docs/CLI-PARITY.md:132` | Inner `|`s hid the row from the probe's awk | Bare `` `pilot` `` cell | Resolved |
| R-05 | Major | `state.rs:198-224,275-283,375,698` | Transient `ctrl_c` receiver ate mid-handler SIGINT | Persistent `SigInt`/`SigStream` receivers | Resolved |
| R-06 | Minor | `state.rs:564-572` | `settled` unbounded | `settle()` caps at 4096 + eviction | Resolved |
| R-07 | Minor | `exec.rs:625-638` | `assert!` panic on oversized output | Char-boundary truncation + marker | Resolved |
| R-08 | Minor | `exec.rs:425-442` | `fs_read` metadata-then-read TOCTOU | `take(FS_IO_CAP+1)` bounded read (+`O_NOFOLLOW` post-verdict) | Resolved |
| R-09 | Minor | `mod.rs:217-224` | Banner omitted caps | `caps: {granted_caps}` added | Resolved |
| R-10 | Minor | `custody.rs:134-158` | chmod result discarded; no dir fsync | `map_err` propagate + `#[cfg(unix)]` dir fsync | Resolved |
| R-11 | Nit | `state.rs:151` | `cancelled:not_found` off-vocabulary | Aligned to `cancelled:revoked` | Resolved |
| R-12 | Nit | `openapi.yaml:19,39,96,118,147` | Invalid `"4xx"` + over-required pair fields | `"4XX"` + `required: [pairing_code]` | Resolved |
| R-13 | Nit | `config.rs` | Dead `pilot_*_path` helpers | Deleted | Resolved |
| R-14 | Nit | `docs/CONNECTOR.md:57` | `timeout:<secs>` vs `timeout:300s` impl | Doc aligned to `timeout:<n>s` | Resolved |
| R-15 | Nit | `exec.rs:447-449` | False `0644` claim | Corrected to `0666 & ~umask` | Resolved |
| R-16 | Nit | `state.rs:287` | `MissedTickBehavior::Burst` | `Delay` at construction | Resolved |
| R-17 | Nit | `state.rs:618-622` | `exec_fs` never in `running` | Documented as deliberate | Resolved |
| R-18 | Nit | `source-manifest.json` | File-level writes share U6 hunks | Rationale accepted (file granularity) | Resolved |
| R-19 | Nit | `state.rs:422,439,461` | `Instant + Duration` overflow on hostile hint | `.min(3600)` clamp | Resolved |
| R-20 | Nit | `mod.rs:234` | `--exec-timeout 0` had no floor | `.max(1)` | Resolved |

### Residual notes (post-verdict supplementary pass)

- A parallel read-only review surfaced four additional defects, all
  applied after this verdict: `fs_write` tmp open is now `create_new`
  (`O_CREAT|O_EXCL`, nanosecond-suffixed name) — the predictable-pid
  tmp-symlink escape fails closed; `fs_read` opens with `O_NOFOLLOW`
  (unix) closing the leaf-swap TOCTOU; the `End::Timeout` arm now
  consults `settled` (no double report) and scrubs the tail; V-01's two
  verbatim sleeps clamp `.min(3600)`.
- No build was run; all compile claims are by-inspection. `cargo check
  -p oxidean-cli` plus `cargo test -p oxidean-cli pilot` (or the unit's
  recorded CI shapes) remain the load-bearing verification before
  this unit is committed.

### Findings

| ID | Severity | Location | Finding | Required action | Status |
|---|---|---|---|---|---|
| - | - | - | No findings | No action required | Resolved |
