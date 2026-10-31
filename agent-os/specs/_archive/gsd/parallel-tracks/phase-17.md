# Parallel track: Phase 17 — Notifications

**Branch:** `cursor/phase-17-notifications-c82f`
**Base:** `main` @ 49a97dc8e895
**Kickoff:** 2026-09-16
**Depends on:** Phases 11+12 — plan now; execute after 12 lands

## Campaign rules

- One branch per remaining roadmap phase (parallel GSD tracks).
- Phase **12** runs discuss(skipped — CONTEXT locked) → plan → execute on this campaign.
- Phases **13, 16, 17, 18, 19, 21, 22** run discuss → plan on their branches first; execute in dependency waves after Phase 12 (and 13 for 19; 19+21 for 22).
- Do not merge dependent-phase implementation ahead of Phase 12 without rebasing onto it.

## Planning status (2026-09-16)

- Discuss (`--auto`): `17-CONTEXT.md` + `17-DISCUSSION-LOG.md` locked (GitHub-like in-app; no email).
- Research: `17-RESEARCH.md`
- UI contract: `17-UI-SPEC.md`
- Nyquist: `17-VALIDATION.md`
- Plans: `17-00` … `17-04` (Wave 0 stubs → tracer RPC → issue emitters → PR emitters → UI)
- Next: `/gsd-execute-phase 17` after Phase 12 lands / rebase
