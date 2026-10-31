// Port of crates/oxidean-web/src/session.rs — session validation for the
// per-request stamp + protected gate. Probes GET {api}/api/auth/session-check
// (read-only) when OXIDEAN_API_ORIGIN is configured; callers fall back to the
// cookie-presence heuristic on any failure.
//
// Signals: 'absent' | 'valid' | 'invalid' | 'unknown' (unknown = couldn't ask
// upstream — never cached, so a transient outage can't pin a stale answer).

export const SessionSignal = Object.freeze({
  Absent: 'absent',
  Valid: 'valid',
  Invalid: 'invalid',
  Unknown: 'unknown',
})

// How long a Valid/Invalid verdict is reused before re-probing upstream.
const VERDICT_TTL_MS = 10_000
// Cache bound — cleared on overflow (sessions churn far slower than this).
const VERDICT_CAP = 4096

export class SessionValidator {
  #url
  #cache = new Map() // token -> [signal, at]

  constructor(apiOrigin) {
    this.#url = `${apiOrigin}/api/auth/session-check`
  }

  // `token` is the oxidean_session cookie value. Fresh cached verdicts
  // return without an upstream call.
  async checkToken(token) {
    const hit = this.#cache.get(token)
    if (hit && Date.now() - hit[1] < VERDICT_TTL_MS) return hit[0]
    const sig = await this.#probe(token)
    if (sig === SessionSignal.Valid || sig === SessionSignal.Invalid) {
      if (this.#cache.size >= VERDICT_CAP) this.#cache.clear()
      this.#cache.set(token, [sig, Date.now()])
    }
    return sig
  }

  // Probe upstream. The API reads only `oxidean_session` from Cookie, so a
  // narrowed header is exactly equivalent to forwarding it verbatim.
  async #probe(token) {
    try {
      const resp = await fetch(this.#url, {
        headers: { cookie: `oxidean_session=${token}` },
        redirect: 'manual',
        signal: AbortSignal.timeout(750),
      })
      if (!resp.ok) return SessionSignal.Unknown
      const body = await resp.json()
      return body.valid === true ? SessionSignal.Valid : SessionSignal.Invalid
    } catch {
      return SessionSignal.Unknown
    }
  }
}
