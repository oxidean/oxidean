# Entities — providers (U4)

```yaml
entities:
  - name: OxpilotSettings
    kind: persisted
    description: >
      Singleton sparse-override row for the Oxpilot feature: feature flag,
      provider selection, model, base URL, key custody, and configured
      budgets. NULL fields inherit the matching OXIDEAN_OXPILOT_* env var —
      the admin.mcp.* env-default/DB-override pattern (FR6.5, AC5.1.3).
    attributes:
      - { name: id, type: integer, required: true, unique: true, constraints: ["singleton row (id = 1)"] }
      - { name: enabled_override, type: boolean|null, required: false, constraints: ["NULL = follow OXIDEAN_OXPILOT_ENABLED env"] }
      - { name: provider_id, type: text|null, required: false, constraints: ["NULL = follow OXIDEAN_OXPILOT_PROVIDER env", "must name a registry id when set"] }
      - { name: model, type: text|null, required: false, constraints: ["NULL = provider default / OXIDEAN_OXPILOT_MODEL env", "validated against the resolved provider's models when known"] }
      - { name: base_url, type: text|null, required: false, constraints: ["NULL = provider default / OXIDEAN_OXPILOT_BASE_URL env", "http(s) only; SSRF bound per BR3.2"] }
      - { name: key_ciphertext, type: text|null, required: false, constraints: ["AES-256-GCM nonce||ct hex under OXIDEAN_OXPILOT_SECRETS_KEY → OXIDEAN_SESSION_SECRET chain", "never returned by any read"] }
      - { name: key_last4, type: text|null, required: false, constraints: ["last 4 chars of the stored key", "the only key material a read may emit"] }
      - { name: user_budget, type: integer|null, required: false, constraints: ["NULL = OXIDEAN_OXPILOT_USER_BUDGET env", "tokens per period; U2 owns the counters"] }
      - { name: instance_budget, type: integer|null, required: false, constraints: ["NULL = OXIDEAN_OXPILOT_INSTANCE_BUDGET env"] }
      - { name: updated_at, type: timestamp, required: true }
    constraints:
      - key_ciphertext and key_last4 are always set or cleared together.
      - Clearing the override (all fields NULL) restores pure env behavior.

  - name: ProviderDescriptor
    kind: computed
    description: >
      Static registry metadata for one adapter — what `admin.oxpilot.providers_list`
      returns per entry. Never holds key material.
    attributes:
      - { name: id, type: text, required: true, unique: true, allowed_values: ["openai", "anthropic", "ollama", "stub"] }
      - { name: models, type: "text[]", required: true, description: "advertised model ids the adapter accepts" }
      - { name: configured, type: boolean, required: true, description: "selectable now — key material resolvable for key-required adapters (env OXIDEAN_OXPILOT_API_KEY or stored key), always true for keyless adapters (ollama, stub)" }
      - { name: requires_key, type: boolean, required: true, description: "false only for ollama + stub" }

  - name: EffectiveProviderConfig
    kind: computed
    description: >
      Result of resolving the sparse override row over env defaults — the
      single validation target and the input to provider construction.
      BR3.x rules bind to THIS merged shape, never to raw request fields.
    attributes:
      - { name: enabled, type: boolean, required: true }
      - { name: provider_id, type: text|null, required: false }
      - { name: model, type: text|null, required: false }
      - { name: base_url, type: text|null, required: false }
      - { name: api_key_plaintext, type: text|null, required: false, constraints: ["exists only in memory during provider construction", "never logged, never persisted, never emitted"] }
      - { name: user_budget, type: integer|null, required: false }
      - { name: instance_budget, type: integer|null, required: false }
      - { name: disabled_reason, type: text|null, required: false, allowed_values: ["flag_off", "no_provider", "misconfigured", null] }

  - name: OxpilotRuntimeSlot
    kind: computed
    description: >
      The swappable runtime state replacing U1's env-built immutable
      SharedOxpilot: provider handle, advertised model, disabled reason,
      and the SHARED ActiveTurns handle. Held under an async RwLock on the
      app state; a rebuild constructs a fresh slot reusing the same turns
      handle, then swaps it — in-flight turns keep their captured provider
      Arc and map entries untouched. Resolution is DB-aware (async).
    attributes:
      - { name: provider, type: "Option<Arc<dyn LlmProvider>>", required: true }
      - { name: model, type: "Option<text>", required: false }
      - { name: disabled_reason, type: text|null, required: false, allowed_values: ["flag_off", "no_provider", "misconfigured", null] }
      - { name: turns, type: "Arc<ActiveTurns>", required: true, constraints: ["stable across rebuilds — the Arc is cloned per slot, the map is never re-created"] }

  - name: ProviderHealthNote
    kind: computed
    description: >
      What `oxpilot.status` reports — enabled flag, provider id+model when
      resolved, and the machine reason when not. Carries no secret material.
    attributes:
      - { name: enabled, type: boolean, required: true }
      - { name: provider, type: "{id, model}|null", required: false }
      - { name: reason, type: text|null, required: false }

relationships:
  - OxpilotSettings --resolves-over--> OXIDEAN_OXPILOT_* env vars: sparse override, field-by-field.
  - EffectiveProviderConfig --selects--> ProviderDescriptor: provider_id must name a registry id else misconfigured.
  - OxpilotRuntimeSlot --materializes--> EffectiveProviderConfig: slot state IS the resolved projection.
  - ProviderHealthNote --derived-from--> OxpilotRuntimeSlot: the flag-off/read-only projection.
```

## Summary

One persisted singleton (`OxpilotSettings`) carries every operator-tunable
as a sparse override over env — the `admin.mcp.*` pattern restated for
`OXIDEAN_OXPILOT_*`. Computed views keep secret material off every read
path: `ProviderDescriptor` (registry surface), `EffectiveProviderConfig`
(the merged validation target; plaintext key material exists only inside
construction memory), `OxpilotRuntimeSlot` (the hot-swappable runtime
state whose shared `turns` handle survives rebuilds), and
`ProviderHealthNote` (the status projection).
