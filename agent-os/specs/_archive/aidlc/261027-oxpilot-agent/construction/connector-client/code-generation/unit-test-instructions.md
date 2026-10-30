# Unit Test Instructions — connector-client (U7)

Exact unit-scoped commands. Run from the repo root. NOTE: this intent's
workflow verification is CI-deferred (the unit-checkpoint command
`git diff --check && git status --porcelain` was swapped in under the
standing "rely on CI, no local cargo/bun builds" directive) — the cargo
commands below are the CI-equivalent shapes for reference, not a local
run instruction.

## Rust unit + integration (sqlite, no DATABASE_URL)

```sh
# the connector-client unit's crate
cargo test -p oxidean-cli
# or via the workspace archive path used everywhere else:
cargo nextest run -p oxidean-cli
```

## Connector wire integration binary

```sh
# wiremock C3 fixtures — all four routes × wire shapes, every error
# code → classifier arm, retry_after/Retry-After honored, cancel-set
# applied, pair error vocabulary. Mirrors tests/rpc_http.rs's wiremock
# boot + ServerGuard patterns; no live server needed.
cargo test -p oxidean-cli --test pilot_connector
```

## Colocated unit tests

```sh
# src/pilot/ #[cfg(test)] modules: jail chains, env filter, tail
# buffer, scrub set, settled dedupe, custody backends (tempfile),
# dispatch-table totality
cargo test -p oxidean-cli --lib pilot
```

## Lint/format gates (CI-equivalent)

```sh
cargo fmt --check -p oxidean-cli
cargo clippy -p oxidean-cli --all-targets -- -D warnings
```

## CLI parity gate (same-change CI wiring)

```sh
# exercised inside the existing rpc-sync job against the rust-bins
# artifact; locally requires a built target/debug/ox
OX_BIN=target/debug/ox ./scripts/check-cli-parity.sh
```

## Whole-suite verification (unit-checkpoint command)

```sh
git diff --check && git status --porcelain
```

(engine-pinned Construction Verification Command — deliberately the
CI-deferral check for this intent; `make test` runs in CI lanes
`rust-build`/`api-rust` shards + `ox-cli.yml` on the PR.)
