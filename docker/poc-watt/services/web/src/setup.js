// Port of crates/oxidean-web/src/setup.rs — setup-status probe for the
// per-request `data-oxidean-setup` stamp. Probes GET {api}/api/auth/setup-check;
// failures stamp nothing (fail-open), matching the old query-error path.

// How long a `false` verdict (setup done) is reused before re-probing.
// `true` verdicts are never cached: a stale "1" stamp after /setup completion
// bounces the access gate back to /setup.
const VERDICT_TTL_MS = 10_000

export class SetupChecker {
  #url
  #doneAt = 0 // epoch ms when setup was last confirmed done; 0 = never

  constructor(apiOrigin) {
    this.#url = `${apiOrigin}/api/auth/setup-check`
  }

  // Latest verdict: true/false, or null on probe failure (stamp nothing).
  async needsSetup() {
    if (this.#doneAt !== 0 && Date.now() - this.#doneAt < VERDICT_TTL_MS) return false
    const v = await this.#probe()
    if (v === false) this.#doneAt = Date.now()
    return v
  }

  async #probe() {
    try {
      const resp = await fetch(this.#url, {
        redirect: 'manual',
        signal: AbortSignal.timeout(750),
      })
      if (!resp.ok) return null
      const body = await resp.json()
      return typeof body.needs_setup === 'boolean' ? body.needs_setup : null
    } catch {
      return null
    }
  }
}
