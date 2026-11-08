# Memory — User Stories (Inception)

- Assessment: Execute (3 personas worth of surface + consent-heavy model).
- Plan: 4 personas (Dana/Casey/Quinn/Riley), capability-area breakdown mirroring FR groups, feature-sized stories, scope-MoSCoW.
- Mob round: 3 blind contributions (design/developer/quality) — all blocking findings were mechanical gap fixes; integrated into a 22-story set.
- Key integrations: target-manager story US1.7 (FR1.4 was un-AC'd); consent split (US4.2 connect dialog vs AC4.3.2 per-action card); connector client story US4.6 (greenfield surface); negative paths (offline target, denied grant, mid-action revoke, mid-stream failure); LLM-grading ACs rewritten as artifact-linkage; orphaned reqs bound (FR7.1→AC1.3.5, FR7.5→AC5.5.2, NFR1→AC5.3.3, SessionOnly→AC5.5.1); pairing = device-grant-style; SSE resume = snapshot-replace+append (no Last-Event-ID); PR provenance disclosure AC2.2.4.
- traceability.json: all 52 FR/NFR IDs covered (3 Deferred → nfr-requirements/domain-design).
- Advisory review: READY (iter 1 requested twice — artifacts re-baselined after finding fixes). Pass 1: 8 findings all applied (AC5.1.5 server-side disabled denial, AC5.5.4 cross-user denial, US1.8 queue story, skeleton note, AC1.1.5 role-gate pin, AC4.1.4 pairing lifecycle, AC1.5.2 cascade, AC2.2.4 carve-out). Pass 2 verified all applied + 3 new minors (R-09..R-11) applied: US1.8 bound under FR2.2, US5.5 added to NFR2, US4.5 re-homed to FR2.1, AC1.3.5 extended with issue/PR reads.
- Harness/format lesson persisted: review file needs exact `**Iteration:** N` + `### Prior/New findings` tables with the 6 canonical columns.
- Sensors: required-sections, upstream-coverage, traceability all pass.
