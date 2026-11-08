# Watermarks-remover skills (vendored)

Upstream: [guillaumemeyer/watermarks-remover](https://github.com/guillaumemeyer/watermarks-remover)

Pinned: `v0.7.0` (`321d93d2efd6a8b26915c5eb5193d9d1701e2c4b`)

Skills:

- `remove-ai-marks/` — multi-vendor mark strip via HTTP service (`WATERMARKS_SERVICE_URL`, default `http://127.0.0.1:8765`). Oxidean agents must use `scripts/oxidean-watermarks-service.sh` (`ensure` / `teardown` or `run`) so the service starts for the run and stops afterward when this helper started it.
- `clean-user-facing-text/` — self-contained prose hygiene (no service; no helper needed)

Refresh: clone the desired tag into `tmp/watermarks-remover`, copy upstream `skills/*` here, **re-apply** the Oxidean lifecycle section + `scripts/oxidean-watermarks-service.sh` (not upstream), update this pin, and refresh `.claude/skills/` symlinks if needed.

Do **not** vendor the Python `service/` into this repo. The helper clones/runs it from `tmp/watermarks-remover`.
License: MIT — see [LICENSE-watermarks-remover](LICENSE-watermarks-remover) (copyright Guillaume Meyer and contributors).
