// Watt PoC gateway worker (issue #209): plays Traefik's role on :80 —
// the edge routing contract from docker-compose.yml labels — and proxies:
//   - API-owned surfaces → OXIDEAN_API_ORIGIN (api:8080)
//   - everything else    → OXIDEAN_WEB_ORIGIN (the internal `web` watt worker)
//
// This worker IS the public edge: client-supplied X-Forwarded-* are stripped
// and re-set from observed truth (edge mode, behindProxy=false), so the web
// worker and the API see honest forwarding headers exactly like they did
// behind Traefik.
import Fastify from 'fastify'

import { edgeRoutesToApi, hasDotSegments } from '../../shared/apipaths.js'
import { forward, forwardUpgrade } from '../../shared/proxy.js'

const env = (name, dflt) => {
  const v = process.env[name]
  return v == null || v === '' ? dflt : v
}

export async function create() {
  const api = env('OXIDEAN_API_ORIGIN', 'http://api:8080').replace(/\/+$/, '')
  const web = env('OXIDEAN_WEB_ORIGIN', 'http://web.plt.local').replace(/\/+$/, '')
  const log = console

  const edgeOpts = { behindProxy: false }

  function targetFor(pathname) {
    return edgeRoutesToApi(pathname) ? api : web
  }

  async function handle(req, res) {
    const qIdx = req.url.indexOf('?')
    const p = qIdx < 0 ? req.url : req.url.slice(0, qIdx)
    // Same dot-segment rejection as the web tier — a traversal must never
    // reach an upstream URL parser or a dist/ join.
    if (hasDotSegments(p)) {
      res.writeHead(404, { 'cache-control': 'no-store' })
      return res.end('Not Found')
    }
    // Gateway does not compress (Traefik didn't either); pass bytes through.
    return forward(req, res, targetFor(p), { ...edgeOpts, compress: false })
  }

  const app = Fastify({ logger: false })
  // See the web worker: catch-all route + passthrough parser keep proxied
  // request bodies alive on request.raw (the 404 path drains them first).
  app.addContentTypeParser(['application/json', 'text/plain', '*'], (request, body, done) => done(null, body))
  const rawHandler = (request, reply) => {
    reply.hijack()
    handle(request.raw, reply.raw).catch(e => {
      log.warn(`gateway error: ${e?.stack ?? e}`)
      if (!reply.raw.headersSent) reply.raw.writeHead(502)
      reply.raw.end('API unreachable')
    })
  }
  app.route({
    method: ['GET', 'HEAD', 'POST', 'PUT', 'PATCH', 'DELETE', 'OPTIONS'],
    url: '/*',
    handler: rawHandler,
  })
  app.setNotFoundHandler(rawHandler)

  // Websocket upgrade tunnel (/api/rpc/ws) — Fastify doesn't see 'upgrade'
  // events; splice at the raw http.Server level.
  app.server.on('upgrade', (req, socket, head) => {
    const qIdx = (req.url ?? '/').indexOf('?')
    const p = qIdx < 0 ? req.url : req.url.slice(0, qIdx)
    try {
      forwardUpgrade(req, socket, head, targetFor(p), edgeOpts)
    } catch (e) {
      log.warn(`upgrade failed: ${e?.stack ?? e}`)
      socket.destroy()
    }
  })

  return app
}
