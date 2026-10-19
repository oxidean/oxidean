# Code Summary — u8-vcs-cli

## What landed

The full `ox vcs` command surface — 16 subcommands over the injected `VcsTransport` seam — with the typed logic living in `oxidean-vcs::client` (ADR-011) and `oxidean-cli` carrying only argv parsing, HTTP plumbing, and output/exit mapping.

### `oxidean-vcs::client` (new, default `client` feature)

- **`src/client/transport.rs`** — `VcsTransport` trait (`call`, `object_get`, `object_put`, `#[async_trait]`, object-safe): the seam every op and the working copy run over. `TransportError::{Transport, Http, Envelope(AppError), Malformed}` — `Envelope` preserves object-route `vcs.*` codes verbatim so exit classification still works off-RPC. `CliFault::{Usage, Transport, Fs, Cache, Server}` — local faults are `ox.*`-namespaced (never masquerade as `vcs.*` wire codes); `Server(Box<RpcResponse>)` carries verbatim server envelopes. `CliOutcome{envelope, pretty, bytes}` mirrors `Out::Rpc` + a raw-bytes channel for `cat`.
- **`src/client/object_cache.rs`** — bounded, verify-on-read content cache (`<root>/<2-hex>/<rest>.bin`): `put` verifies the wanted hash before anything lands; `get` re-verifies and evicts corrupt entries as misses; `OXIDEAN_VCS_CACHE_DIR`/`_MAX_BYTES` (1 GiB default) with oldest-mtime eviction; temp-write-rename per entry.
- **`src/client/working_copy.rs`** — sparse working copy (C8): pages `vcs.tree` at the server page bound, `PathFilter` client-side selection (exact or dir-prefix; empty set = full checkout — the AC3.4.2 degenerate case), want-fetch only missing blobs through `object_get`, per-file temp-write-rename materialization preserving exec bits and symlinks on unix (typed `CliFault::Fs` elsewhere — AC6.3.1), hostile-tree path rejection (`..`/`.`/empty segments), `.oxidean-vcs-checkout` marker certifying completed checkouts (non-empty unmarked dirs refused — AC6.1.3), `CheckoutResult` tallies.
- **`src/client/cli.rs`** — `CliSurface`: the `VcsCommand` enum (16 subs), `procedure()` mapping each to its wire procedure, and `run()` — request shaping (`{owner,name}` first per C1), inline-vs-object-PUT staging at the 256 KiB `INLINE_CONTENT_CAP`, fs-derived modes with `--mode` override, symlink staging via link target as blob content, `ref set` CAS flag validation (never unconstrained), `Nullable` CAS fields emitted as explicit JSON nulls, `{ok:false}` envelopes passed verbatim, `vcs.conflict` envelopes rendering their typed `data.conflicts[]` list into the pretty channel, `checkout` tallies emitted as a synthesized ok envelope.

### `oxidean-cli` (argv glue only)

- **`src/args.rs`** — `Command::Vcs(VcsCommand)`; USAGE row + 16 HELP entries; `vcs` in GROUPS and NESTED (`ref` → `list|set`); parse arms for all subs — `repo_and_arg`/`repo_and_two_args` (new `flag_repo` helper fixing `-R` shadowed by trailing positionals — a latent bug in the pre-existing `repo_and_arg` shape), repeatable `--parent/--add/--modify/--delete/--rename/--paths`, `vcs_pair` (`<v>=<l>` validation), `vcs_mode` (the three wire modes), `opt_limit` (signed wire limits), `--dir` default `./<repo-name>`; 9 parser tests.
- **`src/commands.rs`** — `required_procedures` maps `Command::Vcs` via the crate's own `procedure()` table; `CLI_PROCS` gains all 17 `vcs.*` names; `CliVcsTransport` impl over `rpc::Client` (`/api/rpc`) + `HttpClient` (`/api/vcs/.../objects`); `vcs_dispatch` resolves instance/auth, maps `CliOutcome` → `Out::{Rpc,Bytes}` and `CliFault` → `CliError::{Usage,Transport}` (`Server` envelopes go back through `Out::Rpc` so `error_exit_code` classifies); `Out::Bytes{envelope, bytes}` variant added.
- **`src/http.rs`** — `vcs_object_get` (raw bytes) / `vcs_object_put` (octet-stream → `{data:{hash}}`); `VcsObjectError::{Transport, Api(AppError), Http}` parses `{error:{code,message}}` bodies into verbatim `AppError`s.
- **`src/rpc.rs`** — `error_exit_code` gains `vcs.unauthenticated`/`vcs.forbidden` → 3.
- **`src/main.rs`** — `Out::Bytes` arm (binary-safe stdout write); `rpc_err` helper extracted (shared `Err` handling across `Rpc`/`Bytes` arms); `auth_hints` gains `vcs.unauthenticated`/`vcs.forbidden` rows.
- **`src/completion.rs`** — the `vcs` tree (16 subs + `ref list|set`).
- **`tests/commands_e2e.rs`** — 4 wiremock tests: `vcs.status` input + pretty, verbatim `vcs.disabled` passthrough, `ref set` CAS null payload, object-route `vcs.unauthenticated` envelope → `Out::Rpc`.
- **`tests/manifest_gate.rs`** — exhaustive `Out` match gains the `Bytes` arm.
- **`Cargo.toml`** — `oxidean-vcs = { features = ["client"] }` + `async-trait` (trait impl); dev-dep `hex` for the e2e fixture.

### Boundary verification

`cargo tree -p oxidean-vcs` (default and `--no-default-features --features client`): zero `oxidean-db`/`oxidean-git`/`sqlx`/`tokio`/`futures-util`/`chrono` edges — the client surface is dependency-light per ADR-011.

## Fixes folded in during implementation

1. **`PathFilter::matches` empty-set bug** — `any()` on an empty prefix list returned `false` for every path: a default `ox vcs checkout` would have written nothing. The empty set is the AC3.4.2 full-checkout filter — `matches` now short-circuits true.
2. **`repo_and_arg`/`repo_and_two_args` `-R` shadowing** — both helpers re-entered `repo_spec`, which prefers `positionals[0]` over `-R`: `vcs show -R o/n cs_1` parsed `cs_1` as the repo. New `flag_repo` helper resolves `-R`/`--repo` directly in the flag branch.
3. **`CliFault::Server` boxed** — `RpcResponse` is ≥128 B; boxing clears `result_large_err` across the client surface.
4. **Object-route envelopes keep their code** — first pass flattened `vcs.*` errors to strings (losing exit-3 auth classification); `VcsObjectError::Api` + `TransportError::Envelope` carry them as verbatim `AppError`s end-to-end.

## Test evidence

29 `oxidean-vcs` client tests (transport fault taxonomy/cache verify-on-read + eviction/sparse+want-fetch checkout + marker refusal + traversal rejection/commit staging + CAS + cat channels + conflict pretty), 9 `oxidean-cli` parser tests (all 16 subs + flag validation + positional splitting), 4 wiremock e2e (status/ref-set/disabled/object-route-envelope), `error_exit_code` extended for the vcs codes. `cargo nextest run --workspace`: **1520/1520**.
