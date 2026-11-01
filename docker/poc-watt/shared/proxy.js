// Port of crates/oxidean-web/src/proxy.rs — reverse proxy to OXIDEAN_API_ORIGIN.
//
// Two transports:
//   - forward()        — undici.request. Handles every regular request;
//                        streams request and response bodies; required for
//                        internal watt mesh targets (*.plt.local resolves only
//                        through the runtime's undici dispatcher, not node:http).
//                        `request` (not `fetch`) so response bytes pass raw —
//                        fetch transparently decodes content-encoding while its
//                        headers still claim the encoding, and the gateway must
//                        relay compressed bodies untouched like Traefik does.
//   - forwardUpgrade() — node:http. Raw TCP splice for websocket handshakes
//                        (e.g. /api/rpc/ws); fetch can't drive an upgrade.
//
// Header semantics mirror the Rust:
//   - hop-by-hop headers never cross (connection/upgrade re-admitted only for
//     genuine upgrade requests)
//   - behindProxy=false (this process IS the edge): client-supplied
//     X-Forwarded-*/Forwarded/etc. are attacker input — stripped, then re-set
//     from observed truth (peer IP, Host, proto http)
//   - behindProxy=true (sanitizing edge in front): the edge's chain forwards
//     verbatim; X-Forwarded-Host is filled from Host only when the edge left
//     it unset
import http from 'node:http'
import { pipeline } from 'node:stream/promises'
import zlib from 'node:zlib'
import { request as undiciRequest } from 'undici'
import { compressible, acceptsGzip } from './compress.js'

// Hop-by-hop headers never cross a proxy hop — the fixed set plus whatever
// the request's `Connection` header nominates (RFC 9110 §7.6.1).
const HOP_BY_HOP = new Set([
  'host',
  'connection',
  'transfer-encoding',
  'keep-alive',
  'upgrade',
  'te',
  'trailer',
  'proxy-connection',
  'proxy-authorization',
  'proxy-authenticate',
])
// Client-settable headers describing the request to the next hop.
const FORWARDED_HEADERS = new Set([
  'forwarded',
  'x-forwarded-for',
  'x-forwarded-host',
  'x-forwarded-proto',
  'x-forwarded-server',
  'x-real-ip',
  'x-client-ip',
  'cf-connecting-ip',
  'true-client-ip',
])
// Response headers stripped on the way back — the fixed hop-by-hop set.
const RESP_HOP_BY_HOP = new Set([
  'connection',
  'transfer-encoding',
  'keep-alive',
  'upgrade',
  'te',
  'trailer',
  'proxy-connection',
  'proxy-authorization',
  'proxy-authenticate',
])

// Fields `Connection` names are hop-by-hop too (RFC 9110 §7.6.1) — applies to
// request AND response headers crossing this hop.
function connNominated(value) {
  if (value == null) return null
  const s = new Set(
    String(value)
      .split(',')
      .map(t => t.trim().toLowerCase())
      .filter(Boolean),
  )
  return s.size ? s : null
}

// Build the outbound header set per the rules above. `isUpgrade` re-admits
// connection/upgrade so the websocket handshake crosses verbatim.
export function upstreamHeaders(req, { behindProxy, peerIp, isUpgrade = false }) {
  const out = {}
  // `Connection: keep-alive, X-Foo` must not forward X-Foo. Skipped for
  // genuine upgrades, whose connection/upgrade pair is itself the handshake
  // and re-admitted below.
  const nominated = isUpgrade ? null : connNominated(req.headers.connection)
  for (const [name, value] of Object.entries(req.headers)) {
    const n = name.toLowerCase()
    if (HOP_BY_HOP.has(n) && !(isUpgrade && (n === 'connection' || n === 'upgrade'))) continue
    if (nominated?.has(n)) continue
    if (!behindProxy && FORWARDED_HEADERS.has(n)) continue
    out[n] = value
  }
  const host = req.headers.host
  if (behindProxy) {
    if (out['x-forwarded-host'] == null && host != null) out['x-forwarded-host'] = host
  } else {
    out['x-forwarded-for'] = peerIp ?? req.socket?.remoteAddress ?? ''
    out['x-forwarded-proto'] = 'http'
    if (host != null) out['x-forwarded-host'] = host
  }
  return out
}

function badGateway(res) {
  if (!res.headersSent) {
    res.writeHead(502, { 'content-type': 'text/plain; charset=utf-8' })
  }
  res.end('API unreachable')
}

// Forward a regular request upstream and stream the response back.
//   req          — client http.IncomingMessage
//   res          — client http.ServerResponse
//   target       — origin string, e.g. 'http://api:8080' or 'http://web.plt.local'
//   opts.compress — gzip upstream bodies when the tower_http predicate passes
//                   (web tier only; the gateway worker passes bytes through)
export async function forward(req, res, target, opts = {}) {
  const { behindProxy = false, peerIp = req.socket?.remoteAddress, compress = false } = opts
  // req.url is normally origin-form, but a client CAN send absolute-form
  // (`GET http://host/x`) — new URL() would then ignore `target` entirely and
  // turn this into an open proxy. Take only path+query from the parsed URL so
  // the upstream origin is always `target` (same as the Rust
  // format!("{api_origin}{path_and_query}")).
  const parsed = new URL(req.url, target)
  const url = new URL(parsed.pathname + parsed.search, target)
  const headers = upstreamHeaders(req, { behindProxy, peerIp })
  const hasBody = req.method !== 'GET' && req.method !== 'HEAD'
  let upstream
  try {
    upstream = await undiciRequest(url, {
      method: req.method,
      headers,
      body: hasBody ? req : undefined,
    })
  } catch (e) {
    console.warn(`proxy ${url} failed: ${e?.message} cause=${e?.cause?.message ?? e?.cause?.code ?? e?.cause ?? 'none'}`)
    return badGateway(res)
  }

  const respNominated = connNominated(upstream.headers.connection)
  const respHeaders = {}
  for (const [name, value] of Object.entries(upstream.headers)) {
    const n = name.toLowerCase()
    if (RESP_HOP_BY_HOP.has(n) || respNominated?.has(n)) continue
    // Preserve multi-value headers (Set-Cookie on login sets several) —
    // undici already yields string[]; writeHead emits one line per element.
    respHeaders[n] = value
  }

  // Web-tier compression: upstream answered with a sized, compressible body
  // and no content-encoding, and the client accepts gzip. Streamed through
  // zlib like tower_http's adapter — bounded memory, no event-loop-blocking
  // gzipSync; Content-Length/ETag drop because the encoded size is only known
  // when the stream ends (chunked transfer like the Rust layer emits).
  if (
    compress &&
    acceptsGzip(req.headers['accept-encoding']) &&
    compressible({ statusCode: upstream.statusCode, headers: respHeaders })
  ) {
    delete respHeaders['content-length']
    delete respHeaders.etag
    respHeaders['content-encoding'] = 'gzip'
    respHeaders.vary = respHeaders.vary ? `${respHeaders.vary}, Accept-Encoding` : 'Accept-Encoding'
    res.writeHead(upstream.statusCode, respHeaders)
    pipeline(upstream.body, zlib.createGzip(), res).catch(() => res.destroy())
    return
  }

  res.writeHead(upstream.statusCode, respHeaders)
  // pipeline: backpressure + destroys the upstream stream when the client
  // disconnects mid-response (SSE, package blobs, git fetches) instead of
  // leaking the connection waiting for a drain that never comes.
  pipeline(upstream.body, res).catch(() => res.destroy())
}

// Websocket upgrade splice: forward the handshake upstream via node:http
// (real DNS targets — the only upgrade surface is /api/rpc/ws → api:8080),
// then pipe the two raw sockets bidirectionally.
//   req    — the 'upgrade' event's http.IncomingMessage
//   socket — client net.Socket
//   head   — first buffered packet after the handshake, if any
export function forwardUpgrade(req, socket, head, target, opts = {}) {
  const { behindProxy = false, peerIp = req.socket?.remoteAddress } = opts
  const t = new URL(target)
  const headers = upstreamHeaders(req, { behindProxy, peerIp, isUpgrade: true })
  const upstream = http.request({
    hostname: t.hostname,
    port: t.port || 80,
    path: req.url,
    method: req.method,
    headers,
    agent: false,
  })

  upstream.on('upgrade', (upRes, upSocket, upHead) => {
    // Relay the 101 handshake verbatim — sec-websocket-accept/protocol are
    // end-to-end headers.
    let raw = `HTTP/${upRes.httpVersion} ${upRes.statusCode} ${upRes.statusMessage ?? ''}\r\n`
    for (let i = 0; i < upRes.rawHeaders.length; i += 2) {
      raw += `${upRes.rawHeaders[i]}: ${upRes.rawHeaders[i + 1]}\r\n`
    }
    socket.write(raw + '\r\n')
    if (head?.length) upSocket.write(head)
    if (upHead?.length) socket.write(upHead)
    upSocket.pipe(socket)
    socket.pipe(upSocket)
    upSocket.on('error', () => socket.destroy())
    socket.on('error', () => upSocket.destroy())
  })

  // Upstream refused the upgrade with a normal response (auth, 404, …) —
  // relay it as an HTTP response on the raw socket.
  upstream.on('response', upRes => {
    const upNominated = connNominated(upRes.headers.connection)
    const lines = [`HTTP/1.1 ${upRes.statusCode} ${upRes.statusMessage ?? ''}`]
    let hasLength = false
    for (let i = 0; i < upRes.rawHeaders.length; i += 2) {
      const n = upRes.rawHeaders[i].toLowerCase()
      if (RESP_HOP_BY_HOP.has(n) || upNominated?.has(n)) continue
      if (n === 'content-length') hasLength = true
      lines.push(`${upRes.rawHeaders[i]}: ${upRes.rawHeaders[i + 1]}`)
    }
    if (!hasLength) lines.push('connection: close')
    socket.write(lines.join('\r\n') + '\r\n\r\n')
    upRes.pipe(socket)
    if (!hasLength) upRes.on('end', () => socket.end())
  })

  upstream.on('error', () => socket.destroy())
  upstream.end()
}
