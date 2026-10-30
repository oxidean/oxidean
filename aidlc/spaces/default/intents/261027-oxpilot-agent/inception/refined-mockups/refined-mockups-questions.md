# Refined Mockups Questions — `/oxpilot`

Upstream inputs: `ideation/rough-mockups/wireframes.md`,
`ideation/rough-mockups/user-flow.md`,
`inception/user-stories/stories.md`,
`inception/requirements-analysis/requirements.md`.

## Sources

- [desc] Intent: `/oxpilot` agentic chat/builder/debugger with opt-in computer use.
- [scope] Workflow-selected scope: `feature`.

## Q1. Mockup fidelity and format?

- A. Annotated mid-fidelity ASCII screens + per-component specs (per the design-agent component-spec template) + state tables — enough for construction to map 1:1 to Octane components.
- B. Low-fidelity ASCII only — defer component specs to contract-design.
- C. Full visual design (pixel specs, color tokens) — beyond what construction needs here.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — mid-fidelity ASCII + component specs + state tables; construction maps them 1:1 to Octane. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q2. Which surfaces get component-level specs?

The rough pass produced 5 surfaces: index, session workspace, new-session
dialog, connect/consent dialog, per-action consent card. User stories add:
tool-call card, proposed-diff card, queued-message state, disable state,
delete/revoke confirms.

- A. Spec every interactive component (session list, workspace panes, composer, tool-call card, diff card, consent dialogs, consent card, target row, empty/error states) — consent surfaces are the risk area and need precision.
- B. Spec only the two consent surfaces; describe the rest in prose.
- C. Not decided.
- X. Other (please specify)

[Answer]: A — component-level specs for every interactive surface; consent surfaces get the deepest precision. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q3. Responsive behavior?

- A. Desktop-first (forge app norm): workspace three-pane ≥1024px, rail collapses to a toggleable drawer below 1024px, single-column <768px with transcript priority — matches affirmed practices (no mobile-native commitment).
- B. Full responsive matrix at all breakpoints now.
- C. Desktop only, no responsive spec.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — desktop-first, drawer-collar <1024px, single-column <768px; transcript stays primary. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q4. Accessibility target?

- A. WCAG 2.1 AA — labelled dialogs with focus trap + Esc, `aria-live="polite"` on the transcript, real buttons/inputs, 4.5:1 text contrast — matches the wireframe a11y notes and NFR7.
- B. A only — lower burden.
- C. AAA.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — WCAG 2.1 AA, matching wireframe a11y notes and NFR7. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Consolidated Summary Confirmation

- Format: mid-fidelity annotated ASCII + component specs + state tables (Q1).
- Component specs: every interactive surface, deepest on consent (Q2).
- Responsive: desktop-first; drawer <1024px, single-column <768px (Q3).
- Accessibility: WCAG 2.1 AA (Q4).

Does this all look correct before I generate the refined mockups?

- Looks correct
- Request changes

[Answer]: Looks correct
