### Product-Lead Review — Rough Mockups (`/oxpilot` Agentic Chat / Builder / Debugger)

### Review

**Verdict:** READY
**Reviewer:** aidlc-product-lead-agent
**Date:** 2026-10-27T01:25:00Z
**Iteration:** 1

### Findings

| ID | Severity | Location | Finding | Required action | Status |
|---|---|---|---|---|---|
| R-01 | Minor | wireframes.md > Screen 5 — Unconfigured / flag-off state | Copy tells operators to set `OXPILOT_*` env vars, but scope-document and constraint TC-5 mandate the `OXIDEAN_*` env prefix (e.g. `OXIDEAN_OXPILOT_*`) documented in `docs/CONFIGURATION.md`. | Align the env-var prefix in the screen copy to the `OXIDEAN_*` convention, or annotate it as a placeholder to be fixed at requirements/code-generation. | New |
| R-02 | Minor | wireframes.md > Screen 1 — Link row | The index shows a link `/oxpilot → /settings/ai` ("Agent & MCP integrations"), but decided answer Q1-A places the entry point in the other direction: the computer-use targets manager is linked *from* `/settings/ai`. The reverse link is a fine addition but does not cover the decided direction. | At detailed design, add the `/settings/ai` → `/oxpilot` targets-section entry point; keep or drop the reverse link as a UX choice. | New |
| R-03 | Minor | wireframes.md > Screen 2 — Sessions rail | Decided answer Q3-A puts "sessions + connection status (collapsible)" in the left rail; the wireframe shows sessions only, with connection status carried solely by the header target badge, and no collapse affordance is drawn for the rail. | Add a connection-status row and a collapse affordance to the rail in detailed-design mockups, or record the simplification as a deliberate deviation. | New |
| R-04 | Minor | wireframes.md > Screen 1 — New session entry | Decided answers Q1-A/Q2-A call for a "New session" step where the user picks repo + goal; the wireframe shows only a `[+ New session]` button — the repo-picker/goal-composer surface (named in the happy path) is unmocked. | Cover the new-session creation surface (dialog or inline composer) in detailed design before code-generation; not a blocker at rough-mockup depth. | New |

### Summary

The wireframes cover all five named surfaces — `/oxpilot` index, session workspace, connect/consent dialog, per-action consent card, and the flag-off state — and faithfully implement the decided answers: rail|transcript|drawer hierarchy, existing AppPage/PageHeader/Tailwind idiom, desktop-first framing, and WCAG AA notes including message-boundary `aria-live` and focus-trapped, labelled dialogs. Assumptions are honestly labelled, the consent model (outbound-only connector, per-action/session grants, "agent never merges") stays inside the v1 scope boundary, and nothing contradicts the request's computerUse/`/oxpilot`/builder-debugger intent. Four minor fidelity gaps (env-var prefix, settings-link direction, rail contents, unmocked new-session step) are advisory and safe to resolve downstream.
