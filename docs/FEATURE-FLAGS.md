# Feature flags and experiments

Per-user experiments gate unfinished or alternate experiences behind an
explicit opt-in under **Settings → Experiments**. Flags are off by default:
anonymous sessions, pending loads, errors, and unenrolled users all see the
flag as disabled ("fail closed").

The registry is the single source of truth. It defines which keys exist,
their display metadata, and their maturity status; the settings page, the
RPC surface, and this document are all derived from or checked against it.

## Pieces

| Layer | Location | Role |
|-------|----------|------|
| Registry | `crates/oxidean-core/src/experiments.rs` | `EXPERIMENTS` — static `ExperimentDef` list (key, title, description, status, since_version). |
| Storage | `crates/oxidean-db/migrations/*/0100_user_experiments.sql` | `user_experiments (user_id, flag_key)` — a row means enabled for that user; cascades on user delete. |
| RPC | `oxidean-api-user` (`experiments.rs`) | `experiments.list` (registry + caller state; anonymous → all off), `experiments.set` (validates key, returns updated `UserPublic`). |
| Session | `UserPublic.experiments` | Enabled keys ride `auth.me`, so flag checks cost no extra request. |
| Web read | `apps/web/src/lib/use-experiment.tsrx` | `useExperiment(key) -> boolean`, reads the shared `auth.me` query. |
| Web list | `apps/web/src/lib/session-queries.ts` | `experimentsListQueryOptions()` for the settings page. |
| UI | `apps/web/src/routes/settings/experiments.tsrx` | `/settings/experiments` toggle page. |
| Consistency | `crates/oxidean-core/tests/experiment_docs.rs` | Fails when this doc drifts from the registry. |

## Using a flag in web UI

```tsrx
import { useExperiment } from "@/lib/use-experiment";

const prNewUi = useExperiment("pr-new-ui");
```

Gate at the route/component seam, not inside leaf components: render the
new experience when the flag is on and the existing one otherwise, so flag
retirement is a deletion, not a refactor.

## Adding a flag

1. Add an `ExperimentDef` to `EXPERIMENTS` in
   `crates/oxidean-core/src/experiments.rs` (key: kebab-case, permanent —
   never reuse a retired key; `since_version: "unreleased"` until release).
2. Add a `### \`<key>\`` section under [Flags](#flags) below — the
   consistency test requires a doc section for every registry key and
   rejects documented keys the registry no longer knows.
3. `make rpc-gen` if you changed request/response types (not needed for a
   registry-only entry).
4. Gate the UI with `useExperiment` (or read `UserPublic.experiments`
   server-side) and add tests for both flag states.

## Retiring a flag

1. Remove the gate and either keep the new behavior unconditionally or
   delete the gated code.
2. Remove the `ExperimentDef` and its doc section.
3. Orphaned `user_experiments` rows for the key are inert (the registry
   filters reads at the RPC boundary); a cleanup migration is optional.

## Flags

### `pr-new-ui`

GitHub-style pull request experience: real per-tab URLs
(`/pull/N/commits|checks|changes`), a resizable file tree, per-file viewed
state, any-line comments collected into a pending review, diff settings,
and a comments side panel. Status: **experimental**, since unreleased.
