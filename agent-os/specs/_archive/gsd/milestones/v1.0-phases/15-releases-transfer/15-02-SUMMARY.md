---
phase: 15-releases-transfer
plan: "02"
subsystem: api
tags: [releases, assets, multipart, compose]
requires:
  - phase: 15-releases-transfer
    provides: "release.* RPC + Releases UI notes from 15-01/15-06"
provides:
  - "POST/GET release asset HTTP on dedicated volume"
  - "release.deleteAsset + file cleanup on release.delete"
  - "Vite /api/releases proxy + Compose release-assets bind"
affects: [15-05]
actuals:
  tokens: 12328
  tasks: 2
  commits: 1
plan_head_before: 7af0bec
tech-stack:
  added: []
  patterns: ["Opaque asset_id filesystem + multipart replace-by-filename"]
key-files:
  created:
    - crates/oxidean-api/src/routes/release_assets.rs
  modified:
    - crates/oxidean-api/src/app.rs
    - apps/web/src/routes/$owner.$repo.releases.$tag.tsrx
    - docker-compose.yml
requirements-completed: [GIT-14, GIT-15]
coverage:
  - id: D1
    description: "Asset upload/download/replace/size/draft ACL"
    requirement: GIT-15
    verification:
      - kind: integration
        ref: "cargo nextest run -p oxidean-api -E 'test(release_asset)'"
        status: pass
    human_judgment: false
duration: 20min
completed: 2026-09-14
status: complete
---

# Phase 15 Plan 02: Release Assets Summary

**Release assets store on `OXIDEAN_RELEASE_ASSETS_DIR` by opaque id with multipart upload, size limits, ACL'd download, and detail UI controls (GIT-14/15).**

## Task Commits

| Task | Commit |
|------|--------|
| 1–2 HTTP + UI/Compose/docs | `833640b` |

## Deviations

Combined T1–T2 into one commit (shared wiring).

## Self-Check: PASSED

- FOUND: release_assets.rs, release_asset tests green, compose+vite+docs ENV
