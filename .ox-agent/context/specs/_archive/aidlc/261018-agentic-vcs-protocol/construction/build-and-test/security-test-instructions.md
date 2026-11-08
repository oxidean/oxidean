# Security Test Instructions — agentic-vcs-protocol

NFR9 plus the per-unit `security-requirements.md` registers define the
security surface. Every check below is a pass/fail test or mechanical gate —
no new tooling is introduced.

## Credential surface (NFR9 / FR14 / SR1)

- `/api/vcs/*` object routes accept PAT (Basic/Bearer) + OAuth access tokens
  only — session cookies never authorize them.
- PAT lookup reuses the sha256 path + `FailedAuthLimiter`; a valid PAT without
  repo permission is denied (repo ACL, not token validity).
- `vcs.*` `proc_classes` read/write/admin split; unlisted names fail closed
  `SessionOnly`; read-class PAT denied on writes (AC8.6.2).

```bash
cargo nextest run -p oxidean-api -- mcp vcs rpc
cargo nextest run -p oxidean-vcs --all-features -- api
```

## Input validation (SR4.1 / SR6.7)

- Path grammar: 1..=512, POSIX `/`, no `\0`/control bytes/`//`/`.`/`..`.
- `old_path` iff rename; pinned mode set `{33188, 33261, 40960, 16384}`;
  `author` ≤255 `Name <email>`; `message` ≤65_535; `parents` ≤64.
- `limit` ≤500 and `changes[]` ≤10_000 clamped/rejected at the handler layer.
- Cursor payloads opaque base64, size-capped (`cursor_payload_max_bytes`
  32_768) and revalidated field-by-field — forged tips can never inject argv.

```bash
cargo nextest run -p oxidean-vcs --all-features
```

## Data-integrity bounds (SR2.5 / SR3.x / SR4.8 / SR5 / SR6)

- `vcs.conflict` `data.conflicts` caps at 500 + `truncated:true` flag —
  error bodies can never be unbounded.
- Concurrent-PUT quota overshoot bounded by `max_object_bytes` × in-flight;
  `QuotaExceeded(Repo)` on fresh global entries eagerly unlinks.
- All walk caps named + injectable → `FrontierTooLarge`/`TooLarge`, never
  silent truncation or wrapped `Internal`.
- Merge two-pass shape: zero DB rows + zero object rows/links on any
  deterministic abort (quota-burn vector closed).
- Import/export subprocess stdout bounded incrementally; `ident` sidecars
  bound the raw-byte channel; `is_skipped()` ⇒ `DbNotConfigured` at entry.

## Client-side safety (u8)

- Checkout refuses non-empty target dirs without `.oxidean-vcs-checkout`;
  temp-write-then-rename; cache hash-verifies and refetches corruption.
- `vcs.unauthenticated`/`vcs.forbidden` map to exit 3; `vcs.*` codes never
  rewritten on the wire (`ref_name` field-name parity asserted in e2e).

```bash
cargo nextest run -p oxidean-cli
cargo tree -p oxidean-vcs --no-default-features --features client | \
  grep -E "oxidean-db|oxidean-git|sqlx" || echo "boundary clean"
```

## Secrets hygiene

- No credentials, tokens, or fixture secrets in tests or `OXIDEAN_VCS_*`
  examples — `.env.example` carries placeholders only.
