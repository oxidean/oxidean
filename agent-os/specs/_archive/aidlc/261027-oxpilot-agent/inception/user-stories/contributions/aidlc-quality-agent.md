**Collaborator:** aidlc-quality-agent

## Contribution

Testability review of `stories.md` and `personas.md` against the affirmed test
layers in `inception/practices-discovery/team-practices.md` (cargo-nextest +
real SQLite/provisioned dialects, Vitest unit/integration/happy-dom/Chromium
projects, `e2e-stack` `*.stack.test.ts` for SSE, wiremock outbound-HTTP
fixtures for connector/provider, route- and browser-coverage manifests,
`procedures_list_matches_dispatch` + closed-set `proc_classes` pin tests) and
the FR/NFR set in `inception/requirements-analysis/requirements.md`. Findings
ordered roughly by severity.

1. **(blocking — US1.4)** The two hard provider-failure cases are
   unspecified. AC1.4.1 covers a provider error at request time but not the
   mid-stream case: partial tokens emitted, then provider dies — does the
   partial reply persist, is it marked failed, does the SSE terminal frame
   become a typed `oxpilot.<reason>` error event? Without that contract the
   stack test can't assert pass/fail. And "never a silent stall" is
   unverifiable until a bound is pinned — either a server-side timeout value
   or the guarantee "every request produces a terminal SSE frame (reply or
   typed error)". Pin the terminal-frame contract; that is the assertable
   form.
2. **(blocking — US4.3)** Connector-offline is the biggest missing negative
   path. NFR4 requires connector disconnects degrade to chat-only
   gracefully, but no AC covers an exec/fs action attempted against an
   offline or unreachable target → typed error, chat unaffected. AC4.5.1
   shows status; nothing asserts behavior on offline. Add the AC — it binds
   cleanly to an RPC integration test with the connector fixture absent.
3. **(blocking — US4.2)** Consent-denial path absent. AC4.2.1/AC4.2.2 cover
   the grant side only; Casey clicking Deny is unspecified — the agent must
   receive a typed denial, no tool call executes, and (per US5.3's lifecycle
   claim) a denial/decision audit row should exist. Add the negative AC.
4. **(blocking — US4.4)** Revocation mid-action unstated. AC4.4.1 terminates
   *pending* grants; an in-flight exec when revoke lands is the case that
   matters — killed, allowed to complete, result discarded? AC4.4.2 covers
   only new actions. State the in-flight semantics or the fault-injection
   test can't be authored.
5. **(blocking — AC1.6.1)** SSE-resume contract under-specified. "Resumes
   without duplicated messages" doesn't say whether resume means
   continue-from-offset or snapshot-replay-then-dedup, and doesn't assert
   whether generation continues server-side while disconnected (FR2.4
   implies yes). Pin the observable: post-reconnect transcript contains each
   message exactly once, order preserved, including tokens generated during
   the disconnect. Layer binding exists (stack test asserting event-stream
   frames + happy-dom append/dedup) — the AC just needs to state what's
   asserted.
6. **(blocking — AC2.1.2, AC3.1.2)** LLM-content ACs are unverifiable as
   written. "The agent explains the change" and "summarizes the failure"
   grade model output, not a contract — a test cannot assert explanation
   quality. Rewrite to artifact linkage: an assistant message accompanies
   the proposal artifact; a non-empty assistant reply accompanies the
   fetched log artifact. Content quality stays with provider evals, not
   story ACs.
7. **(blocking — US5.1)** Flag-off RPC denial missing. AC5.1.1 asserts the
   rendered disable state but no AC asserts `oxpilot.*` calls with flag off
   or no provider return the typed `oxpilot.disabled` denial the practices
   name. Page render and fail-closed RPC are the pair — add the RPC side
   (integration test, one assertion).
8. **(blocking — cross-cutting)** Per-user isolation negatives are thin
   (NFR2). AC1.1.1 scopes the index to the signed-in user, but no AC asserts
   fetch/delete on another user's session, or connect/act on another user's
   target, is denied. AC1.5.1's "for that user" is ambiguous between
   actor-scope and ownership-denial. Add per-surface denial ACs or one
   blanket "non-owner id → typed denial, rows untouched" AC.
9. **(blocking — dropped FRs)** Four requirements have no AC at all:
   FR7.1's repo read tools (file read, tree, branch, commits — "repo-aware"
   is claimed in AC1.3.x but never verified, and read-only-by-default is
   unasserted); FR7.5's issue/PR read + comment-only-on-own-artifacts (no
   story touches it); NFR1's secrets-pattern redaction on stored transcript
   content (planted-secret → stored row redacted is directly testable and
   this feature uniquely owns the exfiltration edge); and the FR6.2/NFR1
   `SessionOnly` classification — PAT/`ox`/MCP bearer on `oxpilot.*` →
   denied, pinned by the closed-set test requirements already mandates.
10. **(suggestion — AC2.2.3, AC4.1.3, AC3.1.3)** Universal negatives
    ("never pushes/merges", "never returned by any agent-reachable
    surface", "no rerun/cancel control") need structural binding, not
    behavioral phrasing. The repo idiom supports them: the
    `OXPILOT_PROCEDURES` closed-set pin asserts no push/merge/rerun/cancel
    procedure exists, and a source-scan test asserts response DTOs carry no
    secret fields. Restate as "attempt → typed denial" plus "no such
    procedure in PROCEDURES" — both executable; "never" alone is not.
    AC4.1.3 is also three assertions in one line (AES-256-GCM at rest,
    hashed verifier, never-returned) — split or enumerate for atomic
    pass/fail.
11. **(suggestion — AC1.3.1)** "Token-by-token" over-specifies granularity
    — providers batch; the wire contract is incremental SSE frames. Reword
    to "streams incrementally over SSE; terminal frame is the reply or a
    typed error" so the stack test asserts frames, not token count.
12. **(suggestion — US5.2)** "Defaults generous" (AC5.2.1) is unquantified
    taste and AC5.2.2's "clear refusal" names no code. To be testable:
    state the enforced unit (messages and/or tokens), the typed refusal
    code (`oxpilot.budget_exceeded` or whichever), boundary behavior (the
    at-limit request), and which scope tripped (per-user vs per-instance).
    Numbers can defer to design — the assertion shape cannot.
13. **(suggestion — US4.2)** Grant expiry has no negative AC. AC4.2.2 bans
    cross-session "always allow" but nothing asserts a grant actually
    expires: new session on the same target → fresh consent required;
    per-action grant → second action re-prompts. The expiry side is the
    testable part of "per-action or per-session".
14. **(suggestion — US1.2)** Missing negatives and empty states: Start with
    an invalid/inaccessible repo or branch → typed error; repo/branch/target
    selects with zero options → defined empty state. Note the Select
    controls and the dialog itself are high-risk UI — a browser-coverage
    manifest row is same-change mandatory, not optional.
15. **(suggestion — US3.1)** Missing debugger negatives: repo without
    Actions or zero runs → empty result (or typed error — specify); log
    fetch on a missing/expired job → typed error; truncation behavior for
    large logs unstated (caps matter for the wire contract).
16. **(suggestion — US2.2)** Materialize partial failure: branch created
    but PR create fails (name collision, permission) → orphan-branch
    semantics and session state unspecified. Add "PR create failure →
    typed error naming the fix; session shows no PR link" AC.
17. **(suggestion — US1.3/US1.5 races)** Two lifecycle races unstated:
    composer double-submit (send while a reply streams — queued, rejected,
    or interrupts?) and delete-session-while-streaming (terminal SSE
    behavior + which transcript rows persist). Pick the observable
    behavior for each; both are cheap to assert once named.
18. **(suggestion — AC4.5.1)** Online/offline status needs a staleness
    bound to be a pass/fail — when does a dead connector read offline?
    Pin the heartbeat/timeout expectation or mark defer-to-design.
19. **(nit — AC1.1.2)** "Relative time" formatting rules unstated; pin
    "renders a relative timestamp" as the assertion, not a specific
    string.
20. **(nit — coverage promises)** Stories correctly make no coverage-floor
    claims — keep it that way (`oxidean-api-oxpilot` is collect-only per
    affirmed practice; the weighted web gate and change-type minimums
    carry the load). Conversely, note the same-change registration
    obligations the ACs only imply: `dialect_oxpilot.rs` must join the
    `db-dialects` `-E` filter + TESTING.md job table in the same change or
    it is dead coverage, and every route/AC pair ships its manifest rows —
    worth one INVEST-note line so construction units inherit the
    obligation.

## Positions

Endorse — these must survive integration:

- **Story shape and AC idiom overall.** Most ACs are already wire-level and
  bind cleanly: AC1.1.3 → `check-route-sync` triangle + route-coverage
  manifest; AC1.5.1/AC5.3.1 → row-level assertions on real SQLite
  (integration); AC4.3.2/AC5.3.2 → planted-secret redaction probe on
  `audit_events` (the best negative-path ACs in the doc — keep the
  actor/action/time/result field list verbatim); AC5.1.1 → happy-dom text
  assertions; AC5.1.3 → the `admin.mcp.*` env-default + DB-override
  precedent test; AC5.4.1 → wiremock-fixture adapter test, never live
  Ollama. The fixes above are conformance to the doc's own style, not a
  direction change.
- **AC1.4.1's "never a silent stall" phrasing** — right instinct; finding
  #1 asks only for the pinned terminal-frame/timeout bound that makes it
  falsifiable.
- **AC2.2.1's "explicit user action" gate** and **AC4.3.1's embedded
  negative** ("only against a connected target with a live grant") — the
  consent-gated, positive+negative-paired shape the missing ACs should
  copy.
- **US1.6 as a Should** — correct tiering; the assigned SSE test layers
  (stack frames + happy-dom append) make it shippable without inventing
  harness.
- **Personas P1–P4** — sufficient for test binding; P4 rightly needs no
  story (consumes the existing PR surface), and P2's "revoke instantly"
  correctly lands in AC4.4.1 as an ordering guarantee, not a latency
  assertion.
- **No perf ACs** — correct call: no perf harness exists (recorded debt),
  and NFR3's targets are marked aspirational. The only v1 performance
  evidence should stay smoke-level inside existing layers (first SSE frame
  observed in the stack test; session teardown completes) — do not let
  later edits add unmeasurable latency promises to ACs.
