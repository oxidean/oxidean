# Memory — Domain Design (Inception)

- Mode inline (aws-platform + design perspectives folded in); architect lead.
- Questions: 5 (decomposition, provider slot, connector transport, client packaging, grant ownership) — all conductor-decided, summary confirmed.
- Eight components: OxpilotWeb, OxpilotApi, SessionService, AgentRuntime, OxpilotTools, LlmProviderRegistry, ConnectorGateway, OxpilotConnector. 10 ADRs.
- Architecture review: NOT-READY, 5 majors + 6 minors — ALL applied post-verdict (advisory single-pass):
  R-01 connector auth → dedicated HTTP routes + gateway-owned verifier check; R-02 new Action entity + cancel set in claim/heartbeat; R-03 Grant session_id nullable (connect grants) + expires_at lazy expiry + cancelled state; R-04 ADR-009 UsageCounter entity (SessionService single owner); R-06 ADR-010 provider pinned per turn + delete aborts before cascade + gateway-audited cascade; R-05 PR materialize rides OxpilotTools seam; R-07 queue drain = runtime on turn end; R-08 ProposedDiff owned by SessionService + ToolCall ref; R-09 OxpilotApi loses direct db dep; R-10 gateway per-user scoping; R-11 eight-component count, frame edge via API, tool-audit owner, actions-read assumption noted.
- Learnings persisted: cancel-propagation-in-poll pattern; connector creds on routes-not-RPC.
- Sensors: required-sections, upstream-coverage, traceability all pass.
