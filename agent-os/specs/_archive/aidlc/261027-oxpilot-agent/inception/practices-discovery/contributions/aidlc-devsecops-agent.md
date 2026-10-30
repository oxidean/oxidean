**Collaborator:** aidlc-devsecops-agent

## Contribution

Blind support pass over the security-relevant slice of the evidence: pipeline
scanning posture, dependency/supply-chain controls, and the credential-custody
precedents this intent's connector surface will sit on.

### What I inspected

- `.github/dependabot.yml` (all 80 lines), `.github/workflows/ci.yml` (full —
  all 23 job stanzas), `.github/workflows/ci-base.yml` header + triggers,
  `.github/workflows/pr-env-sweep.yml`, `docker/ci-base/Dockerfile`,
  `docker-compose.yml`, `scripts/sync-workflow-templates.mjs`.
- Repo-wide scan for scanners: `rg -i 'cargo.audit|cargo-deny|gitleaks|
  trufflehog|trivy|semgrep|sonar|snyk|codeql|secret.scan|vuln|osv'` across
  `.github/workflows/`, `Makefile`, `scripts/`, `.githooks/` — zero hits.
  Also zero `pull_request_target` anywhere in the workflows.
- Credential custody code: `crates/oxidean-api-core/src/crypto.rs`,
  `pat/bearer.rs`, `pat/policy.rs`, `pat/rate_limit.rs`, `audit.rs`,
  `oxidean-core/src/proc_classes.rs`.
- Custody schema precedents: `crates/oxidean-db/migrations/sqlite/
  0008_pats.sql`, `0021_actions.sql`, `0083_runner_remove_tokens.sql`.
- `.env.example`, `docs/CONFIGURATION.md` (key-material rows + PAT policy
  rows), `docs/CODE_PRACTICES.md` § Security & privacy, root `Cargo.toml` /
  `package.json`, `aidlc/spaces/default/memory/{team,project}.md`
  (affirmed baseline), and this intent's `ideation/feasibility/
  feasibility-assessment.md` § Computer-use transport + Compliance lens.

### Findings

1. **Dependabot landed exactly as the draft describes — and the exclusion
   mechanism is structural, not just a comment.** The `docker` ecosystem's
   `directories:` list (`/apps/web`, `/crates/oxidean-api`,
   `/docker/oxidean-runner`, `/scripts/dev-auth`) simply omits
   `/docker/ci-base` (`.github/dependabot.yml` L71-80). Even if listed,
   ci-base `FROM`s are `ARG`-indirected (`FROM ${RUST_IMAGE}`,
   `docker/ci-base/Dockerfile` L30-47), so the bot could not rewrite them —
   the header comment L10-13 documents intent; directory scoping enforces it.
   The `bun` block also carries a curated `ignore:` for `oxlint-tsgolint`
   (L44-48: hard-pinned by `@tsrx/oxc`; a solo bump breaks `make web-lint`).
   Recent history is dense with grouped `chore(deps):` merges (#195, #196,
   #197 vs `git log`).

2. **"No SAST/DAST/vuln/secret scanning" is fully verified in-repo — with
   one verification boundary worth naming.** CI runs quality/drift/e2e gates
   only; nothing invokes `cargo audit`, `cargo deny`, `bun audit`, an SBOM
   tool, or a secret scanner, and there is no pre-commit secret hook in
   `.githooks/` (only `commit-msg` grammar). **But** Dependabot *alerts /
   security updates* and GitHub secret scanning are repo/org *settings*, not
   file-resident config — dependabot.yml only governs *version updates*. The
   affirmed wording can state "no scanner in our CI/hooks" with certainty;
   "no vulnerability alerting anywhere" needs a repo-settings check or an
   interview answer.

3. **The CI threat model is deliberate, not accidental.** `ci.yml` L1:
   "Phase 1 quality gates — no production secrets (T-01-12)"; default
   `permissions: contents: read` with per-job elevation (`packages: read` for
   container pulls, `actions: write` only on the cleanup job L1028-1029);
   fork PRs get a read-only token, publish nothing, and skip artifact
   cleanup (L1023); `ci-image` resolve refuses to wait on fork PRs (L92-94);
   no `pull_request_target`. `pr-env-sweep.yml` mints release-App tokens per
   leg with narrowly-scoped grants (`permission-administration`,
   `permission-packages`, org-scoped installation for GHCR deletes) and
   degrades loudly on a missing grant (L52-94) — the best least-privilege
   minting pattern in the repo.

4. **Credential custody has two clean, complementary precedents — and the
   draft's "aes-gcm at rest" only names the weaker half.** Verifier-side
   credentials (secrets the forge *checks*, never replays) are **hash-at-
   rest**: `personal_access_tokens.token_hash` SHA-256 + display
   `token_prefix` (migration `0008_pats.sql`), `action_runner_tokens` /
   `action_job_tokens` the same (`0021_actions.sql`, `0083_…`). Retrievable
   secrets (the forge must *re-present* them) are **AES-256-GCM ciphertext**:
   `action_secrets.ciphertext`, TOTP secrets — via `crypto.rs`'s shared box
   keyed by SHA-256 of the first set var in a per-domain env preference
   chain (`OXIDEAN_ACTIONS_SECRETS_KEY` → `OXIDEAN_SECRETS_KEY` →
   `OXIDEAN_SESSION_SECRET`), **fail-closed** when unset (crypto.rs L19-28;
   CONFIGURATION.md L88-89 documents rotation-invalidates-ciphertexts). The
   `pat/bearer.rs` edge adds: session cookie always wins over Bearer;
   Bearer never mints sessions or sends `Set-Cookie`; PATs can never mint
   PATs; per-job tokens synthesize repo-locked FG identities.

5. **PAT hygiene is policy-enforced, not advisory.** `pat/policy.rs`:
   default TTL 90d, max 365d, `expires_at:"never"` rejected unless the
   instance opts in (`OXIDEAN_PAT_ALLOW_NO_EXPIRY`, default false);
   DB-resident admin overrides layer over env defaults with default≤max
   clamping. `pat/rate_limit.rs` (D-26): failed-auth sliding window —
   20/IP + 10/user per 15 min — shared by Smart HTTP and Bearer RPC auth;
   **per-process only** ("multi-replica deferred" L3), which matters if
   connector auth rides it.

6. **`oxpilot.*` procedures are already fail-closed.** `proc_classes.rs`
   classifies any unlisted procedure `SessionOnly` — a PAT cannot call new
   `oxpilot.*` RPCs until `classify_pat_proc` gets explicit entries, and the
   affirmed "PAT/pubkey-class on transports, never session cookies" rule
   (project.md Mandated, D-12) already binds the connector's wire credential.

7. **Audit trail convention is directly on-point.** `audit.rs`: durable
   `audit_events` write-through, fire-and-forget (failure never fails the
   operation), `detail` must never carry secrets. `CODE_PRACTICES.md` §80
   affirms security-relevant operations record an event — feasibility already
   lists "audit logging of agent actions" as a standing obligation.

8. **Runtime hardening evidence is unchanged.** `docker-compose.yml` L193:
   runner mounts `docker.sock` RW behind `profiles:["actions"]` + opt-in
   `docker://` labels (L185-188 comment); Traefik mounts it `:ro` (L23).
   Service images still run as root. Orthogonal to this intent's new surface.

## Positions

**On the lead's flagged items:**

- **Dependabot drift — agree it's real, but sharpen the debt wording.** The
  bot is a *freshness* control, not a *vulnerability* control: a Critical CVE
  in a merged dep still waits for the weekly Monday batch plus human review
  plus a manual promote. The draft's "update bot ≠ scanner" is right; I'd go
  further — the affirmed debt line should name the actual gap: *no
  CVE-alerting or triage path* (`cargo audit`/`cargo deny advisories`/
  `bun audit` absent; Dependabot security-updates state unverifiable from the
  repo). For an intent whose own success metric is "zero credential or secret
  leaks through agent sessions (hard security gate)" (intent-statement L39),
  I position this as: affirm the bot as the sanctioned update path, and ask
  the interview whether a **non-gating** audit job (`cargo audit` +
  `bun audit --audit-level=high`, advisory-only) is worth adding now —
  cheap, zero merge-friction, and the first rung of a real triage loop. If
  declined, record the refusal deliberately rather than re-carrying a vague
  "manual freshness" debt that no longer describes reality.

- **ci-base ARG-pin hand-bump — support as Mandated, with two-sided
  wording.** The rule should affirm both halves: (a) `RUST_IMAGE`/`BUN_IMAGE`
  /`NODE_IMAGE`/`DEBIAN_IMAGE` and the `CARGO_*`/`DOCKER_*` tool ARGs are
  bumped by deliberate one-line PRs, never by automation — they are
  content-hash inputs and a bump must mint the image when *intended*; (b)
  `/docker/ci-base` stays outside dependabot's `docker` `directories:`
  coverage. Also note for the record: weekly `ci-base.yml` cron (Mon 04:37)
  still absorbs *base-image patch drift* under an unchanged tag — the
  hand-bump rule governs declared pins, not Debian patchlevel.

- **Root/docker.sock accepted risk — re-affirm unchanged, and do not merge
  it with the connector question.** It's an opt-in, documented, self-host
  risk (`profiles:["actions"]`, `docker://` labels) whose blast radius is CI
  job code — already an untrusted-code-by-design surface. The connector adds
  a *different* axis; conflating them dilutes both records.

**On the connector credential surface (the question the draft should sharpen):**

- The feasibility decision (connector dials **out**; custody user-side;
  PAT-class on the wire) is the right shape and already matches two affirmed
  rules. What the intent adds platform-side must follow the custody split the
  codebase already enforces: **verifier-class secrets hash-at-rest**
  (`token_hash` SHA-256 precedent — connector registration/session tokens);
  **retrievable secrets AES-256-GCM via `crypto.rs`** with a dedicated
  `OXIDEAN_OXPILOT_*` key var added to the env preference chain (or a clear
  decision that nothing server-side needs retrieval), documented in
  `.env.example` + `docs/CONFIGURATION.md` in the same PR per the affirmed
  env-doc Mandated rule. No plaintext-credential column should ever ship.
- The connector's auth channel should ride the D-26 `FailedAuthLimiter`
  pattern (or ship an equivalent keyed limiter) — a new credential-accepting
  surface without failure throttling breaks the standing pattern; note the
  limiter is per-process (single-replica assumption).
- `oxpilot.*` PAT classification is a deliberate design decision the
  interview/design should force: leaving them `SessionOnly` (fail-closed
  default) is defensible for v1, but if the connector or `ox` ever needs to
  reach them, they need an explicit `PatProc` class — never a blanket
  session→PAT widening.
- Audit events for agent actions (session lifecycle, tool-approval
  decisions, connector connect/disconnect, credential mint/revoke) should be
  enumerated in requirements and ride `audit.rs` fire-and-forget — and the
  "never put secret material in `detail`" convention extends naturally to
  "never put secret material in transcripts/provider calls," which is the
  prompt-injection/exfiltration edge this feature uniquely owns.
- **Candidate Mandated rule for the interview:** *agent-reachable surfaces
  never return stored secret material* (decrypted connector creds, action
  secrets, provider keys) — the one-line hard rule that operationalizes the
  intent's zero-leak success gate. Worth proposing; the team decides whether
  it affirms as Mandated or lives as an NFR.

**Interview questions my lens adds:**

1. Is GitHub-side Dependabot *alerts/security updates* and secret scanning
   enabled at repo/org level? (Not file-verifiable; determines whether the
   "no vuln scanning" claim is a gap or a setting.)
2. Advisory dependency-audit CI job — add now, or record a deliberate
   refusal? (My recommendation: add non-gating.)
3. Which of this intent's new secrets are verifier-class (→ hash) vs
   retrievable-class (→ `crypto.rs` GCM + named env var), and does the
   connector-side store ship from this repo (inherits the same pattern) or
   live entirely user-side?
4. What auth does the connector channel present on the wire, and does it
   ride the D-26 failed-auth limiter?
5. Which agent actions emit `audit_events`, and is "agent surfaces never
   return stored secrets" affirmed as a Mandated rule?
