// Port of the path matchers in crates/oxidean-web/src/proxy.rs plus the
// Traefik edge contract from docker-compose.yml labels.
//
// Two layers, kept separate on purpose:
//   - API_PREFIXES/isApiPrefix/isApiGitPath/isApiBadgePath = what the web tier
//     proxies when OXIDEAN_API_ORIGIN is set (dev parity / single-origin).
//   - edgeRoutesToApi = what Traefik sends straight to api:8080 on :80.
// In the watt lane the gateway worker plays Traefik's role and the web worker
// keeps the full prefix list — /twirp, /oauth/authorize, … still reach the API
// through the web tier exactly like the Traefik + oxidean-web pair.

// Path prefixes the API owns (kept in sync with docker-compose Traefik
// labels and proxy.rs API_PREFIXES).
const API_PREFIXES = [
  '/api/',
  // actions/cache@v4 Twirp service — the toolkit client resolves
  // `new URL("/twirp/…", ACTIONS_RESULTS_URL)`, so it always lands at the
  // origin root regardless of the env var's path.
  '/twirp',
  '/uploads',
  // Pages site trees (pages.* materialized sites) — user HTML/JS forwards
  // verbatim; the API's CSP sandbox carries the hardening.
  '/pages',
  // Package registry surfaces (D-PKG-01).
  '/v2',
  '/npm',
  '/generic',
  // CLI distribution (install.sh + ox binaries).
  '/cli',
  // OAuth2 endpoints (API-03). `/oauth/consent` and the `/oauth/device`
  // approval page stay SPA routes; device-code creation is API-owned.
  '/oauth/authorize',
  '/oauth/token',
  '/oauth/userinfo',
  '/oauth/device/code',
  // RFC 8414/9728 discovery docs. Named literally rather than as a
  // `/.well-known/oauth` prefix: the boundary check below is segment-aware,
  // so that prefix would not match `oauth-protected-resource` (`-` is not
  // `/`). `/.well-known/acme-*` stays on the web tier for TLS challenges.
  '/.well-known/oauth-authorization-server',
  '/.well-known/oauth-protected-resource',
]

// Path-segment-aware prefix check (`/v2x` must not match `/v2`).
function segmentPrefix(path, prefix) {
  const bare = prefix.endsWith('/') ? prefix.slice(0, -1) : prefix
  return path === bare || path.startsWith(bare + '/')
}

export function isApiPrefix(path) {
  return API_PREFIXES.some(p => segmentPrefix(path, p))
}

// Git smart-HTTP + LFS surface: `/{owner}/{repo}.git[...]` — the repo segment
// carries the `.git` suffix, so this can't be a prefix. Mirrors Traefik's
// `api-git` `PathRegexp(^/[^/]+/[^/]+\.git)` exactly, including the bare
// `/o/x.git` case (API 404s it there; identical here).
export function isApiGitPath(path) {
  const segs = path.replace(/^\/+/, '').split('/')
  return segs.length >= 2 && segs[0] !== '' && segs[1].endsWith('.git')
}

// GitHub-shaped workflow badge: `/{owner}/{repo}/actions/workflows/{file}/
// badge.svg` — README <img> URLs can't live under /api/ when users copy
// GitHub badge markup, so the repo-path shape forwards to the API. The
// workflow segment is a single flat filename.
export function isApiBadgePath(path) {
  const segs = path.replace(/^\/+/, '').split('/')
  return (
    segs.length === 6 &&
    segs[0] !== '' &&
    segs[1] !== '' &&
    segs[2] === 'actions' &&
    segs[3] === 'workflows' &&
    segs[4] !== '' &&
    segs[5] === 'badge.svg'
  )
}

// The Traefik label contract on :80 (docker-compose.yml). Everything listed
// here goes to the API; all other paths fall through to the web tier.
//
// Traefik PathPrefix is a RAW string-prefix match — `/apiary` hits the API
// in the baseline, so the gateway uses startsWith for the declared prefixes
// to stay faithful. (proxy.rs's segment-aware rule only applies to paths the
// web worker proxies, which is a different layer.)
const EDGE_RAW_PREFIXES = [
  '/api',
  '/uploads',
  '/.well-known/oauth',
  '/v2',
  '/npm',
  '/generic',
  '/cli',
  '/pages',
]

export function edgeRoutesToApi(path) {
  // Priority 110 routers (api-git, api-packages, api-badge, api-pages).
  if (isApiGitPath(path) || isApiBadgePath(path)) return true
  // Priority 100 api router + package/pages prefixes, raw match like Traefik.
  if (EDGE_RAW_PREFIXES.some(p => path.startsWith(p))) return true
  if (path === '/health') return true
  return false
}

// Does `path` contain a `..` segment after folding %2e/%2E to `.`?
// Both consumers that treat the path structurally — the URL parser upstream
// and the filesystem join — resolve dot segments, so the check runs on the
// decoded form, not just raw bytes. Rejected with the SPA 404, not 400.
export function hasDotSegments(path) {
  return path.split('/').some(seg => seg.replace(/%2e/gi, '.') === '..')
}
