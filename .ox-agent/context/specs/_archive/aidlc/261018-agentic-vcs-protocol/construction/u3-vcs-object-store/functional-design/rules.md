# Rules — u3-vcs-object-store

```yaml
business_rules:
  - id: BR3.1
    name: server-feature-gate
    rule: "All ObjectStore code compiles only under oxidean-vcs `server` feature; client feature gets zero of it. Server deps: oxidean-db, sha2, tokio(fs/io), futures-util, bytes, tracing, chrono; shared dep: oxidean-core. Host crates depend on oxidean-vcs, never reverse (FR16.1, ADR-011). put() takes the StreamExt<Item = Result<Bytes,E>> + Unpin shape (LFS precedent); get() returns a tokio::fs::File."
    enforcement: "cargo check -p oxidean-vcs (default features) links no oxidean-db; --all-features builds both"
    source: ADR-011 + ADR-002 + contribution 7

  - id: BR3.2
    name: put-ordering
    rule: "PUT writes tmp/{pid}-{counter}-{nanos}.tmp → hash+size verify → rename to shard path → insert_if_absent row(s). Nothing is served from tmp; a failed verify removes tmp and returns a typed error — never a truncated or mis-hashed object (AC3.1.3/AC3.2.2). Empty body produces the legal sha256-of-empty size=0 object."
    enforcement: "test: injected mid-stream abort leaves no final file, no row, tmp gone"
    source: C6a + LFS put_stream precedent + contribution 6/12

  - id: BR3.3
    name: dedup-semantics
    rule: "Identical content PUT → same hash, tmp dropped, Dedup — and the per-repo ObjectLink is still ensured (cross-repo dedup must not starve attribution). Re-upload is always a safe no-op (AC3.1.2)."
    enforcement: "test: second PUT same bytes returns same hash + Dedup; second repo's PUT creates a link without a second file"
    source: AC3.1.2 + FR1.5 + ADR-010

  - id: BR3.4
    name: quota-two-site
    rule: "Two vcs.quota_exceeded sites only: (a) streamed byte cap vs max_object_bytes mid-write; (b) repo storage_bytes+size vs repo_quota_bytes before link insert — gated by vcs_get_object_link membership probe so an existing (repo,hash) link skips the charge (re-reference adds no bytes). i64 rows vs u64 config: checked cast. Both typed; no other quota path. Concurrent-PUT overshoot is a documented single-node limitation."
    enforcement: "tests: oversize body aborts typed; quota-full repo denies new-link PUT typed, still accepts re-reference"
    source: AC3.1.3 + AC3.5.3 + contribution 3 + Q3

  - id: BR3.5
    name: gc-grace-and-links
    rule: "GC reclaims only objects with ZERO live links AND entry older than gc_grace_secs. Links are created inside the changeset create_tx (object_links param — C6a): U4 MUST pass every manifest blob_hash, else a referenced-but-unlinked object appears orphaned while file_changes RESTRICT blocks its delete permanently. The PUT→commit window is covered by grace+tx ordering, never a refcount (AC3.5.1/3.5.2). Referenced-ness computed at GC time via the link anti-join (ADR-010) — self-healing under cascade deletes."
    enforcement: "test: object linked inside tx survives a GC tick between PUT and commit; unlinked old object is reclaimed"
    source: AC3.5.x + ADR-010 + C6a + contribution 2

  - id: BR3.6
    name: gc-three-pass
    rule: "gc_once = orphan-row pass (list_orphaned_before → vcs_delete_object_if_unlinked → delete file, row first) → shard-shaped row-less file sweep (names matching {2hex}/{2hex}/{64hex} only, mtime past grace; dotfiles/.gc-lock/strays excluded; canonicalize-under-root + no-follow-symlinks) → stale *.tmp sweep. Per-object failures logged + skipped — one bad row never aborts the pass (GC-vs-re-reference FK edge self-heals). Lock = create_new lockfile, age-stale past gc_grace_secs; clear_stale_lock() runs at host boot."
    enforcement: "test: crashed-PUT fixtures (tmp, row-less file, dead lock) reclaimed; live lock skips; lockfile survives the sweep; per-object delete failure doesn't stop the pass"
    source: unit spec + LFS precedent + contribution 1/4/5/9

  - id: BR3.7
    name: env-var-contract
    rule: "The five OXIDEAN_VCS_* names/defaults are frozen (objects_dir ./var/vcs-objects; gc_interval 3600; gc_grace 86400; max_object 268435456; repo_quota 10737418240). from_env validates — malformed → config error, not silent default. Host wiring/docs/factory-reset are U7. ObjectStore::new/put create_dir_all the dir; gc_once early-returns on absent dir."
    enforcement: "test: each default resolves; malformed var errors; absent-dir gc returns zero stats"
    source: contract env table + contribution 12

  - id: BR3.8
    name: hash-format-guard
    rule: "Any hash crossing an fs boundary validates bare 64-hex lowercase before path join (traversal guard — validate_oid precedent). Wire sha256: prefix is stripped at the U7 boundary, never stored; unit-internal params are bare &str."
    enforcement: "test: '../', mixed-case, non-hex, wrong-length all rejected before fs touch"
    source: T-14-04 precedent + U1 wire pin + contribution 11

  - id: BR3.9
    name: typed-error-surface
    rule: "ObjectStoreError is a closed enum: QuotaExceeded{Object|Repo}→vcs.quota_exceeded; NotFound→vcs.not_found INCLUDING io::ErrorKind::NotFound (row-exists-file-gone / mid-lookup delete); InvalidHash→vcs.invalid_input; DbNotConfigured→db.not_configured via EAGER Database::is_skipped() check at put/get/gc_once entry (facade errors are untyped Strings — no error-text matching); Db/Io/Internal→host internal. No io error is ever a domain denial; no String error crosses the unit boundary."
    enforcement: "test: each variant maps to the pinned wire code; skipped DB → db.not_configured via is_skipped"
    source: frozen taxonomy + Q6 + contribution 8/11

  - id: BR3.10
    name: no-auth-no-routes
    rule: "No auth check, no route code, no header logic inside this unit — VcsApi gates first (U7). ObjectStore trusts an already-authorized caller; it never sees the wire."
    enforcement: "code review: module has no http/auth imports"
    source: unit boundary note

  - id: BR3.11
    name: sanctioned-facade-delta
    rule: "U3 adds exactly two methods inside crates/oxidean-db/src/vcs.rs — vcs_delete_object_if_unlinked(hash) (conditional DELETE … WHERE hash AND NOT EXISTS(link)) and vcs_get_object_link(repo_id, hash) -> bool — both following existing facade idioms (Database-by-value, require_pool, per-dialect rows_affected). No other oxidean-db change; dialect SQL never leaves that crate."
    enforcement: "source-manifest claims vcs.rs under this unit; dialect-idiom review in code-gen"
    source: contribution 1+3 + Q7
```

## Error mapping

| Store result | Surfaced as |
|---|---|
| QuotaExceeded{Object\|Repo} | `vcs.quota_exceeded` |
| NotFound (incl. io::ErrorKind::NotFound) | `vcs.not_found` |
| InvalidHash | `vcs.invalid_input` |
| DbNotConfigured (eager is_skipped) | `db.not_configured` |
| Db/Io/Internal | host internal error (logged, not a domain code) |
