# Phase 18: Webhooks - Context

**Gathered:** 2026-09-16  
**Status:** Ready for planning  
**Discuss mode:** `--auto` / user-directed auto-decide — gray areas locked to **GitHub-like outbound webhooks** (2026-09-16). Align PR event payloads with Phase 12 PR model (`12-CONTEXT.md`).

[--auto] Selected all gray areas: (A) Scope & ACL, (B) Event catalog & payloads, (C) Delivery & signing, (D) Secrets & config, (E) UI & delivery history, (F) Retries / reliability.

[auto] A — Q: "Who can manage webhooks?" → Selected: "Repo Admin only (GitHub)" (recommended default)  
[auto] A — Q: "Org-level webhooks?" → Selected: "Defer org/global hooks; repo-scoped only in Phase 18" (recommended — HOOK-01 is repo admin)  
[auto] B — Q: "Which events?" → Selected: "push + pull_request + issues (minimum); plus ping; issue_comment optional if cheap" (recommended — HOOK-02)  
[auto] B — Q: "PR payload shape?" → Selected: "GitHub-shaped envelopes; PR object fields mirror Phase 12 D-PR identity (#N shared with issues, fork heads, draft if present)"  
[auto] C — Q: "Signing?" → Selected: "HMAC-SHA256 X-Hub-Signature-256 only (no SHA-1)" (recommended)  
[auto] C — Q: "Content type?" → Selected: "application/json only" (recommended)  
[auto] D — Q: "Secret UX?" → Selected: "Required secret on create; masked in list; rotate via update; one-time reveal on create/rotate" (recommended)  
[auto] E — Q: "UI placement?" → Selected: "Repo Settings → Webhooks (Admin-gated), list + create/edit + recent deliveries" (recommended)  
[auto] F — Q: "Retries?" → Selected: "Bounded automatic retries with backoff; record every attempt" (recommended)

<domain>
## Phase Boundary

Repo admins subscribe outbound HTTPS webhooks for repository events, the instance delivers signed JSON payloads for subscribed events, and admins can inspect recent delivery attempts and HTTP status. Delivers HOOK-01, HOOK-02, HOOK-03.

**Requirements:** HOOK-01, HOOK-02, HOOK-03  
**Depends on:** Phase 11 (Issues), Phase 12 (Pull Requests — PR event emitters + payload fields)

**Success criteria (from ROADMAP):**
1. Repo admin can create, edit, and delete outbound webhooks for repo events
2. Instance delivers webhook payloads for subscribed events (at least push, PR, and issue events)
3. Repo admin can view recent webhook delivery attempts and response status

**Out of scope:**
- Organization-wide or instance-global webhooks (repo-scoped only)
- Inbound / reverse webhooks (GitHub Apps-style)
- Webhook UI outside repo settings (no chrome tab)
- In-app notifications (Phase 17)
- Actions workflow triggers (Phase 19 — may consume the same event bus later; do not invent Actions coupling here)
- Custom payload templates / JMESPath transforms
- `application/x-www-form-urlencoded` content type
- Legacy `X-Hub-Signature` (SHA-1)
- Guaranteed exactly-once delivery / external queue brokers (Redis/SQS)

**UI hint:** yes — Admin-only section under `/{owner}/{repo}/settings` (Webhooks list, form, delivery detail/history). Reuse existing Admin gate (`can_admin` / `resolve_repo_for_admin`).

</domain>

<decisions>
## Implementation Decisions

### A — Scope & ACL
- **D-HOOK-01:** Webhooks are **repository-scoped** only in Phase 18 (no org/global hooks) — **Reversibility:** reversible (org hooks can add later)
- **D-HOOK-02:** **Admin** capability required for create / update / delete / list / view deliveries / ping (HOOK-01, HOOK-03). Non-admins get the same anti-enumeration behavior as other Admin settings (`repo.not_found` where private) — **Reversibility:** reversible
- **D-HOOK-03:** Each webhook has: **HTTPS URL**, **secret**, **active** boolean, **subscribed event set**, optional **description**/name — **Reversibility:** reversible
- **D-HOOK-04:** URL must be **https://** in production/cloud; **http://localhost / loopback** allowed in dev/test only (ENV or compile-test override) — **Reversibility:** reversible — SSRF hygiene

### B — Event catalog & payloads
- **D-HOOK-05:** Ship at least: **`push`**, **`pull_request`**, **`issues`**, plus synthetic **`ping`** on create/manual redeliver-test — **Reversibility:** reversible
- **D-HOOK-06:** Event names and envelope headers follow **GitHub Hookshot conventions**: `X-GitHub-Event`, `X-GitHub-Delivery` (UUID), `X-GitHub-Hook-ID`, `User-Agent: Oxidean-Hookshot/*`, `Content-Type: application/json` — **Reversibility:** costly — public integration contract
- **D-HOOK-07:** Payload bodies are **GitHub-shaped JSON** (action + resource + repository + sender). Prefer field names familiar to GitHub consumers; Oxidean-specific extras only when no GitHub analog exists — **Reversibility:** costly — payload contract
- **D-HOOK-08:** **`issues` actions** at minimum: `opened`, `edited`, `closed`, `reopened` (map from Phase 11 issue.create/update lifecycle). **`issue_comment`** is Claude's discretion if it fits the same emitter without scope creep — **Reversibility:** reversible
- **D-HOOK-09:** **`pull_request` actions** at minimum: `opened`, `edited`, `closed`, `reopened`, `synchronize`, `merged` (or GitHub's `closed` + `merged: true`). Payload **PR identity** MUST align with Phase 12: shared per-repo **`#N`** with issues (D-PR-02), head may be same-repo or fork (D-PR-01…03), include draft flag if Phase 12 ships it — **Reversibility:** costly — couples to PR schema
- **D-HOOK-10:** **`push`** fires after successful **git-receive-pack** (HTTPS and SSH) when refs update; include ref, before/after SHAs, commits summary, pusher/sender, repository — **Reversibility:** reversible
- **D-HOOK-11:** Delivery is **best-effort async** after the mutating request succeeds (do not fail the user RPC/git push if the hook endpoint is down) — **Reversibility:** reversible

### C — Delivery & signing
- **D-HOOK-12:** Sign every delivery with **HMAC-SHA256** over the **raw JSON body**; header `X-Hub-Signature-256: sha256=<hex>`. Do **not** emit SHA-1 `X-Hub-Signature` — **Reversibility:** costly — signing contract
- **D-HOOK-13:** Persist a **delivery** row per logical event fan-out and **attempt** rows (or equivalent) with: timestamp, event, action, HTTP status (or error class), duration, request-id (`X-GitHub-Delivery`), success/failure — **Reversibility:** costly — audit schema
- **D-HOOK-14:** **Automatic retries** with exponential backoff for transient failures (timeouts, 5xx, connection errors); bounded max attempts (discretion: ~3–5). Do not retry definitive 4xx except optionally 429 — **Reversibility:** reversible
- **D-HOOK-15:** Inactive webhooks skip enqueue; deleted webhooks stop new deliveries (in-flight attempts may finish) — **Reversibility:** reversible

### D — Secrets & SSRF
- **D-HOOK-16:** Secret is **required** on create; list/get returns **masked** secret (never full value). Create/rotate may return plaintext **once** in the mutation response (PAT reveal pattern) — **Reversibility:** reversible
- **D-HOOK-17:** Store secret in DB such that the API process can sign (plaintext column acceptable for v1; prefer not logging it). Rotate via update — **Reversibility:** reversible (at-rest encryption later)
- **D-HOOK-18:** Outbound client must apply **SSRF guards**: block link-local / metadata IPs / non-http(s) schemes; enforce timeouts and max response body read for status capture — **Reversibility:** reversible — security

### E — UI & history
- **D-HOOK-19:** Repo Settings gains a **Webhooks** section (Admin-only): list hooks, create/edit/delete, toggle active, manage event checkboxes, view **recent deliveries** with status — **Reversibility:** reversible
- **D-HOOK-20:** Delivery history shows recent attempts (discretion: last ~25–50 per hook) with event, time, HTTP status / error, success badge; optional expand for response snippet (truncated) — **Reversibility:** reversible
- **D-HOOK-21:** Support **Redeliver** of a past delivery and **Ping** to test the endpoint (Admin) — **Reversibility:** reversible

### F — Cross-phase wiring
- **D-HOOK-22:** Emitters hook Phase 11 **issue** mutations and Phase 12 **pull** mutations; push hooks Smart HTTP receive-pack + SSH receive path after successful update — **Reversibility:** reversible
- **D-HOOK-23:** Introduce a small internal **`WebhookDispatcher` / event-bus seam** in `oxidean-api` so Phase 17 notifications and Phase 19 Actions can subscribe later without rewriting callers — **Reversibility:** costly — shared event seam
- **D-HOOK-24:** On repo **transfer/rename**, webhooks stay on `repository_id` (already Phase 15 D-REL-11 intent) — **Reversibility:** reversible

### Claude's Discretion
- Exact max retry count / backoff schedule / delivery retention purge interval
- Whether `issue_comment` ships in Phase 18
- Exact RPC naming (`webhook.*` vs `repo.webhook.*`)
- Whether delivery response body is stored (truncated) vs status-only
- Exact Settings IA (inline section vs nested `/settings/hooks` routes)
- Migration number (next free after packages)
- Whether push payload includes full commit list or truncated like GitHub

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Planning / requirements
- `.planning/ROADMAP.md` — Phase 18 goal, success criteria, depends on 11 + 12
- `.planning/REQUIREMENTS.md` — HOOK-01, HOOK-02, HOOK-03
- `.planning/phases/12-pull-requests/12-CONTEXT.md` — PR identity, fork heads, `#N`, draft, ACL (D-PR-01…29) for `pull_request` payloads
- `.planning/phases/11-issues/11-CONTEXT.md` — issue lifecycle + `#N` (D-ISS-01…)
- `.planning/phases/10-orgs-permissions/10-CONTEXT.md` — Admin capability
- `.planning/phases/15-releases-transfer/15-CONTEXT.md` — D-REL-11 webhooks follow `repository_id` on transfer

### Product / stack
- `docs/ARCHITECTURE.md` — RPC vs HTTP, ACL surfaces
- `docs/API.md` — RPC conventions
- `docs/CONFIGURATION.md` — ENV knobs pattern
- `.agents/skills/octane/SKILL.md` — Octane `.tsrx` UI (mandatory for settings UI)

### External parity (read, do not copy verbatim large docs)
- GitHub: Validating webhook deliveries (`X-Hub-Signature-256`)
- GitHub: Webhook events and payloads (`push`, `pull_request`, `issues`)

### Code mirrors
- `crates/oxidean-api/src/repo/acl.rs` — Admin resolve
- `crates/oxidean-api/src/issue/mod.rs` — issue mutation hooks
- `crates/oxidean-api/src/routes/git_smart_http.rs` — receive-pack success path
- `crates/oxidean-api/src/jobs/schedule.rs` — background job spawn pattern (retry worker)
- `apps/web/src/routes/$owner.$repo.settings.tsrx` — Admin settings shell
- `apps/web/src/components/settings/pat-reveal.tsrx` — one-time secret reveal pattern

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `Capability::Admin` + `can_admin` on repo settings UI
- `reqwest` + `sha2` already in `oxidean-api` (HMAC via RustCrypto `hmac` crate — legitimacy gate if added)
- In-process `spawn_background_jobs` for retry/drain loops
- PAT create one-time reveal UX for secrets
- Issue RPC surface fully shipped (Phase 11); push via Smart HTTP + SSH
- Phase 12 PR domain is CONTEXT-locked; emitter plans must wait on / assume PR tables + RPC from Phase 12 execution

### Integration Points
- After `issue.create` / `issue.update` (and comments if shipping)
- After PR open/update/merge/close (Phase 12 module)
- After successful receive-pack / SSH push ref updates
- Settings page Webhooks section + RPC `webhook.*`
- Optional internal event seam shared with future NOTF/ACT phases

</code_context>

<specifics>
## Specific Ideas

- User: GitHub-like outbound webhooks; CRUD for repo admins; deliver at least push/PR/issue; show delivery attempts/status
- User: Auto-decide gray areas; align PR payloads with Phase 12 PR model from CONTEXT
- Prefer integration compatibility (GitHub header/event names) over inventing Oxidean-only vocabulary

</specifics>

<deferred>
## Deferred Ideas

- Organization / global webhooks
- GitHub Apps / installation tokens / inbound webhooks
- Custom payload transforms
- Form-urlencoded content type + SHA-1 signatures
- External durable queues (Redis/NATS/SQS)
- Webhook event catalog beyond push/PR/issues (releases, packages, star, fork — later phases)
- Coupling Actions triggers to this dispatcher (Phase 19 owns workflow trigger semantics)

</deferred>

---

*Phase: 18-webhooks*  
*Discuss: auto-decide GitHub parity · 2026-09-16*
