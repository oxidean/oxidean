# Unit Test Instructions — u9-vcs-mcp

Runner: `cargo nextest run` (workspace convention; `cargo test` equivalent for iteration).

## Commands

```bash
# Crate-side descriptor tests (client feature — default):
cargo nextest run -p oxidean-vcs mcp::

# Host wiring tests (oxidean-api mcp module tests):
cargo nextest run -p oxidean-api mcp

# Feature-discipline check (client-only compile — descriptors ship in `client`):
cargo check -p oxidean-vcs --no-default-features --features client

# Full-crate passes before unit close:
cargo nextest run -p oxidean-vcs --all-features
cargo nextest run -p oxidean-api
```

## Matrix mapping

- `mcp_surface` → `oxidean-vcs` `client::mcp::tests` — 13-descriptor count, name↔procedure map, `owner`/`name` required on every schema, wire-DTO `required` parity, access-tier split, absence assertions.
- `host_wiring` → `oxidean-api` `mcp::tests` — `tool_defs()` vcs rows, `Access` mapping assertions, `build_rpc_input` passthrough + `vcs_log` `from` injection, `tool_public` JSON shape.
- `dependency` → `cargo check` client-only build (DD-1/AC8.3.3 structural check).
