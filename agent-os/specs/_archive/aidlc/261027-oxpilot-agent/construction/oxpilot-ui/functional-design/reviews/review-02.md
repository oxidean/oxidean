### Architecture Review — oxpilot-ui functional-design (iteration 2)

### Review

**Reviewer:** aidlc-architecture-reviewer-agent
**Verdict:** READY

**Iteration:** 2

### Prior findings

| ID | Severity | Location | Finding | Required action | Status |
|----|----------|----------|---------|-----------------|--------|
| R-01 | Major | functional-spec.md:196-246 (WF5/WF6), functional-design-questions.md:250-274 (Q7-A), traceability.json:205 (AC4.3.2 claim) | Consent and diff cards were unrecoverable after the turn ended — card/tool frames live only in the turn's emitted_deltas buffer and no proc could rebuild them session-scoped. | Pin a mount-time recovery read for pending consent/proposal cards, or explicitly document the post-terminal remount limitation. | Resolved |
| R-02 | Major | rules.md:136-141 vs functional-spec.md:284-291, frontend-components.md:114 | BR7.2 pinned "step 2 polls target_get" but the client never holds a target_id at that point — unimplementable. | Change BR7.2 to target_list-by-pairing_id and disambiguate the components-doc API row. | Resolved |
| R-03 | Major | functional-spec.md:293-298 (WF8.4), 121-128 (WF1), frontend-components.md:47, rules.md:19-22 | pending_consent targets were a UI dead end — no surface called grant_respond for a pending connect grant. | Pin the pending-consent affordance or explicitly scope the dead end out. | Resolved |
| R-04 | Major | functional-spec.md:390-397, rules.md:60-64 (BR3.3), functional-spec.md:341-346 (WF11), functional-design-questions.md:153-176 (Q3-A) | session_create.target_id was a write-only seam — no proc returned the binding, and the migration owner contradicted itself. | Extend the amendment to the read side and reconcile single-line migration ownership. | Resolved |
| R-05 | Major | functional-spec.md:269-276 (WF8.1), frontend-components.md:49 vs rules.md:128-131 (BR7.1) | The connect dialog's local/remote RadioGroup was a dead control — no wire field exists to send or render. | Drop the radio with a recorded deviation, or flag a C1 amendment; make all docs agree. | Resolved |
| R-06 | Major | functional-spec.md:196-246, frontend-components.md:54 vs interaction-spec.md:113 | The ix scroll contract (at-bottom-only autoscroll + jump-to-latest pill) was consumed but dropped — naive implementation scroll-hijacks on every frame. | Pin the ix scroll contract on OxpilotTranscript. | Resolved |
| R-07 | Minor | functional-spec.md:214, 227, 427-429 | Frame union gained tool/card variants without pinning the type:"tool_call" discriminant literal; the lib OxpilotMessage lacked tool_calls for the snapshot merge. | Pin type:"tool_call" verbatim and add tool_calls to the stream-lib message type. | Resolved |
| R-08 | Minor | functional-spec.md:494-496, rules.md:184-186, apps/web/src/routes/oxpilot.$session.tsrx:247-253 | The Ctrl/Cmd+Enter aria-describedby hint advertised a shortcut no handler implements; the "Enter submits" claim was false for the shipped Textarea. | Pin a Ctrl/Cmd+Enter submit handler or drop the hint; correct the Enter claim. | Resolved |
| R-09 | Minor | frontend-components.md:51, functional-spec.md:154-166 (WF3), 174-184 (WF4.2), 135-139 (WF2) | Seed shape omitted activeTurn/pendingQueue/cards; session_get never pinned as mount read though header/drawer need it; branch Select pinned no options proc. | Extend the Seed shape, pin session_get mount read, pin the branch options source. | Resolved |
| R-10 | Minor | functional-spec.md:259-264, 373-374, frontend-components.md:57 | ConsentCard outcome enum covered five states but U6 BR3.1's revoke-all produces a sixth (revoked) with no outcome line. | Map revoked to a frozen outcome line. | Resolved |
| R-11 | Minor | functional-spec.md:471-472, 196-218 (WF5), apps/web/src/lib/oxpilot-stream.ts:185-199 | Post-attach non-2xx had no path — no es.onerror, EventSource retrying indefinitely with no UI signal, contradicting BR9.1. | Pin a fatal-error close path or record the limitation. | Resolved |
| R-12 | Minor | functional-design-questions.md:233-235 (Q6-A), functional-spec.md:413-415 vs mockups.md:237-239, interaction-spec.md:165-167 | The inline queued-message cancel was mislabeled "optional" — M8 and ix both pin it. | Correct the framing; propose queue_delete or record an explicit deferral rationale. | Resolved |
| R-13 | Minor | functional-spec.md:123, 297-298, 368-370, rules.md:19-22 | The "pairing" presentation state cannot exist — target rows mint at /pair redemption already pending_consent. | Pin the lifecycle-to-presentation mapping and drop the impossible pre-pair pairing row state. | Resolved |
| R-14 | Nit | rules.md:19, 131 | device_label is not a wire field — OxpilotTargetView carries label; the Input binds target_name. | Rename to label and target_name per the wire. | Resolved |
| R-15 | Nit | traceability.json:4-97 | upstream_ids claimed AC1.5.x and AC4.4.x rows but omitted parent stories US1.5/US4.4, and omitted NFR4 though WF12 cites it. | Add US1.5, US4.4, and NFR4 to upstream_ids or drop the claims. | Resolved |
| R-16 | Nit | functional-spec.md:124 vs mockups.md:49 | M1's Manage-expander rename affordance was dropped without a recorded deviation (unlike the queue-cancel gap). | Add a one-line deviation note in the contract-deltas section. | Resolved |

### New findings

| ID | Severity | Location | Finding | Required action | Status |
|----|----------|----------|---------|-----------------|--------|
| N-01 | Nit | functional-spec.md:406-409 (WF11.2), frontend-components.md:60 vs inception/refined-mockups/mockups.md:85 | M2's context drawer pins "Repo: owner/repo · branch · HEAD sha"; the spec's drawer carries owner/name · branch but no HEAD sha — no proc returns one and, unlike the rename and local/remote drops, the deviation is not recorded in Contract deltas. | Add a one-line M2 deviation note for the dropped HEAD sha (same treatment the R-16 fix gave rename), or pin a source for it. | Open |
| N-02 | Nit | functional-spec.md:463-465 (C1 consumed list), 508-511 (ix resolution list), frontend-components.md:119 vs interaction-spec.md:216,245 | target_get is listed under "C1 consumed verbatim" but no surface calls it — the R-02 fix moved the pairing poll to target_list-by-pairing_id, and the interaction-spec's pinned target_get poll substitution is not recorded in the "[ix] placeholder procs resolved" list; usage_get got an "unread" note, target_get did not. | Give target_get the same unread-proc note as usage_get and add the poll-proc substitution (ix target_get poll → target_list matched on pairing_id) to the ix resolution list. | Open |
| N-03 | Nit | rules.md:66 (BR3.2), rules.md:90-92 (BR4.2) vs functional-spec.md:230-233 (WF5.2), functional-design-questions.md:211-217 (Q5-A) | BR3.2 says "every snapshot frame performs a wholesale setSeed(version+1) keyed remount" and BR4.2 says "snapshot = wholesale replace + remount" — both omit the pinned first-snapshot skip (the attach handler skips the frame the seed already rendered); a literal read double-reseeds on attach. | Qualify both rule summaries with "every snapshot after the attach-time first". | Open |
| N-04 | Nit | rules.md:265-269 (Notes) vs functional-spec.md:573-600 (Negative paths) | The Notes promise "if an amendment is refused the dependent pin degrades per the negative-path section" but the negative-path section has no arm for a refused grant_list{session_id} — amendments 1 and 3 name their refusal degradations, 2 does not; also target_list's pairing_id is grouped with pending amendments though it is already generated in OxpilotTargetView (landed, not pending). | Name amendment 2's refusal degradation (the post-terminal remount limitation returns) and drop pairing_id from the pending-amendment grouping. | Open |

### Verification notes

- R-01 fix verified end-to-end: WF3.3 pairs the snapshot with grant_list{session_id} (flagged amendment 2) plus diff_list{session_id}; WF6/BR5.5/Q7-A describe pending-or-frozen reconstruction; the Seed shape carries cards. Checked api-client: OxpilotGrantListRequest takes only target_id (amendment description accurate), OxpilotDiffListRequest already takes session_id, OxpilotGrantView carries kind/capability/args_summary/state/expires_at sufficient for card reconstruction, and U6 BR3.2's connect grants are session_id NULL so a session filter returns exactly the action grants consent cards need. emit_frame_drops_after_terminal (crates/oxidean-api-oxpilot/src/runtime.rs:1506) confirms the premise.
- R-04 fix verified: amendment 1 now covers both write (session_create.target_id) and read (session_get/session_list.computer_use_target_id); ownership resolved to U8's codegen migration 0090_oxpilot_session_target.sql — verified 0089 is the latest migration so 0090 is free, the dir layout is migrations/{mysql,postgres,sqlite}, no existing migration adds the column, and no other unit's functional-design claims it. U1's deferral text (skeleton-slice entities.md:118, functional-spec.md:144) matches the spec's citation.
- R-02 fix verified: BR7.2/WF8.3/Q10-A poll target_list matched on pairing_id; OxpilotTargetView carries pairing_id (api-client line 5474); the frontend-components API table now pins targetList for the poll.
- R-07 fix verified: type:"tool_call" pinned verbatim at functional-spec.md:244, 265, 497 and rules.md:93; runtime.rs:90 emits "type": "tool_call". The questions doc's prose "tool frames" matches the C2 yaml frame-key name tool (contract-summary.md:117), so it is consistent naming, not a residual defect.
- R-09 fix verified: Seed shape {version, messages, streamingIds, activeTurn, pendingQueue, toolCalls, cards, session} is identical across spec (533-535) and components (51-52); session_get pinned as the mount read; branch options pinned to repo.refs{owner,name} — verified repo.refs takes RepoGetRequest{owner,name} (api-client lines 6155, 7867-7874) and M3 (mockups.md:113) does pin a Select.
- R-12 fix verified: "required-but-unprocced" framing at functional-spec.md:80-83, 296-299 and Q6-A; queue_delete is the third flagged amendment in Contract deltas item 3, rules.md Notes, and the traceability reverse row.
- R-15 fix verified: upstream_ids now include US1.5, US4.4, NFR4 with UI-half coverage rows; JSON parses and coverage/upstream sets are symmetric.
- Traceability revalidated: all 84 OK + 11 N/A coverage ids match upstream_ids exactly; the three flagged amendments are internally consistent across questions, spec, rules, and traceability.

### Findings

| ID | Severity | Location | Finding | Required action | Status |
|---|---|---|---|---|---|
| - | - | - | No findings | No action required | Resolved |
