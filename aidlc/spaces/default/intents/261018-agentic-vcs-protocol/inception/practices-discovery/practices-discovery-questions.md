# Practices Discovery — Questions

Mode note: this stage ran under the user's standing instruction — "skip human verification until we have a sound implementation on a PR with green CI and reply to questions that might arise yourself through recommendation based on the document I shared." Brownfield rule applied: questions are only the items the lead draft and the three blind support reviews could not establish from evidence. Each answer records the conductor's recommendation, its rationale, and the evidence it rests on; the user ratifies or overturns any of them at PR review.

Design-level questions the support reviews surfaced (new-crate error type, crate topology vs `rpc.rs` dispatch, decision-ID prefix, dependency bar, `too_many_arguments` policy) are solution-level, not team practices — per the learned project rule they are deferred to requirements-analysis / domain-design and recorded in `evidence.md`.

## Q1. Way of working — is the discovered model affirmed?

Context: evidence is unambiguous (zero merge commits, `(#NNN)` suffixes, PR-only `Main` ruleset with the release App as sole bypass, `type/<slug>` short-lived branches in ~14 worktrees, enforced SemVer commit grammar). The only soft spots are GitHub-side settings invisible in-repo and whether AI-DLC Bolt worktrees keep the engine's `bolt-<id8>_<slug>` naming.

- A. Affirm as discovered — trunk-based `main`, PR-only squash-merge, `type/<slug>` branches (+ optional issue number) for human/agent work; Bolt worktrees keep engine naming since the engine owns them; `relaxed` guard policy stands (matches the intent's standing-autonomy instruction)
- B. Affirm with changes (specify)
- C. Request changes
- X. Other (please specify)

[Answer]: A — affirmed as discovered. Every claim rests on in-repo evidence; the GitHub-side ruleset details are corroborated by workflow comments and release history and don't change the affirmed practice. Bolt naming stays engine-owned. Conductor-decided under standing instruction (2026-10-18); ratify at PR review.

## Q2. Walking skeleton — should the first Construction unit prove a thin end-to-end slice?

Context: the active `classic` scope ships `skeleton: off`, and a buildable stack with e2e/smoke/deploy infrastructure already exists — the ceremony would repeat what we have. The counter-argument: this intent adds a genuinely new subsystem (changeset engine + protocol surfaces), so sequencing the first unit as a thin vertical slice (repo create → changeset → fetch) is still valuable even without the ceremony.

- A. Skeleton off — skip the bootstrap-slice ceremony (scope default); Construction takes the explicit entry choice (continue automatically under standing instruction); delivery-planning should still order the first unit as a thin end-to-end slice as ordinary sequencing, not ceremony
- B. Skeleton on — run the full skeleton ceremony with a human checkpoint on the first verified unit
- C. Request changes
- X. Other (please specify)

[Answer]: A — skeleton off, with the slice-first sequencing note carried to delivery-planning. The ceremony's value (a human checkpoint proving pieces connect) is redundant with standing autonomy + green-CI requirement; the *sequencing* wisdom survives as a normal unit-ordering decision. Conductor-decided under standing instruction (2026-10-18); ratify at PR review.

## Q3. Testing posture — methodology and coverage floor?

Context: three-way conflict the reviews surfaced — org.md's `classic` default says an 80% line-coverage floor; the repo's actual enforced gate is a 0.70 *weighted composite on web only* (unit .25 / integration .40 / e2e-checklist .35); Rust coverage is collect-only. The new VCS crate is Rust — it lands with no coverage gate at all. No red-first mandate exists anywhere; the in-repo `tdd` skill is opt-in. No performance-test infrastructure exists (no benches, no load harness).

- A. Affirm repo reality — methodology `test-after`, ordering implement-then-test-per-change-type with full CI green before merge; stated floor is the enforced 0.70 weighted composite (web); the new crate ships tests per CODE_PRACTICES minimums (unit + integration + protocol smoke) but no new numeric Rust floor is invented for it — Rust coverage stays collect-only, recorded as debt; perf-test absence recorded as debt for NFR work, not a blocker
- B. Adopt the org 80% line floor and extend it to the new crate
- C. TDD (red-first) mandated for the protocol engine
- D. Request changes
- X. Other (please specify)

[Answer]: A — affirm the enforced gate as the floor. An 80% line floor with no measuring infrastructure is unenforceable noise; the honest record is "0.70 weighted on web + Rust collect-only". The new crate still owes real tests per the change-type table — the standard strategy + CI-green requirement carries that. Conductor-decided under standing instruction (2026-10-18); ratify at PR review.

## Q4. Deployment — is the discovered model affirmed, including the security posture?

Context: devsecops verified merge→staging auto-deploy + environment-gated manual promote end-to-end, with one correction (`release.yml` dispatch also cuts stable on `release/*` branches — "only inside promote" was overstated) and one addition (the security/supply-chain posture that makes the mechanics trustworthy). The scanning gap (no SAST/DAST/dependency/secret scanning, no update bot) is real and was previously unrecorded.

- A. Affirm with the correction and additions — staging-on-merge + gated manual promote (ancestor-of-main + latest-CI-green, dry-run/rollback), maintainer stable-cut lane acknowledged; token-scoping/fork-isolation/checksum contract added to Deployment; scanning absence and remaining supply-chain soft spots recorded as accepted status quo, not silently assumed deliberate policy
- B. Affirm mechanics only — drop the security posture text
- C. Request changes
- X. Other (please specify)

[Answer]: A — affirmed with corrections. Recording "no scanning" as an acknowledged posture is strictly better than silence; it doesn't create new gates and can be revisited as its own intent. Conductor-decided under standing instruction (2026-10-18); ratify at PR review.

## Q5. Which candidate constraints become hard rules?

Context: the drafts surfaced candidates that aren't all equal — some are already hard-enforced (devsecops' four deploy rules), some are stated constraints without CI teeth (env-var docs via PR checklist, D-12 transport-auth precedent, FR3.4 release-token rule), and some are genuinely open debt (checksum fail-open on `install.sh`, scanning absence, root containers, digest-pinning granularity).

- A. Promote the enforced + stated ones, record the rest as debt — Mandated gains: frozen-lockfile/`npm ci` installs only; all-or-nothing `sha-<sha>` deploy set; `railway config apply` never on CI; promote SHA must be ancestor-of-main with latest CI green; new `OXIDEAN_*` env vars documented in `.env.example` + `docs/CONFIGURATION.md` in the same PR; new protocol/machine transports authenticate via PAT/pubkey-class credentials, never session cookies (D-12 precedent); release-automation pushes never use `GITHUB_TOKEN` (FR3.4). Forbidden unchanged. Candidates-that-are-debt (install.sh fail-open checksum, scanning absence, tag-vs-digest pinning, root containers/`docker.sock` mount) recorded in evidence, not as rules
- B. Promote only CI-enforced items — stated-without-CI items stay candidates
- C. Promote everything including the debt items as rules now
- D. Request changes
- X. Other (please specify)

[Answer]: A — a hard rule doesn't need a CI gate to be hard; the PR checklist and recorded decision precedents (D-12, FR3.4) are already the team's stated constraints, and this intent's new transport/config surfaces make D-12 and the env-doc rule directly load-bearing. Debt items get honesty, not premature mandates. Conductor-decided under standing instruction (2026-10-18); ratify at PR review.

## Q6. Code style — is the developer-corrected section affirmed?

Context: the developer review found the draft's style section accurate but incomplete — it omitted the real layer model, the three coexisting error idioms (thiserror enums / `Result<T,String>` db façade / `AppError` wire contract), RPC conventions (mixed snake/camel drift; new names should be `domain.snake_case`; the five-place registration checklist), the naming/module taxonomy, and the codebase's two most distinctive idioms (trait+adapter seams, self-checking drift gates). All corrections are evidence-backed.

- A. Affirm the corrected section — developer's suggested wording adopted: real layer model stated, three error idioms recorded by layer (not a single "prefer thiserror" claim), RPC conventions fixed (`vcs.*` names will be snake_case; existing camelCase stay frozen — wire-stable), naming taxonomy + mod.rs façade convention, trait+adapter seam as the shape for the new crate, every new duplicated/generated surface ships its own sync check, new crate registers a `versioning.toml` component/scope
- B. Affirm the original draft wording
- C. Request changes
- X. Other (please specify)

[Answer]: A — the corrected section is what the code actually does; the draft's single-idiom error claim would have generated wrong code in the new crate. Conductor-decided under standing instruction (2026-10-18); ratify at PR review.

## Consolidated Summary

- Way of working: trunk-based, PR-only squash-merge to `main`, `type/<slug>` branches, worktree parallelism, enforced SemVer subjects, relaxed guard policy — affirmed.
- Walking skeleton: off for this intent; first-unit slice ordering is a delivery-planning sequencing note, not a ceremony.
- Testing posture: `test-after`; implement-then-test ordering; enforced floor = 0.70 weighted composite (web); Rust collect-only recorded debt; no new numeric floor for the VCS crate — tests per change-type minimums + green CI.
- Deployment: merge→staging auto-deploy + gated manual promote, maintainer stable lane acknowledged, security posture + supply-chain contract recorded, scanning gap acknowledged.
- Code style: layer model, three error idioms, RPC naming/registration conventions, naming taxonomy, trait+adapter + drift-gate idioms — developer wording adopted.
- Rules: hard constraints promoted (including env-doc, transport-auth, release-token, and the four deploy rules); scanning/checksum/root-container items recorded as debt, not rules.

All answers are conductor decisions under the standing continuation instruction (2026-10-18) — each carries rationale and is open to reversal at PR review.
