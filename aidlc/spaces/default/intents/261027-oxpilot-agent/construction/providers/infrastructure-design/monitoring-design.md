# Monitoring Design — providers (U4)

Consumes: `nfr-design/observability-design.md`,
`nfr-requirements/observability-requirements.md` (NFR5, NFR6).
Questions: Q5 — no new monitoring infrastructure (conductor-decided).

U4 adds **no monitoring infrastructure**: `tracing` stdout on the api
service rides Railway platform logs; there are no new metrics,
exporters, dashboards, or alert rules. The operator readiness surfaces
are `admin.oxpilot.providers_list`.`configured` and
`oxpilot.status`.`reason`. Tables record the signals and their meaning.

## Metrics & KPIs (tracing signals; no registry)

| Metric | Source | Threshold | Why it matters |
|--------|--------|-----------|----------------|
| Boot resolution reason | one `info!` per boot: `enabled` / `flag_off` / `no_provider` / `misconfigured` | `misconfigured` after boot = check provider env/row | the operator-facing misconfig signal (NFR6.3) |
| Unrecognized `OXIDEAN_OXPILOT_PROVIDER` | `warn!` at resolution | any occurrence | typo'd provider ids surface before a turn fails (NFR6.3) |
| `oxpilot.provider` span | per `stream()` call: `provider_id`, `model` attrs only | span count = provider call volume | adoption + per-provider usage (NFR6.1); payload/key/base_url never attrs |
| Provider terminal failures | one `warn!` per terminal: `provider_id`, session, turn, `ProviderError.code` | sustained 5xx/auth codes = provider-side outage or bad key | distinguishes key issues (401/403) from rate limits (429) (NFR6.2) |
| Rebuild events | mutex-section span on `settings_set` + boot `rebuild` | rate should be admin-rare | a rebuild storm = config flapping (NFR6.3) |
| Audit-write failures | `warn!` from the swallowed audit error | any occurrence | audit loss is observable even though it never fails the response |
| Settings-write rejections | `invalid_provider` on `settings_set` | spike = operator fighting validation | the recovery path (`flag_off`/budget-only writes) must stay open |

## Alerts

| Alert | Condition | Severity | Routes to |
|-------|-----------|----------|-----------|
| none configured | — | — | — |

No alert rules are designed: there is no alert pipeline in the repo and
NFR6 forbids new observability infrastructure. `providers_list`
`configured` + `oxpilot.status` `reason` are the pull-based
equivalents.

## SLIs / SLOs

| SLI | SLO target | Measurement window |
|-----|------------|--------------------|
| `settings_set` round-trip (build+persist+swap+audit) | completes within the 300s provider-build ceiling (dominated by `lookup_host` — no provider call) | per write |
| `providers_list` / `settings_get` | single reads + static table — sub-10ms | per call |
| Slot rebuild | off request path; O(1) swap; row read + optional `lookup_host` only | per rebuild |

Smoke-level assertions live in adapter wiremock tests (fixtures pin
decode correctness, not provider latency).

## Logs & Tracing

`tracing::info_span!("oxpilot.provider", provider_id, model)` wraps each
`stream()`; `ProviderError` terminal logs carry `provider_id`, session
id, turn id, `code` — the same code rides the C2 terminal frame.
`audit_oxpilot_settings` rows land inside the rebuild mutex post-swap
(`{actor, action, field_names, ts}` — names only; `"api_key"` is the
most sensitive string audited). Secret hygiene: `key.rs`'s decrypt is
`pub(in crate::providers)`; ciphertext is the only persisted/emitted
key form (NFR6.4). Boot logging: one info line naming the resolved
reason + one distinct warn for unrecognized provider ids.
No distributed-trace propagation — provider HTTP calls are plain
outbound fetches; trace headers would leak nothing but also buy
nothing for a single-hop call (NFR6.1 keeps attrs minimal anyway).
