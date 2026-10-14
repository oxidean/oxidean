# Project-Level Rules

> Project-specific specialisation and corrections. Loaded after `org.md` and
> `team.md` as strict-additive guidance; contradictions with broader policy
> are rejected. Populated by practices-discovery and the self-learning loop.
>
> Use sparingly: most teams don't need a project layer. Reach for it
> only when this specific project needs stable, durable guidance beyond the
> team practice (for example, package-specific release checks or an additional
> regression suite for a legacy component).

## Way of Working

<!-- Project-specific specialisation. Example: -->
<!-- This monorepo requires package-scoped branch names and a package owner -->
<!-- review in addition to the team's normal merge policy. -->

## Walking Skeleton

<!-- Project-specific specialisation. Example: -->
<!-- The walking skeleton must exercise the legacy service adapter as well -->
<!-- as the new service boundary. -->

## Testing Posture

<!-- Project-specific specialisation. -->

- CI/Docker-related changes are verified locally with docker + act before shipping; deadlines are ASAP-but-quality-gated — solid tests are required, not optional (learned 2026-10-14) <!-- cid:261014-semver-rollout:scope-definition:e0b8c754d11a5bb705baa2e6ade501fd1cf383cb6451fe582dcfeeb0a7f41278 -->

## Guard Policy

<!-- Project-specific. Mode: strict, relaxed, or off. Strict here holds for every intent and cannot be changed from chat. A section under the retired Change Control heading, written by an earlier release, is still read. -->

## Deployment

<!-- Project-specific specialisation. -->

## Code Style

<!-- Project-specific specialisation. -->

## Tech Stack

<!-- Technology choices locked for this project. -->

## Decided

<!-- Decisions made in earlier stages that should not be re-asked. -->
<!-- Format: DECIDED: [decision] (Stage [slug], [date]) -->

## Scope Overrides

<!-- Custom scope rules for this project. -->

## Forbidden

<!-- Populated by practices-discovery affirmation gate. -->
<!-- Format: NEVER [behavior] (affirmed [date]) -->
<!-- Example: NEVER throw exceptions across service layer boundaries (affirmed 2026-05-17) -->

## Mandated

<!-- Populated by practices-discovery affirmation gate. -->
<!-- Format: ALWAYS [behavior] (affirmed [date]) -->
<!-- Example: ALWAYS use Result<T,E> for fallible operations in service layer (affirmed 2026-05-17) -->

## Corrections

<!-- Project-specific corrections from human feedback. -->
<!-- Format: NEVER/ALWAYS [behavior] (learned [date]) -->
- Treat "uniform versioning" as one semver practice with per-component version numbers (all starting at the same baseline, e.g. 0.1.0), where a component bumps only when changes hit it and a shared-dependency change counts as a hit on dependents — confirm with the user rather than assuming a single shared version (learned 2026-10-14) <!-- cid:261014-semver-rollout:intent-capture:cc8a97843155c5e0ae79fed94a02343bfc7e1c52ce07a38c78f9719c2aa647a3 -->
- Prefer a two-tier changelog layout — per-application/crate changelogs plus one global consolidated changelog that references the inner ones (learned 2026-10-14) <!-- cid:261014-semver-rollout:intent-capture:4a87e98964b8943cb05393e23515845e008ca70aeac02b6ca7acb8989e131da1 -->
- Keep solution-level questions (automation tool choice, commit-prefix grammar, hook distribution, CI enforcement depth) out of ideation stages — they land in requirements-analysis and domain-design (learned 2026-10-14) <!-- cid:261014-semver-rollout:intent-capture:5095a3d3c9d2434b11a71f93d28308f5d13546e9b6e3aff0d44cec4ab28552c6 -->
- Every Makefile target must be .PHONY-declared with a runnable recipe; scripts/check-makefile.sh enforces .PHONY coverage, non-empty recipes, and scripts/ path validity — keep new targets compliant (learned 2026-10-14) <!-- cid:261014-semver-rollout:scope-definition:d266bb7ec53daccdb6e215eafeab3cbb51b9c2626d98510329e70d8f73b4691d -->
