# Deferred items — Phase 06

Shipped in 06-03 or the 2026-09-12 post-close addendum. Phase 7+ agents should treat these as **current codebase truth** (do not re-discover or regress).

## Deferred Items

- bootstrap_strict_rpc_allowlist_while_needs_setup — D-11 allowlist in `rpc::dispatch` (`auth.bootstrap_*`, `system.health`, `system.db_probe`)
  status: resolved

- bootstrap_allow_signup_* stubs — wizard persists `allow_signup`; `signup_closed` enforced when false
  status: resolved

- Setup auth stack — wizard selects local / WorkOS / OIDC; persists public provider fields on bootstrap (`setup.index.tsrx`, `BootstrapSetupRequest`, `bootstrap_setup`)
  status: resolved

- Factory reset — sys-admin RPC + Admin Auth danger zone (`RESET`) → wipe → `needs_setup` (`admin.instance.factory_reset`, `admin/auth.tsrx`, `Database::factory_reset_instance`)
  status: resolved

- Auth SSR / no form skeleton — `/login`, `/signup`, `/setup` loaders; forms `method="post" action="#"` + button handlers (`ssr-auth.ts`, auth routes)
  status: resolved

- OIDC harden — Reqwest connect 5s / request 10s; mock healthcheck + compose issuer docs (`auth/oidc.rs`, `docker-compose.dev-auth.yml`, `docs/dev-auth.md`)
  status: resolved

- SSO query alias — `returnTo` accepted alongside `return_to` on SSO start (`auth_callbacks.rs`)
  status: resolved

- TanStack Query — root `QueryClientProvider`; shared soft `auth.me` + bootstrap + providerConfig; `/status` + admin settings via Query; cache clear/update on logout/profile/factory reset (`lib/query-client.ts`, `lib/session-queries.ts`, `chrome.tsrx`, `verify-banner.tsrx`, `__root.tsrx`)
  status: resolved

- Tests — unit + integration for session cache; stack browser e2e for `/status` + `auth.me` dedupe (`session-queries.unit.test.ts`, `session-cache.integration.test.ts`, `auth-ui.stack.browser.test.tsx`)
  status: resolved

- Octane authoring — prefer `function Page() @{` + `@if` / `@else` / `@for`; avoid React `return (` mixed with Rivet in the same component (Vite import-protection break); see https://octanejs.dev/llms.txt
  status: resolved

## Notes (not deferred work)

**Explicit non-goals** (intentionally out of scope, not open deferred items): domain Zustand global store; replacing all form `useState` with Query mutations. AGENTS.md / Octane skill already document the authoring preference.
