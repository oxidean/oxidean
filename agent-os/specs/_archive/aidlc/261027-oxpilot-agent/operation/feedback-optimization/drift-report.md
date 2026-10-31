# Drift Report — oxpilot

## Spec → implementation drift (post-review sweep, 2026-10-30)

The PR's review loop landed changes the canonical specs did not yet
reflect; the definitional docs are now amended in place (marked
"post-build" where semantics changed), and stage-time artifacts
(reviews, code-summaries, question logs, audit) stay frozen as the
historical record:

- **Stub default + sys-admin gate**: unset provider resolves `stub`
  (was `no_provider` — now a defensive backstop only); `stub` is
  sys-admin-only at every surface (`restricted` status reason,
  `PageNotFound` UX, `oxpilot.disabled` proc denials — anonymous/PAT
  keep `auth.*`). Amended: requirements FR1.5/FR1.6/FR6.4,
  contract-summary, providers spec + rules BR1.4/BR2.5/BR2.6/BR3.2,
  domain-design components, oxpilot-ui spec WF1/Scope.
- **`openai-compatible` provider**: new registry member (LiteLLM,
  vLLM, NIM, llama.cpp) — `base_url` required, key optional, private
  egress exempt. Amended in the same docs (was unspecified anywhere).
- **`/admin/oxpilot` sys-admin settings page**: new surface — not a
  planned unit; recorded under the U8 scope addition.
- **Review-driven hardening** (already in code): per-connect
  `PublicOnlyResolver` DNS re-validation, `jail_dirfd`/`openat`
  descriptor-relative connector fs ops, `scrub_bytes` pre-base64,
  canonical diff generated from `files[]` (agent `diff_body` never
  stored), pinned `base_sha`, streaming `StreamRedactor`, token-only
  budget recheck, declared-order op application, MySQL `ops_json`
  legacy-read handling — these refine implementation, not contracts.

Earlier gate: cross-unit traceability passed with 122/122 enumerated
ids covered (see `construction/build-and-test/cross-unit-traceability.md`).

## Known deliberate deferrals

| Item | Where deferred | Why |
|---|---|---|
| p99 latency SLO measurement | first flag-on deploy | needs live env — recorded in performance-validation matrix |
| Rust coverage gating | platform level | collect-only today (team.md debt) — not invented here |
| Full dialect parity in CI | platform level | `dialect_probe` per dialect is the CI contract; local-capable only (team.md) |
| Load-test harness | platform level | no bench infra exists (team.md debt) — plan documented instead |
| Multi-replica turn state | future | documented single-replica caveat — no hidden claim |

## Drift watch

The generated client (`@oxidean/api-client`) is the one true drift surface — `make rpc-sync-check` is the standing gate and stays green.
