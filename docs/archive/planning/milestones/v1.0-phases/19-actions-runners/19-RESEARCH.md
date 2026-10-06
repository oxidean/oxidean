# Phase 19: Actions & Runners - Research

**Researched:** 2026-09-16  
**Domain:** GitHub Actions–compatible workflows on self-hosted forges; act_runner / Gitea Actions registration protocol; commit statuses for branch protection  
**Confidence:** HIGH (protocol + product constraints); MEDIUM (exact ConnectRPC Rust wiring details)

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- **D-ACT-01…03:** `.github/workflows` YAML subset; act-compatible runner executes jobs
- **D-ACT-04…06:** push + pull_request only; receive-pack + PR hooks; instance + repo enable gates
- **D-ACT-07…11:** Gitea Actions–compatible open protocol; registration tokens; custom labels; no managed minutes; official runner image
- **D-ACT-12…14:** Actions tab + routes; `OXIDEAN_ACTIONS_LOG_DIR`; Compose + standalone docs
- **D-ACT-15…16:** Publish check contexts for Phase 13; queryable status API/RPC
- **D-ACT-17…20:** Repo secrets; ACL + runner-token auth; factory-reset wipe; queue until runner matches

### Deferred (OUT OF SCOPE)
- Managed cloud minutes, extra triggers, marketplace/OIDC deep parity, artifacts UI, Windows/macOS official images

</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| ACT-01 | GHA-compatible YAML layout | Parse `.github/workflows/*.{yml,yaml}` from triggering commit tree via `oxidean-git` |
| ACT-02 | push + pull_request triggers | Post–receive-pack hook + Phase 12 PR lifecycle dispatcher |
| ACT-03 | Run status + logs UI | DB run/job rows + log files under `OXIDEAN_ACTIONS_LOG_DIR`; Octane Actions routes |
| ACT-04 | Official runner image | Dockerfile wrapping act_runner; register against instance |
| ACT-05 | Docs Compose sidecar / standalone | DEPLOYMENT/CONFIGURATION + runner README |
| ACT-06 | Open registration/job-dispatch + custom labels | Gitea actions-proto ConnectRPC-over-HTTP; `runs-on` label match |
| ACT-07 | Only registered runners; no managed minutes | No forge-side job executor; Cloud/self-host identical policy |

</phase_requirements>

## Summary

Phase 19 adds an Actions control plane to the existing Axum API: discover/parse GitHub-shaped workflow YAML from git trees, enqueue runs on push and pull_request, expose a **Gitea Actions–compatible** runner protocol so operator and third-party runners can FetchTask/UpdateTask, store logs on a dedicated volume, show runs in repo UI, and publish **commit status contexts** that Phase 13 branch protection will require. Job execution never runs inside `oxidean-api` — only on registered runners (ACT-07).

**Primary recommendation:** Mirror Gitea’s split (forge = scheduler + protocol + UI; runner = act fork) rather than embedding nektos/act in-process. Implement ConnectRPC-compatible RunnerService handlers under `/api/actions/` using `prost` (+ build-time codegen from pinned actions-proto). Ship `docker/oxidean-runner` based on `gitea/act_runner` (or pinned fork) with Oxidean docs for labels and `OXIDEAN_PUBLIC_ORIGIN`. Publish statuses with context `{workflow_name} / {job_id}` for Phase 13.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|--------------|----------------|-----------|
| Workflow YAML discover/parse | API / Backend | Database | Tree walk via git; metadata in DB |
| Event dispatch (push / PR) | API / Backend | — | Hooks after receive-pack + PR service |
| Runner Register/Declare/FetchTask/Update* | API / Backend | — | Wire protocol on API HTTP port |
| Job execution (steps/actions) | Runner (external) | — | act_runner; never in API process |
| Log persistence | Database / Storage | API | Files under ACTIONS_LOG_DIR; DB pointers |
| Actions UI | Browser / Client | Frontend Server | Octane + TanStack Query |
| Commit statuses for protection | API / Backend | Phase 13 consumer | Shared status schema |
| Official runner image + Compose | CDN / Static (ops) | API | Dockerfile + compose profile |

## Project Constraints (from .cursor/rules/)

- One product cloud + self-host; Bun + Cargo — no parallel app. [VERIFIED]
- Octane `.tsrx` UI; TanStack Query for server data. [VERIFIED]
- RPC: Rust → `make rpc-gen`. [VERIFIED]
- Dialect SQL only in `oxidean-db`. [VERIFIED]
- No secrets in commits; prefer `make test` / smokes. [VERIFIED]
- Extend existing patterns over new frameworks. [VERIFIED]

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `axum` 0.8 | existing | HTTP mounts for `/api/actions/…` + session RPC | Already API framework |
| `serde` + **`serde_yaml`** | add pin | Parse workflow YAML | De-facto Rust YAML; GHA files are YAML |
| **`prost` / `prost-build`** | add pin | Decode/encode actions-proto messages | Matches Gitea proto source of truth |
| `oxidean-git` | workspace | Read workflow blobs from commit trees | Already used for browse |
| `oxidean-db` migrations | next after `0015_packages` | runners, runs, jobs, statuses, secrets | Dialect boundary |
| `sha2` / `uuid` / `rand` | existing | Token hashing / IDs | Existing crypto patterns |
| Official **act_runner**-based image | pin digest | Execute jobs | D-ACT-03 / D-ACT-11 |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `@octanejs/*` + TanStack Query | existing | Actions list/detail UI | ACT-03 |
| Vitest / cargo-nextest | existing | Wave 0 → green | Nyquist |
| Traefik v3.3 | Compose | Ensure `/api/actions` reaches API (likely already under API host) | Edge |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Gitea-compatible protocol | Custom Oxidean-only runner API | Breaks ACT-06 “Blacksmith-class” / act_runner reuse |
| Embed nektos/act in API | External runner | Violates ACT-07; couples API to Docker socket |
| GitHub Actions runner binary protocol | Gitea protocol | Closed/ephemeral GitHub agent; poor self-host fit |
| Store logs only in DB | Volume files | Huge rows; volume matches LFS/packages ops story |

Don't hand-roll: YAML parser, protobuf codecs, container job isolation (runner owns that).

## Package Legitimacy Audit

| Package | Registry | Legitimacy | Stars/Downloads signal | Notes |
|---------|----------|------------|------------------------|-------|
| `serde_yaml` | crates.io | [VERIFIED] | Widely used; maintainership historically thin but standard for GHA YAML | Prefer latest 0.9.x; no substitute in-tree |
| `prost` | crates.io | [VERIFIED] | Official protobuf for Rust (tokio org) | Pair with `prost-types` |
| `prost-build` | crates.io | [VERIFIED] | Build dependency only | Codegen in `build.rs` of api or small `oxidean-actions-proto` crate **only if** workspace pattern warrants — prefer keep codegen inside `oxidean-api` build.rs to avoid new workspace crate unless proto size forces it |
| `pbjson` / `pbjson-types` | crates.io | [ASSUMED] | Optional JSON mapping for Connect | Use only if Connect JSON needed; prefer protobuf binary Connect like act_runner |
| New npm packages | — | N/A | — | UI uses existing Octane stack only |

**Install policy:** Any `[ASSUMED]` package requires blocking human checkpoint before `cargo add`. `[VERIFIED]` prost/serde_yaml may proceed. No `[SLOP]`.

## Architecture Patterns

### Control plane vs data plane
```
git push / PR event
    → actions::dispatcher (match on: filters)
    → create action_run + action_jobs (queued)
    → publish commit statuses (pending)
    → runner FetchTask (label match) → running
    → UpdateLog / UpdateTask → success|failure
    → UI + Phase 13 status readers
```

### Protocol surface (Gitea-compatible)
Mount Connect-style RunnerService under paths compatible with act_runner (research pin exact service paths from actions-proto-def at execute time). Minimum RPCs: **Register**, **Declare**, **FetchTask**, **UpdateTask**, **UpdateLog** (names per proto). Auth: registration token once; persistent runner token thereafter. **No session cookies** on protocol routes.

### Label matching
Job `runs-on: [ubuntu-latest]` matches runner AgentLabels containing `ubuntu-latest` (with optional `:docker://…` schema on the runner side). Custom labels supported (D-ACT-09). Unmatched → remain queued (D-ACT-20).

### Phase 12 / 13 contracts
| Producer (19) | Consumer | Contract |
|---------------|----------|----------|
| `actions::dispatch_pull_request(event)` | Phase 12 PR service | Call after open/sync/reopen/(close) |
| `commit_statuses` rows + RPC `repo.statuses` / `checks.list` | Phase 13 protection | Context string stable; state enum pending/success/failure/error/cancelled |
| Push hook after receive-pack | Phase 08/09 git paths | Fire once per successful push with ref tips |

### Workflow parse
Read blob(s) from commit SHA associated with event. Skip disabled workflows if `on:` lacks event. Invalid YAML → create failed run with parse error log (user-visible).

## Data Model (sketch)

Tables (dialect SQL in `oxidean-db`):
- `action_runners` — uuid, name, token_hash, labels JSON, owner_id/repo_id scope, ephemeral, last_online
- `action_runner_tokens` — registration tokens (instance/org/repo), active flag
- `action_runs` — repo_id, workflow_path, event, head_sha, status, title, triggered_by
- `action_jobs` — run_id, job_id, runs_on JSON, status, runner_id, started/finished
- `commit_statuses` — repo_id, sha, context, state, description, target_url (Actions run link)
- `action_secrets` — repo_id, name, ciphertext (reuse existing crypto patterns if present; else libsodium/chacha via existing deps — discretion: prefer OS key from env `OXIDEAN_ACTIONS_SECRETS_KEY`)
- Repo flag `actions_enabled`

Files: `{OXIDEAN_ACTIONS_LOG_DIR}/{run_id}/{job_id}.log`

## Common Pitfalls

| Pitfall | Why it hurts | Mitigation |
|---------|--------------|------------|
| Register runner with `localhost` while jobs need checkout | Job containers cannot reach forge | Docs: use `OXIDEAN_PUBLIC_ORIGIN` / published hostname (Gitea design Connection 2) |
| Executing steps inside API | Violates ACT-07; Docker socket in API | Protocol-only control plane |
| Ignoring Phase 13 context naming | Required checks never match | Lock `{workflow} / {job}` and document |
| Session cookie on runner API | Runner spoof / CSRF confusion | Token-only protocol auth |
| Blocking receive-pack on workflow parse | Push latency / failures | Async dispatch after ACK |
| Log unbounded growth | Disk fill | Retention job discretion + factory reset wipe |
| Assuming Blacksmith GitHub agent works unmodified | Blacksmith today targets GitHub | Document open protocol; “Blacksmith-class” = third-party implementers |

## Security Domain

### ASVS (Level 1)

| Category | Control |
|----------|---------|
| V2 Authn | Runner registration + runner tokens; secrets never echoed |
| V4 Access | Capability ACL for UI; protocol token auth fail-closed |
| V5 Validation | Workflow path confinement under `.github/workflows/`; label length limits |
| V7 Error | Parse errors visible to Read+; no secret leakage in logs redaction where possible |

### Threat patterns

| Pattern | STRIDE | Mitigation |
|---------|--------|------------|
| Stolen registration token registers attacker runner | Spoofing | Scoped tokens; Admin rotate; hash at rest |
| Malicious workflow exfiltrates secrets | Information Disclosure | Secrets only to assigned job task token; redaction heuristics discretionary |
| Unregistered execution on API host | Elevation | No in-process executor (D-ACT-10) |
| Cross-repo job pickup | Elevation | Scope runners + tasks to token owner/repo |
| Log path traversal | Tampering | Internal IDs only in paths |
| Supply-chain YAML bombs | Denial of Service | Size limits on workflow files; parse timeouts |

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | cargo-nextest + Vitest |
| Quick run | `cargo nextest run -p oxidean-api -E 'test(actions_)\\|test(runner_)\\|test(commit_status)'` |
| Full | `make test` + `make smoke-actions` (skip-ok without Docker) |

### Phase Requirements → Test Map
| Req ID | Behavior | Test Type | File (Wave 0) |
|--------|----------|-----------|---------------|
| ACT-01 | Parse workflow from tree | integration | `actions_workflow_parse.rs` |
| ACT-02 | push + PR dispatch | integration | `actions_triggers.rs` |
| ACT-03 | RPC list/detail/logs | integration + Vitest | `actions_rpc.rs` + web Actions routes |
| ACT-04/05 | Runner image + docs | smoke + rg docs | `smoke-actions.sh` |
| ACT-06 | Register/FetchTask/labels | integration | `actions_runner_protocol.rs` |
| ACT-07 | No match → queued; no local exec | integration | `actions_dispatch_policy.rs` |
| Phase 13 surface | Status publish | integration | `commit_statuses.rs` |
| Dialect | migrations | unit | `dialect_actions.rs` |

### Wave 0 Gaps
- [ ] All test files above (RED/ignored stubs)
- [ ] Web Actions integration stubs
- [ ] `scripts/smoke-actions.sh` + `make smoke-actions`
- [ ] `19-VALIDATION.md` seeded from this section

## Open Questions (resolved for planning)

1. **Protocol family?** → **Gitea Actions / actions-proto** (D-ACT-07).  
2. **Managed minutes?** → **None** (D-ACT-10 / ACT-07).  
3. **Status context format?** → **`{workflow_name} / {job_id}`** (D-ACT-15).  
4. **New workspace crate for proto?** → Prefer api `build.rs` first; split only if compile times force it (discretion).

## Sources

### Primary (HIGH)
- Gitea Actions design — https://docs.gitea.com/usage/actions/design/
- Gitea runner registration — https://docs.gitea.com/runner/registration/
- go-gitea act_runner / actions-proto-def (GitHub)
- In-repo: receive-pack, RepoChrome, docker-compose volume patterns, Phase 12/14/20 CONTEXT

### Secondary (MEDIUM)
- GitHub Actions workflow syntax docs (subset)
- ConnectRPC over HTTP documentation
- DeepWiki Gitea runner protocol notes

### Tertiary (LOW)
- Blacksmith marketing (confirms “class of provider” is GitHub-oriented today; Oxidean exposes open forge protocol for that class to implement)

---

*Phase: 19-actions-runners*  
*Research date: 2026-09-16*
