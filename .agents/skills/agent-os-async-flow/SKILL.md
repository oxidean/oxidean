---
name: agent-os-async-flow
description: Drive Agent OS spec work over async chat channels (Telegram, messaging surfaces, ACP hosts) where interactive plan mode and question forms do not exist. Use when the user asks to plan, shape, spec, or discuss work through a chat conversation.
---

# Agent OS over async chat

The upstream Agent OS skills (`agent-os-shape-spec`, `agent-os-plan-product`) are written for interactive coding sessions: they hard-require "plan mode" and ask questions through a form tool (`AskUserQuestion`). On chat channels — Telegram via Hermes, messaging bridges, ACP embeds — neither exists: there is no plan-mode toggle, and question cards may never render for the user.

This skill adapts the flow. Follow it whenever the user drives Agent OS work (plan, shape, spec, spec review) through a chat conversation.

## Adaptation rules

| Upstream instruction | Chat-channel behavior |
|---|---|
| "Must run in plan mode" | Ignore — there is no plan mode. Follow the intent: investigate before writing code, land changes by PR. |
| "Use AskUserQuestion" | Ask in plain chat text (see below). Never assume a question UI exists or will be delivered. |
| Sequential interview | Do NOT drip questions one per turn. Gather context autonomously, then ask everything in one message. |
| User reviews in-session | Deliver as a draft PR; the user reviews the spec on GitHub and answers in chat or PR comments. |

## Asking questions in chat

- One message, numbered list, max ~4 questions per round.
- State your recommended option first for each question and say why — the user should be able to answer with "all defaults" or a couple of overrides.
- Prefer an autonomous draft with flagged assumptions over stalling: write the spec first, mark unresolved points in an `## Open questions` section, then ask.
- If the user answers partially, fold the answers in and re-flag only what remains open.

## Workflow

1. **Gather context autonomously** — read the code surfaces the request touches, `agent-os/product/` (mission, roadmap, tech-stack, decisions), prior records in `agent-os/specs/` (newest first; `_archive/` is read-only provenance, never edit), and `agent-os/standards/index.yml` for binding standards. Search the issue tracker for related issues and PRs.
2. **Draft the spec folder** at `agent-os/specs/<YYYY-MM-DD-HHMM>-<slug>/` using the real clock (`date '+%Y-%m-%d-%H%M'`), never session metadata. Write `shape.md` (context, recommended decisions with rationale, alternatives considered, open questions), `plan.md` (phased tasks, each with verify criteria), `references.md` (code and doc surfaces), `standards.md` (binding standards inlined — see `agent-os-shape-spec` for the format).
   Use a lowercase, hyphenated feature slug with at most 40 characters. If visuals exist, add them under `visuals/`.
3. **Open a draft PR** — branch `spec/<slug>`, commit `docs(spec): <subject>`, draft PR titled `spec: <title>`; body summarizes the spec and lists the open questions.
4. **Ask the open questions in chat** (format above) and keep them mirrored in the PR body. Fold answers into `shape.md`/`plan.md`, push, keep the PR body in sync.
5. **Related updates ride the same PR** — roadmap line items, standards edits, `standards/index.yml` regeneration via `agent-os-index-standards`.
6. **Implementation** — when the user approves and says build it, `plan.md` is the task list; mark the spec PR ready for review or open a fresh implementation branch per the spec's plan.

## House rules still apply

- Commit subjects follow the `type(scope):` grammar (`docs(spec)`, `chore(agents)`, …) — see `semver-conventions`; enforced by the commit-msg hook and CI commit-lint.
- Substantial prose gets a `clean-user-facing-text` audit before committing (repo rule).
- No AI attribution in commits or PRs (repo rule).
- No secrets in specs, plans, or examples.
