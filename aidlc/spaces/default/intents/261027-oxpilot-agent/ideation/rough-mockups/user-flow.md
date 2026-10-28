# User Flow — `/oxpilot` Agentic Chat / Builder / Debugger

Provenance: consumes `intent-capture/intent-statement.md`,
`scope-definition/scope-document.md`, `scope-definition/intent-backlog.md`;
happy path from Q2 in `rough-mockups-questions.md`.

## Happy path — chat → builder → PR

```
sign in
   │
   ▼
/oxpilot index ─── no sessions? ──▶ empty state + composer hint
   │ [+ New session]
   ▼
pick repo + goal ─────────────────▶ /oxpilot/<session>
   │                                   │
   ▼                                   ▼
streamed chat ◀── follow-ups ──── tool cards inline
   │                                   │  (repo reads,
   │                                   │   CI log reads)
   ▼                                   ▼
agent proposes change ◀────────── builder card
   │                                   │
   ├─ [View diff] ───────────────▶ context drawer: diff preview
   ├─ [Show patch] ──────────────▶ patch card (copy/apply)
   └─ [Create branch + PR] ──────▶ PR created → normal forge review
                                        (agent never merges)
```

## Happy path — debugger

```
session on repo X
   │
   ▼
"why is main failing?" ─▶ agent reads Actions run logs (tool card)
   │                       lists failing job → tail + diagnosis
   ▼
explanation + proposed fix ─▶ builder card (same PR flow as above)
```

## Computer-use path (opt-in, consented)

```
/oxpilot index ─▶ [Connect a system]
   │
   ▼
consent dialog: name + type (local|remote) + capability grants
   │  (shell / files / GUI)
   ▼
connector command shown (token in read-only field)
   │
   ▼
user runs connector on their system ─▶ it dials OUT to the forge
   │                                    (no inbound listener)
   ▼
target appears: connected ●idle
   │
   ▼
in-session: agent requests an action on the target
   │
   ▼
per-action consent card ─▶ Allow once | Allow this session | Deny
   │
   ▼
result streams back into the transcript (audited)
```

## Failure/abandon paths

- **Deny action** → denial recorded in transcript; agent continues without
  the capability.
- **Connector disconnects** → target shows `offline`; queued actions cancel
  with a transcript note; reconnect resumes (no silent retries on target).
- **Provider misconfigured / flag off** → Screen 5 state; chat composer
  disabled with an operator-docs pointer.
- **SSE drop** → composer stays enabled; partial reply marked; retry action
  offered (no silent message loss).

## Assumptions & Open Questions

- [assumption] "Allow this session" is the deepest consent shortcut —
  per-action or per-session grants only; no "always allow" (cross-session)
  grant in v1, pending the security model in nfr-design.
