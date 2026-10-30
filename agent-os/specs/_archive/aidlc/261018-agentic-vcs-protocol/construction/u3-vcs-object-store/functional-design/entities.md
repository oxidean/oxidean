# Entities — u3-vcs-object-store

Object-store unit: `oxidean-vcs` (server feature) owns the blob filesystem store and the *behavior* of `ObjectEntry`/`ObjectLink` (DDL owned by U2 — this file describes the entities behaviorally, not a second schema). Plus the GC lockfile, the tmp dir, and the env-var config struct.

```yaml
entities:
  - name: BlobFile
    description: "immutable blob bytes on disk — the CAS substrate (ADR-004: FS, not BLOB)"
    identifier: "sha256 hash → {OXIDEAN_VCS_OBJECTS_DIR}/{hash[0:2]}/{hash[2:4]}/{hash}"
    attributes:
      - {name: path, type: "shard path (bare-hex hash, validate before join)", required: true}
      - {name: bytes, type: "raw content ≤ max_object_bytes; empty allowed (sha256-of-empty, size=0)", required: true}
      - {name: mtime, type: "fs timestamp — grace checks for row-less sweep", required: true}
    constraints:
      - "written ONLY via tmp/{pid}-{counter}-{nanos}.tmp → rename (atomic same-fs); never served from tmp"
      - "immutable after rename — GC delete is the only mutation"
      - "*.tmp files are never live content — always swept past grace"
    relationships: "1:1 ObjectEntry (row implies file; file may briefly precede row inside a PUT)"

  - name: TmpFile
    description: "in-flight PUT staging under {objects_dir}/tmp/ — unique per attempt (pid+counter+nanos), never shared across concurrent same-hash PUTs"
    identifier: "tmp filename"
    constraints: ["swept when older than gc_grace_secs", "removed on every abort path"]

  - name: ObjectEntry (behavioral owner)
    description: "global CAS row keyed by hash — shared across repos (FR1.5); DDL in U2 (0053)"
    identifier: hash
    constraints:
      - "insert_if_absent → bool: true=Stored / false=Dedup (facade's rows_affected idiom)"
      - "list_orphaned_before(grace_ts) = rows with zero live ObjectLinks AND created_at < grace"
      - "deleted only by GC pass 2 via vcs_delete_object_if_unlinked — conditional DELETE WHERE NOT EXISTS(link) (sanctioned U3 facade addition)"
    relationships: "1:N ObjectLink; 1:1 BlobFile"

  - name: ObjectLink (behavioral owner)
    description: "per-repo attribution/quota reference to a global object"
    identifier: "repo_id + hash"
    constraints:
      - "created ONLY inside the changeset create_tx (object_links param of vcs_create_changeset_tx — C6a) or by put()'s link-ensure step"
      - "membership probe vcs_get_object_link(repo_id, hash) -> bool (sanctioned U3 facade addition) gates the repo-quota charge — a re-reference adds no bytes"
      - "repo delete cascades links (U2 FK) → object becomes GC-eligible after grace"
    relationships: "N:1 ObjectEntry; N:1 repositories"

  - name: GcLockFile
    description: "single-node GC mutual exclusion + stale recovery — {objects_dir}/.gc-lock (excluded from the row-less sweep by predicate shape)"
    identifier: ".gc-lock filename"
    attributes:
      - {name: acquired_at, type: "mtime — the age-stale criterion", required: true}
    constraints:
      - "create_new acquire; present-and-younger-than-grace ⇒ gc_once skips the pass"
      - "older than gc_grace_secs ⇒ treated dead, replaced in-place (age-based staleness — no pid liveness; LFS precedent has none)"
      - "clear_stale_lock() at boot removes any lockfile unconditionally"
    relationships: "none (fs-only)"

  - name: ObjectStoreConfig
    description: "env-var contract owned by this unit — names/defaults frozen at contract design"
    identifier: "five OXIDEAN_VCS_* vars"
    attributes:
      - {name: objects_dir, type: "path (OXIDEAN_VCS_OBJECTS_DIR)", default: "./var/vcs-objects"}
      - {name: gc_interval_secs, type: "u64 (OXIDEAN_VCS_GC_INTERVAL_SECS)", default: "3600"}
      - {name: gc_grace_secs, type: "u64 (OXIDEAN_VCS_GC_GRACE_SECS)", default: "86400"}
      - {name: max_object_bytes, type: "u64 (OXIDEAN_VCS_MAX_OBJECT_BYTES)", default: "268435456"}
      - {name: repo_quota_bytes, type: "u64 (OXIDEAN_VCS_REPO_QUOTA_BYTES)", default: "10737418240"}
    constraints:
      - "from_env parses + validates (numeric ranges, dir non-empty); malformed → config error, never silent default"
      - "i64 row values (size/storage_bytes) vs u64 config: explicit checked cast at compare sites"
      - "host wiring (.env.example/docs/compose/railway/factory-reset) is U7's; this unit ships names+defaults+loader only"
    relationships: "consumed by ObjectStore::new / host wiring"

  - name: PutOutcome
    description: "put() discriminated result — Stored when the global entry is new, Dedup when it existed"
    identifier: "{hash, kind}"
    constraints: ["hash returned bare; wire adds sha256: prefix (U7 boundary)"]

  - name: ObjectStoreError
    description: "closed error enum — the unit boundary's only failure surface"
    identifier: variant
    attributes:
      - {name: QuotaExceeded, type: "{scope: Object|Repo} → vcs.quota_exceeded", required: true}
      - {name: NotFound, type: "→ vcs.not_found (incl. io::ErrorKind::NotFound carve-out)", required: true}
      - {name: InvalidHash, type: "→ vcs.invalid_input", required: true}
      - {name: DbNotConfigured, type: "→ db.not_configured (eager is_skipped() check — never error-text matching)", required: true}
      - {name: Db, type: "String facade error → host internal", required: true}
      - {name: Io, type: "io failure → host internal", required: true}
```

**Boundary notes:** `BlobFile`/`TmpFile`/`GcLockFile` are the only fs entities — everything else durable lives in U2's tables. The `sha256:` prefix, `Cache-Control` headers, auth, and flag gating never enter this unit (U7 surface). C6a's link-insert is a façade param consumed by U4, not a U3 callable — see WF3.
