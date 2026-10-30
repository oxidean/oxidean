# Team Allocation — `/oxpilot`

Per the 1.5 Team Formation decision (Q5): no fixed mob roster — persona
assignment is **stage-driven**. A "mob" here is the lead persona the
stage graph declares for each per-unit stage, plus its supports; there
is one build lane (serial execution), so this table is a duty roster,
not a parallel plan.

## Bolt-to-mob roster

| Bolt | Unit | Mob (stage-declared personas) |
|---|---|---|
| B1 | u1-skeleton-slice | architect (design stages) + developer (code-gen/build-test) |
| B2 | u2-session-core | architect + developer |
| B3 | u4-providers | architect + developer; devsecops eye on key custody |
| B4 | u3-agent-runtime | architect + developer |
| B5 | u6-connector-gateway | architect + developer; devsecops on grants/authz |
| B6 | u5-oxpilot-tools | architect + developer |
| B7 | u7-connector-client | architect + developer |
| B8 | u8-oxpilot-ui | design + developer (ui unit: mockups-informed stages) |

## Review surface

Quality evidence lands per unit per the testing posture (real SQLite
nextest, happy-dom/Chromium, SSE, wiremock, dialect tests). The
maintainer is PR ratifier at the human-review surface — the PR +
CodeRabbit loop is where stage output becomes reviewed code.
