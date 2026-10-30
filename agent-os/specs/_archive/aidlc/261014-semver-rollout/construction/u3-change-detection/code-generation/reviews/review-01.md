**Reviewer:** aidlc-architecture-reviewer-agent
**Iteration:** 1
**Verdict:** READY

Review of the change-detection / bump-planner unit on current bytes:
`lib/semver.ts`, `lib/git.ts`, `lib/plan.ts`, `commands/detect.ts`,
`commands/plan.ts`, `commands/bump.ts`, their tests, plus the `usage` exports
the shared dispatcher now reads.

Inspected against the functional spec (WF-1/WF-2/WF-3, contract C3):

1. `collectRange` resolves the lower bound per component: explicit `--since`,
   else the latest well-formed `<tag_prefix>-v<semver>` tag, else the registry
   baseline, else the repo root while baseline is `pending`. Malformed tags
   (`*-vjunk`) are filtered with `isSemver` before selection — verified the
   filter on the `prefixedTags` output; a junk tag can no longer poison bounds.
2. `attributeCommit` unions scope-mapped attribution with glob attribution;
   reserved scopes attribute to nothing and never fall back; unmapped/absent
   scopes fall back to paths; multi-glob matches fan out and warn.
3. `computePlan` computes the dependent closure transitively, records
   `propagatedFrom`, applies 0.x-aware bump rules (breaking/feat → minor,
   fix/perf/revert → patch, housekeeping → none; propagated ≥ patch), reads
   `fromVersion` from carriers only, and emits sorted C3 entries.
4. `bump` writes only registered carrier fields (Cargo `[package] version`,
   `package.json` `"version"`, `WIRE_VERSION`), regenerates Cargo.lock/bun.lock
   via native tooling, reports `ManifestEdit[]`, exits nonzero on partial
   failure. `--dry-run` verified on the real repo: no writes.

### Findings

| ID | Severity | Location | Finding | Resolution |
|---|---|---|---|---|
| - | - | - | `writePackageVersion` single-line JSON handling and the semver tag filter were fixed before this request bound; current bytes are clean. | - |

### Verification

- `bun test scripts/versioning` — 82 pass / 0 fail (plan, semver, detect, bump
  integration suite on a temp git + Cargo fixture).
- Live run on the repo: `detect`/`plan` attribute real history correctly;
  cross-cutting commits fan out via globs; `wire` correctly absent.
- `bump --dry-run` — reports the plan, writes nothing.

### Summary

Detection, propagation, bump math, and the applier all match the spec on the
bound bytes; tests cover the contract's edge cases. READY.
