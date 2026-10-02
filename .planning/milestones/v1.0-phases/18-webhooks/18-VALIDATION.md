# Phase 18: Webhooks - Validation Strategy

**Nyquist validation** for HOOK-01, HOOK-02, HOOK-03.

## Wave 0 Gaps

| Gap | Stub file | Turns green in |
|-----|-----------|----------------|
| webhook CRUD RPC | `crates/oxidean-api/tests/webhook_rpc.rs` | 18-01, 18-02 |
| delivery + HMAC | `crates/oxidean-api/tests/webhook_delivery.rs` | 18-01, 18-02, 18-03 |
| dialect migration | `crates/oxidean-db/tests/dialect_webhooks.rs` | 18-01 |
| settings Webhooks UI | `apps/web/src/routes/$owner.$repo.settings.webhooks.integration.test.ts` | 18-04 |

## Requirements ↔ Tests

| Req | Automated | Notes |
|-----|-----------|-------|
| HOOK-01 | `cargo nextest run -p oxidean-api -E 'test(webhook_)'` + Vitest settings webhooks | Admin CRUD |
| HOOK-02 | `webhook_delivery` + issue/push/PR emit filters | Mock HTTP sink |
| HOOK-03 | deliveries.list assertions + Vitest history rows | Status visible |

## Prior verify commands to reuse

Prefer `cargo nextest run -p oxidean-api -E '…'` and `bun`/`make` web test filters already used in Phase 15 Wave 0.
