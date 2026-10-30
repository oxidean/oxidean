# Security Test Instructions — semver-release-automation

**N/A — no dedicated security test battery; the applicable security
properties are covered by existing checks.**

Requirements carry no NFR security targets (no auth, injection, or
SAST/DAST surfaces changed). The security-relevant properties of this
scope are enforced elsewhere and verified by the integration run:

- **NFR5 (no secrets; no silent git mutation; CI authoritative)** —
  `make setup` is the only hook-install path and announces its
  `core.hooksPath` change; `--no-verify` bypass is local-only because CI
  `commit-lint` re-checks the range; asserted in `hook.test.ts` +
  `docs.test.ts`.
- **RELEASE_TOKEN vs GITHUB_TOKEN** — tag pushes refuse when the secret
  is unset (`release.yml` "RELEASE_TOKEN is configured" step); workflows
  never echo secrets. Asserted by workflow-shape tests.
- **Header gate rejects malformed input** — `check_version_header`
  fuzz-adjacent cases (empty, whitespace, garbage, future-incompatible)
  covered in `rpc.rs` tests; advisory paths never block with attacker-
  controlled data.

No additional SAST/DAST or dynamic security commands are run in this
stage.
