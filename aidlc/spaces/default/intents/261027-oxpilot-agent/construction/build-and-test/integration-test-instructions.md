# Integration Test Instructions — oxpilot

Strategy: **Standard** — key boundary tests + cross-unit interaction.

## Rust proc/dialect integration (cross-unit: session-core ↔ agent-runtime ↔ providers ↔ oxpilot-tools ↔ connector-gateway)

```bash
cargo nextest run -p oxidean-api --test oxpilot        # 20-proc surface incl. SSE + turn_cancel + queue_delete
cargo nextest run -p oxidean-api-oxpilot              # crate units (runtime, session, tools, grant views)
cargo nextest run -p oxidean-api-oxpilot --test oxpilot_settings
cargo nextest run -p oxidean-api-core -E 'test(oxpilot)'
cargo nextest run -p oxidean-db -E 'binary_id(oxidean-db::dialect_oxpilot)' -j 1
cargo test -p oxidean-api-oxpilot --test oxpilot_connector
cargo test -p oxidean-cli --test pilot_connector       # connector-client ↔ gateway loop
```

## Web integration (cross-unit: oxpilot-ui ↔ generated client ↔ stream lib)

```bash
bunx vitest run apps/web/src/routes/oxpilot.integration.test.ts    # 53 tests — route + all components
bunx vitest run apps/web/src/routes/oxpilot.browser.test.tsx --project browser   # Chromium DOM-race
bunx vitest run apps/web/src/lib/oxpilot-stream.unit.test.ts
```

## End-to-end stack (full assembly boundary)

```bash
bunx vitest run e2e/stack/oxpilot.stack.test.ts --project e2e-stack   # requires make up-with-dev-auth
```

## Coverage / drift gates (cross-cutting)

```bash
make rpc-sync-check        # Rust RPC ↔ generated TS client
make route-coverage-check && make browser-coverage-check
make dead-code-check
```

## Notes

- Local heavyweight Rust/Bun runs are deferred to CI per user directive; CI runs each command once per the deduplicated list above and is the authoritative result.
- The e2e-stack test accepts the contract-legal zero-or-more `append*` frames — a fast stub turn may deliver the reply via `snapshot.messages` instead.
- Expected: all suites green; `dialect_oxpilot` runs `-j 1` (serial, shared DB fixture).
