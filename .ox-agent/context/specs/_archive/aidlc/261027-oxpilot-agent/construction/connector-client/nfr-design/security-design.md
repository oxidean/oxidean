# Security Design — connector-client (U7)

Consumes: `construction/connector-client/nfr-requirements/
security-requirements.md` (NFR1.1–1.12), `functional-design/rules.md`
(BR1–BR9), `entities.md` (ConnectorIdentity/Verifier/RunState/
ConnectorError), `nfr-design-questions.md` (Q1–Q7 answered A), shipped
`crates/oxidean-cli/src/{keystore,config,device}.rs` seams.

The client's security surface is two-sided: it CUSTODIES the only
usable verifier material (server holds the hash), and it EXECUTES
agent-dispatched work on the user's own machine. Both sides are
designed below as concrete mechanisms, not policies.

## Custody design (NFR1.1–1.4)

### Backend seam (`custody.rs`)

```text
write_verifier(instance, connector_id, verifier) -> Result<Backend>
  1. keystore::set(format!("pilot/{instance}/{connector_id}"), v)
     → Ok => Backend::Keyring
  2. on Err (headless, no Secret Service):
     load-or-init config_dir()/pilot-secrets.json  (create 0600)
     map["{instance}/{connector_id}"] = verifier
     write tmp + chmod 0600 + fsync + rename        (strictly stronger
     than the config.rs::save non-atomic write; the crate's tmp+rename
     precedent is self_update.rs's binary swap)
     → Ok => Backend::File
  3. both fail → Err(CustodyUnavailable)  — BR1.2's abort: pairing
     exits non-zero WITHOUT writing the target row (never a
     row-without-verifier zombie minted at pair time)

create-path ordering: write_verifier FIRST, then the
pilot-targets.json row upsert. On upsert failure after a successful
custody write, attempt delete_verifier before aborting pair — no row
means no discovery index, so a written-but-unindexed verifier is an
undiscoverable orphan otherwise.

read_verifier(instance, connector_id)
        -> Result<Option<(Backend, SecretString)>>
  keyring get → file map lookup   (read order mirrors write order;
  SecretString/zeroize-on-drop so the value never lingers in a
  String's heap copy). The Backend rides the return so `status`'s
  "custody backend in use" field is the same live probe that yields
  present/missing — no recorded attribute to drift.

delete_verifier(instance, connector_id) -> ()   // best-effort BOTH
  keyring delete + file-map remove — a credential in either backend
  must die; errors are logged, never fatal (the row delete is the
  identity-of-record)
```

`Backend` is NOT recorded on the identity row (ConnectorIdentity's
upstream schema is `{instance, connector_id, target_id, device_label,
granted_caps, requested_caps, paired_at}` — no backend field, and a
recorded label could drift from live custody). `status` reports the
backend from the same live probe that yields `verifier: present|
missing`: it calls read_verifier (which returns `(Backend,
SecretString)`) and reports which backend answered — no secret bytes
read beyond the probe, satisfying NFR6.5's
"custody backend in use" field without a stale-prone attribute.

### Deletion triggers (NFR1.3 — terminal-keyed)

| trigger | verifier | row | exit |
|---------|----------|-----|------|
| `disconnect [--target]` | delete (both backends) | delete | 0 |
| BR8.4 terminals (heartbeat `revoked:true`, `revoked` claim/report, `not_found` claim/heartbeat) | delete | delete | 0 |
| BR8.3 auth death (`unauthorized`, off-pair `forbidden`) | delete | delete | non-zero |
| `disabled` / `rate_limited` / `bad_input` / transport / `internal` | KEEP | KEEP | (n/a — loop continues or exits creds-kept) |

Deletion order is verifier-then-row within a best-effort sweep — a
crash between leaves a recoverable zombie, never a credential orphan
(the row is the discovery index; deleting it first would strand
verifier material with no row pointing at it).

### Zombie handling (NFR1.4)

Row present, verifier absent in BOTH backends → `run` reports
`credentials missing for <target_id> — re-pair or ox pilot
disconnect --target <id>`, exits non-zero, row PRESERVED (the grant
may be live server-side). `disconnect --target` is the named remedy:
it deletes the row + attempts backend deletes (no-ops on a zombie).
`status` renders the row with `verifier: missing` (a state, not a
secret).

Corrupt-vs-absent is pinned: a parse error on `pilot-secrets.json` is
a NAMED error ("pilot-secrets.json is corrupt — <file path>"), not a
zombie — a hand-edited file must never be diagnosed as missing
credentials (same treatment reliability-design gives a corrupt
`pilot-targets.json`).

## Execution boundary design (NFR1.5–1.8)

### Jail resolve chain (`exec.rs::resolve_path`)

```text
launch:  canonical_root = fs::canonicalize(--root)
         canonical_cfg  = fs::canonicalize(config_dir())
         require canonical_root.is_dir()
         require !is_within(canonical_root, canonical_cfg)   // root is/inside cfg
         require !is_within(canonical_cfg,  canonical_root)  // root contains cfg
            — BOTH directions refuse at startup per Q6-A: --root as or
            inside the config dir, and --root containing it
            (BR6.3's hard exclusion is absolute, never per-op luck)

per-op:  p = lexical_normalize(canonical_root.join(input))     // fold . / ..
         reject if p escapes canonical_root lexically          // .. past root
         c = fs::canonicalize(p-or-deepest-existing-ancestor)  // symlink realpath
         reject if c escapes canonical_root                    // link → /etc
         reject if is_within(c, canonical_config_dir)          // BR6.3 hard excl
         leaf = symlink_metadata(p)                            // non-regular refusal
         reject if !leaf.file_type().is_file() && !write-create // fifo/dev/sock
```

The two-step (lexical then canonical) is the established pattern:
lexical normalization alone loses to symlinks; canonicalize alone
can't express the intent for a not-yet-existing write target — so the
write path canonicalizes the deepest existing ancestor.

`exec_shell` is cwd-jailed ONLY (documented): `cwd=canonical_root`, the
shell is ambient — v1 exec is the user's own machine under their own
consent; the fs_* jail is the deterministic surface.

### Env filtering (NFR1.8)

```text
child_env = parent_env minus:
  - the verifier value (never an env var at all — it lives in
    SecretString, no name exists to leak)
  - OXIDEAN_*TOKEN*, OXIDEAN_*KEY*, OXIDEAN_*SECRET*, OXIDEAN_*PASS*
  - *PAIRING*                    (bare glob — pairing codes are ours)
The OXIDEAN_ prefix scope is the pin: a child's own AWS_SECRET_*/keys
survive (the connector must not over-strip the user's environment).
```

## Output scrub design (NFR1.9)

```text
scrub(output, secrets: &[SecretValue]) -> String
  for v in secrets where v.len() >= 8:
      output = output.replace(v, "[redacted]")
```

The enumerated set, assembled once at launch + on custody events:
- the verifier itself
- every env var value whose NAME matches `*TOKEN*`/`*KEY*`/`*SECRET*`/
  `*PASS*` in the CONNECTOR's env (unprefixed — output scrub protects
  values, broader than the env strip's OXIDEAN-scoped name filter)
- `config.json` `instances.*.token` values (parsed once)
- `pilot-secrets.json` values

≥8-char floor: a value like "token" would shred ordinary output;
shorter values can't be verifier-class anyway (160-bit).

This catches the `cat config.json` residual — the file's OWN values
are scrubbed wherever they appear, so the exec-jail's disclosed gap
can't carry stored Oxidean secrets into the forge transcript (FR5.4's
client half). The server-side redactor is the second pass (FR5.5).

## Protocol honesty design (NFR1.10–1.12)

`client.rs` types the flat error envelope as `C3Error`:

```rust
enum C3Error {
  Unauthorized, Forbidden, Revoked, Disabled, NotFound, BadInput,
  Internal, RateLimited{retry_after:u64}, PairingExpired,
  PairingUsed, Transport(reqwest::Error), Unknown(String)
}
// decode: status+body → {"error":"<code>"} parsed FIRST; an
// unparseable body maps Transport (never a guessed code).
// RateLimited{retry_after} reads the Retry-After RESPONSE HEADER on
// the 429 (its body is {"error":"rate_limited"} alone) — distinct
// from the claim-200 `retry_after` body field and the 204
// `Retry-After` header.
```

The loop's classifier maps enum → disposition per BR2.3 verbatim —
the enum makes an unhandled code a compile error, not a runtime
surprise. `/pair` is the sole unauthenticated call; all others send
`Authorization: Bearer <verifier>` and NOTHING else (never
resolve_token/PAT/cookie — NFR1.10). Claimed kinds outside
`min(requested, granted)` refuse before exec (NFR1.12).

## Threat model → mechanism map

| threat | mechanism |
|--------|-----------|
| Leaked `pilot-targets.json` | metadata only — verifier never in-file (custody split) |
| Stolen verifier | authenticates until owner revokes; revocation reaches client via heartbeat/error → local delete (documented asymmetry: disconnect is local-forget) |
| `cat config.json` via exec | BR7.2 enumerated scrub — the VALUES die in output regardless of path |
| FIFO/device hang under fs_* | non-regular refusal at resolve — uncancellable ops are impossible, not unlikely |
| `..`/symlink escape | lexical→canonical chain + absolute config_dir exclusion |
| Child inherits connector secrets | OXIDEAN-scoped env strip + no-verifier-in-env invariant |
| Dead-cred crash loop | exit-0 on BR8.4 terminals; backoff-not-exit on `disabled`/transport |
