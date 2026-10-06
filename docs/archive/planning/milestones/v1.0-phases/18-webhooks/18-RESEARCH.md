# Phase 18: Webhooks - Research

**Researched:** 2026-09-16  
**Domain:** Outbound repository webhooks (CRUD, signed HTTPS delivery, attempt history)  
**Confidence:** HIGH (GitHub parity + existing Oxidean ACL/jobs/reqwest patterns); MEDIUM (exact PR payload fields until Phase 12 RPC lands)

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
### A — Scope & ACL
- **D-HOOK-01:** Webhooks are **repository-scoped** only in Phase 18 (no org/global hooks)
- **D-HOOK-02:** **Admin** capability required for create / update / delete / list / view deliveries / ping
- **D-HOOK-03:** Each webhook has: **HTTPS URL**, **secret**, **active** boolean, **subscribed event set**, optional **description**/name
- **D-HOOK-04:** URL must be **https://** in production/cloud; **http://localhost / loopback** allowed in dev/test only

### B — Event catalog & payloads
- **D-HOOK-05:** Ship at least: **`push`**, **`pull_request`**, **`issues`**, plus synthetic **`ping`**
- **D-HOOK-06:** GitHub Hookshot header conventions (`X-GitHub-Event`, `X-GitHub-Delivery`, `X-GitHub-Hook-ID`, `User-Agent: Oxidean-Hookshot/*`, `Content-Type: application/json`)
- **D-HOOK-07:** GitHub-shaped JSON payloads (action + resource + repository + sender)
- **D-HOOK-08:** **`issues` actions** at minimum: `opened`, `edited`, `closed`, `reopened`
- **D-HOOK-09:** **`pull_request` actions** align with Phase 12 PR identity (shared `#N`, forks, draft)
- **D-HOOK-10:** **`push`** after successful HTTPS/SSH receive-pack ref updates
- **D-HOOK-11:** Best-effort async delivery (mutating request must not fail if hook is down)

### C — Delivery & signing
- **D-HOOK-12:** HMAC-SHA256 `X-Hub-Signature-256` only (no SHA-1)
- **D-HOOK-13:** Persist delivery + attempt history with status
- **D-HOOK-14:** Bounded automatic retries with exponential backoff
- **D-HOOK-15:** Inactive skip; deleted stop new deliveries

### D — Secrets & SSRF
- **D-HOOK-16:** Required secret; masked in list; one-time reveal on create/rotate
- **D-HOOK-17:** Secret stored so API can sign (plaintext column OK for v1)
- **D-HOOK-18:** SSRF guards + timeouts + bounded response read

### E — UI & history
- **D-HOOK-19:** Repo Settings → Webhooks (Admin-only)
- **D-HOOK-20:** Recent deliveries with status
- **D-HOOK-21:** Redeliver + Ping

### F — Cross-phase wiring
- **D-HOOK-22:** Emit from issues, PRs, push paths
- **D-HOOK-23:** Internal `WebhookDispatcher` / event-bus seam for future NOTF/ACT
- **D-HOOK-24:** Webhooks stay on `repository_id` across rename/transfer

### Claude's Discretion
- Exact max retry count / backoff / retention
- Whether `issue_comment` ships
- Exact RPC naming (`webhook.*` vs `repo.webhook.*`)
- Response body storage vs status-only
- Settings IA (section vs nested routes)
- Migration number; push commit list truncation

### Deferred Ideas (OUT OF SCOPE)
- Org/global webhooks; GitHub Apps; payload transforms; form-urlencoded + SHA-1; external queues; events beyond push/PR/issues; Actions coupling
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| HOOK-01 | Repo admin CRUD outbound webhooks for repo events | Admin-gated `webhook.*` RPC + Settings UI; tables `webhooks` |
| HOOK-02 | Deliver payloads for subscribed events (push, PR, issues) | Dispatcher + emitters on issue/PR/push; signed HTTP POST |
| HOOK-03 | View recent delivery attempts and response status | `webhook_deliveries` / attempts tables + list RPC + Settings history UI |
</phase_requirements>

<architectural_responsibility_map>
## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Webhook CRUD + ACL | API / Backend | Browser / Client | Admin gates live in Axum/RPC; UI is forms |
| Event emit on issue/PR mutate | API / Backend | — | Call sites after successful domain mutations |
| Event emit on git push | API / Backend | — | After successful receive-pack / SSH update |
| Async delivery + retries | API / Backend | Database / Storage | In-process worker + DB attempt rows (no Redis in v1) |
| HMAC signing | API / Backend | — | Sign raw JSON with webhook secret |
| SSRF / URL policy | API / Backend | — | Outbound HTTP must not hit metadata IPs |
| Delivery history UI | Browser / Client | API / Backend | Octane settings; data from RPC |
| Dialect schema | Database / Storage | — | Migrations only in `oxidean-db` |
| Generated TS client | API / Backend → packages | — | `make rpc-gen` after RPC types |
</architectural_responsibility_map>

<research_summary>
## Summary

Outbound webhooks are a solved forge pattern: GitHub and Gitea store per-repo hook configs (URL, secret, events, active), POST JSON on domain events with Hookshot-style headers, sign with HMAC-SHA256, and keep a delivery attempt log for operators. Oxidean already has the hard pieces: Admin ACL, issue mutations, Smart HTTP/SSH push, `reqwest` + `sha2`, and in-process background jobs.

**Primary recommendation:** Add `webhooks` + `webhook_deliveries` (and attempts) in `oxidean-db`; implement `webhook.*` Admin RPC; centralize fan-out in a `WebhookDispatcher` that enqueues after issue/PR/push success; deliver via `reqwest` with SSRF guards, `X-Hub-Signature-256`, and a retry loop beside existing `spawn_background_jobs`; surface CRUD + history under repo Settings (Octane), mirroring PAT reveal for secrets.

Do **not** introduce Redis/SQS for v1 — use DB-backed pending deliveries + in-process worker (same reliability class as orphan reconcile). Phase 12 PR emitters are a hard dependency for HOOK-02 PR coverage; ship issue+push+ping first if PR module is not yet merged, but plans must include PR wiring against `12-CONTEXT.md` (not invent a parallel PR model).
</research_summary>

<standard_stack>
## Standard Stack

### Core (already in workspace)

| Library | Version (workspace) | Purpose | Why Standard |
|---------|---------------------|---------|--------------|
| `reqwest` | 0.13.x (rustls, json) | Outbound HTTPS POST | Already used in API |
| `sha2` | 0.11.x | SHA-256 digest for HMAC | Already present |
| `serde_json` | workspace | Payload serialization | Existing |
| `uuid` / `ulid` | (check workspace) | `X-GitHub-Delivery` IDs | Prefer existing ID helpers |
| Tokio | workspace | Async worker + timeouts | Existing runtime |

### Supporting (may add — legitimacy gate)

| Library | Purpose | When to Use |
|---------|---------|-------------|
| `hmac` (RustCrypto) | HMAC-SHA256 construction | Preferred over hand-rolled HMAC; appears as transitive dep in Cargo.lock already — still run Package Legitimacy Gate if declaring direct dep |
| `subtle` / constant-time compare | Signature helpers | Only if needed beyond existing crypto |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| In-process DB queue | Redis / NATS | Better durability/scale; out of v1 ops model (Compose simplicity) |
| Sync delivery in RPC | Async enqueue | Sync would block git push / issue create — forbidden by D-HOOK-11 |
| Custom Oxidean headers only | GitHub Hookshot names | Breaks ecosystem tooling; CONTEXT locks GitHub names |

**Installation (only if direct dep needed):**
```bash
# crates/oxidean-api/Cargo.toml — after legitimacy checkpoint if [ASSUMED]
hmac = "0.12"   # or current RustCrypto line matching sha2 major
```

## Package Legitimacy Audit

| Package | Registry | Status | Notes |
|---------|----------|--------|-------|
| `hmac` | crates.io | [ASSUMED] | RustCrypto org; widely used; confirm downloads/maintainers at install time if added as direct dep |
| (no new npm) | — | — | UI uses existing Octane/ShadCN |

If install task adds `hmac`: blocking human legitimacy checkpoint before `cargo add` (workflow.security_enforcement). Prefer implementing HMAC with existing `sha2` + minimal code only if legitimacy gate would block autonomy — but CONTEXT/security prefers standard `hmac` crate; plan includes checkpoint when adding.
</standard_stack>

<architecture_patterns>
## Architecture Patterns

### System flow

```
Domain mutate (issue / PR / push)
        │
        ▼
 WebhookDispatcher.emit(repo_id, event, action, payload)
        │
        ├── select active webhooks WHERE event IN subscribed
        ├── insert webhook_deliveries (pending) + first attempt schedule
        └── return immediately (D-HOOK-11)
                │
                ▼
 DeliveryWorker (tokio loop / after-enqueue spawn)
        ├── resolve URL (SSRF check)
        ├── POST JSON + Hookshot headers + HMAC-SHA256
        ├── record attempt (status, latency, error)
        └── schedule retry if transient (D-HOOK-14)
```

### Recommended module layout

| Path | Role |
|------|------|
| `crates/oxidean-core/src/webhook_types.rs` | DTOs: WebhookPublic, DeliveryPublic, event enums |
| `crates/oxidean-db/src/webhooks.rs` | CRUD + delivery/attempt queries (no dialect leak to API) |
| `crates/oxidean-db/migrations/*/00NN_webhooks.sql` | Schema |
| `crates/oxidean-api/src/webhook/mod.rs` | RPC handlers |
| `crates/oxidean-api/src/webhook/dispatch.rs` | Emit + enqueue |
| `crates/oxidean-api/src/webhook/deliver.rs` | HTTP client, sign, SSRF |
| `crates/oxidean-api/src/webhook/worker.rs` | Retry drain |
| `apps/web/.../webhooks-*.tsrx` | Settings panels |

### Schema sketch (dialect-neutral intent)

**webhooks:** id, repository_id, url, secret, active, events (JSON array or join table), name/description, created_by, created_at, updated_at  

**webhook_deliveries:** id, webhook_id, delivery_guid, event, action, payload_json, status (pending/success/failed), created_at  

**webhook_delivery_attempts:** id, delivery_id, attempted_at, http_status, error_message, duration_ms, response_snippet (nullable truncated)

Prefer JSON events array for simplicity (Gitea-like) unless join table is cleaner for querying — discretion.

### RPC surface (recommended names)

- `webhook.list` / `webhook.get` / `webhook.create` / `webhook.update` / `webhook.delete`
- `webhook.ping`
- `webhook.deliveries.list` / `webhook.deliveries.get`
- `webhook.redeliver`

All Admin-gated via existing repo ACL helpers.

### Emitter call sites

| Event | Call site (existing or Phase 12) |
|-------|-----------------------------------|
| `issues` | `issue::create`, `issue::update` (map state transitions → actions) |
| `pull_request` | Phase 12 PR create/update/merge/close handlers |
| `push` | After successful Smart HTTP `receive_pack` CGI and SSH receive path when refs change |
| `ping` | `webhook.create` + `webhook.ping` / redeliver |

### UI pattern

Extend `$owner.$repo.settings.tsrx` (or nested `$owner.$repo.settings.hooks*.tsrx`) with Admin-only Webhooks panel: list → create form (URL, secret, event checkboxes, active) → detail with recent deliveries table. Reuse `AlertDialog` confirm for delete; reuse PAT reveal for secret once.
</architecture_patterns>

<dont_hand_roll>
## Don't Hand-Roll

- Don't invent a new ACL ladder — reuse `Capability::Admin` / `resolve_repo_for_admin`
- Don't put dialect SQL in `oxidean-api`
- Don't hand-edit `@oxidean/api-client` — change Rust + `make rpc-gen`
- Don't block git push / issue RPC on outbound HTTP
- Don't use SHA-1 signatures or form-urlencoded payloads
- Don't add Redis/queue infrastructure for v1
- Don't invent a parallel PR number space in payloads — use Phase 12 shared `#N`
</dont_hand_roll>

<common_pitfalls>
## Common Pitfalls

1. **Signing re-serialized JSON** — HMAC must cover the exact bytes POSTed; serialize once, store/send those bytes.
2. **SSRF via webhook URL** — attackers with Admin on a public test repo could still be risky on self-host; always block cloud metadata ranges.
3. **Failing the user action when hook fails** — violates D-HOOK-11.
4. **Missing SSH push emit** — HTTPS-only hooks would miss D-HOOK-10.
5. **PR emitter without Phase 12** — plan must precondition PR module; do not stub fake PR tables in Phase 18.
6. **Logging secrets** — never log `secret` or full `Authorization`-like material; mask in RPC.
7. **Delivery history unbounded growth** — add retention purge (job or on-write trim) per discretion.
</common_pitfalls>

<security_domain>
## Security Domain (ASVS L1)

| Trust boundary | Risks | Mitigations |
|----------------|-------|-------------|
| Admin → webhook URL | SSRF, internal network scan | HTTPS policy, IP denylist, DNS rebinding care, timeouts |
| API → third-party endpoint | Secret leakage in logs/UI | Mask secret; one-time reveal; no log of body+secret |
| Delivery history | Response body may contain third-party secrets | Truncate/store status primarily |
| Event payload | PII in issue/PR bodies | Same visibility as forge already exposes to Admin-configured endpoints (accept; document) |

Threat IDs for plans: `T-18-01` SSRF, `T-18-02` secret exposure, `T-18-03` unsigned/forged deliveries (mitigate by always signing), `T-18-SC` package install if `hmac` added.
</security_domain>

<codebase_patterns>
## Codebase Patterns to Follow

- RPC module + match arms in `rpc.rs` (see `release/`, `issue/`)
- Migrations triple (sqlite/postgres/mysql) + `dialect_*` test
- Admin settings UI gate: `can_admin` / `RepoNotFound` anti-enum
- Background jobs: `jobs/schedule.rs` `tokio::spawn` loops
- Secret reveal: `pat-reveal.tsrx` + create response plaintext once
- Tests: `crates/oxidean-api/tests/*.rs` nextest + web integration Vitest stubs

### Push emit seam note

`receive_pack` currently delegates to CGI via `authorize_and_cgi`. Research recommends capturing **successful** ref-update completion (parse CGI result or hook after process exit 0) before emitting `push`. SSH path needs the same after successful receive. If parsing refs from the request body is easier than post-hooks, prefer that with verified success status only.
</codebase_patterns>

<validation_architecture>
## Validation Architecture

| Requirement | Automated proof |
|-------------|-----------------|
| HOOK-01 | nextest `webhook_create|update|delete|list` + Admin denial; Vitest settings hooks UI |
| HOOK-02 | nextest delivery to mock HTTP server for issues + push; PR when Phase 12 present; HMAC header assert |
| HOOK-03 | nextest deliveries.list status; Vitest deliveries table render |

Wave 0 stubs: `webhook_rpc.rs`, `webhook_delivery.rs`, `dialect_webhooks.rs`, settings hooks integration tests.

Prefer wiremock / local `axum` listener or `tiny_http` for outbound capture in tests — check existing test helpers before adding crates.
</validation_architecture>

<env_config>
## Configuration Knobs (discretion)

| ENV | Purpose | Default sketch |
|-----|---------|----------------|
| `OXIDEAN_WEBHOOK_MAX_ATTEMPTS` | Retry cap | 5 |
| `OXIDEAN_WEBHOOK_TIMEOUT_MS` | Per-attempt HTTP timeout | 10000 |
| `OXIDEAN_WEBHOOK_ALLOW_HTTP_LOOPBACK` | Dev http://127.0.0.1 | true in debug/test |
| `OXIDEAN_WEBHOOK_DELIVERY_RETENTION_DAYS` | History purge | 30 |
| `OXIDEAN_WEBHOOK_WORKER_INTERVAL_MS` | Drain tick | 1000 |
</env_config>

<dependency_notes>
## Phase Dependencies

- **Phase 11:** Available — issue emitters can land immediately.
- **Phase 12:** CONTEXT locked; code may land in parallel. PR emitter plan carries `<precondition>` that PR RPC/tables exist. Payload field mapping MUST follow D-PR-01…03, D-PR-02 shared `#N`.
- **Phase 15:** Transfer keeps `repository_id` — no special migrate for hooks beyond FK.
- **Phase 17/19:** Consume dispatcher seam later; do not implement notifications/Actions here.
</dependency_notes>
