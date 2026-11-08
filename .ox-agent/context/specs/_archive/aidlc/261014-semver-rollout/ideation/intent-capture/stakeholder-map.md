# Stakeholder Map

## Key Stakeholders and Their Interests

| Stakeholder | Interest | Source |
|-------------|----------|--------|
| Primary maintainer | Holds the primary call on version policy; wants semver automation wired into releases, CLI/MCP surfaces, the commit hook, and agent practices | [Q6] [desc] |
| Self-hosted admins | Need trustworthy versions and per-component changelogs to decide whether to upgrade their deployment | [Q2] [Q7] |
| Contributors and maintainers | Need commit-prefix conventions and automated changelog generation to coordinate changes in the repo | [Q2] [desc] |
| Downstream tooling consumers (ox CLI users, MCP clients, API consumers) | Need consistent, trustworthy versions on every versioned surface | [Q2] [desc] |

## Decision-Makers vs. Influencers

| Role | Who | Source |
|------|-----|--------|
| Decision-maker | Primary maintainer — holds the primary call on version policy | [Q6] |
| Influencers | Contributors (affected by commit conventions and agent practices) and all consumer classes (affected by version and changelog output) | [Q6] [Q2] |

## Communication Requirements

| Requirement | Detail | Source |
|-------------|--------|--------|
| Changelog layout | Per-application/crate changelogs plus one global consolidated changelog referencing the inner ones | [Q7] |
| Reporting cadence | Unknown (open question) [assumption] | [assumption] |

## Assumptions & Open Questions

- No reporting cadence beyond the changelog artifacts was established; if one is needed it should be raised at scope-definition. [assumption]
- Whether the stakeholder list is complete beyond the primary maintainer and consumer classes is unconfirmed — a wider group was not identified. [assumption]
