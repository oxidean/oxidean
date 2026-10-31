// Port of crates/oxidean-web/src/inject.rs — per-request HTML rewriting on
// shell bytes: <html> theme class, color-scheme meta, route <title>, session /
// setup / signup stamps, SSH advertise metas, and the OXIDEAN_PUBLIC_ORIGIN
// rewrite of the build-time http://localhost placeholder.
//
// `headers` is the Node req.headers object (lowercase names, string|string[]).

// "light"/"dark" after applying the same chain the FOUC boot script mirrors
// into cookies: explicit oxidean-theme cookie → resolved oxidean-color-scheme
// cookie → Sec-CH-Prefers-Color-Scheme header → light.
export function resolveTheme(cookie, secCh) {
  if (cookie != null) {
    for (const name of ['oxidean-theme', 'oxidean-color-scheme']) {
      const m = findCookie(cookie, name)
      if (m === 'light' || m === 'dark') return m
    }
  }
  if (secCh != null) {
    const ch = secCh.trim().toLowerCase()
    if (ch === 'dark' || ch === 'light') return ch
  }
  return 'light'
}

// `name=value` lookup inside a Cookie header (no decoding — our cookies are
// all ASCII flag values).
export function findCookie(header, name) {
  for (const pair of header.split(';')) {
    const p = pair.trim()
    const eq = p.indexOf('=')
    if (eq > 0 && p.slice(0, eq) === name) {
      let v = p.slice(eq + 1)
      if (v.startsWith('"') && v.endsWith('"')) v = v.slice(1, -1)
      return v
    }
  }
  return null
}

// Anonymous-gate input: the `oxidean_signed_in=1` presence flag OR a real
// `oxidean_session` cookie. The hint is client-set and can be absent on
// sessions minted before it existed — the HttpOnly session cookie is the
// stronger signal. Neither is authoritative for ACL.
export function signedIn(cookie) {
  if (cookie == null) return false
  if (findCookie(cookie, 'oxidean_signed_in') === '1') return true
  const tok = findCookie(cookie, 'oxidean_session')
  return tok != null && tok !== ''
}

// Build-time origin placeholder. Astro prerenders shells before the deploy
// origin exists; the serving tier rewrites it per request. Never carries a
// port: `http://localhost:8080` in docs/example text must survive untouched.
const BUILD_ORIGIN_PLACEHOLDER = 'http://localhost'

// Public origin for this request: configured OXIDEAN_PUBLIC_ORIGIN wins;
// otherwise derive from the request — X-Forwarded-Host/Proto only behind a
// declared edge (behindProxy), raw Host always.
function requestOrigin(headers, behindProxy) {
  const hdr = name => {
    const v = headers[name]
    const s = Array.isArray(v) ? v[0] : v
    const t = s == null ? null : String(s).trim()
    return t || null
  }
  const host = (behindProxy ? hdr('x-forwarded-host') : null) ?? hdr('host')
  if (!host) return null
  const protoHdr = behindProxy ? hdr('x-forwarded-proto') : null
  const proto = protoHdr === 'https' || protoHdr === 'http' ? protoHdr : 'http'
  return `${proto}://${host}`
}

// Replace every bare `http://localhost` placeholder with the resolved origin.
// `http://localhost:PORT` occurrences are left alone.
function rewriteBuildOrigin(html, origin) {
  if (origin === BUILD_ORIGIN_PLACEHOLDER) return html
  let out = html
  let from = 0
  for (;;) {
    const rel = out.indexOf(BUILD_ORIGIN_PLACEHOLDER, from)
    if (rel < 0) return out
    const end = rel + BUILD_ORIGIN_PLACEHOLDER.length
    if (out[end] === ':') { from = end; continue }
    out = out.slice(0, rel) + origin + out.slice(end)
    from = rel + origin.length
  }
}

function escapeAttr(s) {
  return s.replace(/&/g, '&amp;').replace(/"/g, '&quot;').replace(/</g, '&lt;')
}

// Index just before the `>` closing the opening `<html …>` tag — attribute
// order-insensitive. Null when the document has no `<html>` open tag.
function htmlTagOpenEnd(s) {
  const start = s.indexOf('<html')
  if (start < 0) return null
  const next = s[start + '<html'.length]
  if (!' \t\r\n>/'.includes(next)) return null
  const rel = s.indexOf('>', start)
  return rel < 0 ? null : rel
}

// Add `class` to the `<html>` tag — merges into an existing `class="…"`
// (duplicated attributes are dropped by parsers).
function stampClass(html, cls) {
  const end = htmlTagOpenEnd(html)
  if (end == null) return html
  const start = html.indexOf('<html')
  const cs = html.slice(start, end).indexOf('class="')
  if (cs >= 0) {
    const valStart = start + cs + 'class="'.length
    const valLen = html.slice(valStart, end).indexOf('"')
    if (valLen >= 0) {
      const cur = html.slice(valStart, valStart + valLen)
      if (!cur.split(/\s+/).includes(cls)) {
        return html.slice(0, valStart + valLen) + ' ' + cls + html.slice(valStart + valLen)
      }
      return html
    }
  }
  return html.slice(0, end) + ` class="${cls}"` + html.slice(end)
}

// Rewrite `content="…"` on the `<meta name="{name}">` tag.
function setMetaContent(html, name, value) {
  const needle = `name="${name}"`
  let out = html
  let from = 0
  for (;;) {
    const rel = out.indexOf('<meta', from)
    if (rel < 0) return out
    const tagLen = out.indexOf('>', rel)
    if (tagLen < 0) return out
    if (out.slice(rel, tagLen).includes(needle)) {
      const valRel = out.slice(rel, tagLen).indexOf('content="')
      if (valRel < 0) return out
      const valStart = rel + valRel + 'content="'.length
      const valLen = out.slice(valStart, tagLen).indexOf('"')
      if (valLen < 0) return out
      return out.slice(0, valStart) + value + out.slice(valStart + valLen)
    }
    from = tagLen
  }
}

// Session signal values — see src/session.js.
// 'valid' stamps unconditionally, 'invalid'/'absent' never, 'unknown' falls
// back to cookie presence.
export function inject(html, headers, title, meta, signal, needsSetup) {
  const cookie = headers.cookie == null ? null : String(Array.isArray(headers.cookie) ? headers.cookie.join('; ') : headers.cookie)
  const secCh = headers['sec-ch-prefers-color-scheme'] == null
    ? null
    : String(headers['sec-ch-prefers-color-scheme'])
  const theme = resolveTheme(cookie, secCh)

  let out = html

  if (theme === 'dark') out = stampClass(out, 'dark')
  let htmlAttrs = ''
  const stamped = signal === 'valid' ? true : signal === 'unknown' ? signedIn(cookie) : false
  if (stamped) htmlAttrs += ' data-oxidean-session="1"'
  // Open-registration companion flag written by the client once
  // auth.provider_config resolves (see Rust inject).
  if (cookie != null && findCookie(cookie, 'oxidean_allow_signup') === '1') {
    htmlAttrs += ' data-oxidean-signup="1"'
  }
  if (needsSetup != null) {
    htmlAttrs += needsSetup ? ' data-oxidean-setup="1"' : ' data-oxidean-setup="0"'
  }
  if (htmlAttrs) {
    const end = htmlTagOpenEnd(out)
    if (end != null) out = out.slice(0, end) + htmlAttrs + out.slice(end)
  }
  if (theme === 'dark') out = setMetaContent(out, 'color-scheme', 'dark')

  // Route <title> — shells ship a generic "Oxidean" title for dynamic pages.
  if (title != null) {
    const a = out.indexOf('<title>')
    const b = out.indexOf('</title>')
    if (a >= 0 && b >= 0) out = out.slice(0, a) + `<title>${title}</title>` + out.slice(b + '</title>'.length)
  }

  const origin = meta.publicOrigin ?? requestOrigin(headers, meta.behindProxy)
  if (origin != null) out = rewriteBuildOrigin(out, origin)

  let extra = ''
  if (origin != null) {
    extra += `<meta name="oxidean:public-origin" content="${escapeAttr(origin)}">`
  }
  if (meta.sshHost != null) {
    extra += `<meta name="oxidean:ssh-host" content="${escapeAttr(meta.sshHost)}">`
  }
  if (meta.sshPort != null) {
    extra += `<meta name="oxidean:ssh-port" content="${meta.sshPort}">`
  }
  if (extra) {
    const pos = out.lastIndexOf('</head>')
    if (pos >= 0) out = out.slice(0, pos) + extra + out.slice(pos)
  }
  return out
}
