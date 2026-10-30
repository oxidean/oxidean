# Runbooks — oxpilot

## Sources

Consumes `observability-setup/dashboards.md` (**dashboards**), `observability-setup/alarms.md` (**alarms**), per-unit `nfr-design/reliability-design.md` (**reliability-design**) and `nfr-design/security-design.md` (**security-design**), and `infrastructure-design/infrastructure-specification.md` (**infrastructure-specification**).
## RB-1 Provider outage / misconfig

Symptoms: turns terminal `error`, provider-call refusal codes. Steps: verify `OXIDEAN_OXPILOT_PROVIDER`/`MODEL` env; check provider status; switch provider via Railway var + restart; if unrecoverable, `OXIDEAN_OXPILOT_ENABLED=false`.

## RB-2 Connector misuse / consent abuse

Symptoms: unexpected `exec_shell`/`fs_write` on a target, denial spikes. Steps: `oxpilot.target_revoke` for the target → audit `oxpilot_grants` + tool-call rows (ids only) → rotate connector shared secret (Railway var) → invalidate sessions if needed.

## RB-3 Wedged queue / turn

Symptoms: `message_queue` rows not draining; turn stuck `running`. Steps: restart api replica (in-memory turn state clears; transcripts persist) → verify sweep cadence for `oxpilot_streaming_messages` residue → check `drain_and_chain` logs.

## RB-4 Redaction breach (P1)

Symptoms: provider credentials or message content found in logs. Steps: `OXIDEAN_OXPILOT_ENABLED=false` immediately → rotate the leaked credential → file security incident → patch redaction in `oxidean-core` + regression test before re-enable.

## RB-5 Budget misconfig flood

Symptoms: mass `budget_exceeded` refusals. Steps: check `oxpilot_settings` ceilings vs intent; raise or unbind ceilings; verify per-user vs per-instance axis.
