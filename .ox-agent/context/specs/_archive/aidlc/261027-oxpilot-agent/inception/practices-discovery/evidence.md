# Practices Discovery — Evidence Log

> Re-run context: `aidlc/spaces/default/memory/team.md` already contains an
> affirmed baseline (five sections). This log records what the lead
> inspected, what each support reviewer inspected, what each inference rests
> on, and what the interview resolved — **integration complete**; see
> "Integration record" at the bottom for decisions and residual uncertainty.

## Sources

- Affirmed baseline: `aidlc/spaces/default/memory/team.md` (five populated
  sections; `## Forbidden`/`## Mandated` empty — the affirmed hard rules
  live in `project.md`, affirmed 2026-10-18).
- Prior affirmation: `aidlc/spaces/default/memory/project.md`
  `## Mandated` (14 rules) + `## Forbidden` (4 rules) + `## Corrections`.
- CodeKB (refreshed): `aidlc/spaces/default/codekb/oxidean-app/`
  `architecture.md`, `code-structure.md`, `code-quality-assessment.md`,
  `technology-stack.md`, `dependencies.md`, `business-overview.md`.
- Live repo: `git log --oneline -30`, `git log --format='%s' -50`,
  `git log --merges` (empty), `git worktree list`, `.githooks/commit-msg`,
  `.github/workflows/{ci,images,production-deploy,release,release-beta,
  ox-cli,ci-base,pr-env-sweep}.yml`, `.github/dependabot.yml`,
  `.github/pull_request_template.md`, `Makefile` (69 recipe targets, 19
  `.PHONY` groups), `versioning.toml` (39 `[[component]]` ids incl.
  virtual `wire`), `scripts/coverage-e2e-checklist.sh` (7 ITEMS),
  `docker-compose*.yml`, `deploy/cloud/{Caddyfile,Dockerfile,README.md}`,
  `.cursor/rules/{oxidean-core,octane-ui,rpc-codegen,rust-crates,
  no-ai-attribution,use-in-repo-skills,clean-user-facing-text}.mdc`,
  `AGENTS.md`, `CONTRIBUTING.md`, `docs/{CODE_PRACTICES,TESTING,
  VERSIONING,DEPLOYMENT}.md`, `.gitignore`.
- Intent state: `aidlc/spaces/default/intents/261027-oxpilot-agent/
  aidlc-state.md` (scope `feature`, guard `relaxed`, depth/test-strategy
  Standard); `.claude/scopes/{aidlc-feature,aidlc-classic}.md`
  (`skeleton:` field comparison).

## What each inference rests on

### Way of Working — holds
- Trunk-based + PR-only + release-App bypass: `Main` ruleset documented in
  AGENTS.md § Required status checks, `docs/TESTING.md` L314-335, and the
  `production-deploy.yml` header comment; `git log` shows linear history
  with `(#NNN)` squash suffixes; `git log --merges` empty.
- `type/<slug>` branches + worktrees: `git worktree list` shows sibling
  checkouts on `feat/*` and `cursor/*` branches; baseline's bolt naming
  unchanged per persona runbook.
- Commit grammar: `.githooks/commit-msg` execs
  `scripts/versioning/cli.ts commit-check`; `versioning.toml [grammar]`
  carries allowed types + reserved `release` scope; CI `commit-lint` job
  checks PR range + title (TESTING.md job table).
- **New since baseline**: the required-status-checks upkeep rule —
  `.cursor/rules/oxidean-core.mdc` L20, AGENTS.md L95-97, CONTRIBUTING.md
  L59, TESTING.md § Required status checks, and commit `1fffb67a`
  `docs(ci): document required-status-checks upkeep` (#180).

### Walking Skeleton — contested by scope change
- Prior affirmation: "off" under scope `classic` (`skeleton: off` in
  `.claude/scopes/aidlc-classic.md` L6).
- This intent: `aidlc-state.md` records **scope `feature`**, and
  `.claude/scopes/aidlc-feature.md` L6 ships **`skeleton: on`** — the
  scope default flipped.
- The maturity argument is independent of scope and still holds (23-job
  gated pipeline, e2e harnesses, smokes, promote-gated deploy — all in
  evidence). Whether that justifies overriding the scope default again is
  a human judgment → interview question.

### Testing Posture — holds with enumerated deltas
- `test-after`: no red-first gate exists; `tdd` skill is opt-in
  (`.agents/skills/tdd/`); `docs/CODE_PRACTICES.md` § Testing expectations
  is change-type minimums, matching baseline ordering.
- Weighted gate: `coverage-weighted` CI job + `scripts/coverage-weighted.sh`
  floor `0.70`; e2e third is `scripts/coverage-e2e-checklist.sh` —
  **exactly 7 ITEMS** (verified), matching baseline.
- Rust coverage collect-only: TESTING.md residual note (llvm-cov runs,
  gate consumes web numbers only).
- Dialect gap: `db-dialects` job now selects `dialect_probe` **or**
  `dialect_vcs` per dialect (TESTING.md L309) — baseline named only
  `dialect_probe`; parity files still `DATABASE_URL`-gated skip-pass.
- Web-browser sharding: `ci.yml` `web-browser` job runs 5 internal
  vitest shards + `VITEST_BROWSER_CONNECTION_CLOSED`-keyed retry
  (commit `cc30e0fe`, #191).
- `vcs-server` job runs the `oxidean-vcs` `server`-feature suite leg;
  `make test` now also runs `cargo nextest run -p oxidean-vcs
  --features server` (Makefile L241-251).
- Zero-skip invariant: `rg` for `#[ignore]`/`it.skip`/`test.skip`/
  `describe.skip` across `crates/`, `apps/web/src`, `apps/web/e2e`
  returns 0 hits.
- Manifests: route-coverage manifest ~83 entries (codekb); browser
  coverage manifest gates high-risk UI; both change-aware vs
  `origin/main`/`UI_COVERAGE_BASE` (TESTING.md L198-230).
- CI job count: `ci.yml` currently defines **23 job stanzas** (baseline/
  earlier codekb cited 21 — additions incl. `e2e-stack`, `actions-e2e`,
  `docker-ci-images`, `vcs-server` depending on counting).

### Deployment — holds with two real drifts
- On-merge image set + staging deploy + ephemeral PR envs:
  `images.yml` header + jobs; **new** `pr-env-sweep.yml` (hourly cron
  `23 * * * *` + workflow_dispatch) is the authoritative teardown
  backstop — merged-PR `closed` runs are cancelled before jobs start
  (workflow comment + commit `b03f3f73`, #167).
- Manual promote gate: `production-deploy.yml` — `git merge-base
  --is-ancestor` check (L156) + "latest CI run must be completed
  successfully" check (L161-180); `dry_run`, `cut_release`, `rollback`
  inputs present; `Oxidean / production` environment binding present.
- Per-component SemVer: `versioning.toml` now lists **39 component ids
  (38 real + virtual `wire`)** — the baseline's "10 components" predates
  the #162 domain-crate split (commit `refactor(api): split oxidean-api
  into api-core + per-domain crates (#162)`).
- **Dependabot (biggest drift)**: `.github/dependabot.yml` exists —
  weekly Monday batches for `cargo`, `bun` (grouped, `oxlint-tsgolint`
  ignored), `github-actions`, `docker-compose`, `docker`; deliberately
  excludes `docker/ci-base` ARG pins. Landed in `e7ad77f5` (#168);
  recent history is dense with merged `chore(deps):` PRs. The baseline's
  "no update bot — freshness is manual, recorded debt" is **contradicted
  by evidence**; the "no SAST/DAST/vulnerability/secret scanning" claim
  still holds (version bumps ≠ scanning).
- Token scoping, railway-apply CI refusal, release-App pushes, frozen
  lockfiles, digest-pinned `oxidean-ci:*`: all still evidenced
  (`ci.yml` permissions, `docs/DEPLOYMENT.md` D-CLOUD-07, `AGENTS.md`
  § CI images, TESTING.md § CI base images).
- Runtime hardening: `docker-compose.yml` runner mounts
  `/var/run/docker.sock` read-write (L193) behind opt-in
  `docker://` labels; Traefik mounts it `:ro` (L23) — baseline's
  accepted-risk note unchanged.

### Code Style — holds with post-#162 shape update
- The #162 refactor (`refactor(api): split oxidean-api into api-core +
  per-domain crates`) reshaped the layer description: composition shell
  + `oxidean-api-core` + 25 domain crates with machine-enforced
  dependency direction (`scripts/check-api-domain-graph.mjs`) and
  disjoint `PROCEDURES`. Baseline's "one `match` dispatch → domain
  modules" is stale; the *practices* (layer order, no `services/`, seam
  discipline) hold.
- Web gate expansion: `make web-lint` now runs five check scripts
  (Makefile L411-417) + post-build `web-render-check` (L422-424); the
  baseline names oxlint + the DOM-race heuristic only.
- Dead-code/docs gates gained `oxidean-vcs --all-features` /
  `--features server` legs (Makefile L436-446).
- Bun pin `1.4.4` (`package.json` `packageManager`; TESTING.md); baseline
  says 1.4.0.
- RPC registration checklist extended by the domain-crate registry +
  `proc_classes` fail-closed classification (`oxidean-core/src/
  proc_classes.rs`; codekb architecture § decisions).
- Suffix taxonomy, naming, comment conventions, env-doc rule, no-secrets,
  no-attribution, `/api/` prefix preference: all re-verified in
  CODE_PRACTICES.md / AGENTS.md / cursor rules / project.md corrections.

## Interview — open questions (all resolved; decisions recorded below)

1. **Skeleton**: scope `feature` defaults `skeleton: on`; the affirmed
   stance was "off" under `classic`. Re-affirm off on the maturity
   argument, or accept the ceremony this run? (Draft keeps "off (drift?)".)
2. **Dependabot**: affirm the bot as the sanctioned update path (weekly
   grouped batches, ci-base ARG pins hand-bumped only)? The baseline's
   "manual freshness, recorded debt" line is stale either way — the debt
   record itself needs rewording to "no vuln/secret scanning".
3. **Registry counts**: affirm updating "10 components" → "38 + wire"
   (and the ~14 → ~6 worktree count, Bun 1.4.0 → 1.4.4, "~90" → ~70
   Makefile targets, 21 → 23 CI jobs) as housekeeping, or does the team
   want counts dropped from the affirmed text to reduce churn?
4. **`dialect_vcs` in `db-dialects`** and the `vcs-server`/`make test`
   vcs legs: fold into the affirmed dialect-gap description, or keep the
   "probe-only" wording?
5. **Domain-crate registry as the affirmed "Layers" shape**: the #162
   split is structural evidence, not an affirmed convention statement —
   confirm the draft's rewritten Layers bullet (composition shell +
   `oxidean-api-core` + domain crates + graph check) reads as *team
   practice*, not just current fact.
6. **`#[allow(clippy::too_many_arguments)]` on wide inserts** — carried
   "accepted, forward policy deferred" from the baseline; still deferred?
7. **Root-container / docker.sock accepted risk** — unchanged in
   evidence; re-affirm as recorded debt for this intent (connectors will
   handle user credentials — does that change the risk appetite)?
8. **Guard policy**: `relaxed` is affirmed and matches this intent's
   self-drive instruction — confirm no change wanted.

## Notes for support reviewers

- **quality**: verify the coverage-floor composition and the
  change-aware manifest semantics still match the affirmed wording;
  sanity-check the `dialect_vcs` + `web-browser` shard additions;
  confirm the `/oxpilot` manifest-row requirement in this draft.
- **developer**: focus on the rewritten Layers bullet (post-#162 shape),
  the extended RPC registration checklist (domain crate + `proc_classes`),
  and whether the domain-crate plugin registry should be stated as
  *mandated* practice for `oxidean-api-oxpilot`.
- **devsecops**: assess the dependabot drift (update bot without vuln
  scanning), the ci-base ARG-pin hand-bump rule as a new Mandated
  candidate, the root/docker.sock accepted-risk note, and whether the
  computer-use connector plan changes any affirmed credential-custody
  stance.

## Integration record — support reviews

Three blind support passes ran against the lead draft; their files are
under `contributions/`. What each inspected and what it changed:

- **aidlc-quality-agent** — inspected `scripts/coverage-weighted.sh`,
  `coverage-e2e-checklist.sh`, `coverage-weighted.contract.sh`,
  `ci.yml` (`coverage-weighted`, `db-dialects`, `web-browser`,
  `vcs-server` jobs), `.config/nextest.toml`, both coverage manifests +
  check scripts, `check-route-sync.ts`, `proc_classes.rs`, `Makefile`.
  Verdicts integrated: floor composition affirmed verbatim; **`db-dialects`
  corrected** — the `-E` filter is three binaries (`dialect_probe` +
  `dialect_vcs` + `dialect_actions`), plus a `workflow_conformance` leg on
  real Postgres AND MySQL via `OXIDEAN_TEST_DATABASE_URL`; the "~30
  `dialect_*` files `DATABASE_URL`-gated skip-pass" claim was materially
  wrong — reality is **23 files, only 4 gated** (`dialect_probe`,
  `dialect_auth`, `dialect_vcs`, `dialect_actions`), the rest execute for
  real on sqlite in `api-rust` shards; **`dialect_auth` flagged as dead
  coverage** (gated, never in the filter — added in `89e4a7c2` #118,
  never run cross-dialect); `retries = 1` added to the flake-tolerance
  story; counts corrected (82 manifest rows, 130 API test files,
  `web-lint` = 6 commands, 23 CI jobs); `/oxpilot` registration set
  widened beyond the manifest row (route-coverage row + `check-route-sync`
  triangle incl. `RESERVED_TOP_SEGMENTS` + conditional browser-coverage
  row + routeTree sync); SSE streaming assigned test layers (HTTP-level
  `*.stack.test.ts` for the stream contract + happy-dom for
  append/fallback + optional stack-browser); connector tests assigned to
  the crate's `tests/` with wiremock outbound fixtures.
- **aidlc-developer-agent** — inspected the post-#162 layer tree
  (`crates/oxidean-api/{app,rpc}.rs`, `oxidean-api-core/src/rpc.rs`,
  `oxidean-api-vcs`, `oxidean-api-issues`, `check-api-domain-graph.mjs`,
  `Cargo.toml`s, `versioning.toml`), `proc_classes.rs`,
  `stream_job_log` in `oxidean-api-rest`, `settings/ai.tsrx`, rule files,
  affirmed memory. Verdicts integrated: Layers bullet affirmed with the
  precision fix — "may depend on `oxidean-api-core` **only among
  `oxidean-api*` crates**"; new-crate registration checklist extended
  (workspace `members` + shell dep/aggregate/arm + `versioning.toml`
  component + `oxidean-api` `depends_on` edge + crate `CHANGELOG.md` +
  per-crate `procedures_list_matches_dispatch`); domain-crate registry
  supported as a Mandated rule (taken to Q5); `SessionOnly` framed as a
  decision to pin with a `VCS_PROCEDURES`-style const + closed-set test
  (taken to Q4); SSE server precedent located (`stream_job_log`,
  `api-rest` — shares download auth helpers, not a `RouteDef` surface);
  oxpilot seam shapes named (`email_slot` / `SharedVcs` precedents,
  `RpcCtx.rpc_dispatch` for agent-internal tool calls).
- **aidlc-devsecops-agent** — inspected `dependabot.yml` (all 80 lines),
  full `ci.yml`, `ci-base.yml`, `pr-env-sweep.yml`,
  `docker/ci-base/Dockerfile`, `docker-compose.yml`, repo-wide scanner
  grep (zero hits for audit/deny/gitleaks/trivy/semgrep/codeql/secret-scan
  patterns; zero `pull_request_target`), `crypto.rs`, `pat/*`, `audit.rs`,
  `proc_classes.rs`, custody-schema migrations (`0008_pats.sql`,
  `0021_actions.sql`, `0083_…`), `.env.example`, `docs/CONFIGURATION.md`,
  feasibility § Computer-use transport. Verdicts integrated: dependabot's
  ci-base exclusion is structural (directories list + `ARG`-indirected
  `FROM`s), not just a comment — drove the two-sided Q6 wording; the
  custody split (verifier → `token_hash`; retrievable → `crypto.rs` GCM +
  env-key chain) and the never-return-secret-material rule proposed as
  Mandated (taken to Q7, adopted); PAT hygiene (`policy.rs` TTL clamps,
  `rate_limit.rs` D-26 limiter — per-process) and the audit-trail
  convention (`audit.rs` fire-and-forget, `detail` never carries secrets)
  folded into Boundaries & comments; root/`docker.sock` accepted risk
  re-affirmed unchanged and deliberately not merged with the connector
  question.

## Integration record — interview decisions (Q1–Q7)

All seven answered "A" + summary confirmation "Looks correct"
(`practices-discovery-questions.md`; conductor-decided under standing
instruction 2026-10-27, ratify at PR review):

- **Q1** — dependabot affirmed as the sanctioned routine-freshness path;
  the affirmed debt is reworded to name the real gap — **no
  CVE-alerting/triage path** — without adding a new gate. (Devsecops's
  advisory `cargo audit`/`bun audit` job was offered as option C and
  declined.)
- **Q2** — feature-scope skeleton **on** for this intent: a thin
  `/oxpilot` chat round-trip (page → session → provider → streamed reply)
  lands first to prove the seams; affirmed "off" stays the recorded
  default for `classic`-style work.
- **Q3** — collect-only Rust coverage for `oxidean-api-oxpilot`, no new
  floor; decision recorded consciously (quality's ask).
- **Q4** — `oxpilot.*` stays `SessionOnly` in v1 (fail-closed
  `proc_classes` default is the desired posture); pinned with a
  `VCS_PROCEDURES`-style `OXPILOT_PROCEDURES` const in `oxidean-core` +
  closed-set classification test; revisit if `ox`/MCP access is wanted.
- **Q5** — domain-crate rule **mandated**: new RPC family → own
  `oxidean-api-*` domain crate; NEVER add families to `oxidean-api-core`
  residual (both halves written into `discovered-rules.md`).
- **Q6** — ci-base ARG-pin rule adopted with devsecops's two-sided
  wording (hand-bump + dependabot structurally excludes the directory).
- **Q7** — custody split AND never-return-secret-material both
  **mandated** (Mandated: GCM-under-env-key-chain + hash-at-rest;
  Forbidden: never return stored secret material from agent-reachable
  surfaces — RPC, MCP, SSE, logs).

## Integration record — unresolved uncertainty (carried, not blocking)

- **`.cursor/rules/rust-crates.mdc` + `docs/CODE_PRACTICES.md` boundary
  table** still teach the pre-#162 monolith ("`oxidean-api`:
  HTTP/RPC/auth/email"; no `api-core`/domain-crate/graph-check mention).
  Housekeeping during this intent's implementation — Construction-stage
  agents read these, so fix alongside the first `oxidean-api-oxpilot`
  change or as a chore.
- **`docs/TESTING.md` L309 job table** is stale: documents probe+vcs only,
  missing `dialect_actions` (joined `c3c12c9a`, #158) and the
  `workflow_conformance` leg. Same housekeeping lane as above.
- **`dialect_auth` dead coverage** — gated on `DATABASE_URL` but absent
  from the `db-dialects` `-E` filter; has never executed against
  Postgres/MySQL in CI. Fix (join the filter) or document the exclusion
  during implementation; flagged in team-practices.
- **GitHub-side controls unverifiable in-repo**: Dependabot *security
  updates/alerts* and secret scanning are repo/org settings, not
  file-resident config — the "no CVE-alerting path" claim is certain for
  CI/hooks, unverified for GitHub settings. Devsecops's interview Q1
  stands open for a repo-settings check.
- **Advisory dependency-audit job** — devsecops recommended a non-gating
  `cargo audit` + `bun audit --audit-level=high` job; interview Q1 chose
  "no new gate". Recorded here as a deliberate refusal, revisit when a
  triage path is wanted.
- **`#[allow(clippy::too_many_arguments)]` forward policy** — still
  deferred to design (unchanged from baseline).
- **PAT rate limiter is per-process** — matters if connector auth rides
  the D-26 `FailedAuthLimiter`; multi-replica deferred. Design-stage
  note, not a blocker.
