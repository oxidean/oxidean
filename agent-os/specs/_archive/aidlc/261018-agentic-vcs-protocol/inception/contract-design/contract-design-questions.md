# Contract Design — Questions

## Q1 — Which boundaries get formal specs?

- **A** — Spec every edge crossing a unit boundary OR an external consumer: the `vcs.*` RPC family (17 procedures, external), the `/api/vcs/…/objects` object routes (external), the MCP descriptor set (external-facing schema), the `VcsPersistence` façade (u2→stores/api), the `oxidean-vcs` engine interfaces consumed by `VcsApi` (u4/u5/u6→u7), the `ProtectionChecker`/`GitBackend` injection ports (u6→host), the WorkingCopy/CliSurface client API (u8), and the `repo_activity` row contract for `vcs.*` kinds (u7→u10/u2).
- **B** — External surfaces only; internal crate seams stay informal (Rust typechecking is the contract).
- **C** — Inter-unit only; the external API is well-trodden territory (the RPC convention is fixed).

[Answer]: A — the external surfaces need frozen schemas for codegen + agent clients, and the internal façades are where the parked open questions live (activity-signature nullability, component interface shapes). Spec both; keep internal specs at signature granularity, not implementation prose.

## Q2 — Frozen resolutions for the parked open questions?

Parked items needing freezes: `vcs.status` schema; `vcs.ref_update` arg names/semantics; merge-conflict payload beyond `{path,kind}`; `vcs.diff` boundedness; `vcs.file_get` inline cap; import bounds/cursor; `insert_repo_activity` nullability; `vcs.changeset_get` as MCP tool; `OXIDEAN_VCS_*` defaults; RPC/route payload caps.

- **A** — Freeze all ten in the contract spec blocks (values listed per contract).
- **B** — Freeze only wire-visible ones; leave storage/activity decisions to functional design.
- **C** — Defer the whole set again — contract design shouldn't freeze values.

[Answer]: A — freeze all ten; each is named in its owning contract's spec block. The activity-signature decision is wire-adjacent (kind strings + row shape are consumed by the web renderer) so it belongs here too.

## Q3 — Contract ownership + versioning policy?

- **A** — Owner = the provider unit listed in the contracts table; breaking changes require the SemVer commit grammar's breaking marker (`!` scope) + `wire_version` minor bump discipline already in the repo; additive changes are always safe because consumers ignore unknown fields (serde `deny_unknown_fields` is NOT used).
- **B** — A shared schema registry repo — overkill for one workspace.
- **C** — No policy; version churn is tolerable in 0.x.

[Answer]: A — matches repo convention: `WIRE_VERSION` is 0.x so minor = breaking boundary; additive-only changes inside a release keep clients unbroken.

## Q4 — Error, timeout, retry behaviour?

- **A** — RPC: `{ok:false, error:{code,message,data?}}` with the pinned `vcs.*` taxonomy; object routes: real HTTP statuses (octet-stream success, JSON error otherwise); idempotent procedures safe to retry (`changeset_create`, ref CAS with same `expect`, object PUT dedup); conflicts/quotas/validations are typed, not retried. Server-side long ops (import) are bounded + resumable via cursor, not timed RPC retries.
- **B** — Uniform retry-with-backoff for all errors — wrong for typed denials.
- **C** — Timeouts only, no idempotency contract — drops AC2.2.

[Answer]: A.

## Summary confirmation

[Answer]: Looks correct
