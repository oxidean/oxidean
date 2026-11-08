# User Stories Assessment — `/oxpilot`

## Decision

**Execute.**

## Rationale

`/oxpilot` is a user-facing feature with at least three distinct personas
(end user driving sessions, operator deciding enablement/provider,
reviewer consuming agent-produced PRs), a consent-heavy interaction model
(computer-use grants), and cross-cutting workflows (chat → proposal →
review → PR; connect → consent → act → revoke). Requirements alone capture
the WHAT; stories keep the WHO and the acceptance surface explicit for
construction units.

## Factors considered

- Project type: brownfield feature — user stories map cleanly onto the
  existing persona vocabulary the codekb already describes (authenticated
  forge users, operators).
- User-facing scope: high — new routes, dialogs, consent UX, session UX.
- Complexity signals: two actor classes (human + agent connector),
  authorization boundaries, streamed UI — stories force the acceptance
  criteria that make these testable.
- Skip alternatives: requirements.md already carries FR/NFR IDs; stories
  add persona × workflow coverage and AC IDs for test binding, which pure
  FRs lack.

## Highest-value areas

1. Session lifecycle (create → chat → propose → review → materialize).
2. Computer-use lifecycle (register → consent → act → audit → revoke).
3. Operator enablement/disable states.
