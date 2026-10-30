# Entities — connector-gateway (U6)

## Entity model

```yaml
entities:
  - name: Pairing
    kind: persisted (oxpilot_pairings)
    provenance: components.md §ConnectorGateway — new table this unit.
    description: >
      The device-grant-style enrollment record. `pairing_create` mints
      it for the owning user; the connector redeems it once via
      `POST /api/oxpilot/connector/pair`. Single-use, short-lived,
      lazily expired.
    attributes:
      - { name: pairing_id, type: text, required: true, constraints: ["pk; uuid"] }
      - { name: user_id, type: text, required: true, constraints: ["owner scoping — the code was minted for this user; the redeemed Target inherits it"] }
      - { name: code_hash, type: text, required: true, constraints: ["sha256 of the plaintext code; plaintext leaves the process exactly once — in the pairing_create response (AC4.1.4)"] }
      - { name: target_name, type: text, required: true, constraints: ["label the user gave at mint; carried onto the Target row"] }
      - { name: requested_caps, type: json, required: true, constraints: ["capability set the user pre-authorized at mint; the connector's request must be a subset"] }
      - { name: state, type: text, required: true, allowed_values: ["pending", "claimed", "expired"], constraints: ["pending→claimed on successful redeem; pending→expired lazily when expires_at passes — minted codes die only by expiry or redeem (no cancel proc exists in C1)"] }
      - { name: expires_at, type: timestamp, required: true, constraints: ["mint + PAIRING_CODE_TTL (10 min); checked lazily, no sweeper"] }
      - { name: created_at, type: timestamp, required: true }
    constraints:
      - code uniqueness enforced by UNIQUE(code_hash).
      - A claimed pairing is immutable — the redeem path is the only
        writer after mint.
      - Multiple pending pairings per user coexist — each minted code
        stays independently redeemable until expiry (no supersede;
        the user picks whichever they ran).
      - Pairing rows are never returned by any proc — their existence
        surfaces only through the mint response and audit rows.

  - name: Target
    kind: persisted (oxpilot_targets)
    provenance: components.md §ConnectorGateway — new table this unit.
    description: >
      A paired computer-use endpoint bound to one user. The
      `pending_consent` → `connected` transition is gated by the
      connect grant (AC4.2.3 — server-enforced, never UI-only).
    attributes:
      - { name: target_id, type: text, required: true, constraints: ["pk; uuid; the connector-facing identity in claim/report/heartbeat; C3's response field `connector_id` is this same value in v1 (the sketch carries both names for client flexibility — documented in the C3 delta so U7 emits one value for both)"] }
      - { name: user_id, type: text, required: true, constraints: ["owner scoping on every read/write (AC5.5.4)"] }
      - { name: pairing_id, type: text, required: true, constraints: ["the mint that created it — one pairing mints ≤ 1 target"] }
      - { name: name, type: text, required: true }
      - { name: caps, type: json, required: true, constraints: ["closed subset of {exec_shell, fs_read, fs_write} — FR5.3's bound; no GUI caps exist"] }
      - { name: status, type: text, required: true, allowed_values: ["pending_consent", "connected", "disconnected", "revoked"], constraints: ["pending_consent→connected on connect-grant allow; connected→disconnected on target_disconnect; any→revoked on target_revoke (terminal for the row)"] }
      - { name: verifier_hash, type: text, required: true, constraints: ["sha256 of the connector's verifier token; the token itself leaves the process once, in the pair response (Q2)"] }
      - { name: last_seen_at, type: timestamp|null, required: false, constraints: ["touched by every authenticated connector touch (claim + heartbeat + report); derives TargetView online/offline — never a stored flag"] }
      - { name: created_at, type: timestamp, required: true }
      - { name: disconnected_at, type: timestamp|null }
      - { name: revoked_at, type: timestamp|null }
    constraints:
      - UNIQUE(verifier_hash) — a verifier resolves to ≤ 1 target.
      - Re-pairing redeems a NEW pairing into a NEW Target row — one
        pairing mints ≤ 1 target, there is no row rotation. The
        disconnected/revoked row is terminal and persists as
        audit-visible history in `target_list`.
      - `revoked` is terminal: dispatch, claim dequeue, and
        grant_respond all refuse; heartbeat still authenticates to
        report `revoked: true` (Q3/Q7).

  - name: Grant
    kind: persisted (oxpilot_grants)
    provenance: components.md §ConnectorGateway — new table this unit.
    description: >
      The consent record. Two shapes: connect grants (session_id NULL,
      from the connect dialog — activate a target) and action grants
      (session-scoped, minted by dispatch_action's Denied path —
      authorize work). No cross-session "always allow" exists (FR5.2).
    attributes:
      - { name: grant_id, type: text, required: true, constraints: ["pk; uuid; consent card ref"] }
      - { name: target_id, type: text, required: true, constraints: ["fk → Target"] }
      - { name: user_id, type: text, required: true, constraints: ["denormalized owner for scoped reads + audit without a join"] }
      - { name: session_id, type: text|null, required: false, constraints: ["NULL = connect grant; set = action grant tied to the dispatching session; FK → oxpilot_sessions ON DELETE CASCADE (AC1.5.2 pins row removal — the durable consent evidence is the audit_events row, which survives the cascade)"] }
      - { name: kind, type: text, required: true, allowed_values: ["connect", "action", "action_once", "action_session"], constraints: ["pending action grants mint as `action` (unflavored) — the once-vs-session choice is the user's at consent time (AC4.3.2); grant_respond stamps `action_once` (consumed by the one Action it materializes) or `action_session` (covers dispatches on that target in that session until expiry/revoke)"] }
      - { name: action_kind, type: text|null, required: false, constraints: ["action grants only — exec_shell|fs_read|fs_write"] }
      - { name: action_args, type: text|null, required: false, constraints: ["action grants only — bounded (≤ 2 KiB) redacted dispatch args; allow materializes the Action from these without a model round-trip"] }
      - { name: action_summary, type: text|null, required: false, constraints: ["≤ 512 B human-readable card text — C1's args_summary field"] }
      - { name: state, type: text, required: true, allowed_values: ["pending", "allowed", "denied", "expired", "revoked", "cancelled"], constraints: ["pending→allowed|denied via grant_respond; pending→expired lazily; any live state→revoked|cancelled via revoke/delete paths"] }
      - { name: expires_at, type: timestamp, required: true, constraints: ["action grants: mint + ACTION_GRANT_TTL (5 min); connect grants: mint + CONNECT_GRANT_TTL (10 min)"] }
      - { name: resolved_at, type: timestamp|null, required: false }
      - { name: created_at, type: timestamp, required: true }
    constraints:
      - Grant transitions emit audit rows (BR7.2) — the audit_events
        row is the durable consent evidence; the grant row itself
        cascades with its session (AC1.5.2).
      - `denied`/`expired`/`revoked`/`cancelled` are terminal; an
        `allowed` action_once grant with `resolved_at` set is
        consumed — it covers exactly the Action it materialized and
        never a later dispatch (BR4.2).
      - A second `grant_respond` on a resolved grant →
        `grant_not_pending` (C1 error vocabulary).

  - name: Action
    kind: persisted (oxpilot_actions)
    provenance: components.md §ConnectorGateway — new table this unit.
    description: >
      One unit of authorized work for a target. Minted `queued` either
      by dispatch_action (grant already live) or grant_respond(allow)
      on a pending action grant (args carried on the grant row).
    attributes:
      - { name: action_id, type: text, required: true, constraints: ["pk; uuid; claim/report/status key"] }
      - { name: target_id, type: text, required: true, constraints: ["fk → Target"] }
      - { name: user_id, type: text, required: true }
      - { name: session_id, type: text, required: true, constraints: ["actions are always session-scoped — connect grants never carry work; FK → oxpilot_sessions ON DELETE CASCADE (AC1.5.2)"] }
      - { name: grant_id, type: text, required: true, constraints: ["fk → Grant; re-checked at claim dequeue (BR5.1)"] }
      - { name: kind, type: text, required: true, allowed_values: ["exec_shell", "fs_read", "fs_write"] }
      - { name: args, type: text, required: true, constraints: ["bounded (≤ 2 KiB) redacted payload delivered verbatim in the claim response — the connector needs the full command"] }
      - { name: args_summary, type: text, required: true, constraints: ["≤ 512 B card/audit text"] }
      - { name: state, type: text, required: true, allowed_values: ["queued", "claimed", "running", "done", "failed", "cancel_requested", "cancelled"], constraints: ["queued→claimed at dequeue; claimed→running on the connector's next authenticated touch for that target (heartbeat or claim — proof the delivered work is executing; no C3 message carries per-action progress) ; terminal done|failed|cancelled"] }
      - { name: result_summary, type: text|null, required: false, constraints: ["bounded (≤ 2 KiB) redacted report excerpt — output tail + exit_code; C7's ActionView never carries raw output"] }
      - { name: created_at, type: timestamp, required: true }
      - { name: claimed_at, type: timestamp|null }
      - { name: finished_at, type: timestamp|null }
    constraints:
      - Terminal: done|failed|cancelled. cancel_requested is
        re-announced on every claim/heartbeat response until the row
        closes (idempotent kills — no delivery tracking needed); its
        terminal lands via report (closes to cancelled) or via the
        disconnect path (direct cancel — dead channel) — session
        delete closes to `cancelled` too since the row cascades
        before any cancel could deliver.
      - A report on an already-terminal action is an idempotent no-op
        returning {ack: true} (BR5.2).

  - name: ConnectorIdentity
    kind: in-memory (per-request, resolved by the connector-auth extractor)
    provenance: new — C3's verifier-class auth.
    description: >
      The authenticated connector request's scope, resolved by
      verifier_hash lookup. Every connector route consumes this —
      body-supplied target_id is validated against it, never trusted.
    attributes:
      - { name: target_id, type: text, required: true }
      - { name: user_id, type: text, required: true }
      - { name: caps, type: json, required: true, constraints: ["the target's granted caps — claim's capabilities are intersected, never widened"] }
      - { name: status, type: text, required: true, constraints: ["carried so heartbeat can answer revoked:true for an authenticating revoked target"] }

  - name: TargetView (wire)
    kind: wire (C1 oxpilot.target_list payload member)
    provenance: C1 — public trim.
    attributes:
      - { name: id, type: text, required: true }
      - { name: label, type: text, required: true }
      - { name: status, type: text, required: true, allowed_values: ["online", "offline", "revoked"], constraints: ["C1's pinned enum — online derives last_seen_at < LIVENESS_WINDOW; pending_consent/disconnected map to offline, revoked maps to revoked"] }
      - { name: lifecycle, type: text, required: true, allowed_values: ["pending_consent", "connected", "disconnected", "revoked"], constraints: ["the Target.status column verbatim — C1 additive amendment (U6) since the connect dialog needs the consent state the liveness enum can't express"] }
      - { name: pairing_id, type: text, required: true, constraints: ["the mint that created it — the pairing→target correlation the connect dialog polls against (C1 additive amendment)"] }
      - { name: granted_caps, type: json, required: true }
    constraints:
      - Never carries verifier_hash, raw action/verifier material, or
        last_seen_at — C1's 'secret material never in payload' +
        minimal-disclosure trims. (`pairing_id` IS carried — the
        amendment deliberately exposes the correlation handle.)

  - name: GrantView (wire)
    kind: wire (C1 grant_list/grant_get payload)
    provenance: C1 — payload {id, kind, capability, args_summary, state, expires_at}.
    attributes:
      - { name: id, type: text, required: true }
      - { name: kind, type: text, required: true, allowed_values: ["connect", "action"], constraints: ["wire collapses action_once/action_session to 'action' — C1's enum; the flavor rides capability/expires_at"] }
      - { name: capability, type: text, required: true, constraints: ["connect → the target's caps summary; action → action_kind"] }
      - { name: args_summary, type: text|null, required: false }
      - { name: state, type: text, required: true }
      - { name: expires_at, type: timestamp, required: true }
    constraints:
      - Never carries action_args — the card shows the summary only
        (C1 pins args_summary; raw args stay server-side for
        materialize).

  - name: ActionView (wire)
    kind: wire (C7 action_status return — the U5-facing trim)
    provenance: C7 — 'U5 never sees connector credentials or raw
      Action internals (verifier refs, claim cursors)'.
    attributes:
      - { name: action_id, type: text, required: true }
      - { name: kind, type: text, required: true }
      - { name: state, type: text, required: true }
      - { name: created_at, type: timestamp, required: true }
      - { name: args_summary, type: text, required: true }
      - { name: result_summary, type: text|null, required: false }
    constraints:
      - target_id/verifier/claim cursors never present — C7 trim.

  - name: ConnectorGateway (port)
    kind: seam (trait in oxidean-api-core::oxpilot beside SessionService/
      ToolExecutor/LlmProvider; impl in api-oxpilot)
    provenance: new — C7 + the route/proc backing.
    description: >
      The cross-unit seam U5's ToolExecutor consumes (C7) and the
      api-oxpilot routes/procs call. Installed on
      RpcCtx.oxpilot.gateway at boot.
    surface:
      - dispatch_action(user_id, session_id, target_id, kind, args) -> Result<Action, Denied>
      - action_status(user_id, action_id) -> Result<ActionView, AppError>
      - Denied: { reason: not_connected|offline|capability|consent_required, grant_id? }
        (C7 amendment — consent_required carries grant_id for the card)
      - # backing verbs for routes/procs (impl-side, not part of C7):
      - #   mint_pairing / redeem_pairing / resolve_verifier
      - #   claim_actions / report_action / heartbeat
      - #   list_targets / get_grant / respond_grant
      - #   disconnect_target / revoke_target / on_session_delete
```

## Observations

- **Table count**: four new tables (`oxpilot_pairings`,
  `oxpilot_targets`, `oxpilot_grants`, `oxpilot_actions`) in all three
  dialects — FR6.6 + AC5.5.3's dialect-test pin applies to each
  (migrations + the `dialect_oxpilot` `-E` filter + the TESTING.md
  table row in the same change).
- **`session_id` FK posture**: `ON DELETE CASCADE` on grants/actions —
  AC1.5.2 pins removal of per-session grant rows, and the shipped
  0087 migration convention is declared FK cascades (no raw per-table
  deletes in the session path). The durable consent evidence is the
  `audit_events` row, which is not session-scoped and survives the
  cascade — the hook's audited `cancelled` transitions land BEFORE the
  cascade removes the rows, so "what was
  cancelled because the session went away" is provable from audit
  alone. Connect grants (`session_id NULL`) are untouched.
- **Denormalized `user_id` on Grant/Action** mirrors the session
  tables' pattern: owner scoping without a join through Target, and the
  audit row's `who` is a column, not a lookup.
- **`action_args` on Grant but not on the wire**: the consent card
  renders `action_summary`; the stored args exist solely so
  `grant_respond(allow)` can materialize the Action atomically — a
  round-trip back through the model to re-dispatch would strand the
  consent UX (Q5).
- **Action args redacted at insert, not at claim**: the stored row is
  already the redacted form — claim delivers it verbatim; no second
  redaction pass per dequeue. Execution-vs-intent divergence is
  intended fail-closed behavior: a command containing a secret-shaped
  literal executes in altered form rather than the gateway ever
  shipping raw secrets to the connector.
- **`running` is implicit**: C7's state machine lists it; C3 has no
  per-action progress message, so the gateway flips `claimed→running`
  on the connector's next authenticated touch for that target
  (heartbeat suffices — proof the delivered work is alive and
  executing). A claim response itself marks `claimed`, not `running`.
- **All mutation is state-conditional**: dequeue, respond, revoke,
  and cancel paths update `WHERE state = <expected>` inside the
  per-target lock — the atomicity claims in the rules stand on this
  convention.
- **No sweeper anywhere**: every `expires_at`/`last_seen_at` field is
  evaluated lazily at the read/write that observes it — the
  components.md behaviour pin is explicit and the delete-hook path
  needs no timer either.
