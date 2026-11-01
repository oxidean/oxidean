# PoC: watt web+edge (issue #209)

One Platformatic Watt process replacing the `traefik` + `oxidean-web` pair.
The Bun-built `apps/web/dist` is copied into a Node 22 image that runs
`wattpm` with two `@platformatic/node` worker applications:

| Watt app | Replaces | Role |
|----------|----------|------|
| `services/gateway` | `traefik` | Public edge on :80 — the `docker-compose.yml` label contract (`edgeRoutesToApi`): API prefixes, `/{o}/{r}.git`, badge SVGs, `/pages`, `/health` → `api:8080`; everything else → `web.plt.local` (the watt mesh). Websocket upgrades spliced at the raw socket. |
| `services/web` | `oxidean-web` | Port of `crates/oxidean-web` middleware (~3.4k LOC Rust → ~1.1k LOC JS): shell dispatch, HTML injection, session/setup probes, static files, 404, header-gated `/health`, well-known docs, plus the single-origin API proxy for prefixes the edge doesn't claim (`/twirp`, `/oauth/*`, …). |

The gateway is a hand-rolled `@platformatic/node` app (an "in-app proxy
plugin" per the issue) rather than `@platformatic/gateway` — the `.git`
path-regexp rule, edge-mode `X-Forwarded-*` rewriting, and the upgrade
splice are all explicit in `shared/proxy.js` + `shared/apipaths.js` instead
of hidden behind composer config we can't verify without docker.

## Middleware porting status

| Item (issue checklist) | Status | Where |
|------------------------|--------|-------|
| Route→shell dispatch incl. asset-overlap rule | ported | `services/web/src/shells.js` + `shells-table.js` (table extracted verbatim from `shells.rs`) |
| Per-request HTML injection (theme CH + Accept-CH, `<title>`, SSH metas, `http://localhost` rewrite, session/setup/signup stamps) | ported | `services/web/src/inject.js` |
| Anonymous→login gate (10s cached session probe) + setup stamp | ported | `services/web/src/session.js`, `setup.js`, gate in `server.js` |
| API reverse proxy (hop-by-hop filter, X-Forwarded-* edge vs behind-proxy, segment-aware prefixes, WS upgrade, SSE pass-through) | ported | `shared/proxy.js` (`undici.request` for regular traffic — the mesh dispatcher resolves `*.plt.local` and, unlike `fetch`, relays encoded bytes raw — `node:http` for the upgrade splice) |
| Compression with same exclusions (SSE, archives, images, >64B, no re-encode) | ported | `shared/compress.js` (predicate) — applied by the web worker; gateway passes bytes through (Traefik never compressed either) |
| Header-gated `/health` + `/.well-known/{webmcp,mcp}` | ported | `services/web/server.js` + `src/wellknown.js` |
| Dot-segment/path-traversal rejection | ported | `shared/apipaths.js` (`hasDotSegments`), enforced in both workers |

### Known deltas vs the Rust edge (deliberate, PoC scope)

- `Range`/`If-Modified-Since` on static files is a minimal port (single
  `bytes=a-b` range; no `If-Unmodified-Since`/`If-Range`, no multipart).
- Fastify is only a lifecycle shell (catch-all route + `reply.hijack()`)
  — all response bytes are produced by the ported raw handlers. A passthrough
  content-type parser keeps proxied request bodies alive on `request.raw`.
- Request tracing (`TraceLayer`) is not ported; watt's own logger level is
  configurable via `PLT_SERVER_LOGGER_LEVEL`.
- `/health` on the web worker is internal-only exactly like `web:3000/health`
  behind Traefik — the edge routes `/health` → `api:8080`.
- Traefik `PathPrefix` is a raw string prefix (not segment-aware), so the
  gateway matches `/apiary`-style paths to the API exactly like the baseline.
- Traefik's routers are also `Host(\`localhost\`)`-gated; the watt gateway is
  host-agnostic (the Host gate is a routing constraint, not middleware).
  Benchmarks therefore use `http://localhost` for both lanes.
- Open-proxy hardening: only path+query is taken from the request line —
  an absolute-form request target (`GET http://host/x`) can't redirect the
  proxy to a different upstream (matches the Rust `format!("{origin}{path}")`).
- Themed status pages (`oxidean-status-pages`, landed on main after this
  PoC) are not ported — a missing `dist/404.html` degrades to a bare
  text/plain 404 instead of the branded page, and `405`/bare proxy errors
  are not themed either. Benchmark classes are unaffected (error paths are
  not in the harness).
- Request-derived origin host (raw `Host`, or `X-Forwarded-Host` behind the
  declared edge) is validated as hostname/IP-literal + optional port in
  `inject.js`; a malformed value drops the origin instead of writing
  attacker text into rewritten HTML. Stricter than the Rust, which trusts
  the header value hyper already parsed.

## Run

```bash
docker compose -f docker-compose.yml -f docker-compose.watt.yml up -d postgres api watt
# http://localhost → gateway → web (or api:8080 for API-owned paths)
```

`scripts/bench-web-edge.sh` runs both lanes and emits JSON to `tmp/`.
Benchmarks are pending a docker-capable machine — see issue #209.
