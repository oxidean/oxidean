**Collaborator:** aidlc-design-agent

## Contribution

Lens: UX and persona fidelity — personas as real users, story coverage of
the decided wireframe surfaces, consent/moment-of-truth UX, and
accessibility carried to acceptance criteria.

### Personas verdict

Personas pass the fidelity bar. Each has goals, pain points, and context —
not a role-by-name. P2 Casey's "same human, different hat" framing is the
honest split (consent context differs even when the person doesn't), and
"reads consent dialogs carefully" is exactly the trait that should drive
AC-level consent detail. P3 Quinn's pain points (undocumented env vars,
forced cloud dependency) map cleanly to US5.1 ACs. Two issues:

- **P4 Riley is decorative.** "Attributable author, clean diff, honest
  description of what the agent did, no hidden tool provenance" — none of
  this appears in any AC. US2.2 covers the mechanics (branch+PR, link,
  no-auto-push) but never the properties Riley cares about. Either encode
  attribution/provenance as US2.2 ACs or the persona earns nothing.
- Minor nit (non-blocking): pronoun use is inconsistent (Dana "she",
  Quinn "he", Casey/Riley none) — cosmetic.

### Surface coverage vs wireframes (Screens 1/2/2b/3/4/5)

| Surface | Covered? | Gap |
|---|---|---|
| Screen 1 index — session list | Yes (US1.1) | AC1.1.2 says "repository, branch, status, relative time" — FR1.2/wireframe say **title** + repo + status + last activity. Title is the row's primary label and its source (derived from goal?) is unspecified. Also no AC for the promised empty state ("No sessions yet — describe a task to get started."). |
| Screen 1 index — Computer use target manager | **No** | FR1.4: the index hosts the target manager (list, connect, disconnect, revoke) linked from `/settings/ai`; wireframe shows status dot, local/remote type, [Manage]/[Revoke] per row, [Connect a system] button. US4.1/US4.4 ACs are all backend semantics — no AC places any of this on a surface. The `/settings/ai` → `/oxpilot` link is also un-AC'd. |
| Screen 2 workspace | Mostly (US1.3) | Rail/transcript/drawer + aria-live covered (AC1.3.2/1.3.4 — good). Missing: tool-call transparency cards (wireframe shows `tool: read run logs` / `repo.readFile` cards inline — the visibility-of-agent-action pattern Casey and Riley both depend on); streaming-in-progress indicator; session-header connected-target line; landmarks/heading order (wireframe a11y note: `nav` rail, `main` transcript, `complementary` drawer, h1 = session title) per FR2.5/NFR7. |
| Screen 2b new-session dialog | Mostly (US1.2) | Elements covered; **dialog a11y absent**: labelled dialog, focus-trapped, Escape cancels, focus returns to trigger, goal field is initial focus, labelled comboboxes (wireframe a11y note). Also the target picker is a RadioGroup defaulting to "none" — safe-default AC worth stating, and RadioGroup is a high-risk control needing browser-coverage evidence per team practices. |
| Screen 3 connect/consent dialog | **Barely** | US4.1 ACs are crypto/transport only; US4.2 AC4.2.1 says consent names granted capabilities but nothing captures the dialog: labelled + focus-trapped + Escape, capabilities shown **before** activation (FR5.2), the "I understand the agent will act on this system under my consent" acknowledgment gating [Connect], and the token in a read-only field with copy button **never logged to the transcript**. |
| Screen 4 per-action consent card | **Barely** | AC4.2.2 encodes the grant semantics (per-action/per-session, no always-allow) but the moment-of-truth card is un-AC'd: it appears **in-transcript**, names the exact command, target, working dir, timeout; offers Allow once / Allow this session / Deny; **denial is always available and narrated in the transcript** (wireframe a11y note). US4.2 conflates connect-time consent (Screen 3) with per-action consent (Screen 4) — these are two distinct UX moments. |
| Screen 5 disabled state | Yes (US5.1 AC5.1.1) | Matches FR1.5/NFR8 — explanatory, names env vars, points at docs. |

### Other UX gaps

- **Destructive-action asymmetry.** US1.5 deletes a session + transcript
  irreversibly with no confirmation AC — violates the error-prevention
  heuristic. Conversely US4.4 revoke should stay **low-friction** (Casey:
  "access ends the moment I say so") — no blocking confirm that delays
  effect. State both deliberately.
- **Connector degradation UX.** NFR4: "connector disconnects degrade to
  chat-only gracefully." No AC covers what Casey sees — rail status flip,
  in-session notice, pending action requests resolving. US4.5 shows status
  but not the degradation behavior.
- **Over-budget refusal UX.** AC5.2.2 "clear refusal" — sharpen per NFR8:
  rendered in-session, naming the env var that raises the budget.
- **Should-tier drop.** Scope Should list includes "queued/follow-up
  messages" — no story exists. Q4 said mirror MoSCoW exactly; this is
  either an oversight or an unrecorded cut.
- **Transcript authorship.** "You — 14:02" / "OxPilot — 14:02" attribution
  per message is wireframe-promised and cheap to AC.

### What works

US1.3 AC1.3.4 (aria-live, labelled composer, real buttons) is the right
shape — more of that, please. US4.x ordering encodes consent-before-action
structurally. AC5.1.1 nails the disabled-state contract. MoSCoW otherwise
mirrors scope. INVEST granularity is right for construction slicing.

## Positions

**Stances**

1. **Approve personas** with the P4 fix and pronoun nit — the four-persona
   split is correct and each is a real user.
2. **Conditional-approve stories.** Core loop is sound, but the
   differentiating feature's UX (consent dialogs, target manager surface)
   is the least-specified part — backwards for a consent-driven feature.
   The gaps above are AC-level additions to existing stories, not new scope.

**Objections**

- **OBJ-1 (blocking):** The index's computer-use target manager — a decided
  surface per FR1.4 + Screen 1 — has zero ACs. No story says where Connect,
  Manage, Revoke, status, or the `/settings/ai` link live. Add ACs to
  US4.1/US4.4/US4.5 or a US4.x surface story.
- **OBJ-2 (blocking):** Consent UX is underspecified at the exact moment it
  matters. Screen 3 (connect-time: capabilities pre-activation,
  acknowledgment gate, token never in transcript, labelled/focus-trapped/
  Escape) and Screen 4 (per-action: in-transcript card with command + dir +
  timeout, Allow once / Allow session / Deny, denial narrated) each need
  ACs — and US4.2 conflates the two moments. For a feature whose entire
  safety story is informed consent, "requires explicit consent naming the
  granted capabilities" is not enough to build or test against.
- **OBJ-3 (blocking):** Dialog + landmark accessibility from FR2.5/NFR7 and
  the wireframes' own a11y notes are missing outside US1.3. Add ACs:
  Screen 2b focus-trap/Escape/initial-focus/labelled selects; workspace
  landmarks + h1; consent card buttons in tab order.
- **OBJ-4:** P4 Riley's goals are unmet by every AC. Add to US2.2: the PR
  is attributed to the requesting user **and** its body honestly describes
  agent provenance (session link / "proposed by OxPilot") — or drop the
  persona. This needs a product decision, not silence.
- **OBJ-5:** Screen 1 session rows must show **title** per FR1.2/wireframe —
  AC1.1.2 swapped it for branch. Add the promised empty-state copy AC too.
- **OBJ-6:** Tool-call transparency cards (every agent action visible
  in-transcript) are wireframe-promised and serve Casey's audit need and
  Riley's provenance need — AC them on US1.3/US4.3.
- **OBJ-7 (process):** Should-tier "queued/follow-up messages" vanished
  despite Q4's mirror-exactly decision — restore as a [Should] story or
  record the cut explicitly.

**Must-resolve questions**

1. Does US4.2 cover both consent moments (connect-time Screen 3 and
   per-action Screen 4), or split into two stories? Current ACs blur them.
2. What does the index's [Manage] button open in v1 — and do
   revoke/disconnect live on the index only, or also in the session rail?
3. PR body provenance: does a materialized PR disclose agent involvement
   (session link, honest description) while attributed to the user? Riley
   requires honesty; this is a product call that must land in ACs.
4. Delete-session (US1.5): confirmation dialog, undo, or none? And revoke
   (US4.4): confirm the no-confirm low-friction stance.
5. Is "queued/follow-up messages" an intentional cut or an oversight?
6. Session title source — derived from the goal field? FR1.2 requires a
   title; Screen 2b collects no title.
