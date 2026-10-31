# Infrastructure Specification — oxpilot-ui (U8)

Consumes: `nfr-design/*`, `nfr-requirements/*`,
`functional-design/{rules,functional-spec,frontend-components}.md`,
`inception/domain-design/components.md` (`OxpilotWeb` — "local UI
state only… the UI enforces nothing itself"),
`contract-summary.md` (C1 consumed + flagged amendments; C2),
`infrastructure-design-questions.md` (Q1–Q8), `team.md` (Testing
Posture, Deployment), `apps/web/Dockerfile`,
`skeleton-slice/infrastructure-design/*` (U1's shipped topology
this unit extends).

## Deployable surface

U8 adds **no deployable, no sidecar, no listener, no route
registration**. The unit is render surface inside the shipped
`web` service: `apps/web` builds one Astro static bundle and the
`oxidean-web` Rust middleware serves it (static assets + `/api/`
proxy + SPA fallback + theme/meta injection + auth gate —
`apps/web/Dockerfile:1-3` header contract). What ships:

| surface | change | infra impact |
|---------|--------|--------------|
| `apps/web` routes | `routes/oxpilot.tsrx` + `routes/oxpilot.$session.tsrx` reworked to the M1/M2 composition — the U1 skeleton's seed/attach/dedupe core extended | none — same two routes, already registered in `SHELLS`/`RESERVED_TOP_SEGMENTS`/routeTree by U1 |
| `apps/web` feature dir | **new** `components/oxpilot/*.tsrx` — 14 files (`flag-off`, `new-session-dialog`, `session-row`, `target-row`, `connect-dialog`, `confirm-dialog`, `sessions-rail`, `transcript`, `tool-call-card`, `diff-card`, `consent-card`, `queued-row`, `composer`, `context-drawer`) | none — compiled into the same bundle; knip covers the graph |
| `apps/web` libs | `lib/oxpilot-stream.ts` + `lib/oxpilot-queries.ts` extended (`tool_call`/`card` frame variants, `es.onerror` probe, the proc-read/mutation set); `lib/oxpilot-chat.tsrx` unchanged | none — framework-free seams inside the same bundle |
| `apps/web` settings | `routes/settings/ai.tsrx` gains an `AppLink` to `/oxpilot#targets` | none |
| server side | **zero** — every proc the UI calls (`oxpilot.*`, `repo.listMine`/`repo.refs`) and both SSE routes are U1–U6's shipped surface | none |
| deps | none — `@tanstack/ai-octane` + `@octanejs/tanstack-query` + the GFM/diff/toast libs already linked (`apps/web/package.json`); `bun.lock` untouched | `bun install --frozen-lockfile` unchanged |

## Environment

No new variables anywhere — and the UI reads **no env at all**.
The flag-off and `budget_exceeded` surfaces render the NAMES
`OXIDEAN_OXPILOT_ENABLED`/`OXIDEAN_OXPILOT_PROVIDER` as fix-naming
copy (NFR8.1/NFR8.2 + `docs/CONFIGURATION.md` link); the VALUES
live server-side on U1/U4's documented chain
(`.env.example:166-167`, `docs/CONFIGURATION.md:398-399`,
`.railway/railway.ts:97-99` `preserve()` rows — U1's surface,
untouched). The `oxpilot.status` probe (gate-exempt, `retry:false`
— `lib/oxpilot-queries.ts`) is the only read seam the UI needs.

## Distribution surface (existing — nothing added)

The UI rides the `web` image pipeline unchanged:

| channel | how U8 rides it |
|---------|-----------------|
| `images.yml` `web` svc | `apps/web/Dockerfile` rebuilds the Astro bundle → `oxidean-web` runtime in the same four-service `sha-<sha>`/`pr-<n>`/`web-v*` tag flow (frontend app + Rust serving crate share the image — `images.yml:11-14`) |
| route-level code split | stays as shipped — the two `/oxpilot*` routes; no new lazy boundary (NFR3.11) |
| deploy topology | unchanged — Traefik/edge → web service; the SSE routes the client attaches are api-side under `/api/` (U1's topology; the web tier's `text/event-stream` compression exclusion already carries them) |
| flag posture | `OXIDEAN_OXPILOT_ENABLED` defaults off ⇒ a green deploy ships the full surface dark; enablement is an operator env flip, not a deploy |

## Operator provisioning

Zero. Nothing is provisioned for a user to reach the surfaces:
repo binding is the session row, identity is the caller's
session, enablement is the instance flag U1 documents. The
pairing code the connect dialog displays is minted by
`pairing_create` at click time (U6's surface) — no pre-issued
material exists on the UI path.

## Amendments — flagged, not shipped

Three consumer-proposed additive deltas are flagged per the
contract's deviation rule (functional-spec §Contract deltas).
This stage records their infra posture only:

| amendment | U8-facing need | infra posture |
|-----------|----------------|---------------|
| `session_create.target_id` + `session_get`/`session_list.computer_use_target_id` | rail chips, drawer bound-target panel, M3 target picker | additive wire fields via `make rpc-gen` — the `rpc-sync` gate regenerates `packages/api-client`; refused ⇒ picker + binding drop, zero schema change |
| `grant_list.session_id` | the 4-read mount/reseed card-recovery set | additive request field via `make rpc-gen`; refused ⇒ recovery narrows to `diff_list` + the Manage-expander `grant_list{target_id}` path — recorded, never silent (NFR4.9) |
| `0090_oxpilot_session_target.sql` | the backing column for `computer_use_target_id` | **U8-owned, db-layer artifact** — additive `oxpilot_sessions` column ×3 dialects under `crates/oxidean-db/migrations/` (TS-10; U1 deferred it, U6's `0088` shipped without it). Append-only like `0085`–`0089`; applied by the existing boot runner; replayed by the `dialect_oxpilot` leg already in `db-dialects`' `-E` filterset (`ci.yml:902`). Cited here — the dialect-parity contract lives in `oxidean-db`, not re-specified |

Amendment #3 (`queue_delete`) is proc-surface only — no infra
footprint beyond the same `rpc-gen` regeneration path (the
functional design numbers three amendments; `0090` is amendment
#1's backing db artifact, not a fourth amendment).

## Restart/reseed invariants (client)

- All UI state is route-local memory — TanStack Query cache +
  `useState`; NOTHING persists to browser storage (SEC-18). A
  remount reseeds from the server: `fetchOxpilotSnapshot` +
  `sessionGet` + `grantList{session_id}` + `diffList{session_id}`
  folded into one keyed remount (`s-${version}`) — server
  state is the single consistency source (NFR-S7).
- Exactly ONE EventSource per mounted workspace; the effect
  return closes it — stream count cannot grow across navigation
  or remount (NFR-S5). Readers cancel in `finally`/on
  `terminal`; nothing writes into a dead mount (REL-4).
- A code rollback is inert: the bundle is stateless, so an image
  repin leaves no client residue; the additive `0090` column
  (if the amendment lands) is harmless to an older bundle —
  unread fields are simply absent on the wire.

## Resource shape (client)

- Per-frame cost is O(delta): dedupe `Set` lookup + `liveTail`
  string extend — the full transcript never re-renders per
  frame; the only wholesale replace is the snapshot reseed.
- Bundle weight: zero new deps; the diff surfaces ride the
  shipped `lazy-diff-rows` viewport-scaled seam + a 200 000-char
  soft max — render cost scales with viewport, not patch length.
- Request budget: one attach stream + the 4-read mount/reseed
  set + ≤1 card-ref read per `card` frame + one bounded pairing
  poll (`PAIRING_POLL_MS` 2 s inside the dialog, capped by
  `expires_at`) — push-first, zero `refetchInterval` anywhere.
