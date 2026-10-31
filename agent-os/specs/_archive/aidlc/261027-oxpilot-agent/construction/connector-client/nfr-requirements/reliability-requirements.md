# Reliability Requirements — connector-client (U7)

Consumes: functional-design `rules.md` BR2/BR4/BR5/BR8, `entities.md`
(RunState), `functional-spec.md` (WF2, WF3, negative paths), questions
Q4-A, requirements.md NFR4 (the client IS the degrade mechanism).

## Graceful degradation — the code-keyed taxonomy

| id | requirement | basis |
|----|-------------|-------|
| NFR4.1 | Transport errors (connect refused, reset, TLS, 5xx without hint), `disabled`, `internal`, and unknown codes → exponential backoff 1s → ×2 → 60s ceiling, ±20% jitter, reset on first success; the loop stays ALIVE — a dead server/feature flap must never kill the connector process (supervisors read aliveness as "still trying") | BR8.2, BR2.3, Q4-A |
| NFR4.2 | `rate_limited` → honor `Retry-After` exactly, then continue; never counted as a transport failure | BR2.3, BR8.1 |
| NFR4.3 | `unauthorized` + off-pair `forbidden` → auth death: kill in-flight child FIRST (process-group kill), NO report attempted (verifier already dead server-side), delete creds, print re-pair hint, exit non-zero | BR8.3, N-04 |
| NFR4.4 | BR8.4 terminals — heartbeat `revoked:true`, `revoked` on claim/report, `not_found` on claim/heartbeat → kill in-flight + report `cancelled:revoked`, delete creds, exit 0 (revocation is success; exit-0 prevents supervisor crash-loops) | BR8.4, FR5.6 |
| NFR4.5 | `bad_input` → kill in-flight, exit non-zero, creds PRESERVED (a client-side wire bug ≠ dead credential — the asymmetry is pinned); report-404 → drop the action and continue (the action row is gone but the identity resolved — creds live) | BR2.3, WF3.5, N-01 |
| NFR4.6 | SIGINT/SIGTERM → kill in-flight child (process-group), report `cancelled:<sigint\|sigterm>`, exit 0 — no `running` row strands server-side | BR4.5, R-08 |

## Idempotency & exactly-once semantics

| id | requirement | basis |
|----|-------------|-------|
| NFR4.7 | Reports post idempotently by `action_id` — retries repost the identical body; a duplicated report is a safe no-op server-side | BR7.4, C3 |
| NFR4.8 | Every cancel id from claim/heartbeat is closed by a report EXACTLY ONCE, including ids the client never claimed (a claim response lost in transit strands a `cancel_requested` row only the client can close): unclaimed ids report `cancelled:pre-start` without executing; `RunState.settled` dedupes re-announced ids | BR5.1, RunState, U6-BR5.3 |
| NFR4.9 | Mid-flight cancel kills the process group (`killpg`/`taskkill /T`) then reports `cancelled:mid-flight`; fs_* cancel granularity is between ops (the non-regular refusal bounds every op to a finite regular file) | BR5.2, BR4.3 |
| NFR4.10 | Credentials and identity rows delete only on defined terminals — a crash between "row deleted, verifier not" leaves a recoverable state (zombie row named by BR9.3), never a silent credential leak | BR9.3, NFR1.3 |

## Serial-execution integrity

| id | requirement | basis |
|----|-------------|-------|
| NFR4.11 | Actions execute SERIALLY in claim order; a report posts before the next action starts — no parallelism to interleave or starve | BR4.1, Q7-A |
| NFR4.12 | `exec_shell` has `EXEC_TIMEOUT` 300s default (`--exec-timeout` override) — a hung child can't pin the serial loop; timeout reports `failed` with reason | BR7.3, Pins |
