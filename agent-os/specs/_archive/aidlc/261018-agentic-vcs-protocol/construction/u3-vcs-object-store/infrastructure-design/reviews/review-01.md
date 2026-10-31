## Review

**Stage:** infrastructure-design
**Unit:** u3-vcs-object-store
**Reviewer:** aidlc-architecture-reviewer-agent
**Verdict:** NOT-READY
**Iteration:** 1

### Findings

| ID | Severity | Finding | Location | Status |
|----|----------|---------|----------|--------|
| INF-01 | Critical | `vcs-server` is a new consumer of `nextest-archive` — `rust-artifacts-cleanup`'s `needs` (ci.yml:893) tracks consumers, not artifact names, so it DOES need the edit: without it cleanup can delete the archive while the leg is queued (race) and a failed `vcs-server` can never re-run green (archive gone) — the exact mute the lane exists to catch. | cicd-pipeline.md:35 | Open |
| INF-02 | Minor | No `version-check` job — registry enforcement is the `registry completeness` step in `commit-lint` (ci.yml:158-159); lane name wrong. | cicd-pipeline.md:11 | Open |
| INF-03 | Minor | `--partition count:N/5` distributes tests one-shard-each — "all 5 shards execute the suite" misstates; coverage true, wording wrong. | cicd-pipeline.md:9, questions.md:7 | Open |
| INF-04 | Minor | Zero-test signal sharpness unpinned: `-E` NO_TESTS_RUN is loud only while the crate ships zero non-server tests — a future ungated test silently degrades it. | cicd-pipeline.md:33 | Open |
| INF-05 | Nit | BR3.11 absent from upstream_ids (substantively covered by NFR6 row). | traceability.json:4 | Open |
| INF-06 | Nit | New-job docs row missing — house convention adds the CI-table entry in docs/TESTING.md:258-259. | cicd-pipeline.md | Open |
| INF-07 | Verified | Core claim TRUE: resolver=2 virtual workspace + unqualified `--features server` enables on oxidean-vcs only, errors only if zero members declare it; same-commit sequencing correctly required; nextest archive forwards the flag. | ci.yml:207 | Verified |
| INF-08 | Verified | Zero-test leg mechanics TRUE: `--no-tests` default `fail` since 0.9.85 (exit 4 NO_TESTS_RUN), pinned 0.9.146; `-E 'package(oxidean-vcs)'` valid; workspace-remap + alias reuse correct. | ci.yml:234-277 | Verified |
| INF-09 | Verified | All line pins land (:198-208, :234-277, :329-334, :806; Makefile :237-243/:402-405/:409-410/:353); dialect-debt note accurate. | as cited | Verified |
| INF-10 | Verified | Precedents real (machete ignored api Cargo.toml:103-107; .config/nextest.toml:8-12; upstream IDs all exist; facade deltas genuinely absent). | multiple | Verified |

### Summary

Load-bearing claims verify hard: `--features server` on the workspace archive under resolver=2 does exactly what is claimed, the -E leg's NO_TESTS_RUN signal is real, every pin lands. One blocking defect: a missed `needs` edge — the artifact asserts cleanup needs no edit because no second artifact is created, but `needs` lists consumers and `vcs-server` consumes `nextest-archive`. One-line fix (add `vcs-server` to `needs` at ci.yml:893) plus cheap minor folds. 1 Critical -> NOT-READY.
