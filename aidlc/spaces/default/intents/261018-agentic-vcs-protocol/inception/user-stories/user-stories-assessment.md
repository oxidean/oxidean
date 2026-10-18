# User Stories Assessment — Side-by-Side Agentic VCS Protocol

## Decision: Execute

## Rationale

User stories add real value here despite the feature being developer-facing infrastructure:

- **Multiple personas with different goals.** The primary "user" is a coding *agent* driving high-volume writes over API/MCP, but forge operators (enable/disable, quotas, lifecycle), human developers (CLI checkout/inspection of agent work), and CI integrators (`vcs.*` RPC consumers) each interact with the surface differently. Stories keep each persona's through-line explicit instead of dissolving them into FR numbers.
- **Complex business logic worth expressing as flows.** Changeset creation + idempotent resubmit, want-based fetch with a sparse filter, merge/rebase conflict paths, Git import/export convergence, and the CAS ref contract each have acceptance criteria that read naturally as stories and awkwardly as bare FR prose.
- **Cross-team coordination.** The crate boundary, `oxidean-db` migrations, RPC registration, CLI manifests, MCP tool gating, and edge routing touch every layer; persona-framed stories give the downstream units-generation stage natural seams.

## Factors considered

- **Project type**: Brownfield — stories anchor new capability onto an existing forge whose permission/activity/feed vocabulary is already affirmed.
- **User-facing scope**: API/CLI/MCP surfaces are user-facing for machine users; no web UI this intent (O5), so stories deliberately cover protocol behavior rather than visual flows.
- **Complexity signals**: 16 FR groups + 10 NFRs, three consumption surfaces, two storage tiers (rows + CAS), and a bidirectional Git bridge — high enough that persona-structured acceptance coverage materially helps contract-design and test planning.

## Key areas where stories add most value

1. Agent write-path flows (create → idempotent retry → ref advance → merge/rebase).
2. Agent/integrator read-path flows (want-based object fetch, sparse checkout, API-first history queries with no clone).
3. Operator flows (enable, quota, import/export, lifecycle on repo delete/transfer).
4. Safety/policy flows (auth surfaces, PAT classes, fail-closed paths, activity feed visibility).

## Alternative coverage if skipped

Requirements alone would suffice for a pure refactor — not applicable; this intent adds a user-facing protocol surface.
