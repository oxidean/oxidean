# Rules — providers (U4)

```yaml
rules:

  # ---- BR1 — registry & custody ----

  - id: BR1.1
    statement: >
      `admin.oxpilot.*` requires a session-authenticated admin actor
      (reuses `require_admin`) and is ENABLEMENT-GATE-EXEMPT like
      `oxpilot.status` — a disabled feature must never wedge the admin
      surface that recovers it. Bearer/PAT denied per the SessionOnly
      catch-all; non-admin session ⇒ `admin.forbidden`.
    rationale: Admin surface follows the admin.mcp.* precedent (AC5.5.1);
      gate exemption is the recovery path (R-20).

  - id: BR1.2
    statement: >
      Provider key material is write-only: `settings_set` accepts a new
      plaintext key, encrypts under AES-256-GCM (OXIDEAN_OXPILOT_SECRETS_KEY
      → OXIDEAN_SESSION_SECRET fallback chain), stores ciphertext+last4,
      and never returns ciphertext or plaintext on any read. A missing
      key source resolves `crypto.unavailable` (500).
    rationale: FR6.5, NFR1/NFR2 custody rules, actions-secrets precedent.

  - id: BR1.3
    statement: >
      A provider adapter is instantiated only from an EffectiveProviderConfig
      that passed BR3.1–BR3.4; construction success is required for a
      resolved `provider_id` to count as present — failure ⇒ `misconfigured`.
    rationale: `oxpilot.disabled(no_provider|misconfigured)` vocabulary — the
      disabled-reason contract U1 introduced.

  - id: BR1.4
    statement: >
      The registry is a closed static set {openai, anthropic, ollama, stub}.
      `stub` is excluded from `providers_list` and refused by `settings_set`
      (`invalid_provider`) outside test builds — it exists for tests only.
    rationale: Stub must not be selectable as a production provider.

  # ---- BR2 — resolution & enablement ----

  - id: BR2.1
    statement: >
      Enablement resolution merges the singleton override row over env
      field-by-field: enabled = COALESCE(enabled_override, env-flag);
      provider = COALESCE(provider_id, env-provider); likewise model,
      base_url, key, budgets. Unknown provider_id ⇒ `misconfigured`.
    rationale: FR6.4/FR6.5 — env defaults + DB overrides, mcp precedent.

  - id: BR2.2
    statement: >
      `oxpilot.status` is gate-exempt and always answers from the current
      OxpilotRuntimeSlot: enabled flag, provider id+model when resolved,
      machine reason otherwise — never error-shape.
    rationale: U1 contract C1 (gate-exempt status).

  - id: BR2.3
    statement: >
      `oxpilot_settings` fields absent from a settings_set payload leave
      the stored override untouched; present-but-null clears that field —
      a deliberate DEVIATION from `admin.mcp.updateSettings`'s Option<T>
      null-collapse (requires a Nullable<T>/Option<Option<T>> decode);
      `clear_overrides` nulls the whole row (pure env thereafter); key
      fields follow BR3.4's conflict rules.
    rationale: mcp precedent for the merge shape; null-clears pinned so a
      stored override can be removed field-by-field (R-16).

  - id: BR2.4
    statement: >
      `settings_get` returns the EFFECTIVE resolved values plus per-field
      `*_overridden` flags (admin.mcp precedent): `enabled` (resolved) +
      `enabled_overridden`, `provider_id`, `model`, `base_url`,
      `user_budget`, `instance_budget`, each with its `*_overridden`
      flag — admins must distinguish override from env default. Key
      surfaces as `key_set` + `key_last4` computed against the RESOLVED
      source: stored last4 when DB-sourced, computed last4 when env
      `OXIDEAN_OXPILOT_API_KEY`-sourced, both false/null when absent.
    rationale: Resolved-view ambiguity makes overrides invisible (R-08);
      field names and key-source scope pinned (R-21).

  # ---- BR3 — validation (binds the RESOLVED config) ----

  - id: BR3.1
    statement: >
      `settings_set` validates the EFFECTIVE config (candidate row merged
      over env), not raw submitted fields. Resolved-provider checks apply
      ONLY when a `provider_id` resolves: present-but-unknown id ⇒
      `invalid_provider`; resolved model must appear in that provider's
      `models` when the descriptor advertises one (else `invalid_provider`).
      NO resolved provider_id ⇒ the write is VALID (budget-only, flag-only,
      and `clear_overrides` writes always pass) and persists a config that
      resolves `no_provider` — a disabled slot is a legitimate outcome,
      never a rejection. When a provider_id DOES resolve, provider
      construction must also succeed — for key-requiring adapters that
      makes `api_key` effectively mandatory (resolved env/DB key or one
      supplied in the same payload); unbuildable ⇒ `invalid_provider`.
      EXEMPTION: a write touching ONLY `enabled_override`/budget fields
      skips the build entirely — flag-off and budget writes must never
      wedge on an already-broken resolved provider (recovery path).
    rationale: A partial update that flips provider_id while inheriting a
      stale field must fail on the merge, not pass on the raw input (R-02);
      and rejecting provider-less writes would make `clear_overrides` and
      fresh deployments unwritable (R-12, R-19).

  - id: BR3.2
    statement: >
      base_url bound: when a `provider_id` resolves, the resolved base_url
      must be http(s) — no resolved provider ⇒ the http(s) shape check still
      runs but the private-target bound is skipped (no consumer). With a
      resolved provider:
      non-`ollama` resolved providers additionally forbid private/loopback/
      link-local/metadata targets. Host forms classified: literal IP in any
      notation (dotted, hex, octal, integer, IPv6, IPv4-mapped-IPv6) AND
      DNS names resolved at validation time — every resolved address must
      be public. DNS can re-resolve differently at connect (documented
      TOCTOU bound); operators pinning a private network segment should
      use the Ollama adapter or a literal public IP.
    rationale: SSRF bound on an admin-supplied egress URL; `ollama` is the
      blessed self-hosted exception (US5.4/AC5.4.1).

  - id: BR3.3
    statement: >
      Budget overrides must be positive integers when present; zero/negative/
      non-integer ⇒ `bad_input`. Budget *counters* and refusal are U2's —
      U4 only stores the configured ceilings.
    rationale: AC5.2.1 configures ceilings here; enforcement is session-core's.

  - id: BR3.4
    statement: >
      `settings_set` request semantics: `api_key` sets a new key (encrypt
      + store last4); `clear_key` erases key_ciphertext+key_last4 together;
      both in one payload ⇒ `bad_input`; empty/whitespace `api_key` ⇒
      `bad_input`; `clear_overrides` nulls every override field including
      the key pair and takes precedence over same-payload field writes.
    rationale: Conflict semantics must be pinned before codegen (R-09).

  # ---- BR4 — provider swap & adapter behavior ----

  - id: BR4.1
    statement: >
      Adapter retry: at most one retry and ONLY for connect-phase transport
      failures — DNS/TLS/refused/pool-acquire errors before any request
      bytes are transmitted. Once the request is on the wire the call never
      retries, whatever the outcome; streaming stalls never retry.
      Idempotent only while no request reached the provider ⇒ no double-bill.
    rationale: "Zero bytes received" admits sent-but-unanswered POSTs —
      retrying those bills twice (R-06).

  - id: BR4.2
    statement: >
      Timeout: bounded connect+read budget via reqwest; a provider hang
      produces a `ProviderError{code}` terminal frame, never a wedged turn.
    rationale: NFR4 availability; U1 C4 terminal `error` frame.

  - id: BR4.3
    statement: >
      Provider swap is a whole-slot rebuild under a single async rebuild
      mutex: settings_set serializes resolve → validate → build → persist →
      swap → audit in ONE critical section. Build runs BEFORE persist — a
      failed build returns `invalid_provider`, persists nothing, and leaves
      the old slot untouched; a provider-less resolved config skips the
      build and installs a `provider: None` + reason slot. On success the
      row commits and the new slot (carrying the SHARED `Arc<ActiveTurns>`
      handle) swaps in atomically; in-flight turns keep their captured
      provider Arc. The audit row writes inside the same section, after
      the swap. Concurrent settings_set calls order on the mutex —
      persisted row, installed slot, and audit order can never diverge.
    rationale: Persist-then-build leaves unbuildable config stored (self-DoS
      via misconfigured); unordered rebuilds let a losing writer's slower
      build swap last and diverge (R-01, R-04).

  - id: BR4.4
    statement: >
      `ollama` is keyless: builds without key material; missing base_url
      defaults to `http://127.0.0.1:11434`. Its private-target exemption
      is the SSRF exception (BR3.2).
    rationale: AC5.4.1 — self-host without hosted-provider keys.

  - id: BR4.5
    statement: >
      Provider clients disable redirect following (`redirect(Policy::none())`)
      — a validated public URL must not 302 onto private/metadata egress;
      redirects surface as `ProviderError`, never auto-followed.
    rationale: Redirect chasing defeats the BR3.2 bound end-to-end (R-05).

  # ---- BR5 — error vocabulary & audit ----

  - id: BR5.1
    statement: >
      `admin.oxpilot.*` errors: `auth.unauthenticated` (no session),
      `auth.pat_scope` (bearer/PAT), `admin.forbidden`/`auth.banned`
      (non-admin session), `rpc.bad_input` (malformed fields or key/clear
      conflict), `invalid_provider` (failed build or unknown resolved
      provider_id/model). `crypto.unavailable` (missing key source) is the
      ONE additive code beyond C1's sketch — C1 does not cover crypto
      failure and inventing no substitute is worse.
    rationale: Codes name the shipped error shapes exactly (R-17).

  - id: BR5.2
    statement: >
      Every settings mutation writes an audit row {actor, action,
      field_names_only, ts} — field NAMES only; values never audited
      (key material must not leak even hashed).
    rationale: NFR audit requirement; custody-safe audit (NFR1).

  - id: BR5.3
    statement: >
      Wiremock covers every adapter: golden request bodies asserted,
      canned SSE/non-SSE replies drive the full ChatStreamItem vocabulary;
      live provider calls are never made in tests.
    rationale: Q4 — real SDKs rejected, wiremock only.

  - id: BR5.4
    statement: >
      New-table pairing (US5.5 spanning duty, AC5.5.3): `oxpilot_settings`
      ships migrations for sqlite + postgres + mysql AND extends
      `tests/dialect_oxpilot.rs` AND the TESTING.md dialect row in the same
      change. `admin.oxpilot.*` procs live inside `oxidean-api-oxpilot`'s
      closed PROCEDURES set + dispatch (AC5.5.1's pinning pattern extends).
    rationale: Every new oxidean-db table carries the dialect pairing;
      the crate's procedure registry stays a pinned closed set.

  - id: BR5.5
    statement: >
      U1 seam delta (additive, same PR): the immutable env-built
      `SharedOxpilot` becomes `RwLock<OxpilotRuntimeSlot>`; the enablement
      path (`resolve_enablement`/gate/`oxpilot.status`) becomes DB-aware
      async — reads the resolved slot; `run_turn` keeps capturing the
      provider Arc (in-flight turns unaffected by swaps per U1's
      captured-Arc contract).
    rationale: The slot-rebuild workflow only exists if the holder is
      swappable; U1's sync env-only resolution must read the slot (R-07).

  - id: BR5.6
    statement: >
      New-env-var pairing (AC5.1.4): every `OXIDEAN_OXPILOT_*` var this
      unit introduces — `API_KEY`, `BASE_URL`, `USER_BUDGET`,
      `INSTANCE_BUDGET`, `SECRETS_KEY` — ships a `.env.example` row AND a
      `docs/CONFIGURATION.md` entry AND a `.railway/railway.ts` env
      surface update in the same change.
    rationale: AC5.1.4 requires every new env var documented; U4 adds
      five vars not yet in `.env.example` or CONFIGURATION.md.
```

## Summary

24 rules across five groups. BR1 pins custody (write-only key, closed
registry, construction-required). BR2 pins env+DB resolution, the
gate-exempt status, merge semantics, and the effective+overridden read
shape. BR3 binds validation to the RESOLVED config — SSRF covers literal
IP forms plus DNS-resolved answers, key/clear conflicts are explicit,
budgets are stored-not-enforced. BR4 pins connect-phase-only retry,
timeouts, redirect-free clients, the keyless Ollama exception, and the
mutex-serialized build→persist→swap critical section whose shared turns
handle keeps in-flight work alive. BR5 closes the error vocabulary,
value-free audit, wiremock-only testing, the new-table dialect pairing,
and the U1 seam delta.
