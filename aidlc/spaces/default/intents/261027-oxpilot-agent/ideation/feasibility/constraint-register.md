# Constraint Register — `/oxpilot` Agentic Chat / Builder / Debugger

Provenance: consumes `intent-capture/intent-statement.md`,
`market-research/competitive-analysis.md`, `market-research/market-trends.md`,
`market-research/build-vs-buy.md`; Q1–Q7 in `feasibility-questions.md`.

## Technical constraints

| ID | Constraint | Source | Impact |
|---|---|---|---|
| TC-1 | UI must be Octane `.tsrx` via `@tanstack/ai-octane` — no React, no parallel app structure | request + oxidean-core rule | Chat UI, tool components, interrupts all authored as Octane islands |
| TC-2 | New API surface under `/api/`; `oxpilot.*` snake_case RPC procedures on the existing dispatch; `make rpc-gen` regenerates the client in the same change | learned rule + code style | No new top-level namespace; RPC checklist applies per procedure |
| TC-3 | Session/transcript persistence lives in `oxidean-db` with ×3 migrations (postgres/mysql/sqlite) | oxidean-core rule | Dialect-agnostic session store; no branching on dialect outside the crate |
| TC-4 | Machine/connector transports authenticate with PAT/pubkey-class credentials — never session cookies | project Mandated rule (D-12 precedent) | Computer-use connector auth model is fixed at the credential class level |
| TC-5 | New `OXIDEAN_*` env vars documented in `.env.example` + `docs/CONFIGURATION.md` in the same PR | project Mandated rule | Provider/flag config surface is documentation-bound |
| TC-6 | Feature must be optional/flag-gated on self-host — no forced external dependency | intent-capture Q5/Q7 | Feature flag per deployment; Ollama-class local provider path required |
| TC-7 | Long-lived streaming (SSE) must pass through the existing `/api/` proxy | feasibility Q4 | Edge timeout/drain review at infrastructure-design |
| TC-8 | `octane` stays `>=0.12.0` (currently `^0.15.0`); `@tanstack/ai-octane` pinned exactly | feasibility Q3 | bun.lock is the dependency contract (`bun install --frozen-lockfile`) |

## Organizational constraints

| ID | Constraint | Source | Impact |
|---|---|---|---|
| OC-1 | Delivery under standing maintainer delegation: conductor-decided answers/gates, all ratifiable at PR review | request + affirmed intent pattern | No mid-workflow human stops; PR + CodeRabbit is the review surface |
| OC-2 | Trunk-based development: short-lived `type/<slug>` branch → PR → squash merge; SemVer commit subjects | team.md | One delivery PR (or stacked units) with `feat(...)`/`fix(...)` subjects |
| OC-3 | No dedicated budget/headcount — process gates are the binding constraint | feasibility Q6 | Sequence risk-first: connector + streaming spike before bulk UI work |
| OC-4 | CI must stay green through delivery; coverage manifests gain rows for new routes/high-risk UI | project Mandated rules | Route-coverage + browser-coverage rows are part of done, not follow-up |

## Regulatory / compliance constraints

| ID | Constraint | Source | Impact |
|---|---|---|---|
| RC-1 | Computer-use targets connect only on explicit user action — never auto-discovered or enabled on the user's behalf | accepted intent-capture assumption | Consent + authorization model is a first-class design requirement |
| RC-2 | Transcripts/prompts may contain secrets and private code — redaction + retention policy required before provider egress | feasibility Q2/compliance lens | `nfr-requirements` owns the policy; provider egress disclosed to operators |
| RC-3 | No identified hard regulatory driver (PCI/HIPAA/GDPR-specific) at ideation depth | feasibility Q2 | Formal scan queued for nfr-requirements; not a blocker |
| RC-4 | Self-host privacy posture: code + transcripts must be able to stay fully on operator infra | intent-statement stakeholder requirement | Local-provider path (Ollama) + no mandatory telemetry |

## Assumptions & Open Questions

- [assumption] Constraint RC-3 (no hard regulatory driver) is asserted at
  ideation depth from the compliance lens; the formal scan in
  nfr-requirements could surface obligations (e.g. data-residency
  disclosures) that would be added here rather than argued away.
