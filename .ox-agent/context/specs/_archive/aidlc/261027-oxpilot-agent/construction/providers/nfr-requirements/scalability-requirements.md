# Scalability Requirements — providers (U4)

Consumes: `construction/providers/functional-design/rules.md`
(BR1.4, BR2.1, BR4.3, BR3.3),
`inception/requirements-analysis/requirements.md` (NFR3).

## Data growth

| id | requirement | basis |
|----|-------------|-------|
| NFR3.9 | `oxpilot_settings` is a singleton row (id = 1) — reads are a fixed one-row lookup; no per-user/per-session growth in this unit | entities, BR2.1 |
| NFR3.10 | The provider registry is a closed static set — adding a provider is a code change + contract bump, never a runtime row or per-tenant registry growth | BR1.4 |
| NFR3.11 | Budget values stored here are two integers on the singleton row — counters and their growth are U2's tables, not U4's | BR3.3, AC5.2.1-Deferred |

## Concurrency shape

| id | requirement | basis |
|----|-------------|-------|
| NFR3.12 | Provider egress concurrency inherits U1's one-live-turn-per-session bound — adapter load is turn-bound, not adapter-managed; no additional throttling inside U4 | U1's turn bound (ActiveTurns), FD Q4 |
| NFR3.13 | The rebuild mutex serializes admin writes only — request-path reads take a read lock and never queue behind a rebuild | BR4.3 |
| NFR3.14 | Slot swaps are O(1) pointer swaps; the shared `Arc<ActiveTurns>` means rebuilds cost nothing proportional to live sessions | entities, BR4.3 |

## Multi-replica semantics

| id | requirement | basis |
|----|-------------|-------|
| NFR3.15 | Each replica rebuilds its slot independently (boot + local settings_set) — a settings write on replica A does not propagate to replica B until that replica rebuilds; the documented bound is "replicas converge on next rebuild/boot", accepted for v1 admin-rare writes | BR4.3, Q4 |
