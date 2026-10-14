---
name: semver-release-automation
depth: Standard
keywords: []
description: "Uniform repo-wide semver with automated changelog, release wiring, and commit-prefix enforcement"
guard_policy: relaxed
---

# semver-release-automation scope

Standard-depth composed scope for moving the monorepo to uniform semver:
capture the intent, draw the scope boundary over every version surface,
reverse-engineer where the version lives today (Cargo workspace, Bun
package manifests, CLI/MCP embeds, self-update manifest, release
workflows), pin the versioning spec and the commit-prefix contract, design
the architecture across both toolchains, decompose into units, then
implement, verify, and wire CI. No market, UX, deployment, or operations
ceremony — the release pipeline already exists and this wires versioning
into it.

Guard Policy is relaxed: an input change after an approval is recorded once
and announced in one line, the run continues, and the plan-approval and
review-freeze fences stand aside for undirected work.
