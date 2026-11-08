# API Coverage — PAT RPC + Git Smart HTTP

> Full coverage by default. Opt-outs are explicit, reasoned decisions.
> Phase 8 ships first-party Oxidean PAT mint/list/revoke and HTTPS Smart HTTP via system `git-http-backend` CGI — not a third-party SaaS SDK. Same gate as Phase 7 `GitBackend` coverage (D-01, D-11–D-26, GIT-02, GIT-11).

| capability | decision | reason |
|---|---|---|
| `pat.createClassic` (note, repo scope, optional expiry) | INTEGRATE | |
| `pat.createFineGrained` (all/selected repos, contents read/write) | INTEGRATE | |
| `pat.list` (prefix+fingerprint, last_used_*, no plaintext) | INTEGRATE | |
| `pat.revoke` | INTEGRATE | |
| Smart HTTP `info/refs` + `git-upload-pack` (fetch/clone) | INTEGRATE | |
| Smart HTTP `git-receive-pack` (push) with PAT + verified email | INTEGRATE | |
| Basic auth username aliases (account, `git`, `token`, `oauth2`) | INTEGRATE | |
| Reject account password as git secret (PAT hint) | INTEGRATE | |
| Ignore session cookies on `.git` routes | INTEGRATE | |
| Private unauth → 401 + `WWW-Authenticate` (not web 404) | INTEGRATE | |
| Failed-auth rate limit 20/IP + 10/user / 15m → 429 | INTEGRATE | |
| Traefik `/{owner}/{repo}.git` → API (PathRegexp priority 110) | INTEGRATE | |
| PAT Bearer on typed `/api/rpc` | OPT-OUT | D-01 — Phase 8 PATs are HTTPS-git-only; RPC stays session cookies |
| SSH `git-upload-pack` / `git-receive-pack` | OPT-OUT | Phase 9 |
| `git LFS` over HTTPS | OPT-OUT | Phase 14 |
| Fine-grained scopes beyond contents read/write (issues, admin, …) | OPT-OUT | D-04 forge-parity contents only in Phase 8 |
| Classic scopes beyond `repo` | OPT-OUT | D-04 classic `repo` only |
| Multi-replica shared rate-limit store | OPT-OUT | D-26 in-process limiter; document single-replica |
| OAuth apps / device flow tokens | OPT-OUT | not Phase 8 product surface |
| GitHub-compatible `ghp_` / `github_pat_` prefixes | OPT-OUT | D-08 Oxidean-only `oxidean_pat_` / `oxidean_fg_` |
