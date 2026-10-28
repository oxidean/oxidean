# Reliability Requirements — providers (U4)

Consumes: `construction/providers/functional-design/rules.md`
(BR4.1–BR4.5, BR3.1, BR4.3), `functional-design/functional-spec.md`
(WF2–WF3), `inception/requirements-analysis/requirements.md` (NFR4).

## Write-path integrity

| id | requirement | basis |
|----|-------------|-------|
| NFR4.1 | Build-before-persist: a failed provider build returns `invalid_provider`, persists NOTHING, and leaves the installed slot untouched — a bad settings write can never self-DoS the feature | BR4.3, R-01 |
| NFR4.2 | The whole settings_set section (resolve→validate→build→persist→swap→audit) serializes under one async rebuild mutex — concurrent writers order on the mutex; row, slot, and audit can never diverge | BR4.3, R-04 |
| NFR4.3 | Writes touching ONLY `enabled_override`/budget fields skip the provider build entirely — flag-off and budget writes succeed on an already-broken resolved provider (the recovery path can never wedge) | BR3.1, R-19 |
| NFR4.4 | `clear_overrides` and provider-less resolved writes persist a `no_provider` config and install a `provider: None`+reason slot — a disabled slot is a first-class legitimate outcome | BR3.1, R-12 |

## In-flight survival

| id | requirement | basis |
|----|-------------|-------|
| NFR4.5 | Slot swaps reuse the shared `Arc<ActiveTurns>` — in-flight turns keep their captured provider Arc and map entries; an admin rebuild never kills a live turn | BR4.3, Q4 |
| NFR4.6 | Disabled states (`flag_off`, `no_provider`, `misconfigured`) install as reason-bearing slots — `oxpilot.status` and the gate always answer, never hang on absent provider | entities, BR2.2 |

## Provider-call bounds

| id | requirement | basis |
|----|-------------|-------|
| NFR4.7 | Timeouts are bounded: connect 10 s, total 300 s — a provider hang ends in a coded `ProviderError` terminal frame, never a wedged turn | BR4.2, Q7 |
| NFR4.8 | Retry bound: at most ONE retry and ONLY on connect-phase transport failure (request never transmitted); sent-but-unanswered requests never retry — no double-billing | BR4.1, R-06 |
| NFR4.9 | Redirects are never followed — `redirect(Policy::none())`; a 3xx surfaces as `ProviderError` rather than followed egress | BR4.5, R-05 |
| NFR4.10 | Stream stalls never retry; mid-stream provider failure classifies into a stable `ProviderError.code` terminal frame (C2) | BR4.1, WF4 |

## Boot & degradation

| id | requirement | basis |
|----|-------------|-------|
| NFR4.11 | Boot-time slot build resolves env+DB; an unreadable settings row degrades to env-only resolution + warn log — boot never crashes on the settings table | Q5 |
| NFR4.12 | Misconfigured/unknown provider values at any resolution point resolve `misconfigured` — named and reported, never a panic or silent no-op | BR1.3, BR2.2 |

## Custody integrity

| id | requirement | basis |
|----|-------------|-------|
| NFR4.13 | `key_ciphertext`+`key_last4` are always set or cleared together — no half-written key row can exist after any write path | entities, BR3.4 |
