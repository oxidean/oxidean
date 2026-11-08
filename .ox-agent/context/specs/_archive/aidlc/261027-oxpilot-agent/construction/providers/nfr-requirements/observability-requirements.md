# Observability Requirements — providers (U4)

Consumes: `construction/providers/functional-design/rules.md`
(BR5.2, BR4.1–BR4.2, BR2.2), `inception/requirements-analysis/requirements.md`
(NFR5, NFR6), `codekb/oxidean-app/technology-stack.md`.

## Audit

| id | requirement | basis |
|----|-------------|-------|
| NFR5.1 | Every `admin.oxpilot.settings_set` mutation writes an audit row `{actor, action, field_names_only, ts}` — set, per-field clear, `clear_key`, `clear_overrides` all auditable | BR5.2, Q6 |
| NFR5.2 | Audit rows carry field NAMES only — values (especially anything key-adjacent) are never audited, not even hashed | NFR1, BR5.2 |
| NFR5.3 | The audit write lands inside the mutex-serialized section, after the swap — audit order matches commit order | BR4.3, R-14 |

## Tracing & logging

| id | requirement | basis |
|----|-------------|-------|
| NFR6.1 | Provider calls emit tracing spans carrying `provider_id` + `model` attributes — request payloads and key material are NEVER span attributes | Q6, BR1.2 |
| NFR6.2 | Provider failures log `ProviderError.code` + provider_id + session/turn ids at warn level, once per terminal — the same code lands in the C2 terminal error frame | BR4.1–4.2, C2 |
| NFR6.3 | Boot-time resolution logs the resolved reason (`flag_off`/`no_provider`/`misconfigured`) once at startup; an unrecognized `OXIDEAN_OXPILOT_PROVIDER` value warns distinctly (operator-facing misconfig signal) | BR1.3, Q5 |
| NFR6.4 | No secret material in any log line — the custody helpers' tests assert the encrypted form is the only form that can surface | NFR1, BR1.2 |

## Operator diagnostics

| id | requirement | basis |
|----|-------------|-------|
| NFR6.5 | `oxpilot.status` is the operator diagnostic surface: enabled flag, provider id+model when resolved, machine reason otherwise — always readable, gate-exempt | BR2.2, C1 |
| NFR6.6 | `admin.oxpilot.providers_list`'s `configured` flag is the admin's per-provider readiness signal — "selectable now": key material resolvable for key-required adapters, always true for keyless adapters | WF5, entities |
