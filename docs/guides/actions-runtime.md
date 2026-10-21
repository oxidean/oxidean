# Actions runtime environment

Every job step runs with an `OXIDEAN_*` environment, each variable paired with a `GITHUB_*` alias carrying the identical value, so workflows written for GitHub run unmodified. `PAIRED_VARS` in `crates/oxidean-runner/src/exec.rs` is the single source of truth for the pair set — the two namespaces cannot drift.

`CI=true` and `OXIDEAN=true` are also set. Repo secrets are appended last and can never override a reserved name (a collision is logged and skipped). **`pull_request` runs whose head commit comes from a fork receive no repo secrets at all** — GitHub's model — and the same gate fails closed whenever the run's head-repo provenance was not recorded at dispatch.

## Default variables

| OXIDEAN name | GITHUB alias | Value |
|--------------|--------------|-------|
| `OXIDEAN_ACTIONS` | `GITHUB_ACTIONS` | `"true"` |
| `OXIDEAN_ACTOR` / `OXIDEAN_ACTOR_ID` | `GITHUB_ACTOR` / `GITHUB_ACTOR_ID` | User who triggered the run |
| `OXIDEAN_API_URL` | `GITHUB_API_URL` | `<server>/api/v1` — the GitHub-shaped REST root Octokit / `gh` / third-party actions resolve `{api_url}/repos/…` against |
| `OXIDEAN_SERVER_URL` | `GITHUB_SERVER_URL` | Instance origin |
| `OXIDEAN_BASE_REF` | `GITHUB_BASE_REF` | PR base ref; empty unless `pull_request` |
| `OXIDEAN_HEAD_REF` | `GITHUB_HEAD_REF` | Short source branch on `pull_request`; empty otherwise |
| `OXIDEAN_EVENT_NAME` | `GITHUB_EVENT_NAME` | `push`, `pull_request`, `workflow_dispatch`, … |
| `OXIDEAN_EVENT_PATH` | `GITHUB_EVENT_PATH` | Path to the event JSON payload (below) |
| `OXIDEAN_JOB` | `GITHUB_JOB` | Job key from the workflow file |
| `OXIDEAN_REF` | `GITHUB_REF` | Full ref, e.g. `refs/heads/main`; on `pull_request` runs this is the **head ref**, not GitHub's `refs/pull/N/merge` |
| `OXIDEAN_REF_NAME` / `OXIDEAN_REF_TYPE` | `GITHUB_REF_NAME` / `GITHUB_REF_TYPE` | Short ref (`main`, `v1.2.3`) and `branch`/`tag` |
| `OXIDEAN_REPOSITORY` / `OXIDEAN_REPOSITORY_ID` | `GITHUB_REPOSITORY` / `GITHUB_REPOSITORY_ID` | `owner/name` and the repo id |
| `OXIDEAN_REPOSITORY_OWNER` / `OXIDEAN_REPOSITORY_OWNER_ID` | `GITHUB_REPOSITORY_OWNER` / `GITHUB_REPOSITORY_OWNER_ID` | Owner login and id |
| `OXIDEAN_RUN_ID` / `OXIDEAN_RUN_NUMBER` / `OXIDEAN_RUN_ATTEMPT` | `GITHUB_RUN_ID` / `GITHUB_RUN_NUMBER` / `GITHUB_RUN_ATTEMPT` | Run identity |
| `OXIDEAN_SHA` | `GITHUB_SHA` | Head commit SHA |
| `OXIDEAN_WORKFLOW` / `OXIDEAN_WORKFLOW_REF` / `OXIDEAN_WORKFLOW_SHA` | `GITHUB_WORKFLOW` / `GITHUB_WORKFLOW_REF` / `GITHUB_WORKFLOW_SHA` | Workflow name, ref, and SHA it was resolved at |
| `OXIDEAN_WORKSPACE` | `GITHUB_WORKSPACE` | Checkout dir — host path or `/workspace` in Docker |
| `OXIDEAN_TOKEN` | `GITHUB_TOKEN` | Per-job bearer token (below) |

Runner metadata uses the standard `RUNNER_*` names: `RUNNER_OS` (`Linux`/`macOS`/`Windows`), `RUNNER_ARCH` (`X64`/`ARM64`/…), `RUNNER_NAME`, `RUNNER_TEMP`, `RUNNER_TOOL_CACHE` (`$RUNNER_TEMP/tool-cache`).

## File commands

Each job gets a clean `RUNNER_TEMP` containing `event.json` and four empty command files:

- `OXIDEAN_ENV_FILE` / `GITHUB_ENV` — honored. `KEY=VALUE` lines and `KEY<<DELIM` heredocs merge into the environment of subsequent steps.
- `OXIDEAN_PATH` / `GITHUB_PATH` — honored. One directory per line, prepended to PATH for subsequent steps.
- `OXIDEAN_OUTPUT` / `GITHUB_OUTPUT` and `OXIDEAN_STEP_SUMMARY` / `GITHUB_STEP_SUMMARY` — the files exist and steps can write them, but the runner does not consume them yet (no `$OUTPUT` forwarding, no job summary rendering).

`OXIDEAN_ENV` is deliberately **not** used: the server config reserves that name for the runtime mode (`development`/`production`), so the env-file command is `OXIDEAN_ENV_FILE` while the GitHub alias keeps GitHub's exact `GITHUB_ENV` spelling.

## `OXIDEAN_EVENT_PATH`

`event.json` carries a compact event payload:

```json
{
  "event": "pull_request",
  "ref": "refs/heads/feature",
  "sha": "<head sha>",
  "repository": "owner/name",
  "repository_id": "…",
  "actor": "octo",
  "actor_id": "…",
  "run_id": "…",
  "run_number": 7,
  "pull_request": { "number": 42, "base_ref": "main", "head_ref": "refs/heads/feature" }
}
```

`pull_request` is `null` for non-PR events.

## `OXIDEAN_TOKEN`

A per-job `oxidean_job_…` bearer token minted when the runner claims the job:

- **Repo-locked** — resolves to a fine-grained identity scoped to the job's repository only.
- **Read by default** — `contents: read` unless the workflow sets `permissions: {contents: write}`; `contents: none` injects no token at all.
- **Capability-capped** — a `write` request from a read-only triggering user is downgraded to `read` (permissions can only be lowered). Runs with no triggering user (internal dispatches) get no token.
- **Read-only on fork PRs** — a `pull_request` run with a fork head clamps `contents: write` down to `read` regardless of the workflow's `permissions:` block.
- **Short-lived** — revoked when the job reaches a terminal state or the run is cancelled; 24-hour `expires_at` is the hard backstop. Re-mint on requeue replaces the row, so the previous plaintext stops resolving.
- Usable as `Authorization: Bearer oxidean_job_…` against `POST /api/rpc` (job-token procedure class) and the `/api/v1` REST surface.
- **Git over HTTP(S) + LFS** — works as a Basic-auth password (`git clone https://…/owner/repo.git` with `oauth2:<token>`, or any username). `contents: read` can fetch; `contents: write` can push (subject to the capability cap above). LFS batch/download/upload accept the same credential.
- **Pushes authenticated with `OXIDEAN_TOKEN` do not trigger new workflow runs** — GitHub parity; this prevents job → push → run recursion. Webhooks, statuses, and mirror sync still fire.
- The bundled runner's `actions/checkout` clone authenticates with `OXIDEAN_TOKEN` when the task carries one, falling back to the runner-configured `OXIDEAN_RUNNER_GIT_TOKEN`.

## GitHub REST compatibility (`GITHUB_API_URL`)

`GITHUB_API_URL` points at `/api/v1`, whose surface already mirrors GitHub's route layout (~370 routes translating into RPC procedures). `OXIDEAN_ACTIONS_REST_SHIM` (default on) switches responses to GitHub-exact wire shapes so Octokit/`@actions/github`/third-party actions deserialize them — issue #160:

- List endpoints unwrap `{items,total}` into bare arrays and emit `Link` rel=`next`/`last` pagination; `per_page`/`page` translate into the RPC `limit`/`offset` (cap 100).
- Payloads carry GitHub keys — `html_url`, `url`, `node_id`, `user.login` objects, `labels[]` objects, pull `head`/`base`, `permissions`, release `upload_url`/`browser_download_url`. Fields with no Oxidean analogue return GitHub-shaped placeholders rather than dropping keys.
- Errors render `{message, documentation_url, errors[]}`; `X-RateLimit-*` (static advisory window, not enforced quota) and `ETag`/`If-None-Match` ride every response.
- Meta routes `GET /api/v1/{/,rate_limit,zen}` mount under the flag; `GET /repos/{o}/{r}/{zip,tar}ball[/{ref}]` 302s onto the existing source-archive plumbing; `GET …/issues/{n}/timeline` serves revision snapshots as `edited` events (partial coverage — Oxidean records title/body snapshots, not GitHub's labelled-event feed).
- Release-asset upload follows GitHub's contract at the emitted `upload_url`: `POST ?name={file}` with a raw body → `201` + the GitHub-shaped asset object (the same route still accepts the web client's multipart upload).
- `GET /api/v1/_compat/stats` (sys-admin) reports observed-vs-covered route counts; requests no route covers answer a GitHub-style 404 and count as uncovered — the report is how coverage gaps get measured.
- Job tokens (`Authorization: Bearer oxidean_job_…`) resolve as repo-pinned identities, so Octokit calls from a step enforce the workflow's `permissions:` scopes exactly like the RPC edge.

`OXIDEAN_ACTIONS_REST_SHIM=false` restores the plain Oxidean DTO wire shapes and unmounts the compat meta routes (the `/api/v1` route table itself is unaffected). GraphQL is not emulated — `GITHUB_GRAPHQL_URL` remains a stable-shape value only.
