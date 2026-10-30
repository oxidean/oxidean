**Reviewer:** aidlc-architecture-reviewer-agent
**Iteration:** 1
**Verdict:** READY

First review of the contributor-guidance unit on final bytes. Scope
reviewed: `docs/VERSIONING.md` (new), `CONTRIBUTING.md`,
`docs/DEVELOPMENT.md`, `docs/TESTING.md`, `AGENTS.md`,
`.agents/skills/semver-conventions/SKILL.md` (new),
`.cursor/rules/oxidean-core.mdc`, `scripts/versioning/docs.test.ts`.

Verified claims against the machine surfaces rather than trusting the
prose:

1. Grammar — `type(scope)!: subject`, `allowed_types`, `reserved_scopes:
   ["release"]`, breaking markers, and the three exempt prefixes match
   `versioning.toml` `[grammar]` and `commit.ts` (`EXEMPT_SUBJECTS`,
   `HEADER_RE`, `BREAKING_FOOTER_RE`) exactly.
2. Bump table — `bumpForCommit` in `semver.ts` confirmed: feat→minor;
   fix/perf/revert→patch; breaking→minor while `major===0`, major at
   ≥1.0; other types→none; invalid→patch. Propagation = patch floor
   through `depends_on` (plan.ts) — docs match.
3. Release mechanics — `make release-plan`/`release-tags`,
   `versioning changelog`/`bump`, `RELEASE_TOKEN` (GITHUB_TOKEN
   prohibited), `release.yml` `workflow_dispatch {component, channel}`,
   `release-pr.yml` weekly+dispatch `chore(release):` PRs, `ox-cli.yml`
   `ox-v*` lane + `latest.json` — all named surfaces exist as described.
4. BR7.3 — VERSIONING.md names `versioning.toml` authoritative and
   carries only a 4-alias illustrative parenthetical; the guard test
   enforces ≤8 backticked aliases against the live registry (>10 total),
   so future drift copies fail the suite.
5. BR7.4 — all eight touched paths matched against every component
   `globs` entry: zero hits; unit commits are `docs:`-typed by location.

### Findings

| ID | Severity | Location | Finding | Resolution |
|---|---|---|---|---|
| U7-1 | Note | BR7.3 spec text | Rule cites a `versioning scopes` subcommand that was never built | Docs name `versioning.toml` itself as the authority — same intent, real surface; recorded as deviation in code-summary |
| U7-2 | Low | docs/VERSIONING.md | "git commit -m \"typo\"" phrasing is colloquial for a canonical doc | Acceptable tone for the exemptions example; kept for clarity |
| U7-3 | Low | scripts/versioning/docs.test.ts | Anchor list is substring-matched, not section-matched — a topic could technically appear without its own section | Anchors chosen to require distinct coverage points; adequate for a shape guard |

### Verification

- `bun test scripts/versioning` — 126 pass / 0 fail (11 new cases).
- `make version-check` — ok, 10 components, 1 pending-baseline warning.
- Glob dogfood check — all 8 touched files: zero component-glob matches.
- Traceability sensor — `missing_from_table` empty; residual
  `missing_from_upstream_ids` lists other units' requirements (expected).

### Summary

Both audiences now have the convention: humans get a canonical
VERSIONING.md plus entry-point links, agents get AGENTS.md guidance, a
dedicated skill, and a cursor-rule pointer. Docs reference machine truth
rather than duplicating it, and a shape test guards the invariant. READY
on current bytes.
