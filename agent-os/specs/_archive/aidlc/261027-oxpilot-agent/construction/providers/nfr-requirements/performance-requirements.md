# Performance Requirements — providers (U4)

Consumes: `construction/providers/functional-design/rules.md`
(BR4.x, BR5.5), `functional-design/entities.md`,
`inception/requirements-analysis/requirements.md` (NFR3),
`codekb/oxidean-app/technology-stack.md`.

Provider wall-clock latency and model throughput are EXTERNAL — this
unit bounds OUR overhead: the gate read, the rebuild path, and the
stream-translation cost.

## Hot-path overhead

| id | requirement | basis |
|----|-------------|-------|
| NFR3.1 | Enablement reads on the request path are a read-lock clone of `OxpilotRuntimeSlot` — NO per-request DB read; the resolved projection is memoized in the slot | BR5.5, Q4 |
| NFR3.2 | The `oxpilot_settings` row is read only inside a rebuild (boot, settings_set) — never per chat request, per stream frame, or per `oxpilot.status` call | BR4.3, Q4 |
| NFR3.3 | Stream adaptation adds line-decode only — provider frames map to `ChatStreamItem` as they arrive; no full-response buffering, no extra copy of stream payloads | WF4, C4 |

## Provider client efficiency

| id | requirement | basis |
|----|-------------|-------|
| NFR3.4 | One `reqwest::Client` per built adapter — DNS/TLS/connection pooling shared across turns; no per-turn client construction | BR4.2 |
| NFR3.5 | Key decryption is paid once per provider build (construction memory), never per turn and never on a read path | BR1.2, Q2 |

## Rebuild path (admin-path, not request-path)

| id | requirement | basis |
|----|-------------|-------|
| NFR3.6 | `settings_set` rebuild cost is bounded to one settings-row read + one adapter construction (local, no provider network call at build); it runs under the rebuild mutex and never on the request path | BR4.3, Q4 |
| NFR3.7 | Boot-time initial slot build performs no provider network call — construction is local; boot latency impact is a settings-row read | Q5, BR5.5 |

## Boundedness

| id | requirement | basis |
|----|-------------|-------|
| NFR3.8 | Registry descriptors are static and bounded (4 entries); `providers_list` is O(registry) — no query, no fan-out | BR1.4 |
