// Port of crates/oxidean-web/src/shells.rs — request-path → shell dispatch.
// The table itself lives in ./shells-table.js (extracted verbatim from the
// Rust source). Semantics mirror the Rust implementation:
//   - patterns are tried by literal-segment score (more literals win, earlier
//     entries break ties)
//   - `{param}` binds one segment, `{param...}` (splat) binds the rest
//   - asset_overlap is true when a pattern is all params (/{owner},
//     /{owner}/{repo}) — dotted final segments there may be real files
//   - redirect templates interpolate raw (still %-encoded) segments and are
//     dropped when they would emit `\` or control chars (open-redirect guard)
import { SHELLS } from './shells-table.js'

function htmlEscape(s) {
  return s
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
}

// Substitute `{param}`/`{splat}` placeholders in `tpl` with bound segments.
// `escape` HTML-escapes values (for <title>); redirect targets pass false.
function interpolate(tpl, segs, bindings, pattern, escape) {
  let out = tpl
  for (const [pid, segIdx] of bindings) {
    const seg = pattern[pid]
    const map = v => (escape ? htmlEscape(v) : v)
    if ('param' in seg) {
      out = out.replaceAll(`{${seg.param}}`, map(segs[segIdx]))
    } else if ('splat' in seg) {
      out = out.replaceAll(`{${seg.splat}}`, map(segs.slice(segIdx).join('/')))
    }
  }
  return out
}

// Rust char::is_control — C0, DEL, and the C1 range.
const isControl = ch => {
  const c = ch.codePointAt(0)
  return c < 0x20 || (c >= 0x7f && c <= 0x9f)
}

// Match `path` (no query string, no trailing slash besides root) to a shell.
// Returns null on no match — the caller falls through to static files.
export function dispatch(path) {
  const trimmed = path.replace(/^\/+|\/+$/g, '')
  if (trimmed === '') {
    return {
      file: 'index.html',
      title: null,
      protected: false,
      assetOverlap: false,
      redirect: null,
    }
  }
  const segs = trimmed.split('/')

  let best = null
  let bestScore = Infinity // fewer "non-literal" points = better
  for (const shell of SHELLS) {
    const bindings = [] // [pattern index, segment index]
    let i = 0
    let ok = true
    for (let pid = 0; pid < shell.pattern.length; pid++) {
      const seg = shell.pattern[pid]
      if ('lit' in seg) {
        if (segs[i] !== seg.lit) { ok = false; break }
        i++
      } else if ('param' in seg) {
        if (i >= segs.length) { ok = false; break }
        bindings.push([pid, i]); i++
      } else { // splat — binds one or more trailing segments
        if (i >= segs.length) { ok = false; break }
        bindings.push([pid, i])
        i = segs.length
      }
    }
    if (!ok || i !== segs.length) continue
    const lits = shell.pattern.filter(s => 'lit' in s).length
    const score = Number.MAX_SAFE_INTEGER - lits
    if (!best || score < bestScore) {
      bestScore = score
      best = { shell, bindings }
    }
  }
  if (!best) return null
  const { shell, bindings } = best
  const assetOverlap = !shell.pattern.some(s => 'lit' in s)
  const title = shell.title == null
    ? null
    : interpolate(shell.title, segs, bindings, shell.pattern, true)
  const redirect = shell.redirect == null
    ? null
    : (() => {
        const t = interpolate(shell.redirect, segs, bindings, shell.pattern, false)
        // Fail closed: `Location: /\evil/...` normalizes to `//evil/...`.
        return t.includes('\\') || [...t].some(isControl) ? null : t
      })()
  return {
    file: shell.file,
    title,
    protected: shell.protected,
    assetOverlap,
    redirect,
  }
}

// Detect Astro's static redirect stub: Astro.redirect() in a prerendered page
// emits `<meta http-equiv="refresh" content="N;url=/target">`. We answer real
// 302s, so the interstitial never shows. Same-origin paths only.
export function redirectStubTarget(html) {
  if (html.length > 2048 || !html.includes('http-equiv="refresh"')) return null
  const start = html.indexOf('http-equiv="refresh"')
  const tag = html.slice(start).split('>')[0]
  const content = tag.split('content="')[1]?.split('"')[0]
  if (content == null) return null
  const url = content.split('url=')[1]?.trim()
  if (url == null) return null
  return url.startsWith('/') && !url.startsWith('//') && !url.includes('\\') ? url : null
}
