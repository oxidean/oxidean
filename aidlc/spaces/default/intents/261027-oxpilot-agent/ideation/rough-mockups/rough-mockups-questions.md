# Rough Mockups Questions — `/oxpilot` Agentic Chat / Builder / Debugger

Upstream inputs: `intent-capture/intent-statement.md`,
`scope-definition/scope-document.md`, `scope-definition/intent-backlog.md`
(this stage consumes all three).

## Sources

- [desc] Initial description: "Fully self-driven I want you to plan and build a full setup of a agentic chat/builder/debuger that has its own /oxpilot page, has computerUse (that allows the end-user to connect the agent to their local system or a remote system to their own liking and configuration). For the framework I want to use https://github.com/TanStack/ai/tree/main/packages/ai-octane and please go for best effort around standards, UI design/UX, security/privacy and usability. I want you to self-drive this until we have a new branch with a PR against main with green CI. Once CI is green I want you to trigger coderabbit AI for review, poll for its response, fix them and do this until coderabbit doesnt find anything anymore. To trigger a review, check the comment it posts or just comment `@coderabbitai review` yourself"
- [scope] Workflow-selected scope: `feature`.

## Q1. What are the entry points and key screens?

Context: `/oxpilot` is a top-level authenticated route; `/settings/ai` already exists as the external-agent integration surface (ox skill + MCP) and is the natural anchor for operator/agent settings.

- A. Three surfaces — (1) `/oxpilot` index: session list + "New session" composer + computer-use target status; (2) `/oxpilot/<session>` workspace: session rail | transcript | composer with tool/action cards (proposed diff, CI analysis, computer-use actions); (3) computer-use targets manager — a section on the `/oxpilot` index (list/connect/revoke) plus an entry point linked from `/settings/ai` (which stays focused on external-agent integration). Operator/provider config lives in deployment env, not the UI.
- B. One single screen — everything crammed into `/oxpilot` with modals.
- C. Settings-heavy: a full `/settings/oxpilot` subtree with its own nav section.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — three surfaces: /oxpilot index (session list + composer + target status), /oxpilot/<session> workspace (rail | transcript | composer + tool cards), computer-use targets manager on the index linked from /settings/ai (which stays external-agent focused). Provider config stays env-side. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q2. What is the core user flow (happy path)?

Context: the value stream is open → chat → propose/debug → review → optional computer-use.

- A. Sign in → `/oxpilot` → New session (pick repo + goal) → streamed chat with repo context → agent proposes a change as a branch/PR or patch (builder card) or explains a failing run (debugger card) → user reviews and opens the PR in the normal forge flow → optionally connects a computer-use target (consent screen → connector install snippet → target appears) → agent requests per-action consent when it needs the target.
- B. Wizard-first: mandatory setup wizard before any chat.
- C. Chat is secondary; the page is primarily a dashboard of agent runs.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — sign in → /oxpilot → new session (repo + goal) → streamed repo-aware chat → builder card proposes branch/PR or patch, debugger card explains failing runs → human reviews PR in normal forge flow → optional computer-use target connect (consent → connector snippet → target live) → per-action consent prompts. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q3. What is the information hierarchy on the session workspace?

Context: competing products (Copilot workspace, ChatGPT agent view, GitLab Duo chat) converge on transcript-first with collapsible context.

- A. Left rail = sessions + connection status (collapsible); center = transcript (streaming, tool/action cards inline per turn) with the composer docked at the bottom; right/context drawer = repo context, proposed diff preview, computer-use target status — toggled, not always-on. Header carries session title + repo + target badge.
- B. Full-width transcript with a top toolbar only.
- C. Tab-based layout (Chat / Diff / Targets / Settings tabs).
- D. Not decided.
- X. Other (please specify)

[Answer]: A — left rail: sessions + connection status (collapsible); center: streaming transcript with inline tool/action cards + docked composer; right/context drawer: repo context, diff preview, target status — toggled. Header: session title + repo + target badge. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q4. Which design system and patterns apply?

Context: Oxidean web is Octane `.tsrx` islands on Astro shells — `AppPage`, `PageHeader`, `AccountSettingsLayout`, Tailwind tokens (`font-display`, `muted-foreground`), TanStack Query session helpers.

- A. Use the existing design language only — `AppPage` + `PageHeader`, existing Tailwind tokens and primitives (`AppLink`, `CopyCommand`, card/table patterns from settings pages), Octane islands for the interactive chat; no new design system, no custom palette. Chat-specific pieces (message bubbles, streaming caret, tool cards) are new components in the same idiom.
- B. Introduce a new chat-specific design language.
- C. Copy a competitor's look (Copilot/ChatGPT) wholesale.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — existing design language only: AppPage + PageHeader, Tailwind tokens, existing primitives (AppLink, CopyCommand, settings card patterns); Octane islands for chat; new chat components in the same idiom, no new design system. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q5. Which devices/form factors must v1 support?

Context: scope-document marks mobile layout out of scope for v1.

- A. Desktop-first responsive web (the existing app is desktop-prioritized); layout must not break at tablet widths but mobile-optimized chat is explicitly out of scope v1.
- B. Full mobile-first design.
- C. Desktop only — fixed layout.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — desktop-first responsive; must not break at tablet; mobile-optimized chat out of scope v1. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q6. What accessibility requirements apply?

Context: this is a new interactive surface with live-streamed content.

- A. WCAG AA target consistent with the app: landmark structure (header/main/nav/complementary), h1–h3 hierarchy per screen, full keyboard operability (focus into composer, card actions reachable, Escape closes drawers/modals), `aria-live` polite region for streamed agent replies (not per-token announcements — announce message boundaries), visible focus states, consent dialogs labelled and keyboard-trappable correctly.
- B. No specific accessibility work.
- C. WCAG AAA certification.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — WCAG AA consistent with the app: landmarks + h1–h3, full keyboard operability (composer focus, card actions, Escape closes), aria-live polite on message boundaries (not per-token), visible focus, labelled/trappable consent dialogs. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Consolidated Summary Confirmation

- Screens: /oxpilot index (sessions + targets), /oxpilot/<session> workspace, targets manager linked from /settings/ai (Q1).
- Flow: sign in → new session → repo-aware streamed chat → builder/debugger cards → human-reviewed PR → optional consented computer-use (Q2).
- Hierarchy: sessions rail | streaming transcript + docked composer | toggled context drawer (Q3).
- Design system: existing AppPage/PageHeader/Tailwind idiom; new chat components in kind (Q4).
- Form factors: desktop-first, tablet-safe, mobile out of scope (Q5).
- A11y: WCAG AA — landmarks, keyboard operability, message-boundary aria-live, labelled consent dialogs (Q6).

Does this all look correct before I generate the artifacts?

- Looks correct
- Request changes

[Answer]: Looks correct


## Assumption Confirmation

The artifacts carry these assumptions:

- [assumption] `/oxpilot/<session>` uses a route param file naming consistent with existing `$owner.$repo.*` conventions; exact route naming (index vs `.$session`) is decided at code-generation against the route manifest.
- [assumption] The connector entry point is assumed to be `ox` CLI-shaped (`ox pilot connect`) — the actual install surface is decided in domain-design (could be a script or a small binary); the wireframe's copy is a placeholder for whatever ships.
- [assumption] "Allow this session" is the deepest consent shortcut — per-action or per-session grants only; no "always allow" (cross-session) grant in v1, pending the security model in nfr-design.

- A. Accept assumptions
- B. Convert to follow-up questions

[Answer]: A. Accept assumptions — conductor-decided under standing instruction (2026-10-27); ratify at PR review.
