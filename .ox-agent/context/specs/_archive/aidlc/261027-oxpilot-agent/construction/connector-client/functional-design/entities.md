# Entities — connector-client (U7)

## Entity model

```yaml
entities:
  - name: ConnectorIdentity
    kind: persisted client-side (pilot-targets.json row, 0600)
    provenance: new client-side record this unit — the non-secret
      half of pairing custody (Q3-A).
    description: >
      What `POST /pair` returns minus the verifier — the connector's
      durable knowledge of one paired target on one instance. One row
      per pairing; the file is a map keyed by `target_id` so multiple
      targets per instance coexist.
    attributes:
      - { name: instance, type: string, required: true, constraints: ["resolved instance URL at pair time — the row pins which forge it belongs to (Q11-A)"] }
      - { name: connector_id, type: string, required: true, constraints: ["from pair response; equals target_id in v1 (C3 delta — server emits one value for both names)"] }
      - { name: target_id, type: string, required: true, constraints: ["map key; the claim/report/heartbeat request field"] }
      - { name: device_label, type: string, required: true, constraints: ["the label sent at pair; shown by `ox pilot status` and the run banner"] }
      - { name: granted_caps, type: "[]string", required: true, constraints: ["closed subset of {exec_shell, fs_read, fs_write} — set := the requested_caps the server accepted: pair 200 carries NO caps field (RedeemedPairing is {connector_id, target_id, verifier}), a 200 means the request was a subset of the minted bound (forbidden rejects supersets) so accepted == requested (R-03); it is also the value sent as claim.capabilities — an empty set silently starves dispatch"] }
      - { name: requested_caps, type: "[]string", required: true, constraints: ["what the user asked for at pair time — kept so status can show requested-vs-granted; always non-empty (BR1.1)"] }
      - { name: paired_at, type: "rfc3339", required: true }
    constraints:
      - The verifier is never an attribute of this file — it lives in
        the custody backends (keystore via composite instance key,
        `pilot-secrets.json` fallback — BR2.4). A leaked
        pilot-targets.json must not authorize anything by itself.
      - File perms 0600; the whole file is machine-local state —
        never uploaded, never printed verbatim beyond `status`.
      - Rows delete only via `ox pilot disconnect`, the BR8.4
        terminals (heartbeat `revoked:true`, `revoked` on
        claim/report, `not_found` on claim/heartbeat), or the BR8.3
        auth-death exit (Q10-A) — never edited by hand.

  - name: Verifier
    kind: persisted client-side secret (keystore entry)
    provenance: pair response field `verifier` — shown once by the
      server, stored immediately, never re-displayed (AC4.1.3 client
      half; FR5.4).
    description: >
      The PAT-class bearer token the connector authenticates
      claim/report/heartbeat with. Server stores only its hash; the
      client stores the raw token via the BR2.4 custody seam —
      keystore under the composite instance key
      `pilot/<instance>/<connector_id>` first, `pilot-secrets.json`
      (0600) as the defined headless fallback.
    attributes:
      - { name: keystore_key, type: string, required: true, constraints: ["`pilot/<instance>/<connector_id>` composite instance key — collision-free across instances and targets"] }
      - { name: secrets_map_key, type: string, required: true, constraints: ["`<instance>/<connector_id>` key inside pilot-secrets.json when the file backend is used"] }
      - { name: value, type: secret-string, required: true, constraints: ["raw bearer material — keychain first, 0600 file fallback; never plaintext beside metadata (Q3-B rejected)"] }
    constraints:
      - Never logged, never in `status` output, never serialized into
        report output, never copied into a child process env (Q5-A).
      - Auth-death (`unauthorized`-coded, BR8.3) and the
        revoked/not_found terminals (BR8.4) both delete it before
        exit — a dead credential does not linger (Q10-A).

  - name: ActionClaim
    kind: wire (claim response element)
    provenance: C3 `/claim` 200 body `actions[]`.
    description: >
      One unit of dispatched work the gateway queued for this target.
    attributes:
      - { name: action_id, type: string, required: true, constraints: ["the report key — idempotent by it (C3)"] }
      - { name: kind, type: string, required: true, allowed_values: ["exec_shell", "fs_read", "fs_write"] }
      - { name: args, type: "string (carrying JSON)", required: true, constraints: ["STRING-carrying-JSON — shipped ClaimAction.args: String is the stored redacted column verbatim (oxpilot.rs:683): 'args':'{\"command\":\"ls\"}'. The client DOUBLE-parses: envelope first, then the inner string into the per-kind shape — exec_shell: {command}; fs_read: {path}; fs_write: {path, content_b64} — owned by U5/U6, client validates only what it needs to execute (R-04)"] }
      - { name: grant_id, type: string, required: true, constraints: ["the consent that authorized it — carried into report output for correlation, not executed against"] }

  - name: ActionReport
    kind: wire (report request)
    provenance: C3 `/report` request body.
    description: >
      The single idempotent result the client posts per action —
      exactly once per action_id (retries reuse the same body; the
      server dedupes by action_id).
    attributes:
      - { name: action_id, type: string, required: true }
      - { name: status, type: string, required: true, allowed_values: ["done", "failed"], constraints: ["`cancelled` maps to failed with reason in output in v1 — see BR5.3 note"] }
      - { name: exit_code, type: "int|null", required: false, constraints: ["present when a process actually exited"] }
      - { name: output, type: string, required: true, constraints: ["exec: ≤ 64 KiB UTF-8 tail + truncation marker; fs_*: base64 payload ≤ FS_IO_CAP-derived ≈171 KiB or a failure reason — never partial content; universal bound: final string < server OUTPUT_MAX (256 KiB); value-scrubbed with BR7.2's full set (verifier + env-pattern values + config.json tokens + pilot-secrets.json) before send"] }

  - name: ClaimResponse
    kind: wire (claim response)
    provenance: C3 `/claim` 200 body / 204.
    description: >
      The long-poll answer: work, cancels, and pacing in one payload.
    attributes:
      - { name: actions, type: "[]ActionClaim", required: true }
      - { name: cancel, type: "[]action_id", required: true, constraints: ["honored before starting AND during execution (C3 MUST — BR5)"] }
      - { name: retry_after, type: "seconds", required: false, constraints: ["server pacing hint — honored exactly (BR8.1)"] }
      - { name: "_204", type: marker, required: false, constraints: ["timeout — re-poll; hint rides the Retry-After header (C3)"] }

  - name: HeartbeatResponse
    kind: wire (heartbeat response)
    provenance: C3 `/heartbeat` 200 body.
    description: >
      Liveness + revocation/cancel propagation on the 15s cadence.
    attributes:
      - { name: revoked, type: bool, required: true, constraints: ["true → terminal: delete creds, exit 0 (BR8.4)"] }
      - { name: cancel, type: "[]action_id", required: true }

  - name: ConnectorError
    kind: wire (error envelope on every connector route)
    provenance: shipped `err()` in `routes.rs:52-54` — FLAT
      `{"error":"<code>"}`, NOT the `/api/rpc`
      `{error:{code,message}}` shape (R-07).
    description: >
      The one error envelope every /pair|claim|report|heartbeat
      non-2xx answer carries. Codes: `unauthorized`, `forbidden`,
      `revoked`, `disabled`, `not_found`, `bad_input`, `internal`,
      `rate_limited` (+ `Retry-After` header), `pairing_expired`,
      `pairing_used` — BR2.3's taxonomy keys on the code.
    attributes:
      - { name: error, type: string, required: true, constraints: ["the machine code — the ONLY field; there is no nested object or message"] }

  - name: RunState
    kind: in-memory (per `ox pilot run` process)
    provenance: this unit.
    description: >
      The loop's live bookkeeping — nothing durable, rebuilt each
      launch.
    attributes:
      - { name: running, type: "map<action_id, child-handle>", required: true, constraints: ["the cancel-kill registry (BR5.2)"] }
      - { name: settled, type: "set<action_id>", required: true, constraints: ["reported/cancelled dedupe — the server re-announces cancel ids until their rows close; settled stops re-reporting AND pins that an unknown cancel id still gets its pre-start report (BR5.1, R-06)"] }
      - { name: backoff_ms, type: int, required: true, constraints: ["1s → ×2 → 60s cap, ±20% jitter, reset on success (BR8.2)"] }
      - { name: root, type: path, required: true, constraints: ["the --root jail root, canonicalized at launch (BR6.1)"] }
```

## Observations

- Custody split (Q3-A + BR2.4) keeps the ONE thing that can
  authenticate (Verifier) out of the ONE thing that can leak safely
  (pilot-targets.json). Losing the metadata file loses labels and
  caps — never access. The file backend (`pilot-secrets.json`) is
  the defined headless path — a defined fallback, not an invented
  one (R-02).
- `granted_caps` is recorded at pair and never re-fetched — the
  server can only narrow via revocation, so a stored superset can't
  over-grant; if the server ever returns a kind outside it the
  client refuses (BR3.2).
- `connector_id` vs `target_id`: C3 carries both names; the server
  emits one value for both in v1 (U6 fd). The client stores both
  fields but they hold the same value; `target_id` is the wire key.
