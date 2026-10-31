# Stakeholder Map — `/oxpilot` Agentic Chat / Builder / Debugger

## Key Stakeholders and Interests

| Stakeholder | Role | Interests | Source |
|-------------|------|-----------|--------|
| Maintainer (requester) | Decision-maker; final ratifier | Standards, UI/UX quality, security/privacy of computer-use connections, usability; outcome = PR against `main` with green CI and a clean CodeRabbit loop | [desc] [Q5] [Q6] |
| Oxidean end users (cloud + self-host) | Primary beneficiaries | Trustworthy agent; computer use only on systems they explicitly connect and configure; privacy of credentials and code | [desc] [Q2] [Q5] |
| Oxidean self-host operators | Deployment stakeholders | Feature optionality/configurability; no forced external service dependencies on private deployments | [Q2] [Q5] |

## Decision-Makers vs Influencers

| Stakeholder | Decides | Influences | Source |
|-------------|---------|------------|--------|
| Maintainer | Scope, priority, final ratification at PR review | All in-stage decisions via the standing self-driven instruction | [desc] [Q6] |
| Conductor (this workflow run) | In-stage decisions, delegated under the standing self-driven instruction | All decisions marked for ratification at PR review | [desc] [Q6] |

## Communication Requirements

| Requirement | Detail | Source |
|-------------|--------|--------|
| Reporting surface | The pull request itself: SemVer-prefixed commit subjects, green CI, `@coderabbitai review` trigger + response loop until clean | [desc] [Q7] [memory:M1] [memory:M3] |
| Decision record | The `aidlc/` intent record (questions, artifacts, audit trail) | [Q7] |
| Cadence | None — no separate status reports | [Q7] |

## Assumptions & Open Questions

- [assumption] No external reviewers, compliance officers, or design owners
  beyond the maintainer are involved in this initiative; if a dedicated
  design reviewer exists, UX sign-off authority should be confirmed.
