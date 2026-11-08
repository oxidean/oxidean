# Tech-Stack Decisions — providers (U4)

Consumes: `construction/providers/functional-design/rules.md`
(BR1.2, BR4.x, BR5.3, BR5.5), `codekb/oxidean-app/technology-stack.md`.

## Dependencies — all existing, zero new

| id | decision | basis |
|----|----------|-------|
| NFR7.1 | Outbound provider HTTP is `reqwest` only — OpenAI chat-completions SSE, Anthropic messages SSE, Ollama `/api/chat` NDJSON. NO provider SDK crates: three HTTP calls don't justify heavyweight deps, and reqwest is already in the tree | FD Q4, BR5.3 |
| NFR7.2 | One shared SSE/NDJSON line-decoder maps provider frames to `ChatStreamItem` — per-adapter code is request-shaping + frame classification only | WF4, C4 |
| NFR7.3 | Key custody reuses `crate::crypto` AES-256-GCM (ring) under `["OXIDEAN_OXPILOT_SECRETS_KEY","OXIDEAN_SESSION_SECRET"]` — no new crypto dependency, no new custody mechanism | Q2, BR1.2 |

## Runtime primitives

| id | decision | basis |
|----|----------|-------|
| NFR7.4 | The swappable holder is `RwLock<OxpilotRuntimeSlot>` with a `tokio::sync::Mutex` rebuild lock — std tokio primitives already in the tree; no `arc_swap` dep needed | BR4.3, BR5.5, Q4 |
| NFR7.5 | `oxpilot_settings` follows the `mcp_settings` dialect tri-store pattern — sqlite/postgres/mysql migrations + `dialect_oxpilot.rs` coverage in the same change (BR5.4) | BR5.4, AC5.5.3 |

## Testing isolation

| id | decision | basis |
|----|----------|-------|
| NFR8.1 | `wiremock` dev-dependency fixtures for every adapter — golden request bodies, canned SSE/NDJSON, error-code mapping, timeout path; NEVER a live provider call and no network egress in tests | BR5.3, Q8 |
| NFR8.2 | `admin.oxpilot.*` lives inside `oxidean-api-oxpilot`'s closed PROCEDURES set + dispatch — NFR9's collect-only coverage falls out of the same pinning test pattern U1 shipped | BR5.4, NFR9 |

## Non-goals

- No provider SDK crates, no OpenAI-compatible "generic" adapter (the
  Anthropic messages API is different enough to corrupt the seam).
- No connection-test proc — `admin.oxpilot.test_connection` is additive
  scope for a later change (FD Q6).
