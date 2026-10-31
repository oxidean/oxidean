# Unit Test Instructions — connector-gateway (U6)

Exact unit-scoped commands. Run from the repo root.

## Rust unit + integration (sqlite, no DATABASE_URL)

```sh
# the connector unit's crates, whole workspace build not required
cargo test -p oxidean-db -p oxidean-api-core -p oxidean-api-oxpilot
# or via the workspace archive path used everywhere else:
cargo nextest run -p oxidean-db -p oxidean-api-core -p oxidean-api-oxpilot
```

## Connector route/proc integration binary

```sh
# assembled-router integration — C3 auth matrix (401/429/403-disabled/
# body-mismatch), pair→consent→claim→report→heartbeat, claim park/wake,
# Denied shape, disconnect/revoke/session-delete close paths, audit
# secret-absence spy. Serializes on its own env mutex; no engine needed.
cargo test -p oxidean-api-oxpilot --test oxpilot_connector
```

## Dialect parity (real engines — same invocation CI uses)

```sh
# sqlite leg (always works — dialect_oxpilot self-skips cleanly)
DATABASE_URL=sqlite::memory: cargo nextest run -p oxidean-db --test dialect_oxpilot --test-threads=1
# postgres/mysql legs require a provisioned engine:
DATABASE_URL=postgres://oxi:oxi@localhost:5432/oxpilot_test cargo nextest run -p oxidean-db --test dialect_oxpilot --test-threads=1
DATABASE_URL=mysql://root:oxi@localhost:3306/oxpilot_test cargo nextest run -p oxidean-db --test dialect_oxpilot --test-threads=1
```

`--test-threads=1` is required — the binary serializes on the shared
`DATABASE_URL` (same caveat `db-dialects` documents for sqlite).

## RPC drift check (mandatory same-change)

```sh
make rpc-gen            # regenerate packages/api-client — COMMIT the diff
make rpc-sync-check     # the CI gate's check
```

## Lint/format gates (CI-equivalent)

```sh
cargo fmt --check -p oxidean-db -p oxidean-api-core -p oxidean-api-oxpilot
cargo clippy -p oxidean-db -p oxidean-api-core -p oxidean-api-oxpilot --all-targets -- -D warnings
```

## Whole-suite verification (unit-checkpoint command)

```sh
make test
```

(engine-pinned Construction Verification Command — runs the full
workspace nextest + web legs; green is required for checkpoint
verification.)
