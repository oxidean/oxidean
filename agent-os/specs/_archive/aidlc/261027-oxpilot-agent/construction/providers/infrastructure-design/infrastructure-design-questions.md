# Infrastructure Design — Questions (providers)

Decisions for this stage. Resolved under the standing self-drive instruction; ratify at PR review.

## Q1 — New infrastructure footprint
Does U4 provision any new infrastructure?
[Answer]: A
- A. Zero new services — U4 lands inside the existing `api` service (provider adapters + `admin.oxpilot.*` procs in `oxidean-api-oxpilot`, runtime-slot types in `oxidean-api-core`), one new `oxpilot_settings` table on the existing databases, and NEW outbound egress to operator-configured provider endpoints. No new Railway service, worker, queue, cache, or store.
- B. A dedicated provider-proxy service in front of LLM calls.
- C. A shared secrets vault service.

## Q2 — Egress & networking posture
What network change does provider calling require?
[Answer]: A
- A. Outbound HTTPS from the existing `api` service only — Railway/cloud egress is already open; the api service gains no new listener, port, or ingress rule. Destinations are operator-configured `base_url`s bounded by the SSRF validator (http/https, public-IP-only for non-Ollama, no redirects). Ollama's private-target exception is a documented operator choice. No VPC/subnet change — single-replica posture unchanged.
- B. A NAT gateway with a static egress IP for provider allowlisting.
- C. An egress firewall rule set at the platform layer.

## Q3 — Secrets in deployment
How do the two secret envs reach the service safely?
[Answer]: A
- A. `OXIDEAN_OXPILOT_SECRETS_KEY` (encryption root, falls back to `OXIDEAN_SESSION_SECRET`) and `OXIDEAN_OXPILOT_API_KEY` (provider key) are operator-set secret envs — declared in `.railway/railway.ts` with `preserve()` so a declarative `railway config apply` never prunes dashboard-set secrets; `OXIDEAN_SESSION_SECRET` gets the same declaration since it's the chain's fallback root; documented in `.env.example` + `docs/CONFIGURATION.md` as secrets (never example values). CI never needs them — adapters test against wiremock fixtures only. Non-secret config vars (`BASE_URL`, budgets) get the same preserve-marked declarations.
- B. Railway's secrets manager / external vault integration.
- C. Inject via build args.

## Q4 — CI/CD change set
What does the pipeline need for U4?
[Answer]: A
- A. Reuse existing jobs — `oxpilot_settings` migrations join the already-registered `dialect_oxpilot.rs` binary (U1 owns its `db-dialects -E` filter entry + TESTING.md row); adapter unit + wiremock tests ride `api-rust` shards; `rpc-gen` output for `admin.oxpilot.*` regenerates via the existing `rpc-sync` job. No new job or matrix value ⇒ no `Main` ruleset change.
- B. A dedicated provider-contract CI job.
- C. New jobs + ruleset update.

## Q5 — Monitoring/alerting
What monitoring does U4 add at platform level?
[Answer]: A
- A. None — same tracing story as U1 (`oxpilot.provider` span + warn-per-terminal + boot reason log on stdout via Railway platform logs). `admin.oxpilot.providers_list`'s `configured` flag + `oxpilot.status`'s reason are the operator readiness surfaces. No metrics exporter, alerts, or dashboards.
- B. Provider-latency metrics + alert rules.
- C. Health-check endpoint additions.

## Q6 — Deploy/rollback posture
How does U4's change reach environments and roll back?
[Answer]: A
- A. Existing path unchanged: PR env → squash-merge → `sha-<sha>` → staging → manual promote. Rollback levers are layered: flag-off (`enabled_override=false` or env) restores `oxpilot.disabled` without a deploy; `clear_overrides` restores pure env config; a bad migration is additive-only (new singleton table). Boot rebuild is unconditional-after-`AppState::new` — a deploy ordering where migrations haven't run degrades to env-only + warn, never a boot failure.
- B. Canary for the settings procs.
- C. Feature-branch deploy track.

## Consolidated Summary Confirmation

Decisions consolidated:
- Q1 A: zero new services — adapters + procs in existing api crate, one new singleton table.
- Q2 A: outbound HTTPS only; SSRF-bounded operator destinations; Ollama private exception documented.
- Q3 A: two secret envs preserve-marked in railway.ts; wiremock-only CI; docs as secrets.
- Q4 A: reuse jobs — same dialect binary, same shards, same rpc-sync-check; no ruleset change.
- Q5 A: tracing only; providers_list `configured` + status reason = readiness surfaces.
- Q6 A: unchanged deploy path; flag-off + clear_overrides + additive migration rollback layers.

Does this all look correct before I write the infrastructure artifacts?

- Looks correct
- Request changes

[Answer]: Looks correct
