# Code Generation Plan — u8-vcs-cli

Upstream design stages (functional-design / nfr-requirements / nfr-design / infrastructure-design) were workflow-skipped for u7–u10 under the floor-16 forward jump; this plan embeds the design decisions those stages would have produced, sourced from the frozen contract (`inception/contract-design/contract-summary.md` C8 + C1/C2 consumes), `inception/units-generation/unit-of-work.md` §U8, `inception/requirements-analysis/requirements.md` (FR4.2/FR4.4, FR8.1–FR8.3, FR10.2, FR12.1–FR12.3, FR15.1/FR15.3), and `inception/user-stories/stories.md` (US3.4, US6.1–US6.3, US8.4).

## Unit of work

`oxidean-vcs` gains a real `client` feature (previously an empty marker): `CliSurface` — the 16-command `ox vcs` implementation set — plus `WorkingCopy` (sparse manifest filter, want-fetch, bounded hash-verified cache, `CheckoutResult`) over an injected `VcsTransport`. `oxidean-cli` gains thin argv glue: `Command::Vcs*` variants + USAGE rows + `required_procedures` entries + `CLI_PROCS` additions + a `VcsTransport` impl over the existing `Client`/`HttpClient`; `rpc.rs::error_exit_code` gains `vcs.unauthenticated`|`vcs.forbidden` → exit 3.

## Design decisions (embedded — the skipped design stages)

### DD-1 — Dependency direction and feature discipline (ADR-011, FR15.3, AC8.4.2)

`oxidean-cli` depends on `oxidean-vcs` with **no `server` feature** — the `client` feature compiles with zero `oxidean-db`/`oxidean-git`/sqlx deps. To make the client usable, `oxidean-core` (wire DTOs — already an `oxidean-cli` dep) plus `serde`, `serde_json`, `base64`, `sha2`, `hex`, `async-trait` move to **shared (non-optional)** deps; `server` keeps `bytes`/`chrono`/`futures-util`/`tokio`/`tracing`/`oxidean-db`/`oxidean-git`. The client surface consumes `oxidean_core::vcs_types` DTOs — never duplicates them (u1 boundary rule).

Dependency check: `oxidean-vcs` never imports `oxidean-cli`/`oxidean-api` — the transport is injected by the host.

### DD-2 — `VcsTransport` (the injected seam, C8)

```rust
#[async_trait]
pub trait VcsTransport {
    /// One `vcs.*` (or other) procedure call — returns the raw `RpcResponse`
    /// envelope (`{ok:false}` envelopes included; transport errors are Err).
    async fn call(&self, procedure: &str, input: Value) -> Result<RpcResponse, TransportError>;
    /// `GET /api/vcs/{owner}/{name}/objects/{hash}` — raw bytes on success.
    async fn object_get(&self, owner: &str, name: &str, hash: &str) -> Result<Vec<u8>, TransportError>;
    /// `POST /api/vcs/{owner}/{name}/objects` — server-computed hash.
    async fn object_put(&self, owner: &str, name: &str, bytes: &[u8]) -> Result<String, TransportError>;
}
```

`TransportError` is a typed local error (`{ kind: Transport|Http{status}|Envelope{code,message}, display }`) — it never manufactures `vcs.*` codes itself; server `vcs.*` errors arrive as `Envelope{code}` variants surfaced verbatim (per "local typed errors never masquerade as `vcs.*` codes").

`oxidean-cli` implements it as `CliVcsTransport { rpc: rpc::Client, http: http::HttpClient }`: `call` → `Client::call`; `object_get`/`object_put` → new `HttpClient::vcs_object_get`/`vcs_object_put` methods on the existing raw-HTTP helper (Bearer token, `user-agent`, envelope-on-error parsing — the `download_release_asset` pattern; GET success is raw octet-stream, failure is the JSON envelope parsed for `{error:{code,message}}`).

### DD-3 — `CliSurface`: 16 commands, `Out::Rpc` discipline (C8, FR12.1, AC8.4.1)

Each command = one `async fn(T: &dyn VcsTransport, args: TypedArgs) -> Result<CliOutcome, CliFault>` in `oxidean_vcs::client::cli`, where `CliOutcome { envelope: RpcResponse, pretty: String }` mirrors `oxidean-cli`'s `Out::Rpc` without depending on the crate. On `!envelope.ok` the command returns the envelope verbatim (the `Out::Rpc` path prints `ox: <code>: <message>` on stderr and exits via `error_exit_code` — server codes travel untouched); `pretty` carries the human rendering.

`CliFault` = `{Usage(String), Transport(String), Fs(String), Cache(String)}` — local failures map to `CliError::{Usage,Transport}` with `ox:`-prefixed messages, never `vcs.*` codes.

| # | Command | Wire call(s) | Notes |
|---|---------|--------------|-------|
| 1 | `ox vcs enable <repo> [--disable]` | `vcs.repo_enable {enabled: !disable}` | AC8.4.1: `enable` covers the disable path |
| 2 | `ox vcs status <repo>` | `vcs.status` | heads + counts table |
| 3 | `ox vcs commit <repo> -m <msg> [--parent <cs>]... --add <vpath>=<local>... --delete <vpath>... --rename <old>=<new>...` | `object_put` (per >256 KiB file) + `vcs.changeset_create` | `--add` reads the local file; ≤262144 B → `content_b64`, larger → `object_put` then `blob:` ref (AC2.1.2). Mode from fs metadata: unix exec bit → `executable`, symlink → `symlink`, else `regular`; `--mode` overrides. `--parent` repeatable; absent = `[]` root (AC2.1.4). Zero `--add/--delete/--rename` ⇒ Usage fault. |
| 4 | `ox vcs log <repo> [--from <ref\|cs\|git-oid>] [--limit N] [--cursor c]` | `vcs.log` | `--from` defaults to `"main"` |
| 5 | `ox vcs diff <repo> <from> <to> [--paths p]... [--limit N] [--cursor c]` | `vcs.diff` | `--paths` repeatable (C1 multi-value) |
| 6 | `ox vcs show <repo> <id>` | `vcs.changeset_get` | `id` = cs or git-oid (lazy-import server path) |
| 7 | `ox vcs cat <repo> <cs> <path>` | `vcs.file_get` (+ `object_get` when `too_large`) | decodes `content_b64` → stdout bytes; `too_large:true` → object route fallback |
| 8 | `ox vcs blame <repo> <path> [--at <cs>] [--limit N] [--cursor c]` | `vcs.blame` | |
| 9 | `ox vcs history <repo> <path> [--limit N] [--cursor c]` | `vcs.path_history` | |
| 10 | `ox vcs merge-base <repo> <a> <b>` | `vcs.merge_base` | |
| 11 | `ox vcs merge <repo> <a> <b> [-m <msg>]` | `vcs.merge` | `vcs.conflict` data prints the `{path,kind,detail?}` list on stderr as the pretty channel |
| 12 | `ox vcs rebase <repo> <cs> --onto <cs> [-m <msg>]` | `vcs.rebase` | same conflict rendering |
| 13 | `ox vcs checkout <repo> <cs> [--paths <f>...] [--dir <target>]` | `vcs.tree` (paged) + `object_get` per miss | WorkingCopy — see DD-4; `--dir` defaults to `./<name>`; no `--paths` = full checkout (AC3.4.2 degenerate filter) |
| 14 | `ox vcs ref list <repo> [--limit N] [--cursor c]` | `vcs.ref_list` | one `ref` command with `list`/`set` subcommands |
| 15 | `ox vcs ref set <repo> <name> [--expect <cs>] [--create] [--to <cs> \| --delete]` | `vcs.ref_update` | `--create` ⇒ `expected_old:null`; `--delete` ⇒ `new:null`; neither `--expect` nor `--create` with a move ⇒ Usage (CAS never silently unconstrained) |
| 16 | `ox vcs import <repo> [--from <ref\|oid>] [--limit N] [--cursor c]` | `vcs.git_import` | `--cursor` continues a paged import |
| 17→16 | `ox vcs export <repo> <cs> --branch <b> [--force]` | `vcs.git_export` | prints landed `git_oid` |

(`show`/`cat` distinct + `ref` grouped ⇒ the pinned 16-command count.)

### DD-4 — `WorkingCopy` (C8 signature, FR8.1–FR8.3, AC6.1–AC6.3, AC3.4)

```rust
pub async fn checkout(
    transport: &dyn VcsTransport, repo: &RepoRef {owner,name}, changeset_id: &str,
    filter: &PathFilter, target: &Path, cache: &mut ObjectCache,
) -> Result<CheckoutResult, CliFault>
// CheckoutResult { files_written: u64, blobs_fetched: u64, cache_hits: u64, skipped_outside_filter: u64 }
```

1. Page `vcs.tree` (`limit 500`, cursor loop until absent) → full `{path, mode, blob}` listing — the object store has no tree objects; selection is over blob entries (AC6.1.1).
2. `PathFilter` — client-side matching over declared `--paths` (AC3.4.1): a filter matches an exact path OR as a directory prefix (`filter` matching `filter/...` — trailing-slash and bare-dir forms collapse). Empty filter set = full checkout.
3. **Non-empty target gate** (AC6.1.3): if `target` exists and is non-empty and was not created by a prior `ox vcs checkout` (marker file `.oxidean-vcs-checkout` absent) ⇒ `CliFault::Fs("target directory is not empty")` — never silently merge. Marker present ⇒ overwrite per-file via temp-write-rename.
4. For each selected entry: cache lookup by blob hash (sha256 verify on read — corrupt entry → evict + count as miss per AC6.2.2) → miss ⇒ `object_get` → sha256-verify bytes against the wanted hash (AC3.3.3, AC6.1.2) → cache insert.
5. Materialize: `regular`/`executable` → temp file in target dir + `fs::rename` (per-file atomicity — an interrupted run leaves consistent per-file state); `symlink` → `std::os::unix::fs::symlink` (Windows: `CliFault::Fs` typed degradation per AC6.3.1 — documented, never silently copied); mode `0o644`/`0o755`.
6. Return `CheckoutResult`; pretty line: `checked out N files (M fetched, K cached, S skipped)`.

### DD-5 — `ObjectCache` (bounded, hash-verified, shared)

- Root: `OXIDEAN_VCS_CACHE_DIR` env, else `$XDG_CACHE_HOME/ox/vcs-objects`, else `~/.cache/ox/vcs-objects`. Bound: `OXIDEAN_VCS_CACHE_MAX_BYTES` env, default 1 GiB.
- Layout `<root>/<first-2-of-hex>/<rest>.bin` keyed by content hash — shared across checkouts and repos (FR8.2, AC6.2.1).
- `get(hash)`: read + sha256-verify; mismatch → remove + miss. `put(hash, bytes)`: sha256-verify bytes vs `hash` first (mismatch ⇒ `CliFault::Cache` — never commit corrupt bytes, AC3.3.3), write temp + rename, then evict oldest-mtime entries while total > bound.

### DD-6 — `oxidean-cli` glue (thin shell, FR12.2, FR10.2)

- `args.rs`: `Command::Vcs { sub: VcsSub }` — nested `enum VcsSub` (16 arms, one struct-variant per command) keeps the flat `Command` enum readable. USAGE block + `help_topic("vcs", <sub>)` rows for each subcommand.
- `commands.rs`: `required_procedures` — each sub maps to its `vcs.*` procedure (`ref set` → `vcs.ref_update`; `checkout`/`cat` large-file path → their RPC + `vcs.object_get` is NOT in PROCEDURES (route alias, not an RPC — the manifest gate only checks the RPC procedure)). `CLI_PROCS` gains every `vcs.*` procedure name the surface calls — the fail-closed coverage test (commands.rs:3048) asserts each is in `required_procedures` for some command.
- Dispatch: build `CliVcsTransport` from `client(g, rt)` + `HttpClient::new(instance, token)` → call `oxidean_vcs::client::cli::*` → `CliOutcome` → `Out::Rpc{envelope, pretty}`; `CliFault` → `CliError::{Usage,Transport}` (`ox:`-prefixed messages).
- `rpc.rs::error_exit_code`: add `"vcs.unauthenticated" | "vcs.forbidden"` → 3 (C8 match-list extension).
- `oxidean-cli/Cargo.toml`: `oxidean-vcs = { path = "../oxidean-vcs", features = ["client"] }`.

## Steps (plan_profile ordering)

1. **Skeleton**: `oxidean-vcs` Cargo.toml dep reshuffle (shared vs server-gated); `src/client/{mod,transport,cli,working_copy,object_cache}.rs` under `#[cfg(feature = "client")]`. Verify runner: `cargo nextest run -p oxidean-vcs` (client) / `--features server`.
2. **Transport + fault types** (`client/transport.rs`): `VcsTransport`, `TransportError`, `CliFault`, `CliOutcome`.
3. **`ObjectCache`** (`client/object_cache.rs`) + tests (verify-on-read corrupt→miss, bound eviction, hash-mismatch put refusal, cross-"repo" sharing).
4. **`WorkingCopy::checkout`** (`client/working_copy.rs`) + tests on real tempdirs + fake transport (sparse selection, zero-fetch second run, non-empty-dir refusal, temp-rename consistency, modes/symlink, `CheckoutResult` counters).
5. **`CliSurface`** (`client/cli.rs`) — 16 impls + per-command tests over a scripted fake transport (request shaping `{owner,name,…}`, `!ok` envelope pass-through, `content_b64` inline-vs-PUT split, ref-set CAS mapping, `cat` too_large→object_get, conflict pretty rendering).
6. **`oxidean-cli` glue** — `args.rs` variants + USAGE/help rows; `commands.rs` `required_procedures` + `CLI_PROCS` + dispatch arm + `CliVcsTransport` (incl. `HttpClient::vcs_object_get/put`); `rpc.rs` exit-code rows; `Cargo.toml` dep.
7. **Glue tests** — args parse table (16 subs), `required_procedures` map rows, `error_exit_code` new codes, `CLI_PROCS` coverage test green.
8. **Feature-dependency proof** — `cargo check -p oxidean-vcs --no-default-features --features client` (client-only compile); `cargo tree -p oxidean-cli -i oxidean-vcs` shows no `oxidean-db`/`oxidean-git`/`sqlx` edge.
9. **Docs/traceability** — `code-summary.md`, `traceability.json`, `source-manifest.json`.

## Testing Contract

```json
{
  "version": 1,
  "methodology": "test-after",
  "source": "team",
  "ordering": "We implement the change first, then write and run the tests the change-type minimum requires (`docs/CODE_PRACTICES.md` § Testing expectations), and the full CI suite must be green before the PR can merge.",
  "scope": "classic",
  "test_strategy": "standard",
  "project_type": "brownfield",
  "applicable_notes": [
    {
      "layer": "org",
      "text": "We treat tests as a first-class deliverable in every Bolt. The specific\nmethodology (TDD, BDD, ATDD, or classic test-after) is affirmed at\npractices-discovery and recorded in `team.md` under this heading with explicit\n`Methodology` and `Ordering` fields; Code Generation resolves those fields\nindependently from coverage, tooling, and scope notes.\n\nWhen no posture has been affirmed, our default per scope is:\n- **Methodology**: test-after\n- **Ordering**: implement each applicable testable layer, then write and run\n  that layer's tests.\n- `mvp`, `enterprise`, `feature`, `infra`, `classic` add an 80% line-coverage\n  floor and CI execution before merge.\n- `bugfix`, `security-patch` add a targeted regression for the specific\n  bug/vulnerability and require the existing suite to remain green.\n- `express` uses the Minimal strategy: requirement-driven unit tests (one per\n  requirement, with a happy-path floor per component); existing tests remain\n  green.\n- `poc`, `refactor`, `workshop` add no extra new-test floor and require the\n  existing suite to remain green.\n\nThe active `Test Strategy` still applies in every scope and determines test\nvolume/types. Scope floors are additive; they never reduce or replace the\nselected strategy.\n\nBuild and Test verifies defined coverage floors and affirmed quality targets;\nthey may not be weakened to make a step pass.\n\nAffirm a stricter posture in `team.md` if the team commits to one."
    },
    {
      "layer": "team",
      "text": "- **Methodology**: test-after\n- **Ordering**: We implement the change first, then write and run the tests\n  the change-type minimum requires (`docs/CODE_PRACTICES.md` § Testing\n  expectations), and the full CI suite must be green before the PR can merge.\n- **TDD option**: an in-repo `tdd` skill (`.agents/skills/tdd/`) exists and\n  may be invoked deliberately for a unit, but red-first is not the team's\n  mandated posture.\n- **Coverage**: CI enforces a **weighted composite gate at floor 0.70**\n  (`coverage-weighted` job, `COVERAGE_WEIGHTED_FLOOR`, composed by\n  `scripts/coverage-weighted.sh`): web unit 0.25 + web integration 0.40 (v8\n  line coverage) + e2e 0.35 — where the e2e third is a **7-path presence\n  checklist** (`scripts/coverage-e2e-checklist.sh`), not executed coverage;\n  e2e correctness is enforced by the separate `e2e-stack` / `actions-e2e` /\n  `smoke-protocol` job gates. The composite **consumes web numbers only**:\n  Rust coverage is collected (`cargo-llvm-cov`, artifacts uploaded) but\n  **not gated**, so this intent's new VCS crate lands with no enforced\n  coverage signal — collect-only Rust coverage is recorded debt, and we do\n  not invent a new numeric floor for it (the change-type minimums + green CI\n  carry its testing). Cross-dialect execution in CI is limited to\n  `dialect_probe` per dialect (`db-dialects` job): the ~30 `dialect_*` parity\n  files are `DATABASE_URL`-gated and **skip-pass green** in the `api-rust`\n  shards — full parity is local-capable only, also recorded debt. No\n  performance-test infrastructure exists (no benches, no load harness) —\n  recorded debt for NFR work, not a blocker.\n- **UI coverage is change-aware**: route- and browser-coverage manifests mean\n  every new/changed `apps/web` route needs test evidence, and new/changed\n  high-risk interactive UI (Checkbox / RadioGroup / Select / Switch /\n  portals / `form.Subscribe`) needs Chromium `*.browser.test.tsx` proof —\n  fresh skip-only rows fail CI, and `kind: \"browser\"` entries are\n  source-verified (mount, interact, assert markers). Manifest `skip` rows are\n  the only sanctioned skip: zero `#[ignore]` and zero `it.skip`/`test.skip`\n  in the tree.\n- **Tooling**: cargo-nextest for the Rust workspace (`.config/nextest.toml`,\n  `ci` profile: no fail-fast, 90s slow-timeout; one shared nextest archive\n  from `rust-build` feeds the 5-way `api-rust` shards, `db-dialects`, and\n  archive consumers — one compile per run; shards re-run whole on failure —\n  coarse flake tolerance). Vitest 5 for TS with five projects in `apps/web`\n  (`unit` on node, `integration` on happy-dom with `fileParallelism: false`,\n  `browser` on Playwright Chromium, `e2e-stack` and `e2e-stack-browser` gated\n  on `E2E_STACK=1`) plus `@oxidean/api-client`'s own node-env suite.\n  Playwright drives real-Chromium component and full-stack e2e; wiremock\n  supplies outbound-HTTP fixtures; Rust integration tests open real SQLite\n  tempdir DBs; protocol smokes run real `git`, `ssh`, and `git-lfs` client\n  binaries against a live Compose stack (fail-closed in CI under\n  `SMOKE_REQUIRE_STACK=1`).\n- **Local/CI asymmetry**: `make test` runs only nextest + web\n  unit/integration; browser, stack e2e, and smokes are separate targets — a\n  green local `make test` proves far less than a green CI run, and smoke\n  scripts skip-pass locally without Docker but fail-closed under `CI`.\n- **Test types / minimums**: per change type — pure helper → unit;\n  Query/session UI → happy-dom integration; high-risk UI → Chromium browser\n  test + manifest row; auth/RPC contract → Rust integration and/or stack e2e;\n  dialect-touching DB code → `dialect_*` test (noting the CI-execution gap —\n  the probe is the only enforced cross-dialect run). Suites: colocated Rust\n  units plus `crates/*/tests/` integration (107 API test files), web\n  unit/integration/component tests, stack e2e with Mailpit/OIDC/Resend-WorkOS\n  stubs (`make test-e2e-stack`), forge protocol smokes, compose bring-up\n  smoke per dialect, and an Actions pipeline e2e."
    },
    {
      "layer": "project",
      "text": "- CI/Docker-related changes are verified locally with docker + act before shipping; deadlines are ASAP-but-quality-gated — solid tests are required, not optional (learned 2026-10-14) \n\n- Prefer Docker for debugging and testing (docker compose stack, make up/smoke, act for CI lanes) over bare-metal equivalents (learned 2026-10-18)"
    }
  ],
  "obligations": {
    "strategy": "standard",
    "strategy_volume": [
      "Five to eight tests per component.",
      "Unit tests plus integration tests for key boundaries.",
      "Add E2E, performance, or security tests when requirements demand them."
    ],
    "scope_floor": [
      "Keep the existing test suite green.",
      "This scope adds no extra new-test floor beyond the selected test strategy."
    ],
    "combination_rule": "Apply every selected-strategy obligation and every scope-floor obligation; neither replaces the other, and a targeted scope regression may add the narrowest necessary test type beyond the strategy default."
  },
  "plan_profile": {
    "methodology": "test-after",
    "runner_step": "Verify the existing test runner/configuration and record the exact unit-scoped command.",
    "runner_ready_before_first_test": true,
    "testable_layers": [
      "Data model / database behavior",
      "Repository / data access",
      "Business logic",
      "API / endpoint",
      "Frontend behavior"
    ],
    "steps": [
      "Project structure and production configuration skeleton.",
      "Verify the existing test runner/configuration and record the exact unit-scoped command.",
      "Data model / database behavior - implement.",
      "Data model / database behavior - write and run its tests after implementation.",
      "Repository / data access - implement.",
      "Repository / data access - write and run its tests after implementation.",
      "Business logic - implement.",
      "Business logic - write and run its tests after implementation.",
      "API / endpoint - implement.",
      "API / endpoint - write and run its tests after implementation.",
      "Frontend behavior - implement.",
      "Frontend behavior - write and run its tests after implementation.",
      "Environment/build configuration.",
      "Documentation and traceability."
    ]
  },
  "input_sha256": "sha256:304bf27698e8b5f47dff4e16702de3e3c6f286e25e63e22facb420316bac75bc",
  "contract_sha256": "sha256:d7627c3e92082e54b594f33a1ba945e0e072964270fa08e0adcb8fe891ef16c6"
}
```

## Test matrix (see unit-test-instructions.md for commands)

| Row | Component | Coverage |
|-----|-----------|----------|
| cache | `ObjectCache` | verify-on-read corrupt→miss+refetch, bound eviction, hash-mismatch put refuses, cross-repo shared root |
| working_copy | `checkout` | sparse filter selection (unrequested blobs never fetched — transport call counter), full degenerate filter, second-run zero-fetch (AC6.2.1), corrupt cache refetch, non-empty-dir typed refusal, temp-write-rename, modes/exec/symlink, `CheckoutResult` counters |
| cli_surface | per-command | request body shaping per the C1 wire examples, `!ok` envelope verbatim, commit inline-vs-PUT threshold (262144 B), `ref set` CAS mapping (`--create`/`--delete`/expect), `cat` too_large→object_get, conflict pretty channel, usage faults (missing repo spec, no change args, unconstrained ref move) |
| glue | `oxidean-cli` | args parse for all 16 subs, `required_procedures` rows, `CLI_PROCS` coverage test, `error_exit_code` `vcs.unauthenticated`/`vcs.forbidden`→3, dispatch arm → transport impl wiring |
| feature | dep discipline | client-feature compile w/o server deps (structural) |

`traceability.json` maps deliverables to C8 lines, FR8.1–FR8.3/FR12.1–FR12.3/FR4.2/FR4.4, AC6.1.1–AC6.3.1, AC8.4.1–AC8.4.3, AC3.4.1–AC3.4.2, and the ADR-011 dependency note.
