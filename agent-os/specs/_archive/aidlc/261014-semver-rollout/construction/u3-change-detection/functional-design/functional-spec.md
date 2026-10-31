# U3 Change Detection & Bump Engine — Functional Spec

## Purpose

Turns commit history into a release plan. Consumes U1's registry (globs, aliases, edges, baseline) and U2's parser (raw signals); produces the C3 `BumpPlan` U4 consumes for changelogs/tags/releases, and applies version writes (`bump`) with lockfile regen.

## Workflows

### WF-1 `detect` — attribution over a commit range

1. Resolve scan bound (BR3.1): `--since <ref>` → `ref..HEAD`; else `baseline_commit..HEAD`; `pending` → repo root.
2. For each commit: `parse_commit` (U2) → skip exempt (BR3.2).
3. Direct attribution (BR3.2): attributed_ids = resolved_scope_ids ∪ glob_matched_ids — globs always contribute (path evidence is real regardless of declared scope); `source=scope_and_glob` when both hit; multi-entry glob matches attribute to all + warn.
4. Emit `Attribution[]` — the detector's intermediate result, also printed by `detect --dry-run` for debugging.

### WF-2 `plan` — bump computation (C3 output)

1. WF-1 attribution over the range.
2. Propagation (BR3.3): transitive dependent closure; each reached entry records `propagatedFrom` origin ids.
3. Per-entry bump (BR3.4 + BR3.5): direct_bump from attributed commits per the 0.x-aware table; final = max(direct, propagated-patch).
4. toVersion (BR3.6): fromVersion read from manifest/constant (never the registry); stable → bare semver; channel → prerelease suffix with per-version N.
5. Emit `BumpPlan` sorted by component id (BR3.8). `plan --json` is the machine output; `plan` default prints a human table.

### WF-3 `bump` — apply the plan

1. Compute the plan (WF-2) — `bump` never takes a different code path than `plan` (same release-plan code path per FR8.2's CI requirement).
2. For each entry with `bump ≠ none`: write `toVersion` into `manifest_path` (`version` field) or `constant_path` (WIRE_VERSION literal). Nothing outside registry fields is writable (BR3.7).
3. Regenerate lockfiles unconditionally: Cargo.lock via cargo metadata refresh; bun.lock via `bun install` (BR3.7).
4. Report the applied `ManifestEdit[]`; exit 1 on any write/regen failure (no silent partial bump).

## States

- `Attribution`: computed per run; `ambiguous` flag carries the multi-match warning.
- `BumpPlan`: pure function of (range, registry) — `generatedAt` is the only volatile field (BR3.8).
- `ManifestEdit`: applied or failed; a failed write aborts the run — partial application is reported, never claimed clean.

## Interfaces

- Consumes: `load_registry` (U1), `parse_commit` (U2).
- Emits: `plan`/`detect`/`bump` subcommands; `BumpPlan` JSON (C3) consumed by U4 changelog/tag/release and the FR8.2 release-PR workflow.
- `propagatedFrom` feeds U4's `### Dependencies` changelog section (FR4.4).
