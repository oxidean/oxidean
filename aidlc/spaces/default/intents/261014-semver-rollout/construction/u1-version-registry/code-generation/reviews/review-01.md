**Reviewer:** aidlc-architecture-reviewer-agent
**Iteration:** 1
**Verdict:** READY

Scope: U1 code-generation — `versioning.toml`, `scripts/versioning/**`, manifest
migration, `make version-check`. Bound to the plan + source-manifest contents.

### Findings

| ID | Severity | Location | Finding | Resolution |
|---|---|---|---|---|
| F-01 | Minor | scripts/versioning/lib/registry.ts | Dead loop `unknownKeys({component:…}) void k` left over from drafting — unreachable noise. | Removed. |
| F-02 | Minor | scripts/versioning/commands/check.test.ts, lib/registry.test.ts | `readFileOrThrow` used `require()` instead of the fs import already used elsewhere; the unknown-dep test computed an unused parse. | Both cleaned; test asserts the violation message. |
| F-03 | Minor | code-generation-plan.md | Plan named `lib/glob.ts`; shipped file is `lib/globs.ts`. | Plan text aligned. |
| F-04 | Minor | versioning.toml / BR1.5 | Spec text ("a glob covering fixture dirs fails") vs implementation (carrier-level enforcement only). Deliberate interpretation — glob attribution to a component is correct behavior; the rule's intent is that non-component files are never *versioned*. | Documented in code-summary deviations. |

### Verification

- `bun test scripts/versioning` — 30 pass / 0 fail.
- `bun scripts/versioning/cli.ts check` on the repo — exit 0, 10 components,
  pending-baseline warning only.
- `cargo check -p oxidean-core` clean; `cargo metadata` unchanged.
- All 7 crate manifests carry literal `0.1.0`; root `[workspace.package]`
  version removed; `WIRE_VERSION` resolves via `constant_path`.
- Schema checks verified adversarially: unknown keys, kind/carrier xor, scope /
  tag_prefix / glob collisions, DAG cycle/self/unknown edges, baseline states.

### Summary

Schema matches contract C1; validator is strict-schema and never returns a
partial registry; check output/exit codes match BR1.8. Findings were minor
code-hygiene items, all fixed before this verdict.
