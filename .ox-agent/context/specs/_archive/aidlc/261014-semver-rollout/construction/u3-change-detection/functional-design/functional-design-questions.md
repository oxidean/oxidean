# Functional Design — U3 Change Detection & Bump Engine — Questions

Unit: `u3-change-detection` (kind: library). Scope: commit-range attribution, dependency propagation, bump planning (`plan`/`bump`), manifest rewrites + lockfile regen. Contract: C3 (release-plan). FRs: FR3, FR3.1, FR3.2, FR3.3, FR3.5, FR8.3.

## Q1. Multi-entry glob match at detection time — ambiguous attribution?

- A) Fail the run with the ambiguous path + entries named.
- B) Attribute the path to ALL matching entries and warn — over-attribution beats under-attribution for a change-tracking system.

[Answer]: B — a file matched by two entries' globs is genuinely a shared-input change; both components record the hit (with a stderr warning so overlap debt is visible). BR1.7's `check` still fails on statically-decidable overlap (identical/contained literal patterns); runtime multi-match is the backstop for the undecidable remainder.

## Q2. Direct-commit ties with propagated bumps?

- A) `max(direct_bump, patch)` — a propagated hit can never lower a component's own direct signal.
- B) Direct signal always wins outright.

[Answer]: A — per the bump rules: propagated-only → patch; direct+propagated → max(direct, patch); direct-only → direct (including `none`). `propagatedFrom` records which entries' bumps caused propagation for the changelog `### Dependencies` section (FR4.4).

## Q3. Lockfile regeneration after manifest rewrites?

- A) `bump` rewrites manifests then runs `cargo metadata` (Cargo.lock) and `bun install` (bun.lock) unconditionally — lockfiles always consistent post-bump.
- B) Leave lockfiles for the next normal build.

[Answer]: A — unconditional regen inside `bump` (FR3.3). Skipping risks a stale Cargo.lock version field shipping in the release commit.

## Q4. Scan lower bound — what does `pending` baseline mean at runtime?

- A) Repo root (per U1 BR1.6 fallback) — full-history scan while baseline is pending.
- B) Refuse to run.

[Answer]: A — `pending` → scan from repo root (documented fallback); recorded sha → `baseline_commit..HEAD` unless overridden by `--since`. Also supports `--since <ref>` for ad-hoc/range scans independent of baseline.

## Q5. Prerelease channels (FR8.3) — how are channel versions computed?

- A) `plan --channel alpha|beta|nightly` produces `next.<channel>.<N>` where N counts existing tags on that channel for the target version; stable (no flag) is the default lane. `nightly` uses datestamp `-nightly.YYYYMMDDN`.
- B) Only stable + alpha/beta; drop nightly.

[Answer]: A — all three channels per the C3 contract (`-alpha.N`, `-beta.N`, `-nightly.YYYYMMDDN`); channel selection flows from workflow_dispatch inputs via C4's trigger matrix. N never resets within a target version; each new base bump restarts at `.1`.

## Q6. Propagation depth — transitive closure or one hop?

- A) Transitive dependent closure — a core change bumps direct dependents AND their dependents (e.g. oxidean-core change → oxidean-api + oxidean-web + oxidean-cli through the chain).
- B) One hop only.

[Answer]: A — full transitive closure over `depends_on` edges (the user requirement: "detect shared crates hits on their dependencies"). Cycles are impossible (BR1.4 enforces the DAG at check).

---

## Consolidated Summary Confirmation

- Multi-match glob paths attribute to all matching entries + warn (runtime backstop to BR1.7's static check).
- Bump = max(direct, patch-if-propagated); `propagatedFrom` feeds FR4.4 changelog dependency notes.
- `bump` always regenerates Cargo.lock + bun.lock after manifest rewrites.
- Scan bound: `baseline_commit..HEAD`, repo root while `pending`, `--since` override.
- Prerelease: `plan --channel alpha|beta|nightly` → `-alpha.N`/`-beta.N`/`-nightly.YYYYMMDDN`, N per-version.
- Propagation is full transitive closure over registry edges.

Does this all look correct before I generate the functional-design artifacts?

- Looks correct
- Request changes

[Answer]: Looks correct
