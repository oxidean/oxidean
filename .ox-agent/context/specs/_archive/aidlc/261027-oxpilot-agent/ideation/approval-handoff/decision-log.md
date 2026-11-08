# Ideation Decision Log — `/oxpilot`

Provenance: decision record for `intent-capture/`, `market-research/`,
`feasibility/`, `scope-definition/`, `team-formation/`, `rough-mockups/`.
Every entry was conductor-decided under the standing self-drive instruction
(2026-10-27); all are ratifiable at PR review.

## Framing (intent-capture)

| # | Decision |
|---|---|
| D-1 | Product: in-product agentic chat/builder/debugger on `/oxpilot` over forge repos, with opt-in computerUse onto user-connected systems; complements (not replaces) `ox` CLI and external agents |
| D-2 | Customers: Oxidean end users (cloud + self-host); secondary: self-host operators needing optionality |
| D-3 | Success: branch + PR + green CI + clean CodeRabbit; product signals = session completion, connect success, time-to-first-reply, zero credential leaks |
| D-4 | Trigger: `ai-octane` aligns with Octane stack; in-product agents are table stakes |
| D-5 | Decisions: maintainer owns scope/priority; conductor decides under standing delegation; PR ratification |
| D-6 | Scope `feature` confirmed |

## Market (market-research)

| # | Decision |
|---|---|
| D-7 | Three-layer competitive set; self-hosted forges ship no native agent |
| D-8 | Table stakes = repo-aware chat, reviewable PRs, CI awareness, persistence, human review; differentiators = user-connected computerUse, self-host parity, provider-configurable LLM, OSS trust |
| D-9 | Adopt `ai-octane`; OpenHands Agent Server is the recorded fallback |
| D-10 | Build a first-party computer-use connector — no vendor offers user-hosted targets |
| D-11 | Audience: existing Oxidean users first; packaging = core platform feature, operator-configured providers, flag-optional |

## Feasibility

| # | Decision |
|---|---|
| D-12 | Integrate through existing seams only (`oxpilot.*` RPC, `/api/` proxy, session auth, db crate, Octane route) — no separate service |
| D-13 | No hard regulatory driver; design obligations stand (consent/authz, redaction/retention, PAT-class connector creds, audit, egress disclosure); formal scan in nfr-requirements |
| D-14 | `ai-octane` verified feasible (1.0.0, peer satisfied, adapters at 1.0.0); pin versions + isolate + fallback |
| D-15 | SSE for model streaming; computer-use connector dials OUT to the forge (outbound-only from user system) |
| D-16 | Operator-configured provider per deployment via `OXIDEAN_*` env; per-user BYO-key out of v1 |

## Scope & team

| # | Decision |
|---|---|
| D-17 | MVP = full request at reduced depth per capability |
| D-18 | MoSCoW: named capabilities + flag/config/audit/security = Must; polish Should; per-user override Could; autonomous merge/marketplace/mobile/CLI-replacement Won't |
| D-19 | Out of scope v1: autonomous merge/push, per-user keys, marketplace, mobile, multimodal, agent orchestration, ox replacement, paid packaging |
| D-20 | Ordering: risk-first inside dependency chain — foundation+spike → tools → builder/debugger → connector → hardening → e2e |
| D-21 | No hard deadlines — quality gate is the criterion |
| D-22 | Team = conductor + stage personas + maintainer PR ratification; serial single-lane; stage-driven persona assignment |

## Mockups

| # | Decision |
|---|---|
| D-23 | Surfaces: `/oxpilot` index + `/oxpilot/<session>` workspace + targets manager linked from `/settings/ai` |
| D-24 | Workspace: sessions rail | streamed transcript + docked composer | toggled context drawer |
| D-25 | Design: existing AppPage/PageHeader/Tailwind idiom; new chat components in kind |
| D-26 | Desktop-first; WCAG AA with message-boundary aria-live and labelled consent dialogs |
| D-27 | Consent depth: explicit connect, per-action/per-session grants, no cross-session "always allow" v1 |

## Handoff verdict

Ideation complete — GO to Inception (reverse-engineering).
