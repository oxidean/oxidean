# Deployment Log — oxpilot

Sources: `operation/deployment-pipeline/cd-config.md` (**cd-config**), `operation/deployment-pipeline/deployment-strategy.md` (**deployment-strategy**), `operation/environment-provisioning/environment-inventory.md` (**environment-inventory**), `construction/build-and-test/test-results.md` (**build-test-results**).

## Executed by this stage

No live deployment was executed — correct by design: merge is gated on required status checks + a human merge decision, and the feature ships flag-off. The authoritative "deploy" evidence at this point is the CI pipeline itself plus the PR preview environment.

## What the merge-time deploy does (operator path)

1. Merge `feat/oxpilot-agent` → `main` (squash) once all required checks pass on the head commit.
2. `images.yml` builds `sha-*` images and retags `:main`.
3. Preview/PR envs already run `:main`; staging/production promotes are `railway config plan` → `apply` per `cd-config.md`.
4. API boot applies migrations `0087`–`0090` (additive).
5. Operator sets `OXIDEAN_OXPILOT_ENABLED=true` + provider vars in target envs to enable the feature.

## Preview environment (user inspection surface)

The PR preview deployment rides the same images as CI; when the `Images`/`CI` checks are green on the head commit, the preview serves the flag-off build — the end-user inspection the user asked for happens there.
