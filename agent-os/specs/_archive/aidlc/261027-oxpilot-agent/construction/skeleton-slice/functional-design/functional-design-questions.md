# Functional Design Questions — u1-skeleton-slice

Unit: `skeleton-slice` (U1, kind `service`) — the walking-skeleton
end-to-end chat slice: `oxidean-api-oxpilot` crate, minimal procs, two
SSE routes, session/message tables, stub provider, minimal `/oxpilot`
page, flag gate, env docs.

## Sources

- [unit] `inception/units-generation/unit-of-work.md` §U1.
- [contract] `inception/contract-design/contract-summary.md` — C1, C2,
  C4, plus C5/C8 skeleton subset.
- [req] FR1.* (route + session surfaces), FR2.4 (server-side persistence),
  FR6.2–FR6.6 (crate/procs, SSE, provider env, flag pattern, tables);
  NFR1 (redaction), NFR4.
- [stories] AC5.5.1, AC5.5.3, AC5.5.4(partial), AC5.1.1(partial),
  AC5.1.4(partial), AC5.1.5, AC1.1.3(partial), AC1.2.2(partial),
  AC1.3.1(partial), AC1.3.3(partial).

## Q1. Entity set for the skeleton?

- A. Two persisted entities — `OxpilotSession` and `OxpilotMessage` (minimal columns: ids, user_id, repo/branch binding, role, content, seq, timestamps) — plus two computed views: `EnablementState` (env default + DB override + provider presence → enabled/reason) and `ProviderHandle` (the `LlmProvider` trait slot on `RpcCtx`; stub impl only in U1).
- B. Persist queued messages and diffs now — premature, U2 owns those columns.
- C. In-memory only — violates AC1.3.3's persistence share.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — 2 persisted entities + 1 in-memory turn-lock entity
(`ActiveTurn`, added at review R-03) + 2 computed views.
Conductor-decided under standing instruction (2026-10-27); ratify at
PR review.

## Q2. Enablement gate semantics in the skeleton?

- A. `OxpilotGate::enabled(ctx)` = env `OXIDEAN_OXPILOT_ENABLED` (default **off** — this is a new unproven feature, unlike MCP's default-on) AND provider slot present AND (optional) DB override `oxpilot_settings.enabled` when a row exists. `oxpilot.status` is gate-exempt and returns `{enabled, provider, reason}`; every other proc/route denies `oxpilot.disabled`. U4 adds the `oxpilot_settings` table + `admin.oxpilot.*`; U1 ships the env half + the slot check + a stub-override hook point.
- B. Default on like MCP — wrong for a new unproven agentic feature.
- C. Gate on env only, no DB override seam — contradicts AC5.1.3 pattern.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — env default OFF + DB override seam + provider slot, `oxpilot.status` gate-exempt. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q3. Skeleton proc set?

- A. Exactly three procs in `OXPILOT_PROCEDURES` + dispatch arms: `oxpilot.status` (gate-exempt), `oxpilot.session_create`, `oxpilot.session_list` — all classified SessionOnly by absence from `proc_classes`, proven by a closed-set test that also fails if a future edit leaks any `oxpilot.*` name into a PAT tier.
- B. Register all ~20 planned proc names now with stub handlers — bloats the pinned set before contracts harden.
- C. Only `status` — too thin to prove the session path.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — three procs, SessionOnly-by-omission + closed-set test. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q4. Skeleton SSE behaviour?

- A. `POST /api/oxpilot/sessions/{id}/chat` validates session ownership, appends the user message, streams the stub provider's reply as `append` frames then a `terminal{result:done}`; `GET .../chat/stream` returns a `snapshot` frame of persisted rows then live `append`s. No queueing, no tools, no resume-offset in the skeleton — C2's full vocabulary is pinned but only snapshot/append/terminal/ping are emitted.
- B. Full C2 contract now — overruns the skeleton boundary.
- C. Polling only — violates AC1.3.1's SSE requirement.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — subset emission of the pinned frame vocabulary. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q5. Skeleton page scope?

- A. Minimal `/oxpilot` index: auth-gated shell (SHELLS + astro + `.tsrx` + RESERVED_TOP_SEGMENTS + routeTree + coverage manifest), session list (from `session_list`), new-session form (repo, branch, goal — real fields, no target select yet), and a bare workspace view with transcript + composer wired to the SSE POST. No rail/drawer/cards — those are U8 ACs.
- B. Full mockup fidelity now — collides with U8's boundary.
- C. Placeholder "coming soon" page — doesn't prove the round trip.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — minimal real round-trip page. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q6. Table/migration shape?

- A. `oxpilot_sessions` + `oxpilot_messages` with the full column set U2 will need (avoids a second ALTER migration), foreign-keyed to users, indexes on (user_id, last_activity_at); `dialect_oxpilot.rs` in the `db-dialects` `-E` filter + TESTING.md row in the same change.
- B. Minimal columns now, ALTER in U2 — adds a migration round-trip for no benefit.
- C. Reuse an existing table — contaminates ownership.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — full-shape tables in one migration set. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Consolidated Summary Confirmation

- Entities: `OxpilotSession`, `OxpilotMessage` (full-shape) +
  `ActiveTurn` (in-memory turn lock) + views `EnablementState`,
  `ProviderHandle` (Q1, Q6).
- Gate: env-default OFF + DB-override seam + provider-slot; `status`
  gate-exempt (Q2).
- Procs: `status`, `session_create`, `session_list` — SessionOnly by
  omission + closed-set test (Q3).
- SSE: POST chat + GET stream emitting snapshot/append/terminal/ping
  (Q4).
- Page: minimal real round-trip — list, new-session form, transcript,
  composer (Q5).

Does this all look correct before I write the functional-design artifacts?

- Looks correct
- Request changes

[Answer]: Looks correct
