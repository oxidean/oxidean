# U3 Change Detection & Bump Engine — Business Rules

Source of truth: the fenced YAML block.

```yaml
rules:
  - id: BR3.1
    statement: "The commit range is bounded below by `baseline_commit` (or repo root while it is `pending` per BR1.6), overridable by `--since <ref>`."
    category: policy
    applies_to: detect/plan/bump
    trigger: "every run"
    logic: "IF baseline_commit is a sha THEN scan baseline..HEAD; IF `pending` THEN scan repo root; IF --since given THEN scan <ref>..HEAD. Exempt commits are filtered before attribution."
    violation: "exit 2 on unresolvable refs"
    source: FR3.5
  - id: BR3.2
    statement: "Attribution unions scope and globs: a mapped scope attributes the commit to the resolved entries AND its changed-file globs still attribute independently — a commit touching another component's files is a real change to that component regardless of the declared scope; unmapped/absent scopes attribute via globs only; reserved-scope commits are exempt; a path matching multiple entries' globs attributes to ALL of them with a warning."
    category: validation
    applies_to: Attribution
    trigger: "per commit in range"
    logic: "attributed_ids = resolved_scope_ids ∪ glob_matched_ids; source=scope_and_glob when both contributed. scope_status=reserved → nothing. Multi-glob path → all matches + ambiguous warning (Q1). The commit's signals (type/breaking) apply uniformly to every attributed component."
    violation: "warnings only — never fails (C2 severity)"
    source: FR3
  - id: BR3.3
    statement: "Propagation is the full transitive dependent closure over `depends_on` edges: every attributed component's dependents (direct and indirect) receive a propagated hit."
    category: computation
    applies_to: DependencyEdge
    trigger: "after direct attribution"
    logic: "FOR each entry with ≥1 direct attribution: walk dependents transitively; each reached entry gets a propagated signal naming the origin set. Cycles impossible (BR1.4 DAG check)."
    violation: n/a
    source: FR3, FR3.2
  - id: BR3.4
    statement: "Bump levels: for fromVersion <1.0.0 — breaking or feat → minor; fix/perf/revert → patch; housekeeping types (chore/docs/style/test/build/ci) → none. For >=1.0.0 — breaking → major; feat → minor; fix/perf/revert → patch; housekeeping → none."
    category: computation
    applies_to: BumpPlanEntry.bump
    trigger: "plan computation per entry"
    logic: "direct_bump = max over the entry's attributed commits by the table above, seeded none. Combination with propagated signals is BR3.5's job."
    violation: n/a
    source: FR3.1
  - id: BR3.5
    statement: "Final bump is `max(direct_bump, patch)` when the entry has propagated hits, else `direct_bump` — propagated-only entries always land patch."
    category: computation
    applies_to: BumpPlanEntry.bump
    trigger: "plan computation per entry"
    logic: "IF propagatedFrom non-empty AND direct THEN max(direct,patch); IF propagatedFrom non-empty AND NOT direct THEN patch; ELSE direct."
    violation: n/a
    source: FR3.2
  - id: BR3.6
    statement: "toVersion = fromVersion + bump level; on a prerelease channel the suffix is `-alpha.N`/`-beta.N`/`-nightly.YYYYMMDDN` where N counts prior tags of that component on that channel for the computed base version (starts at 1)."
    category: computation
    applies_to: BumpPlanEntry.toVersion
    trigger: "plan computation"
    logic: "channel=stable → bare semver; else append channel suffix. N is per (component, channel, base-version) — new bases restart at .1."
    violation: n/a
    source: FR8.3, C3
  - id: BR3.7
    statement: "`bump` rewrites ONLY the in-registry version fields (Cargo.toml `version`, package.json `version`, WIRE_VERSION literal) then regenerates lockfiles — `cargo metadata` refresh for Cargo.lock and `bun install` for bun.lock run unconditionally after any manifest write."
    category: constraint
    applies_to: ManifestEdit
    trigger: "`bump` execution"
    logic: "FOR each plan entry with bump ≠ none: write after-version into manifest_path/constant_path; THEN lockfile regen; fixtures/templates/schema markers are never write targets (FR2.4 writer-side discipline)."
    violation: "exit 1 on unparseable manifest or lockfile-regen failure; no partial writes reported as success"
    source: FR3.3, FR2.4
  - id: BR3.8
    statement: "The plan is reproducible: same baseRef + same registry → byte-identical plan modulo `generatedAt`."
    category: constraint
    applies_to: BumpPlan
    trigger: "plan computation"
    logic: "Attribution and propagation are pure functions of (commit range, registry); entries sorted by component id."
    violation: n/a
    source: NFR (determinism), C3
```

## Rules summary

| Rule | Category | In one line |
|---|---|---|
| BR3.1 | policy | scan baseline..HEAD; pending→root; `--since` override |
| BR3.2 | validation | scope→glob attribution; multi-match → all + warn |
| BR3.3 | computation | propagation = transitive dependent closure |
| BR3.4 | computation | 0.x-aware bump table (feat→minor, fix/perf→patch) |
| BR3.5 | computation | final = max(direct, propagated-patch) |
| BR3.6 | computation | channel suffixes `-alpha.N`/`-beta.N`/`-nightly.date` |
| BR3.7 | constraint | rewrites only registry fields; lockfiles regen always |
| BR3.8 | constraint | plan reproducible from (range, registry) |
