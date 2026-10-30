---
phase: "19"
slug: "actions-runners"
status: verified
# threats_open = count of OPEN threats at or above workflow.security_block_on severity (the blocking gate)
threats_open: 0
asvs_level: 1
created: "2026-09-16"
---

# Phase 19 — Security

> Per-phase security contract: threat register, accepted risks, and audit trail.

---

## Trust Boundaries

| Boundary | Description | Data Crossing |
|----------|-------------|---------------|
| Runner ↔ `/api/actions` | Registration + job protocol; no session cookies | Runner token hash; labels; job payload + secrets |
| Admin ↔ registration tokens | Sys-admin mint/rotate; one-time plaintext display | Raw token once → DB hash |
| Repo Admin ↔ Actions secrets | Write-only values; list names only | AES-GCM ciphertext at rest (`OXIDEAN_ACTIONS_SECRETS_KEY`) |
| Operator ↔ Compose runner | Sidecar/host Docker; isolate from API process | Registration env token placeholders |
| Reader ↔ Actions UI | Repo Read sees runs/logs; Actions tab visible | Run metadata; no secret values |

---

## Threat Register

| Threat ID | Category | Component | Severity | Disposition | Mitigation | Status |
|-----------|----------|-----------|----------|-------------|------------|--------|
| T-19-01 | Spoofing | `/api/actions` Cookie | high | mitigate | Ignore Cookie; runner token auth only | closed |
| T-19-02 | Elevation of privilege | Job execution | high | mitigate | Enqueue-only dispatch; no in-process executor | closed |
| T-19-03 | Information Disclosure | Private runs | medium | mitigate | `resolve_repo_for_read` + soft-404 | closed |
| T-19-04 | Information Disclosure | Secrets at rest | high | mitigate | AES-GCM ciphertext column; no plaintext | closed |
| T-19-05 | Tampering | Log paths | medium | mitigate | Reject `/`, `\`, `..` in run/job IDs | closed |
| T-19-06 | Denial of Service | Workflow parse | medium | mitigate | `MAX_WORKFLOW_BYTES` + path escape | closed |
| T-19-07 | Tampering | Workflow discovery | medium | mitigate | `.yml/.yaml` under `.github/workflows/` only | closed |
| T-19-08 | Elevation of privilege | Job claim | high | mitigate | Registered runner + label match claim | closed |
| T-19-09 | Denial of Service | Push notify | medium | mitigate | Async `notify_push_actions` after receive-pack | closed |
| T-19-10 | Spoofing | Runner routes | high | mitigate | Same as T-19-01 — Cookie discarded | closed |
| T-19-11 | Spoofing | Registration | critical | mitigate | Hashed registration tokens; accept/bootstrap | closed |
| T-19-12 | Elevation of privilege | Token scope | high | mitigate | Instance-scoped Admin mint; pickup = labels | closed |
| T-19-13 | Spoofing | PR triggers | medium | mitigate | Internal events/hooks only; no public trigger route | closed |
| T-19-14 | Information Disclosure | Dispatch SHA | medium | mitigate | Trusted internal payload (`head_sha` + bare repo) | closed |
| T-19-15 | Tampering | Commit statuses | high | mitigate | Actions writes via `statuses::publish_*` only | closed |
| T-19-16 | Spoofing | Status context | medium | mitigate | Server-built `status_context(workflow, job)` | closed |
| T-19-17 | Spoofing | Compose token | high | mitigate | Env placeholder; docs never-commit | closed |
| T-19-18 | Elevation of privilege | Runner Docker socket | medium | accept | Isolate runner from API; documented accept | closed |
| T-19-19 | Information Disclosure | Run/log RPC | medium | mitigate | Read ACL + soft-404 | closed |
| T-19-20 | Elevation of privilege | Actions tab visibility | low | accept | Tab visible with Read (plan accept) | closed |
| T-19-21 | Information Disclosure | Secret GET | critical | mitigate | Names-only list; Admin write; never echo | closed |
| T-19-22 | Elevation of privilege | Registration mint | high | mitigate | `require_sys_admin` + hash storage | closed |
| T-19-23 | Elevation of privilege | Managed minutes | high | mitigate | No cloud executor / managed_minutes path | closed |
| T-19-24 | Information Disclosure | Docs examples | medium | mitigate | Placeholder tokens in docs/README | closed |
| T-19-SC | Tampering | New crates | high | mitigate | `serde_yaml` / `prost` / `aes-gcm` [VERIFIED] | closed |

*Status: open · closed · open — below high threshold (non-blocking)*
*Severity: critical > high > medium > low — only open threats at or above workflow.security_block_on count toward threats_open*
*Disposition: mitigate (implementation required) · accept (documented risk) · transfer (third-party)*

---

## Accepted Risks Log

| Risk ID | Threat Ref | Rationale | Accepted By | Date |
|---------|------------|-----------|-------------|------|
| AR-19-18 | T-19-18 | Official runner may mount a Docker socket for job containers. Operators must isolate the runner host/network from the API process (documented in `docker/oxidean-runner/README.md`). Not a forge-hosted executor. | plan disposition + secure-phase audit | 2026-09-16 |
| AR-19-20 | T-19-20 | Actions chrome tab is visible to users with repo Read (metadata/logs only). Secret values and Admin runner controls remain gated. Matches D-ACT visibility intent. | plan disposition + secure-phase audit | 2026-09-16 |

*Accepted risks do not resurface in future audit runs.*

---

## Security Audit Trail

| Audit Date | Threats Total | Closed | Open | Run By |
|------------|---------------|--------|------|--------|
| 2026-09-16 | 25 | 25 | 0 | gsd-security-auditor (ASVS L1) |

### Security Audit 2026-09-16

| Metric | Count |
|--------|-------|
| Threats found | 25 |
| Closed | 25 (23 mitigate + 2 accept) |
| Open (blocking ≥ high) | 0 |
| Accepted documented | 2 (T-19-18, T-19-20) |

Evidence highlights: `runner_proto.rs` Cookie ignore + token auth; `secrets.rs` AES-GCM write-only; `actions_secrets` / `actions_dispatch_policy` / `actions_runner_protocol_*` nextest; Admin registration mint; no managed-minutes paths; Compose/docs placeholders.

---

## Sign-Off

- [x] All threats have a disposition (mitigate / accept / transfer)
- [x] Accepted risks documented in Accepted Risks Log
- [x] `threats_open: 0` confirmed
- [x] `status: verified` set in frontmatter

**Approval:** verified 2026-09-16 (secure-phase auditor → orchestrator write)
