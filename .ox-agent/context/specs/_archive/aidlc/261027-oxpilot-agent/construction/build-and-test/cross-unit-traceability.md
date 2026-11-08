# Cross-Unit Final Coverage Gate — Build and Test

## Verdict

**Verdict:** PASS — every enumerated FR/NFR/AC is covered with status `OK` (direct, via children, or mapped through the functional-design chain) except the three code-generation `Deferred` rows noted below; every `Deferred` row names the unit that owns the deferred element and that unit's own coverage is `OK`. No uncovered element remains.

## Method

Enumeration: every `FR`/`NFR` token in `inception/requirements-analysis/requirements.md`, every `ACx.y.z` in `inception/user-stories/stories.md`, checked against all 8 units' `construction/*/code-generation/traceability.json` `coverage` entries. Coverage states: `OK` = direct `status:"OK"` entry whose target file exists (verified on disk); `OK (mapped)` = covered under a remapped label — the functional-design traceability chain (FR→BR/WF element→code-generation target) or an explicit proc-surface entry; `Deferred` = the unit recorded `status:"Deferred"` with an owning-unit note; `OK (via children)` = heading id whose every leaf is covered.

## Coverage Table

| Requirement | Coverage | Owning unit → target |
|---|---|---|
| AC1.1.1 | OK | `oxpilot-ui` → `apps/web/src/components/oxpilot/session-row.tsrx`; `u1-skeleton-slice` → `crates/oxidean-api-oxpilot/src/session.rs; apps/web/src/lib/oxpilot-queries.ts` |
| AC1.1.2 | OK | `oxpilot-ui` → `apps/web/src/components/oxpilot/session-row.tsrx` |
| AC1.1.3 | OK | `oxpilot-ui` → `apps/web/src/test/route-coverage.manifest.ts`; `u1-skeleton-slice` → `crates/oxidean-web/src/shells.rs; apps/web/src/pages/oxpilot.astro; apps/web/src/pages/oxp` |
| AC1.1.4 | OK | `oxpilot-ui` → `apps/web/src/routes/oxpilot.tsrx` |
| AC1.1.5 | OK | `oxpilot-ui` → `apps/web/src/routes/oxpilot.tsrx`; `u1-skeleton-slice` → `apps/web/src/routes/oxpilot.tsrx; apps/web/src/routes/oxpilot.integration.test.ts` |
| AC1.2.1 | OK | `oxpilot-ui` → `apps/web/src/components/oxpilot/new-session-dialog.tsrx` |
| AC1.2.2 | OK | `oxpilot-ui` → `apps/web/src/components/oxpilot/new-session-dialog.tsrx`; `u1-skeleton-slice` → `crates/oxidean-db/src/oxpilot.rs; crates/oxidean-api-oxpilot/src/session.rs` |
| AC1.2.3 | OK | `oxpilot-ui` → `apps/web/src/components/oxpilot/new-session-dialog.tsrx` |
| AC1.3.1 | OK | `agent-runtime` → `crates/oxidean-api-oxpilot/src/routes.rs`; `oxpilot-ui` → `apps/web/src/lib/oxpilot-stream.ts`; `u1-skeleton-slice` → `crates/oxidean-api-oxpilot/src/routes.rs (post_chat)` |
| AC1.3.2 | OK | `oxpilot-ui` → `apps/web/src/routes/oxpilot.$session.tsrx`; `u1-skeleton-slice` → `crates/oxidean-api-oxpilot/src/routes.rs (run_turn); crates/oxidean-api-core/src/oxpilot.r` |
| AC1.3.3 | OK | `agent-runtime` → `crates/oxidean-api-oxpilot/src/runtime.rs`; `session-core` → `crates/oxidean-db/src/oxpilot.rs`; `oxpilot-ui` → `apps/web/src/lib/oxpilot-stream.ts` … |
| AC1.3.4 | OK | `oxpilot-ui` → `apps/web/src/components/oxpilot/transcript.tsrx`; `u1-skeleton-slice` → `crates/oxidean-api-oxpilot/src/routes.rs (emitted_deltas replay, Lagged resync)` |
| AC1.3.5 | OK | `oxpilot-ui` → `apps/web/src/components/oxpilot/tool-call-card.tsrx`; `u1-skeleton-slice` → `apps/web/src/lib/oxpilot-chat.tsrx; apps/web/src/routes/oxpilot.$session.tsrx` |
| AC1.3.6 | OK | `agent-runtime` → `crates/oxidean-api-oxpilot/src/runtime.rs`; `session-core` → `crates/oxidean-api-oxpilot/src/services.rs`; `oxpilot-ui` → `apps/web/src/components/oxpilot/tool-call-card.tsrx` … |
| AC1.4.1 | OK | `agent-runtime` → `crates/oxidean-api-oxpilot/src/runtime.rs`; `oxpilot-ui` → `apps/web/src/routes/oxpilot.$session.tsrx` |
| AC1.4.2 | OK | `agent-runtime` → `crates/oxidean-api-oxpilot/src/runtime.rs`; `oxpilot-ui` → `apps/web/src/lib/oxpilot-stream.ts` |
| AC1.5.1 | OK | `oxpilot-ui` → `apps/web/src/components/oxpilot/confirm-dialog.tsrx` |
| AC1.5.2 | OK | `session-core` → `crates/oxidean-api-oxpilot/src/services.rs`; `connector-gateway` → `crates/oxidean-db/migrations/sqlite/0088_oxpilot_connector.sql`; `oxpilot-ui` → `apps/web/src/components/oxpilot/session-row.tsrx` |
| AC1.5.3 | OK | `session-core` → `crates/oxidean-db/src/oxpilot.rs`; `oxpilot-ui` → `apps/web/src/components/oxpilot/confirm-dialog.tsrx` |
| AC1.6.1 | OK | `agent-runtime` → `crates/oxidean-api-oxpilot/src/routes.rs`; `oxpilot-ui` → `apps/web/src/lib/oxpilot-stream.ts` |
| AC1.7.1 | OK | `oxpilot-ui` → `apps/web/src/components/oxpilot/target-row.tsrx` |
| AC1.7.2 | OK | `oxpilot-ui` → `apps/web/src/routes/settings/ai.tsrx` |
| AC1.7.3 | OK | `oxpilot-ui` → `apps/web/src/routes/oxpilot.tsrx` |
| AC1.8.1 | OK | `agent-runtime` → `crates/oxidean-api-oxpilot/src/runtime.rs`; `session-core` → `crates/oxidean-api-oxpilot/src/services.rs`; `oxpilot-ui` → `apps/web/src/lib/oxpilot-queries.ts` |
| AC1.8.2 | OK | `agent-runtime` → `crates/oxidean-api-oxpilot/src/session.rs`; `session-core` → `crates/oxidean-api-oxpilot/src/session.rs`; `oxpilot-ui` → `apps/web/src/components/oxpilot/queued-row.tsrx` |
| AC2.1.1 | OK | `oxpilot-ui` → `apps/web/src/components/oxpilot/diff-card.tsrx` |
| AC2.1.2 | OK | `oxpilot-ui` → `apps/web/src/components/oxpilot/transcript.tsrx` |
| AC2.2.1 | OK | `oxpilot-ui` → `apps/web/src/components/oxpilot/transcript.tsrx` |
| AC2.2.2 | OK | `oxpilot-ui` → `apps/web/src/components/oxpilot/diff-card.tsrx` |
| AC2.2.3 | OK | `oxpilot-ui` → `apps/web/src/components/oxpilot/transcript.tsrx` |
| AC2.2.4 | OK | `oxpilot-ui` → `apps/web/src/components/oxpilot/transcript.tsrx` |
| AC2.3.1 | OK | `oxpilot-ui` → `apps/web/src/components/oxpilot/diff-card.tsrx` |
| AC3.1.1 | OK | `oxpilot-ui` → `apps/web/src/components/oxpilot/tool-call-card.tsrx` |
| AC3.1.2 | OK | `oxpilot-ui` → `apps/web/src/components/oxpilot/tool-call-card.tsrx` |
| AC3.1.3 | OK | `oxpilot-ui` → `apps/web/src/components/oxpilot/transcript.tsrx` |
| AC3.2.1 | OK | `oxpilot-ui` → `apps/web/src/components/oxpilot/diff-card.tsrx` |
| AC4.1.1 | OK | `connector-client` → `crates/oxidean-cli/src/args.rs`; `connector-gateway` → `crates/oxidean-api-oxpilot/src/connector/routes.rs`; `oxpilot-ui` → `apps/web/src/components/oxpilot/connect-dialog.tsrx` |
| AC4.1.2 | OK | `connector-client` → `crates/oxidean-cli/src/pilot/mod.rs`; `connector-gateway` → `crates/oxidean-api-oxpilot/src/connector/gateway.rs`; `oxpilot-ui` → `apps/web/src/components/oxpilot/connect-dialog.tsrx` |
| AC4.1.3 | OK | `connector-client` → `crates/oxidean-cli/src/pilot/state.rs`; `connector-gateway` → `crates/oxidean-api-oxpilot/src/connector/gateway.rs`; `oxpilot-ui` → `apps/web/src/components/oxpilot/connect-dialog.tsrx` |
| AC4.1.4 | OK | `connector-gateway` → `crates/oxidean-api-oxpilot/src/connector/gateway.rs`; `oxpilot-ui` → `apps/web/src/components/oxpilot/connect-dialog.tsrx` |
| AC4.2.1 | OK | `oxpilot-ui` → `apps/web/src/components/oxpilot/connect-dialog.tsrx` |
| AC4.2.2 | OK | `connector-gateway` → `crates/oxidean-api-oxpilot/src/connector/gateway.rs`; `oxpilot-ui` → `apps/web/src/components/oxpilot/connect-dialog.tsrx` |
| AC4.2.3 | OK | `connector-gateway` → `crates/oxidean-api-oxpilot/src/connector/gateway.rs`; `oxpilot-ui` → `apps/web/src/components/oxpilot/target-row.tsrx` |
| AC4.3.1 | OK | `connector-client` → `crates/oxidean-cli/src/pilot/exec.rs`; `connector-gateway` → `crates/oxidean-api-oxpilot/src/connector/gateway.rs`; `oxpilot-ui` → `apps/web/src/routes/oxpilot.$session.tsrx` |
| AC4.3.2 | OK | `connector-client` → `crates/oxidean-cli/src/pilot/state.rs`; `connector-gateway` → `crates/oxidean-api-oxpilot/src/connector/gateway.rs`; `oxpilot-ui` → `apps/web/src/components/oxpilot/consent-card.tsrx` |
| AC4.3.3 | OK | `connector-client` → `crates/oxidean-cli/src/pilot/client.rs`; `connector-gateway` → `crates/oxidean-api-oxpilot/src/connector/gateway.rs`; `oxpilot-ui` → `apps/web/src/routes/oxpilot.$session.tsrx` |
| AC4.3.4 | OK | `connector-client` → `crates/oxidean-cli/src/pilot/state.rs`; `connector-gateway` → `crates/oxidean-api-oxpilot/src/connector/gateway.rs`; `oxpilot-ui` → `apps/web/src/routes/oxpilot.$session.tsrx` |
| AC4.3.5 | OK | `connector-client` → `crates/oxidean-cli/src/pilot/exec.rs`; `connector-gateway` → `crates/oxidean-api-oxpilot/src/connector/gateway.rs`; `oxpilot-ui` → `apps/web/src/routes/oxpilot.$session.tsrx` |
| AC4.4.1 | OK | `oxpilot-ui` → `apps/web/src/components/oxpilot/confirm-dialog.tsrx` |
| AC4.4.2 | OK | `connector-client` → `crates/oxidean-cli/src/pilot/mod.rs`; `connector-gateway` → `crates/oxidean-api-oxpilot/src/connector/gateway.rs`; `oxpilot-ui` → `apps/web/src/routes/oxpilot.$session.tsrx` |
| AC4.4.3 | OK | `connector-gateway` → `crates/oxidean-api-oxpilot/src/connector/gateway.rs`; `oxpilot-ui` → `apps/web/src/routes/oxpilot.$session.tsrx` |
| AC4.5.1 | OK | `oxpilot-ui` → `apps/web/src/components/oxpilot/sessions-rail.tsrx` |
| AC4.6.1 | OK | `connector-client` → `crates/oxidean-cli/src/pilot/mod.rs` |
| AC4.6.2 | OK | `connector-client` → `crates/oxidean-cli/src/pilot/state.rs` |
| AC4.6.3 | OK | `connector-client` → `crates/oxidean-cli/src/pilot/custody.rs` |
| AC5.1.1 | OK | `providers` → `crates/oxidean-api-core/src/oxpilot.rs`; `oxpilot-ui` → `apps/web/src/components/oxpilot/flag-off.tsrx`; `u1-skeleton-slice` → `crates/oxidean-api-oxpilot/src/lib.rs (PROCEDURES/dispatch); crates/oxidean-api/tests/oxpi` |
| AC5.1.2 | OK | `providers` → `crates/oxidean-api-oxpilot/src/providers/openai.rs` |
| AC5.1.3 | OK | `providers` → `crates/oxidean-db/src/oxpilot_settings.rs`; `u1-skeleton-slice` → `crates/oxidean-api-oxpilot/src/session.rs; crates/oxidean-api/tests/oxpilot.rs::session_cr` |
| AC5.1.4 | OK | `providers` → `.env.example`; `u1-skeleton-slice` → `crates/oxidean-api-core/src/oxpilot.rs (enablement); crates/oxidean-api/tests/oxpilot.rs::` |
| AC5.1.5 | OK | `providers` → `crates/oxidean-api-oxpilot/src/status.rs`; `oxpilot-ui` → `apps/web/src/routes/oxpilot.tsrx`; `u1-skeleton-slice` → `crates/oxidean-core/src/redact.rs; crates/oxidean-api-oxpilot/src/routes.rs (persist path)` |
| AC5.2.1 | OK | `session-core` → `crates/oxidean-api-oxpilot/src/services.rs` |
| AC5.2.2 | OK | `agent-runtime` → `crates/oxidean-api-oxpilot/src/routes.rs`; `session-core` → `crates/oxidean-api-oxpilot/src/session.rs`; `oxpilot-ui` → `apps/web/src/lib/oxpilot-stream.ts` |
| AC5.3.1 | OK | `connector-gateway` → `crates/oxidean-api-oxpilot/src/connector/gateway.rs` |
| AC5.3.2 | OK | `connector-gateway` → `crates/oxidean-api-oxpilot/tests/oxpilot_connector.rs` |
| AC5.3.3 | OK | `session-core` → `crates/oxidean-api-oxpilot/src/services.rs`; `connector-gateway` → `crates/oxidean-api-oxpilot/src/connector/gateway.rs`; `u1-skeleton-slice` → `crates/oxidean-api/src/main.rs (sweep); crates/oxidean-db/src/oxpilot.rs (sweep_oxpilot_st` |
| AC5.4.1 | OK | `providers` → `crates/oxidean-api-oxpilot/src/providers/ollama.rs` |
| AC5.5.1 | OK | `session-core` → `crates/oxidean-api-oxpilot/src/dispatch.rs`; `connector-gateway` → `crates/oxidean-api-oxpilot/src/dispatch.rs`; `providers` → `crates/oxidean-api-oxpilot/src/lib.rs` … |
| AC5.5.2 | OK | `u1-skeleton-slice` → `apps/web/src/routes/oxpilot.integration.test.ts; apps/web/src/lib/oxpilot-stream.unit.test` |
| AC5.5.3 | OK | `connector-gateway` → `crates/oxidean-db/tests/dialect_oxpilot.rs`; `providers` → `crates/oxidean-db/migrations/sqlite/0086_oxpilot_settings.sql`; `u1-skeleton-slice` → `apps/web/src/routes/oxpilot.browser.test.tsx; apps/web/src/test/browser-coverage.manifest.` |
| AC5.5.4 | OK | `connector-gateway` → `crates/oxidean-api-oxpilot/src/connector/routes.rs`; `oxpilot-ui` → `apps/web/src/lib/oxpilot-stream.ts`; `u1-skeleton-slice` → `.env.example; docs/CONFIGURATION.md; .railway/railway.ts; scripts/dev-auth/run-stack-e2e.s` |
| FR1 | OK (via children) | — |
| FR1.1 | OK (mapped) | `agent-runtime` → `crates/oxidean-api-oxpilot/src/routes.rs`; `session-core` → `crates/oxidean-api-oxpilot/src/services.rs`; `connector-client` → `crates/oxidean-cli/src/args.rs` … |
| FR1.2 | OK | `session-core` → `crates/oxidean-db/src/oxpilot.rs` |
| FR1.3 | OK (mapped) | `agent-runtime` → `crates/oxidean-api-oxpilot/src/runtime.rs`; `session-core` → `crates/oxidean-api-oxpilot/src/services.rs`; `connector-client` → `crates/oxidean-cli/src/pilot/client.rs` … |
| FR1.4 | OK (mapped) | `agent-runtime` → `crates/oxidean-api-core/src/oxpilot.rs`; `session-core` → `crates/oxidean-api-oxpilot/src/services.rs`; `connector-client` → `crates/oxidean-cli/src/pilot/mod.rs` … |
| FR1.5 | OK (mapped) | `agent-runtime` → `crates/oxidean-api-core/src/oxpilot.rs`; `session-core` → `crates/oxidean-api-oxpilot/src/services.rs`; `connector-client` → `crates/oxidean-cli/src/pilot/mod.rs` … |
| FR1.6 | OK (mapped) | `agent-runtime` → `crates/oxidean-api-oxpilot/src/routes.rs`; `session-core` → `crates/oxidean-api-oxpilot/src/services.rs`; `connector-client` → `crates/oxidean-cli/src/args.rs` … |
| FR1.7 | OK | `session-core` → `crates/oxidean-api-oxpilot/src/services.rs` |
| FR2 | OK (via children) | — |
| FR2.1 | Deferred | `agent-runtime` → `` |
| FR2.2 | OK | `agent-runtime` → `crates/oxidean-api-oxpilot/src/runtime.rs` |
| FR2.3 | OK | `agent-runtime` → `crates/oxidean-api-oxpilot/src/runtime.rs` |
| FR2.4 | OK | `agent-runtime` → `crates/oxidean-api-oxpilot/src/runtime.rs`; `session-core` → `crates/oxidean-db/src/oxpilot.rs` |
| FR2.5 | OK (mapped) | `agent-runtime` → `crates/oxidean-api-oxpilot/src/runtime.rs`; `session-core` → `crates/oxidean-api-oxpilot/src/services.rs`; `connector-client` → `crates/oxidean-cli/src/pilot/state.rs` … |
| FR3 | OK (via children) | — |
| FR3.1 | OK (mapped) | `connector-client` → `crates/oxidean-cli/src/pilot/state.rs`; `connector-gateway` → `crates/oxidean-api-oxpilot/src/connector/gateway.rs`; `providers` → `crates/oxidean-api-oxpilot/src/providers/egress.rs` … |
| FR3.2 | OK (mapped) | `agent-runtime` → `crates/oxidean-api-oxpilot/src/runtime.rs`; `session-core` → `crates/oxidean-api-oxpilot/src/services.rs`; `connector-client` → `crates/oxidean-cli/src/pilot/state.rs` … |
| FR3.3 | OK (mapped) | `providers` → `.env.example`; `oxpilot-ui` → `apps/web/src/components/oxpilot/queued-row.tsrx`; `agent-runtime` → `crates/oxidean-api-oxpilot/src/runtime.rs` … |
| FR4 | OK (via children) | — |
| FR4.1 | OK (mapped) | `agent-runtime` → `crates/oxidean-api-oxpilot/src/runtime.rs`; `session-core` → `crates/oxidean-api-oxpilot/src/services.rs`; `connector-client` → `crates/oxidean-cli/src/pilot/exec.rs` … |
| FR4.2 | OK (mapped) | `agent-runtime` → `crates/oxidean-api-oxpilot/src/routes.rs`; `session-core` → `crates/oxidean-api-oxpilot/src/services.rs`; `connector-client` → `crates/oxidean-cli/src/pilot/exec.rs` … |
| FR4.3 | OK (mapped) | `session-core` → `crates/oxidean-api-oxpilot/src/services.rs`; `connector-client` → `crates/oxidean-cli/src/pilot/exec.rs`; `connector-gateway` → `crates/oxidean-api-oxpilot/src/connector/gateway.rs` … |
| FR5 | OK (via children) | — |
| FR5.1 | OK | `connector-client` → `crates/oxidean-cli/src/pilot/mod.rs`; `connector-gateway` → `crates/oxidean-api-oxpilot/src/connector/routes.rs` |
| FR5.2 | OK | `connector-gateway` → `crates/oxidean-api-oxpilot/src/connector/gateway.rs` |
| FR5.3 | OK | `connector-client` → `crates/oxidean-cli/src/pilot/state.rs`; `connector-gateway` → `crates/oxidean-api-core/src/oxpilot.rs` |
| FR5.4 | OK | `connector-client` → `crates/oxidean-cli/src/pilot/mod.rs`; `connector-gateway` → `crates/oxidean-api-oxpilot/src/connector/gateway.rs` |
| FR5.5 | OK | `connector-client` → `crates/oxidean-cli/src/pilot/custody.rs`; `connector-gateway` → `crates/oxidean-api-oxpilot/src/connector/gateway.rs` |
| FR5.6 | OK | `session-core` → `crates/oxidean-api-core/src/oxpilot.rs`; `connector-client` → `crates/oxidean-cli/src/commands.rs`; `connector-gateway` → `crates/oxidean-api-oxpilot/src/connector/gateway.rs` |
| FR6 | OK (via children) | — |
| FR6.1 | OK | `agent-runtime` → `crates/oxidean-api-oxpilot/src/runtime.rs` |
| FR6.2 | OK (mapped) | `providers` → `crates/oxidean-api-core/src/oxpilot.rs`; `oxpilot-ui` → `apps/web/src/components/oxpilot/flag-off.tsrx`; `u1-skeleton-slice` → `crates/oxidean-api-oxpilot/src/lib.rs (PROCEDURES/dispatch); crates/oxidean-api/tests/oxpi` |
| FR6.3 | OK | `agent-runtime` → `crates/oxidean-api-oxpilot/src/routes.rs` |
| FR6.4 | OK | `providers` → `crates/oxidean-api-oxpilot/src/settings.rs` |
| FR6.5 | OK | `providers` → `crates/oxidean-api-oxpilot/src/settings.rs` |
| FR6.6 | OK | `session-core` → `crates/oxidean-db/migrations/sqlite/0087_oxpilot_session_core.sql`; `connector-gateway` → `crates/oxidean-db/migrations/sqlite/0088_oxpilot_connector.sql` |
| FR6.7 | OK | `agent-runtime` → `crates/oxidean-api-oxpilot/src/runtime.rs`; `session-core` → `crates/oxidean-api-oxpilot/src/services.rs` |
| FR7 | OK (via children) | — |
| FR7.1 | Deferred | `agent-runtime` → `` |
| FR7.2 | OK (mapped) | `agent-runtime` → `crates/oxidean-api-oxpilot/src/runtime.rs`; `session-core` → `crates/oxidean-api-oxpilot/src/services.rs`; `connector-client` → `crates/oxidean-cli/src/pilot/state.rs` … |
| FR7.3 | Deferred | `agent-runtime` → `` |
| FR7.4 | OK (mapped) | `connector-client` → `crates/oxidean-cli/src/pilot/state.rs`; `connector-gateway` → `crates/oxidean-api-oxpilot/src/connector/gateway.rs`; `oxpilot-ui` → `apps/web/src/components/oxpilot/target-row.tsrx` … |
| FR7.5 | OK (mapped) | `connector-client` → `crates/oxidean-cli/src/pilot/client.rs`; `oxpilot-ui` → `apps/web/src/lib/oxpilot-queries.ts`; `connector-client` → `crates/oxidean-cli/src/pilot/state.rs` … |
| NFR1 | OK | `agent-runtime` → `crates/oxidean-api-oxpilot/src/runtime.rs`; `session-core` → `crates/oxidean-api-oxpilot/src/dispatch.rs`; `connector-client` → `crates/oxidean-cli/src/pilot/custody.rs` … |
| NFR2 | OK | `session-core` → `crates/oxidean-api-oxpilot/src/services.rs`; `connector-client` → `crates/oxidean-cli/src/pilot/custody.rs` |
| NFR3 | OK | `agent-runtime` → `crates/oxidean-api-oxpilot/src/runtime.rs` |
| NFR4 | OK | `agent-runtime` → `crates/oxidean-api-oxpilot/src/runtime.rs`; `connector-client` → `crates/oxidean-cli/src/pilot/state.rs`; `connector-gateway` → `crates/oxidean-api-oxpilot/src/connector/gateway.rs` … |
| NFR5 | OK | `connector-gateway` → `crates/oxidean-api-oxpilot/src/connector/gateway.rs` |
| NFR6 | OK | `agent-runtime` → `crates/oxidean-api-oxpilot/src/runtime.rs`; `connector-client` → `crates/oxidean-cli/src/pilot/mod.rs` |
| NFR7 | OK (mapped) | `providers` → `.env.example`; `oxpilot-ui` → `apps/web/src/components/oxpilot/queued-row.tsrx`; `oxpilot-ui` → `apps/web/src/components/oxpilot/transcript.tsrx` … |
| NFR8 | OK (mapped) | `connector-client` → `crates/oxidean-cli/src/pilot/mod.rs`; `oxpilot-ui` → `apps/web/src/routes/oxpilot.tsrx` |
| NFR9 | OK | `agent-runtime` → `crates/oxidean-api-oxpilot/src/runtime.rs`; `session-core` → `crates/oxidean-db/tests/dialect_oxpilot.rs`; `connector-client` → `crates/oxidean-cli/src/pilot/custody.rs` … |

