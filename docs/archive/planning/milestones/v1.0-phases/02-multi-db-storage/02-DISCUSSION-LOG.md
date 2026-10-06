# Phase 2: Multi-DB Storage - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-09-09
**Phase:** 02-multi-db-storage
**Areas discussed:** Dialect selection, Migration approach, Proof entity, Operator paths, SQLite specifics

---

## A — Dialect selection

### How dialect is chosen

| Option | Description | Selected |
|--------|-------------|----------|
| URL scheme only | Infer from DATABASE_URL | |
| Explicit dialect required | OXIDEAN_DB_DIALECT mandatory | |
| URL + optional dialect must agree | Prefer scheme; optional env must match | ✓ |
| Something else | — | |

**User's choice:** Prefer URL scheme; optional `OXIDEAN_DB_DIALECT` must agree when set

### Mismatch behavior

| Option | Description | Selected |
|--------|-------------|----------|
| Fail fast | Clear startup error | ✓ |
| Prefer URL + warn | — | |
| Prefer explicit dialect | Rewrite/ignore URL conflict | |
| Something else | — | |

**User's choice:** Fail fast at startup

### Post-boot dialect change

| Option | Description | Selected |
|--------|-------------|----------|
| No — instance identity | — | |
| Empty DB only + docs | Phase 2 supports switch when empty | ✓ |
| Defer entirely | Isolation-only proof | |
| Something else | — | |

**User's choice:** Empty-DB switch with documented path

### Docs location

| Option | Description | Selected |
|--------|-------------|----------|
| .env.example + README | — | |
| + Make help sample URLs | — | ✓ |
| + docs/database.md | Also requested | ✓ |
| Something else | — | |

**User's choice:** `.env.example` + README + Make help sample URLs **and** `docs/database.md`

---

## B — Migration approach

### SQL organization

| Option | Description | Selected |
|--------|-------------|----------|
| Shared portable SQL only | LCD | |
| Per-dialect folders | Parallel versions | |
| Shared logical → adapted | Thin adapt per dialect | ✓ |
| Something else | — | |

**User's choice:** Shared logical migrations adapted per dialect

### Runner / tooling

| Option | Description | Selected |
|--------|-------------|----------|
| Always auto-migrate on boot | — | |
| Explicit migrate only | — | |
| Auto local/Compose; explicit prod-like | Recommended + accepted | ✓ |
| Something else | — | |

**Notes:** User asked for recommendation on Q2–Q4; accepted package: auto-migrate default with `OXIDEAN_AUTO_MIGRATE` escape; trivial schema; `_sqlx_migrations`

### Portability strictness

| Option | Description | Selected |
|--------|-------------|----------|
| Trivial schema | Thin adapter enough | ✓ |
| Dialect SQL from day one | — | |
| Shared now; forks later | — | |
| Something else | — | |

**User's choice:** Accept recommendation — trivial Phase 2 schema + thin adapter

### History table

| Option | Description | Selected |
|--------|-------------|----------|
| sqlx `_sqlx_migrations` | Recommended + accepted | ✓ |
| Custom oxidean table | — | |
| Don't care | — | |

**User's choice:** sqlx defaults

---

## C — Proof entity

### What to prove against

| Option | Description | Selected |
|--------|-------------|----------|
| Tiny internal smoke_kv | — | |
| Product-ish instances/settings | Later phases may keep | ✓ |
| /status history persistence | — | |
| Something else | — | |

**User's choice:** Early product-ish row

### Phase 2 exposure

| Option | Description | Selected |
|--------|-------------|----------|
| RPC only | — | |
| RPC + UI | — | |
| Tests/smoke only | Initially selected | ✓* |
| Something else | — | |

**Notes:** User also required future system-admin diagnostics menu. Area 4 later **superseded** “no RPC”: add minimal `system.db_probe`-style RPC for shared smoke/admin path; still **no Phase 2 UI**.

### Data shape

| Option | Description | Selected |
|--------|-------------|----------|
| Minimal id/key/value/created_at | — | |
| Include dialect/version stamp | ✓ | ✓ |
| Keep under ~5 columns | — | |

**User's choice:** Include dialect/version stamp

### Longevity

| Option | Description | Selected |
|--------|-------------|----------|
| Supported diagnostic | + admin dashboard later | ✓ |
| CI-only hidden | — | |
| Disposable | — | |

**User's choice:** Keep as supported diagnostic; system-admin can run DB R/W test from system diagnostics in system dashboard (UI deferred)

---

## D — Operator paths

### Local switch UX

| Option | Description | Selected |
|--------|-------------|----------|
| Docs + existing Compose only | — | |
| Explicit Make targets | up / up-mysql / up-sqlite | ✓ |
| DB_DIALECT switcher script | — | |

**User's choice:** Explicit Make targets

### CI coverage

| Option | Description | Selected |
|--------|-------------|----------|
| All three in matrix | Phase 2 | ✓ |
| PG + SQLite; MySQL softer | — | |
| Follow PLAT-09 minimum only | — | |

**User's choice:** All three dialects in CI matrix

### Empty-DB switch recipe

| Option | Description | Selected |
|--------|-------------|----------|
| Docs only | — | |
| Make helper only | — | |
| Make + docs | ✓ | ✓ |

**User's choice:** Both Make helper and `docs/database.md`

### Smoke location

| Option | Description | Selected |
|--------|-------------|----------|
| Extend compose-smoke | — | |
| Separate db-dialect-smoke | — | |
| Cargo tests only | — | |
| Combination for admin reuse | Recommended lock | ✓ |

**User's choice:** Whatever makes sense for system-admin trigger later → shared Rust diagnostic + minimal RPC + cargo tests + Compose/Make smoke

---

## E — SQLite specifics

### Default path

| Option | Description | Selected |
|--------|-------------|----------|
| ./data/oxidean.db | — | |
| ./var/oxidean.db | Runtime-state style | ✓ |
| Configurable only | — | |

**User's choice:** `./var/oxidean.db`

### Parent dir creation

| Option | Description | Selected |
|--------|-------------|----------|
| API creates dirs | On SQLite startup | ✓ |
| Operator/Make only | — | |
| Make + API safety net | — | |

**User's choice:** API creates missing parent dirs

### Compose topology

| Option | Description | Selected |
|--------|-------------|----------|
| Bind-mount host file | No DB container | ✓ |
| Named Docker volume | — | |
| SQLite mainly dev/CI | — | |

**User's choice:** api+web(+Traefik), bind-mount host path — aligned to `./var` (not `./data`) to match default path

### Pragmas / concurrency

| Option | Description | Selected |
|--------|-------------|----------|
| WAL + foreign_keys documented | No deep tuning | ✓ |
| Minimal open-and-go | — | |
| Stricter single-writer docs | — | |
| Both 1 and 3 | — | |

**User's choice:** Sensible defaults only (`WAL`, `foreign_keys=ON`)

---

## Deferred (from discussion)

- System dashboard / system diagnostics UI
- Auth-gating diagnostic RPC to system-admin
- Non-empty cross-dialect data migration
- Deep SQLite tuning

---

*Phase: 02-multi-db-storage*
*Discussion log: 2026-09-09*
