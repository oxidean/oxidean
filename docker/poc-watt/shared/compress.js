// Port of the CompressionLayer predicate in crates/oxidean-web/src/main.rs:
// compress only worth-it bodies — >64 bytes and not SSE (streamed JSON-RPC
// over /api/mcp must not be buffered), not an archive/image payload, and not
// already-encoded upstream. Upgrade responses (101) never reach here.
//
// Like tower_http's SizeAbove predicate, the body size is judged on the
// Content-Length header: chunked/streamed responses with unknown length are
// passed through untouched.

const EXCLUDED_TYPES = [
  'text/event-stream',
  'application/grpc',
  'application/octet-stream',
  'application/zip',
]

export function compressible({ statusCode, headers }) {
  if (statusCode < 200 || statusCode >= 300) return false
  // 206 Partial Content: re-encoding breaks the advertised Content-Range.
  if (statusCode === 206 || headers['content-range'] != null) return false
  const len = Number(headers['content-length'] ?? 0)
  if (!Number.isFinite(len) || len <= 64) return false
  if (headers['content-encoding'] != null) return false
  const ct = String(headers['content-type'] ?? '').split(';')[0].trim().toLowerCase()
  if (ct === '') return false
  if (ct.startsWith('image/')) return false
  if (EXCLUDED_TYPES.includes(ct)) return false
  return true
}

export function acceptsGzip(acceptEncoding) {
  if (acceptEncoding == null) return false
  const v = String(acceptEncoding).toLowerCase()
  // `gzip;q=0` opts out — parse the token's q parameter.
  for (const tok of v.split(',')) {
    const t = tok.trim()
    if (t === 'gzip' || t.startsWith('gzip;')) {
      const q = t.match(/q=([\d.]+)/)
      return !(q && Number(q[1]) === 0)
    }
    if (t === '*' || t.startsWith('*;')) {
      const q = t.match(/q=([\d.]+)/)
      return !(q && Number(q[1]) === 0)
    }
  }
  return false
}
