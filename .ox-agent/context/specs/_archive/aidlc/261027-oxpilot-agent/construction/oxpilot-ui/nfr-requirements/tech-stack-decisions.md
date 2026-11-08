# Tech-Stack Decisions — oxpilot-ui (U8)

Consumes: functional-design questions Q1–Q12 answers,
`codekb/oxidean-app/technology-stack.md`,
`functional-design/frontend-components.md` (component map + API
integration points + state ownership), shipped `apps/web` files
(`lib/oxpilot-stream.ts`, `lib/oxpilot-chat.tsrx`,
`lib/oxpilot-queries.ts`, `lib/{markdown,parse-unified-diff,
highlight-diff,lazy-diff-rows,relative-time,toast}.ts`,
`components/settings/copy-command.tsrx`, `components/ui/*`),
`inception/refined-mockups/design-system-mapping.md`,
`inception/contract-design/contract-summary.md` (C1, C2).

## Decisions

| id | decision | rationale |
|----|----------|-----------|
| TS-1 | Octane `.tsrx` for every component — `function X() @{ … }`, `@if`/`@for`/`@switch`, `onInput` — NEVER React JSX patterns; feature components live under `apps/web/src/components/oxpilot/` kebab-case | Mandated web UI runtime ([codekb] `octane ^0.11.0` + `@octanejs/astro`; AGENTS constraint "Octane `.tsrx` only"); the ds-map's pinned feature-dir layout + the `components/settings/` precedent (FD Q1-A) |
| TS-2 | TanStack Query (`@octanejs/tanstack-query ^0.1.63`) owns ALL proc reads/mutations — `lib/oxpilot-queries.ts` queryOptions + mutation helpers + `invalidateOxpilotQueries`; `staleTime` 15 s status / 10 s lists, `retry:false` on auth-gated probes, ZERO `refetchInterval` | Shipped seam already carries status/session-list; the ds-map state-ownership table pins server state on Query and local state on `useState` — never a global store ([codekb], BR8.1, [ds-map]) |
| TS-3 | Transport split stays shipped: `POST …/chat` streams over a fetch `ReadableStream` (a request body rules out `EventSource`); `GET …/chat/stream` attaches over `EventSource` with native auto-reconnect — no WebSocket, no `Last-Event-ID`, no custom backoff | FD Q4-A + C2 verbatim; the shipped `openOxpilotStream`/`postOxpilotChat`/`fetchOxpilotSnapshot` seams extend (`tool_call`/`card` union variants + the `es.onerror` probe), never replaced |
| TS-4 | Mockup components map 1:1 onto shipped primitives — `Dialog` (M3 new-session, M4 connect two-step), `AlertDialog` (M5 confirms), `Checkbox`/`RadioGroup`/`Select` (dialog fields), `EmptyState` (M1/M7/M8 states), `Button`/`Input`/`Label`/`Textarea`/`List`/`Badge`/`toaster`; `CopyCommand` reused from `components/settings/` | [ds-map]'s primitive mapping + [shipped] inventory — every mockup element already has a primitive; a11y pins (focus trap, `aria-label`, labelled inputs) ride the primitives' shipped behavior (BR10.2, FD Q2-A/Q10-A) |
| TS-5 | Zero new dependencies — every library the unit needs already ships: `renderGfm`/`markdown` for completed assistant text (unified 11 + remark/rehype incl. sanitize — [codekb]), `parse-unified-diff`/`highlight-diff`/`lazy-diff-rows` for the diff surfaces, `relative-time` for row timestamps, `toast.ts` for mutation toasts | Q3-A + dependency discipline: the render needs are exactly the libs the shipped UI already links; a closed posture extends to `package.json` |
| TS-6 | All proc calls ride the generated `apiClient` — `apiClient.oxpilot.*` verbatim (incl. `target_*`/`grant_*`/`diff_*`/`queue_list`/`usage_get`/`turn_cancel`) + `repo.listMine`/`repo.refs`; the flagged C1 amendments (`session_create.target_id`, `grant_list{session_id}`, `queue_delete`) arrive through `make rpc-gen` outputs, never hand-authored types | C1 + AGENTS rpc-gen rule ("change Rust → `make rpc-gen`; never treat hand-edited `packages/api-client` as source of truth"); generated client is the typed seam ([codekb], spec Contract deltas) |
| TS-7 | Text rendering split: plaintext `whitespace-pre-wrap` while `streaming`; `renderGfm` ONCE on `complete`; tool/card summaries verbatim `<code>`; diff patches through the escaped highlighter — never HTML injection anywhere | FD Q8-A + BR5.2/BR5.3 — partial GFM re-parses wrongly per delta (NFR3 cost + flicker); the sanitize-carrying shipped toolchain is the only markup path |
| TS-8 | `crypto.randomUUID()` per send/queue mints `client_msg_id` — the C2 idempotency key; no custom id scheme | WF4.1 + C2; shipped pin kept verbatim (spec Pins) |
| TS-9 | Tests ride shipped infra: Vitest projects + happy-dom for `oxpilot.integration.test.ts` (extended), `@octanejs/testing-library`/Playwright browser project for `oxpilot.browser.test.tsx` (per the manifest rows), stream seam covered by `oxpilot-stream.unit.test.ts` — manifests in `test/route-coverage.manifest.ts` + `test/browser-coverage.manifest.ts` gain the pinned rows | BR11 + FD Q12-A + [codekb] test tooling — NFR9's route + browser coverage rows; no new test stack, no stack-browser e2e (spec Scope) |
| TS-10 | Server-side codegen rides the repo's own path: the flagged amendment #1's additive column lands as `crates/oxidean-db/migrations/{mysql,postgres,sqlite}/0090_oxpilot_session_target.sql` ×3 dialects in this unit's codegen — sqlx only through `oxidean-db` | spec Contract deltas + FR6.6 ("migrations for all three dialects") + [codekb] ("DB dialect rules stay inside `crates/oxidean-db`") — single-line ownership pinned at FD Q3-A |

## Accessibility & usability conformance (this unit OWNS NFR7/NFR8)

| id | requirement | basis |
|----|-------------|-------|
| NFR7.1 | Landmark/heading discipline: exactly one `h1` per route; `nav`/`main`/`complementary`/`form` landmarks; sequential heading order (h2 "Sessions"/"Computer use") | BR10.1, spec Accessibility |
| NFR7.2 | Live-region choreography: transcript `aria-live="polite"` + `aria-atomic="false"`; announcements fire at message START/END boundaries only — never per stream frame; consent cards announce once on appearance | BR10.1, WF6/WF7, [a11y] |
| NFR7.3 | Dialog semantics: every `Dialog`/`AlertDialog` carries a visible label or `aria-label`, focus trap, Esc dismissal; the connect dialog's step swap re-announces the label ("Pair the connector"); confirms focus Cancel on open | BR10.2, BR7.1, BR6.3 |
| NFR7.4 | Interactive honesty: status never color-only (text labels everywhere); row actions `focus-within`-revealed, never hover-only; Deny always keyboard-reachable; real labelled `Checkbox`/`Input` fields; contrast ≥4.5:1 text / ≥3:1 badges | BR10.3, BR6.1, BR1.3 |
| NFR7.5 | Keyboard contract: composer carries visible `Label` "Message OxPilot" + `Ctrl/⌘+Enter` `aria-describedby` hint BACKED by the pinned keydown handler (Enter = newline, `Ctrl/⌘+Enter` = send — the handler IS the pinned behavior); Esc in a pane returns focus to its toggle; route open focuses the composer; the autoscroll/jump-pill scroll contract holds | BR10.4, BR5.6, spec Accessibility |
| NFR8.1 | The flag-off state is explanatory, never a dead page: `EmptyState` naming the reason (`flag_off`/`misconfigured`/`no_provider`), `OXIDEAN_OXPILOT_ENABLED` + `OXIDEAN_OXPILOT_PROVIDER`, the `docs/CONFIGURATION.md` link — and ZERO interactive controls | WF1.3, AC5.1.1, NFR8, FR1.5 |
| NFR8.2 | Typed errors name the fix: `budget_exceeded` renders the budget notice naming the `OXIDEAN_OXPILOT_*` env vars + CONFIGURATION.md; per-code banners/notice text carry the remedy (`turn_active` → the queued path, `pairing_expired` → retry, `not_found` → back link); never a bare code, never a dead spinner | BR9.1/BR9.2, AC5.2.2, WF12 |

## Rejected alternatives

- React/`react-dom` or any parallel component framework — Octane is
  the mandated UI runtime; rejected outright (TS-1).
- WebSocket for the stream — a second protocol for no gain;
  `text/event-stream` already rides the compression exclusion
  (rejected at FD Q4-D).
- Fetch-reader on the GET route + manual backoff — re-implements
  native reconnect badly (rejected at FD Q4-B, Q5).
- `Last-Event-ID` resume — C2 deliberately has none; inventing it
  is a contract change (rejected at FD Q4-C).
- A `zustand`/global store for oxpilot state — [ds-map] pins local
  `useState` + TanStack Query ownership; the stream is a dedicated
  channel into local state (TS-2, TS-9 adjacency — no store added).
- Incremental-GFM streaming parser dep — per-delta re-parse was
  rejected at FD Q8-C (worst of both: parse cost + partial-fence
  flicker); plaintext-while-streaming + render-once-on-complete is
  the pinned split (TS-7).
- A `type:"tool"` discriminant arm — the wire emits
  `type:"tool_call"` VERBATIM (C2 yaml; verified
  `oxidean-api-oxpilot/src/runtime.rs`); a `"tool"` arm never
  matches and would silently drop every tool frame (TS-3 adjunct).
- New telemetry/beacon/error-reporting dep — spec Observability
  pins zero UI-side telemetry (TS-5 + NFR6.1).
- Playwright stack-browser e2e suites — out of scope per spec; the
  shipped manifest rows cover the pinned surfaces (TS-9).
