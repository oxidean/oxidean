# Practices Discovery Questions — `/oxpilot` (re-run)

Baseline: `memory/team.md` affirmed sections. Fresh evidence found eight drift
points; the ones below need a decision (the rest are cosmetic staleness the
integration turn fixes in prose).

## Sources

- [desc] Intent: `/oxpilot` agentic chat/builder/debugger with opt-in computer use.
- [team] `aidlc/spaces/default/memory/team.md` — affirmed baseline.
- [evidence] `inception/practices-discovery/evidence.md` + three contributions.

## Q1. Dependabot now exists — how should the dependency-freshness practice read?

`.github/dependabot.yml` (#168) runs weekly grouped updates for cargo/bun/GHA/
docker-compose/docker. The affirmed baseline says "no update bot — freshness is
manual, recorded debt". Devsecops notes it is a freshness control, not a
vulnerability control — there is still no CVE-alerting/triage path
(`cargo audit`/`bun audit`/GHAS absent or unverifiable in-repo).

- A. Update the practice: dependabot handles routine freshness; the affirmed debt is reworded to name the real gap — no CVE-alerting/triage path — without adding a new gate.
- B. Keep the baseline wording unchanged (dependabot is too new to practice-ify).
- C. Update and add a non-gating `cargo audit`/`bun audit` CI job as a new mandated practice.
- D. Not assessed.
- X. Other (please specify)

[Answer]: A — update the practice: dependabot = routine freshness; affirmed debt reworded to the real gap (no CVE-alerting/triage path); no new gate. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q2. Walking-skeleton stance for this intent?

Baseline affirmed skeleton "off" under scope `classic` with a maturity
argument. This intent runs scope `feature`, which ships `skeleton: on` — a thin
end-to-end slice (a minimal version that runs the whole way through, built
first to prove the pieces connect before the real features go in) would mean a
vertical chat round-trip through the real seams before breadth.

- A. Adopt the feature-scope skeleton for this intent: a thin `/oxpilot` chat round-trip (page → session → provider → streamed reply) lands first and proves the seams; the affirmed "off" stays the recorded default for `classic`-style work.
- B. Keep skeleton off for this intent too — go straight to unit-ordered depth.
- C. Not assessed.
- X. Other (please specify)

[Answer]: A — adopt the feature-scope skeleton: thin /oxpilot chat round-trip first to prove seams; affirmed 'off' stays the classic default. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q3. Coverage posture for new `oxpilot.*` Rust code?

Quality verified the floor composition (0.25/0.40/0.35 weights, 0.70 floor,
7-item frozen e2e checklist). New crate `oxidean-api-oxpilot` inherits
collect-only coverage (its tests count toward inputs; no new floor).

- A. Collect-only for `oxidean-api-oxpilot` — no new floor; meet the floor via the new tests it adds; record that decision consciously here.
- B. Raise the floor for the new credential-holding surface.
- C. Not assessed.
- X. Other (please specify)

[Answer]: A — collect-only for oxidean-api-oxpilot, no new floor; decision recorded consciously here. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q4. `oxpilot.*` procedure classification for PATs?

`proc_classes.rs` fails closed — unclassified procedures are `SessionOnly`,
meaning PAT/`ox`/MCP bearer invocation is refused. `oxpilot.*` could stay
SessionOnly (UI/session-only surface) or get a deliberate `PatProc`
classification.

- A. Keep `oxpilot.*` `SessionOnly` in v1 — the surface is session-driven by design; pin the closed-set classification with a `VCS_PROCEDURES`-style const + test; revisit if `ox`/MCP access is ever wanted.
- B. Classify `oxpilot.*` procedures for PAT use now.
- C. Not assessed.
- X. Other (please specify)

[Answer]: A — oxpilot.* stays SessionOnly in v1; pin closed-set classification via VCS_PROCEDURES-style const + test. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q5. Should "new RPC family lands in a domain crate" become a mandated rule?

The developer verified the post-#162 shape is machine-enforced
(`check-api-domain-graph.mjs`). Should `discovered-rules.md` mandate it?

- A. Yes — mandate structurally: "ALWAYS land a new RPC family in its own `oxidean-api-*` domain crate (PROCEDURES + dispatch arm + workspace member + versioning component + changelog); NEVER add new procedure families to `oxidean-api-core` residual."
- B. Keep it as practice prose, not a hard rule.
- C. Not assessed.
- X. Other (please specify)

[Answer]: A — yes, mandate it: new RPC family → own oxidean-api-* domain crate; NEVER add families to oxidean-api-core residual. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q6. ci-base ARG-pin rule wording?

Devsecops supports the `[new]` Mandated candidate but wants two-sided wording:
the `/docker/ci-base` pins are hand-bumped AND dependabot's directory coverage
structurally excludes them.

- A. Adopt two-sided wording: "ALWAYS hand-bump `docker/ci-base` ARG pins in the same change that needs them — dependabot does not cover that directory by design."
- B. Adopt the lead's original one-sided wording.
- C. Drop the rule.
- X. Other (please specify)

[Answer]: A — two-sided wording adopted. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q7. Connector credential custody — mandate the split or leave to design?

Devsecops proposes the existing custody split as practice now: verifier-class
secrets hash-at-rest (`token_hash`), retrievable secrets AES-256-GCM via
`crypto.rs`'s env-key chain under a named `OXIDEAN_OXPILOT_*` var; and that
agent-reachable surfaces NEVER return stored secret material.

- A. Mandate both: "ALWAYS store retrievable secrets AES-256-GCM under the env-key chain; verifier-class as hashes" and "NEVER return stored secret material from agent-reachable surfaces (RPC, MCP, SSE, logs)."
- B. Leave custody entirely to nfr/domain design — no mandated rules now.
- C. Mandate only the no-return rule.
- X. Other (please specify)

[Answer]: A — mandate both custody rules and the no-secret-return rule. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Consolidated Summary Confirmation

- Q1: dependabot = freshness; debt reworded to "no CVE-alerting/triage path"; no gate.
- Q2: feature-scope skeleton ON for this intent (thin chat round-trip first).
- Q3: collect-only coverage for `oxidean-api-oxpilot`, no new floor.
- Q4: `oxpilot.*` SessionOnly v1 + pinned closed-set classification test.
- Q5: domain-crate rule mandated (never in core residual).
- Q6: ci-base ARG-pin rule, two-sided wording.
- Q7: custody split + never-return-secrets both mandated.

Does this all look correct before the lead integrates?

- Looks correct
- Request changes

[Answer]: Looks correct
