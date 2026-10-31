// Port of crates/oxidean-web/src/proxy.rs — reverse proxy to OXIDEAN_API_ORIGIN.
//
// Two transports:
//   - forward()        — undici fetch. Handles every regular request; streams
//                        request and response bodies; required for internal
//                        watt mesh targets (*.plt.local resolves only through
//                        the runtime's undici dispatcher, not node:http).
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
import { Readable } from 'node:stream'
import zlib from 'node:zlib'
import { compressible, acceptsGzip } from './compress.js'

// Hop-by-hop headers never cross a proxy hop.
const HOP_BY_HOP = new Set(['host', 'connection', 'transfer-encoding', 'keep-alive', 'upgrade'])
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
// Response headers stripped on the way back.
const RESP_HOP_BY_HOP = new Set(['connection', 'transfer-encoding', 'keep-alive'])

// Build the outbound header set per the rules above. `isUpgrade` re-admits
// connection/upgrade so the websocket handshake crosses verbatim.
export function upstreamHeaders(req, { behindProxy, peerIp, isUpgrade = false }) {
  const out = {}
  for (const [name, value] of Object.entries(req.headers)) {
    const n = name.toLowerCase()
    if (HOP_BY_HOP.has(n) && !(isUpgrade && (n === 'connection' || n === 'upgrade'))) continue
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
    upstream = await fetch(url, {
      method: req.method,
      headers,
      body: hasBody ? Readable.toWeb(req) : null,
      ...(hasBody ? { duplex: 'half' } : {}),
      redirect: 'manual',
    })
  } catch (e) {
    console.warn(`proxy ${url} failed: ${e?.message} cause=${e?.cause?.message ?? e?.cause?.code ?? e?.cause ?? 'none'}`)
    return badGateway(res)
  }

  const respHeaders = {}
  for (const [name, value] of upstream.headers.entries()) {
    if (RESP_HOP_BY_HOP.has(name)) continue
    // Preserve multi-value headers (Set-Cookie on login sets several) —
    // writeHead accepts string[] and emits one header line per element.
    const prev = respHeaders[name]
    respHeaders[name] = prev === undefined ? value : [].concat(prev, value)
  }

  // Web-tier compression: upstream answered with a sized, compressible body
  // and no content-encoding, and the client accepts gzip. Buffered like the
  // Rust layer effectively is for shell/JSON bodies; oversized streams keep
  // Content-Length absent upstream and never reach here.
  if (
    compress &&
    acceptsGzip(req.headers['accept-encoding']) &&
    compressible({ statusCode: upstream.status, headers: respHeaders })
  ) {
    try {
      const buf = Buffer.from(await upstream.arrayBuffer())
      const gz = zlib.gzipSync(buf)
      delete respHeaders['content-length']
      delete respHeaders.etag
      respHeaders['content-encoding'] = 'gzip'
      respHeaders['content-length'] = String(gz.length)
      respHeaders.vary = respHeaders.vary ? `${respHeaders.vary}, Accept-Encoding` : 'Accept-Encoding'
      res.writeHead(upstream.status, respHeaders)
      return res.end(gz)
    } catch {
      return badGateway(res)
    }
  }

  res.writeHead(upstream.status, respHeaders)
  if (upstream.body == null) return res.end()
  try {
    for await (const chunk of Readable.fromWeb(upstream.body)) {
      if (!res.write(chunk)) await new Promise(r => res.once('drain', r))
    }
    res.end()
  } catch {
    res.destroy()
  }
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
    const lines = [`HTTP/1.1 ${upRes.statusCode} ${upRes.statusMessage ?? ''}`]
    let hasLength = false
    for (let i = 0; i < upRes.rawHeaders.length; i += 2) {
      const n = upRes.rawHeaders[i].toLowerCase()
      if (RESP_HOP_BY_HOP.has(n)) continue
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
