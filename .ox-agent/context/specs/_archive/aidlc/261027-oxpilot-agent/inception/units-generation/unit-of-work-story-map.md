# Unit ↔ Story Map — `/oxpilot`

Every `USx.y` from `inception/user-stories/stories.md` mapped to its
implementing Unit(s). First listed = primary owner; parenthetical =
contributing units (cross-cutting stories land in more than one unit by
design — a story ends in the UI but its data path lives server-side).
In-unit order suggests reading order only; Delivery Planning sequences.

## US1 — Sessions & chat

| Story | Units | Note |
|---|---|---|
| US1.1 | U8 `u8-oxpilot-ui` (U2) | index page over session list proc |
| US1.2 | U8 `u8-oxpilot-ui` (U2, U6) | dialog + bind; optional target pick reads U6 state |
| US1.3 | U3 `u3-agent-runtime` (U1, U2, U8) | the core loop; U1 skeleton already proves the pipe |
| US1.4 | U3 `u3-agent-runtime` (U8) | terminal frames + visible error |
| US1.5 | U2 `u2-session-core` (U6) | cascade incl. gateway revoke entrypoint |
| US1.6 | U3 `u3-agent-runtime` (U8) | snapshot-resume replay |
| US1.7 | U8 `u8-oxpilot-ui` (U6) | target manager surfaces |
| US1.8 | U2 `u2-session-core` (U3, U8) | queue rows + drain + pending chip |

## US2 — Builder

| Story | Units | Note |
|---|---|---|
| US2.1 | U5 `u5-oxpilot-tools` (U3, U8) | proposediff rows + diff card |
| US2.2 | U5 `u5-oxpilot-tools` | branch+PR materialization + provenance |
| US2.3 | U8 `u8-oxpilot-ui` (U5) | plain patch view |

## US3 — Debugger

| Story | Units | Note |
|---|---|---|
| US3.1 | U5 `u5-oxpilot-tools` (U3) | actions read-only tool surface |
| US3.2 | U5 `u5-oxpilot-tools` (U3) | fix proposal → builder flow |

## US4 — Computer use

| Story | Units | Note |
|---|---|---|
| US4.1 | U6 `u6-connector-gateway` (U8) | pairing + target rows |
| US4.2 | U6 `u6-connector-gateway` (U8) | connect-grant consent |
| US4.3 | U7 `u7-connector-client` (U5, U6) | exec/fs tools dispatch via U5 → U6 queue → U7 runs them |
| US4.4 | U6 `u6-connector-gateway` | revoke + cancel propagation |
| US4.5 | U8 `u8-oxpilot-ui` (U6) | status chips rail + index |
| US4.6 | U7 `u7-connector-client` (U6) | `ox pilot` client + dial-out |

## US5 — Operator & platform

| Story | Units | Note |
|---|---|---|
| US5.1 | U4 `u4-providers` (U1) | flag + `admin.oxpilot.*` + provider config |
| US5.2 | U2 `u2-session-core` (U4) | usage counters + refusal |
| US5.3 | U6 `u6-connector-gateway` (U5) | audit rows — consent U6, write-tools U5 |
| US5.4 | U4 `u4-providers` | Ollama adapter (self-host) |
| US5.5 | U1 `u1-skeleton-slice` (U2, U5, U6) | crate + SessionOnly pin + registration; ACs span dialect (U2), tool perms (U5), connector routes (U6) |

## Coverage

- All 24 stories assigned; every unit owns ≥1 story (U1 via US5.5 and the
  skeleton it contributes to US1.3).
- Cross-cutting stories (spanning ≥3 units): US1.3, US1.8 — chat and
  queue touch persistence, runtime, and UI by design.
