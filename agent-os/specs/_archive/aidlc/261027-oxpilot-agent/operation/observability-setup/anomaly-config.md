# Anomaly Config — oxpilot

## Sources

Consumes per-unit `nfr-design/performance-design.md` (**performance-design**), `nfr-design/security-design.md` (**security-design**), `nfr-design/reliability-design.md` (**reliability-design**), `infrastructure-design/monitoring-design.md` (**monitoring-design**), and `infrastructure-design/infrastructure-specification.md` (**infrastructure-specification**).
No dedicated anomaly-detection service exists; anomaly signals are log-derived (see `alarms.md` thresholds).

## Highest-value anomalies for oxpilot

- **Consent-denial flood** — a spike in `exec_shell`/`fs_*` denials on one target may indicate a confused/compromised connector client.
- **Tool-round saturation** — turns repeatedly hitting the 8-round cap suggest prompt/tool-catalog drift.
- **Queue growth** — `message_queue` backlog growing without drains means the drainer is wedged (single-replica caveat).
- **Streaming-sweep residue** — `oxpilot_streaming_messages` rows surviving past sweep cadence indicates stuck writes.

These are log-query patterns for operators, not automated alerts — consistent with NFR6's reuse-only posture.
