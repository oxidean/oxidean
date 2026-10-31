// oxidean-web → watt PoC port (issue #209): the web worker thread of the watt
// runtime. Serves the statically-built Octane SPA shells and assets, applies
// per-request middleware (theme class, route <title>, SSH advertise metas,
// anonymous→login gate, header-gated /health, well-known discovery docs), and
// reverse-proxies API prefixes when OXIDEAN_API_ORIGIN is set — the same
// single-origin surface the Rust binary serves, minus the edge routing the
// gateway worker owns in this lane.
//
// Source: crates/oxidean-web/src/main.rs (~470 LOC handler). Kept as a plain
// Fastify shell only because @platformatic/node manages its lifecycle; all
// request handling is raw http for byte-level parity with the Rust port.
import { createReadStream } from 'node:fs'
import fsp from 'node:fs/promises'
import path from 'node:path'
import zlib from 'node:zlib'
import Fastify from 'fastify'

import { dispatch, redirectStubTarget } from './src/shells.js'
import { inject, signedIn, findCookie } from './src/inject.js'
import { SessionValidator, SessionSignal } from './src/session.js'
import { SetupChecker } from './src/setup.js'
import { WEBMCP_PATH, MCP_PATH, webmcpDocument, mcpDocument } from './src/wellknown.js'
import { isApiPrefix, isApiGitPath, isApiBadgePath, hasDotSegments } from '../../shared/apipaths.js'
import { forward, forwardUpgrade } from '../../shared/proxy.js'
import { compressible, acceptsGzip } from '../../shared/compress.js'

const HEALTH_PATH = '/health'
const HEALTH_PROBE_HEADER = 'oxidean-health-probe'
const HEALTH_PROBE_VALUE = '1'
const ACCEPT_CH = 'accept-ch'

// Minimal extension → content-type map (Rust uses mime_guess via ServeFile).
const MIME = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript',
  '.mjs': 'text/javascript',
  '.css': 'text/css',
  '.json': 'application/json',
  '.map': 'application/json',
  '.svg': 'image/svg+xml',
  '.png': 'image/png',
  '.jpg': 'image/jpeg',
  '.jpeg': 'image/jpeg',
  '.gif': 'image/gif',
  '.webp': 'image/webp',
  '.avif': 'image/avif',
  '.ico': 'image/x-icon',
  '.woff': 'font/woff',
  '.woff2': 'font/woff2',
  '.ttf': 'font/ttf',
  '.otf': 'font/otf',
  '.txt': 'text/plain; charset=utf-8',
  '.xml': 'application/xml',
  '.wasm': 'application/wasm',
  '.webmanifest': 'application/manifest+json',
  '.pdf': 'application/pdf',
  '.zip': 'application/zip',
  '.mp4': 'video/mp4',
  '.webm': 'video/webm',
  '.mp3': 'audio/mpeg',
  '.wav': 'audio/wav',
}

const env = (name, dflt) => {
  const v = process.env[name]
  return v == null || v === '' ? dflt : v
}

export async function create() {
  const dist = path.resolve(env('OXIDEAN_WEB_DIST', 'dist'))
  const apiOrigin = (env('OXIDEAN_API_ORIGIN', null) ?? env('OXIDEAN_E2E_API_ORIGIN', null))
    ?.replace(/\/+$/, '') ?? null
  // OXIDEAN_WEB_BEHIND_PROXY=1 declares a sanitizing edge in front whose
  // X-Forwarded-* values are trusted. In the watt lane that's the gateway
  // worker — which is itself the public edge, so its chain is honest.
  const behindProxy = ['1', 'true', 'yes'].includes(
    (process.env.OXIDEAN_WEB_BEHIND_PROXY ?? '').toLowerCase(),
  )
  const meta = {
    sshHost: env('OXIDEAN_SSH_HOST', null),
    sshPort: env('OXIDEAN_SSH_PORT', null) == null ? null : Number(env('OXIDEAN_SSH_PORT')),
    publicOrigin: env('OXIDEAN_PUBLIC_ORIGIN', null)?.replace(/\/+$/, '') ?? null,
    behindProxy,
  }
  const sessions = apiOrigin ? new SessionValidator(apiOrigin) : null
  const setup = apiOrigin ? new SetupChecker(apiOrigin) : null
  const log = console

  if (!apiOrigin) {
    log.warn('OXIDEAN_API_ORIGIN unset — session stamp/gate fall back to cookie presence')
  }

  // ---- helpers -----------------------------------------------------------

  const cookieHeader = headers => {
    const c = headers.cookie
    return c == null ? null : Array.isArray(c) ? c.join('; ') : String(c)
  }

  async function resolveSignal(cookie) {
    if (cookie == null) return SessionSignal.Absent
    const token = findCookie(cookie, 'oxidean_session')
    if (token == null || token === '') return SessionSignal.Absent
    return sessions ? sessions.checkToken(token) : SessionSignal.Unknown
  }

  // Central body send with the compression predicate from shared/compress.js.
  function sendBody(req, res, status, headers, buf) {
    const head = { 'content-length': String(buf.length), ...headers }
    if (acceptsGzip(req.headers['accept-encoding']) && compressible({ statusCode: status, headers: head })) {
      const gz = zlib.gzipSync(buf)
      delete head.etag
      head['content-encoding'] = 'gzip'
      head['content-length'] = String(gz.length)
      head.vary = head.vary ? `${head.vary}, Accept-Encoding` : 'Accept-Encoding'
      res.writeHead(status, head)
      res.end(req.method === 'HEAD' ? undefined : gz)
      return
    }
    res.writeHead(status, head)
    res.end(req.method === 'HEAD' ? undefined : buf)
  }

  function redirect(res, status, location) {
    res.writeHead(status, { location })
    res.end()
  }

  // `returnTo=` needs the path URL-encoded (`/` stays — readable and matches
  // the old `?returnTo=/settings` shape).
  function urlencoding(s) {
    const bytes = Buffer.from(s, 'utf8')
    let out = ''
    for (const b of bytes) {
      const ch = String.fromCharCode(b)
      out += /[A-Za-z0-9/\-_.~]/.test(ch) ? ch : '%' + b.toString(16).toUpperCase().padStart(2, '0')
    }
    return out
  }

  function health(req, res) {
    const v = req.headers[HEALTH_PROBE_HEADER]
    const ok = (Array.isArray(v) ? v[0] : v) === HEALTH_PROBE_VALUE
    if (!ok) {
      res.writeHead(404, { 'content-type': 'text/plain; charset=utf-8' })
      return res.end('Not Found')
    }
    res.writeHead(200, {
      'content-type': 'application/json; charset=utf-8',
      'cache-control': 'no-store',
    })
    res.end('{"ok":true}')
  }

  function jsonDoc(res, doc) {
    res.writeHead(200, {
      'content-type': 'application/json; charset=utf-8',
      'cache-control': 'public, max-age=300',
    })
    res.end(JSON.stringify(doc))
  }

  // Would tryStatic serve dist{path}? Same guards, without a stat race in the
  // asset-overlap branch needing to know before falling back to the shell.
  async function staticFilePresent(p) {
    const rel = p.replace(/^\/+/, '')
    if (rel === '' || rel.endsWith('.html')) return false
    try {
      const st = await fsp.stat(path.join(dist, rel))
      return st.isFile()
    } catch {
      return false
    }
  }

  // Serve dist{path} for literal asset files only; rejects all `.html` —
  // shell HTML under dist is dispatch-owned (gate + per-request injection).
  // Handles Range, If-Modified-Since, and HEAD like ServeFile does.
  async function tryStatic(req, res, p) {
    const rel = p.replace(/^\/+/, '')
    if (rel === '' || rel.endsWith('.html')) return false
    const cand = path.join(dist, rel)
    let st
    try {
      st = await fsp.stat(cand)
    } catch {
      return false
    }
    if (!st.isFile()) return false
    const cache =
      rel.startsWith('_astro/') || rel.startsWith('fonts/')
        ? 'public, max-age=31536000, immutable'
        : 'public, max-age=3600'
    const type = MIME[path.extname(rel).toLowerCase()] ?? 'application/octet-stream'
    const head = {
      'content-type': type,
      'cache-control': cache,
      'last-modified': st.mtime.toUTCString(),
      'accept-ranges': 'bytes',
    }
    if (req.method !== 'GET' && req.method !== 'HEAD') {
      res.writeHead(405, { allow: 'GET, HEAD' })
      return res.end(), true
    }

    // If-Modified-Since conditional (second granularity, like httpdate).
    const ims = req.headers['if-modified-since']
    if (ims != null) {
      const since = Date.parse(String(Array.isArray(ims) ? ims[0] : ims))
      if (!Number.isNaN(since) && Math.floor(st.mtimeMs / 1000) <= Math.floor(since / 1000)) {
        res.writeHead(304, head)
        return res.end(), true
      }
    }

    // Single-range `bytes=a-b` support; malformed/multiple ranges get 200.
    const range = req.headers.range
    if (range != null && req.method !== 'HEAD') {
      const m = String(Array.isArray(range) ? range[0] : range).match(/^bytes=(\d*)-(\d*)$/)
      if (m && (m[1] !== '' || m[2] !== '')) {
        const start = m[1] === '' ? Math.max(0, st.size - Number(m[2])) : Number(m[1])
        const end = m[1] === '' ? st.size - 1 : Math.min(Number(m[2] || st.size - 1), st.size - 1)
        if (start >= st.size || start > end) {
          res.writeHead(416, { 'content-range': `bytes */${st.size}` })
          return res.end(), true
        }
        head['content-range'] = `bytes ${start}-${end}/${st.size}`
        head['content-length'] = String(end - start + 1)
        res.writeHead(206, head)
        createReadStream(cand, { start, end }).pipe(res)
        return true
      }
    }

    const buf = await fsp.readFile(cand)
    sendBody(req, res, 200, head, buf)
    return true
  }

  // 404 responses inject theme + advertise metas too. The session stamp
  // mirrors cookie *presence* only — never probed upstream on this path.
  async function notFound(req, res) {
    try {
      const html = await fsp.readFile(path.join(dist, '404.html'), 'utf8')
      const signal = signedIn(cookieHeader(req.headers))
        ? SessionSignal.Unknown
        : SessionSignal.Absent
      const out = inject(html, req.headers, null, meta, signal, null)
      sendBody(req, res, 404, {
        'content-type': 'text/html; charset=utf-8',
        'cache-control': 'no-store',
        [ACCEPT_CH]: 'Sec-CH-Prefers-Color-Scheme',
        vary: 'Cookie, Sec-CH-Prefers-Color-Scheme',
      }, Buffer.from(out, 'utf8'))
    } catch {
      res.writeHead(404, { 'cache-control': 'no-store' })
      res.end('Not Found')
    }
  }

  // Serve a dispatched shell: method gate → session signal → protected gate →
  // pattern redirect → file → Astro stub → inject.
  async function serveShell(req, res, m, p, query) {
    if (req.method !== 'GET' && req.method !== 'HEAD') {
      res.writeHead(405, { allow: 'GET, HEAD' })
      return res.end()
    }
    const cookie = cookieHeader(req.headers)
    const signal = await resolveSignal(cookie)
    const passesGate =
      signal === SessionSignal.Valid ||
      (signal === SessionSignal.Unknown && signedIn(cookie))
    if (m.protected && !passesGate) {
      const returnTo = query != null ? `${p}?${query}` : p
      return redirect(res, 302, `/login?returnTo=${urlencoding(returnTo)}`)
    }
    if (m.redirect != null) {
      const location = query != null ? `${m.redirect}?${query}` : m.redirect
      return redirect(res, 302, location)
    }
    let html
    try {
      html = await fsp.readFile(path.join(dist, m.file), 'utf8')
    } catch (e) {
      log.warn(`shell ${m.file} unreadable: ${e.message}`)
      return notFound(req, res)
    }
    // Static redirect stubs (Astro.redirect) become real 302s.
    const stub = redirectStubTarget(html)
    if (stub != null) return redirect(res, 302, stub)
    const needsSetup = setup ? await setup.needsSetup() : null
    const out = inject(html, req.headers, m.title, meta, signal, needsSetup)
    sendBody(req, res, 200, {
      'content-type': 'text/html; charset=utf-8',
      'cache-control': 'no-cache',
      [ACCEPT_CH]: 'Sec-CH-Prefers-Color-Scheme',
      vary: 'Cookie, Sec-CH-Prefers-Color-Scheme',
    }, Buffer.from(out, 'utf8'))
  }

  // Fallback handler: API prefixes → proxy; routed paths → shell dispatch
  // (which carries the auth gate); everything else → static files, then 404.
  async function spa(req, res) {
    const qIdx = req.url.indexOf('?')
    const p = qIdx < 0 ? req.url : req.url.slice(0, qIdx)
    const query = qIdx < 0 ? null : req.url.slice(qIdx + 1)

    const docMethod = req.method === 'GET' || req.method === 'HEAD'
    // These paths are registered as axum get() routes — other methods 405.
    if (!docMethod && (p === HEALTH_PATH || p === WEBMCP_PATH || p === MCP_PATH)) {
      res.writeHead(405, { allow: 'GET, HEAD' })
      return res.end()
    }
    if (docMethod && p === HEALTH_PATH) return health(req, res)
    if (docMethod && p === WEBMCP_PATH) return jsonDoc(res, webmcpDocument())
    if (docMethod && p === MCP_PATH) return jsonDoc(res, mcpDocument())

    // Dot segments (literal or %-encoded) normalize before reaching the
    // upstream URL parser or the filesystem join — reject outright.
    if (hasDotSegments(p)) return notFound(req, res)

    // API-owned prefixes (dev parity / single-origin deploys without the
    // gateway in front) plus the `/{owner}/{repo}.git/*` + badge surfaces.
    if (apiOrigin && (isApiPrefix(p) || isApiGitPath(p) || isApiBadgePath(p))) {
      return forward(req, res, apiOrigin, { behindProxy, compress: true })
    }

    // A dotted final segment on an asset-overlapping pattern may be a real
    // file under dist/ — prefer it, else 404 so missing chunks never return
    // 200 HTML. Literal-prefix routes own their dotted params.
    const lastSeg = p.split('/').pop() ?? ''
    const looksLikeFile = lastSeg.includes('.')

    const m = dispatch(p)
    if (m != null) {
      if (looksLikeFile && m.assetOverlap) {
        if (await staticFilePresent(p)) {
          if (await tryStatic(req, res, p)) return
          return notFound(req, res)
        }
        // …except `/{owner}/{repo}` where repo is a legal *dotted* name
        // (`my.app`, `three.js`): when `owner` doesn't resolve to a real dist
        // entry the path is a repo page, not an asset.
        const segs = p.replace(/^\/+|\/+$/g, '').split('/')
        if (segs.length === 2) {
          try {
            await fsp.stat(path.join(dist, segs[0]))
          } catch {
            return serveShell(req, res, m, p, query)
          }
        }
        return notFound(req, res)
      }
      return serveShell(req, res, m, p, query)
    }

    if (await tryStatic(req, res, p)) return
    return notFound(req, res)
  }

  // Fastify is the lifecycle shell @platformatic/node manages — every request
  // lands in the raw handler above so response bytes match the Rust port.
  // Two details matter for proxy parity:
  //   - the catch-all route (not setNotFoundHandler): Fastify drains request
  //     bodies on the 404 path before the handler runs, which would silently
  //     drop proxied POST bodies
  //   - the '*' passthrough parser: leaves the body stream unconsumed so
  //     forward() can pipe request.raw upstream
  const app = Fastify({ logger: false })
  app.addContentTypeParser(['application/json', 'text/plain', '*'], (request, body, done) => done(null, body))
  const rawHandler = (request, reply) => {
    reply.hijack()
    spa(request.raw, reply.raw).catch(e => {
      log.warn(`handler error: ${e?.stack ?? e}`)
      if (!reply.raw.headersSent) reply.raw.writeHead(500)
      reply.raw.end('Internal Server Error')
    })
  }
  app.route({
    method: ['GET', 'HEAD', 'POST', 'PUT', 'PATCH', 'DELETE', 'OPTIONS'],
    url: '/*',
    handler: rawHandler,
  })
  // Anything the route table doesn't cover (TRACE, CONNECT…) still lands in
  // the same handler — bodies on that path are already drained by Fastify,
  // which is fine: those methods never carry a proxied payload.
  app.setNotFoundHandler(rawHandler)

  // Websocket parity when this worker faces TCP directly (dev/standalone,
  // no gateway in front): tunnel API-path upgrades like the Rust proxy does.
  // In the watt lane the gateway worker owns 'upgrade' on :80 and these
  // events never arrive over the internal mesh.
  if (apiOrigin) {
    app.server.on('upgrade', (req, socket, head) => {
      const qIdx = (req.url ?? '/').indexOf('?')
      const p = qIdx < 0 ? req.url : req.url.slice(0, qIdx)
      if (isApiPrefix(p) || isApiGitPath(p)) {
        try {
          forwardUpgrade(req, socket, head, apiOrigin, { behindProxy })
        } catch (e) {
          log.warn(`upgrade failed: ${e?.stack ?? e}`)
          socket.destroy()
        }
      } else {
        socket.destroy()
      }
    })
  }
  return app
}
