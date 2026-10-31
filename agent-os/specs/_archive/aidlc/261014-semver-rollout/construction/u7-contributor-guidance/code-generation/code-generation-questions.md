# U7 Contributor Guidance — Code Generation Questions

## Plan Approval

Approve this exact Code Generation plan?

[Approval Fingerprint]: sha256:v3:d7662e998bd24a93077d871bc124e68fdd0157b039fa1f22ad937a95ad6abaf6
[Planned Source]: fa179a9160014308ed2ebfe4a22b7df29d693512150ba35bc0ec19f39999d112

Options:
- Approve Plan — proceed to code generation
- Request Changes — revise the plan

[Answer]: Approve Plan

## Consolidated Summary Confirmation

The plan covers: new canonical `docs/VERSIONING.md` (components model,
grammar, scopes, breaking markers, exemptions, hook, changelogs, release
flow with `RELEASE_TOKEN` + prerelease lanes + reserved `chore(release):`);
entry-point links in `CONTRIBUTING.md`, `docs/DEVELOPMENT.md`,
`docs/TESTING.md`; agent surfaces — `AGENTS.md` contribution-standards
section naming the hook + CI gate, new
`.agents/skills/semver-conventions/SKILL.md`, pointer update in
`.cursor/rules/oxidean-core.mdc`; and a docs-shape guard test
(`scripts/versioning/docs.test.ts`) enforcing topic coverage and the
BR7.3 no-duplication rule. All touched files sit outside every component
glob, so the unit's own commits induce no bump (BR7.4).

Options:
- Looks correct — proceed
- Request changes — amend the summary

[Answer]: Looks correct
