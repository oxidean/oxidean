# Bolt Plan — `/oxpilot`

A **Bolt** is one build pass over a piece of the work, ending in
something that runs. This plan is one Bolt per Unit of Work, ordered to
respect the dependency DAG in `unit-of-work-dependency.md` — Bolt
sequencing is the economic decision this stage owns; the engine consumes
the unit DAG itself for runtime batches. Walking skeleton **on**: Bolt 1
is the integrated slice and is checkpoint-approved before any later Bolt
starts.

Construction settings: iteration **unit-major**, execution **serial**,
checkpoints **enabled**. Verification command: `make test` (authorized
2026-10-27) — reused at every Bolt checkpoint; each Bolt's own tests
must be inside it before the checkpoint runs.

## Bolt 1 — `u1-skeleton-slice` [walking skeleton]

Units: U1.
**DoD**: `/oxpilot` page renders; `oxpilot.status` +
`oxpilot.session_create/list` respond over SessionOnly RPC (PAT refused
by closed-set test); a message POST returns a streamed stub reply over
the SSE route; flag-off denies `oxpilot.disabled` everywhere; migrations
×3 + dialect test green; `make test` includes the new tests.
**Confidence hypothesis**: the whole integration shape — domain crate +
wire types + SessionOnly auth + dedicated SSE + Octane route — is real
before breadth is invested on it.
**Expected demo**: open `/oxpilot`, create a session, send a message,
watch a stub reply stream token-by-token; kill the flag, see the
gated error.

## Bolt 2 — `u2-session-core`

Units: U2.
**DoD**: session/message/tool_call/queue/proposal/usage rows persist
across restart; delete cascade covers own rows + `on_session_delete`
hook fires (empty registry → delete still works); per-user scoping
denies cross-user reads; budget counters refuse at cap; redaction-on-write
proven by test.
**Hypothesis**: persistence + budget ownership can stand alone from the
runtime, keeping U3 purely orchestration.
**Demo**: scripted session lifecycle; restart; transcript intact;
over-cap send refused `budget_exceeded`.

## Bolt 3 — `u4-providers`

Units: U4.
**DoD**: OpenAI/Anthropic/Ollama adapters pass wiremock-fixture tests;
`admin.oxpilot.*` settings get/set with masked-key reads; env/DB
override flip rebuilds the provider slot without restart; wrong-key
adapter returns typed `ProviderError`.
**Hypothesis**: three provider APIs can hide behind one streaming trait
with no per-provider code in the runtime.
**Demo**: flip provider in settings; the stub's replacement answers a
real (fixture) stream.

## Bolt 4 — `u3-agent-runtime`

Units: U3.
**DoD**: real turn loop — prompt assembly, provider stream → SSE frames,
tool-call dispatch seam, terminal frames on done/error/abort,
one-active-turn enforcement, queued-message drain, snapshot-resume
replay, abort-on-delete via C8 hook.
**Hypothesis**: the turn state machine holds under real streaming
timing, mid-stream failure, reconnect, and deletion races.
**Demo**: multi-turn chat against the OpenAI fixture; kill the fixture
mid-stream → terminal error frame, session preserved; reconnect →
snapshot-replace + append, no dupes.

## Bolt 5 — `u6-connector-gateway`

Units: U6.
**DoD**: pairing flow (single-use code, shown once); target registry +
liveness; Grant state machine incl. lazy expiry + cancel propagation in
claim/heartbeat responses; the four connector HTTP routes auth via
hashed verifier; consent lifecycle audit rows; registers its C8 revoke
hook.
**Hypothesis**: consent state can be owned server-side so no agent path
ever bypasses a grant check.
**Demo**: pair a fake connector (fixture HTTP client); grant → claim →
cancel mid-action → revoked action dies.

## Bolt 6 — `u5-oxpilot-tools`

Units: U5.
**DoD**: repo read tools + actions read-only tools + ProposedDiff
create/view + `diff_apply` materializes branch+PR with provenance line;
exec/fs tools dispatch through C7 only; write tools emit audit rows;
identity-bound `ToolCtx` enforced.
**Hypothesis**: the builder/debugger value lands entirely behind the
tool trait — no runtime changes needed to grow the catalog.
**Demo**: agent proposes a diff in chat; user confirms; real branch+PR
appears with the session provenance line.

## Bolt 7 — `u7-connector-client`

Units: U7.
**DoD**: `ox pilot connect/pair/disconnect` subcommands; pairing stores
verifier user-side only; claim→execute→report loop with documented
backoff; sandbox honors granted caps; cancel honored before and during
execution; outbound HTTP fixture tests.
**Hypothesis**: a fully outbound connector can deliver real exec/fs
actions with no inbound listener.
**Demo**: `ox pilot connect` on a scratch dir against a compose-spun
api; an action executes and reports; revoke → loop exits.

## Bolt 8 — `u8-oxpilot-ui`

Units: U8.
**DoD**: full index + workspace (rail/transcript/drawer), all cards
(tool-call, diff, consent, queued), dialogs + a11y items, SSE client
snapshot-resume, flag-off state, route-coverage manifest rows +
browser/integration test evidence per covered route.
**Hypothesis**: the composite UX works as designed — consent moments are
legible, stream states recover, and nothing strands the user.
**Demo**: full walkthrough — create session, chat, queue a follow-up,
pair a target, grant an action, watch the consent card drive the run,
propose+apply a diff, delete the session.
