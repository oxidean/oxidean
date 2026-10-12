/**
 * Capture window error / unhandledrejection messages during an interaction.
 * Used to catch Octane DOM races (insertBefore / HierarchyRequestError) that
 * happy-dom may not always surface as a failed assertion otherwise.
 *
 * Prefer this whenever a test clicks a control that also swaps sibling trees
 * (RadioGroup, Select, dialog open/close + parent setState). See Octane skill
 * “Failure modes” — Base UI Radio.Indicator mount + sibling panel update.
 *
 * Integration suite (`setup-integration.ts`) installs a global tracker for every
 * happy-dom test; call `trackDomErrors()` locally for high-risk interactions when
 * you want a tighter scope or clearer failure label.
 *
 * Limitations: happy-dom often does **not** throw the Chromium HierarchyRequestError
 * for Base UI Indicator mount + Octane sibling re-renders. Treat stack-browser
 * `newGuardedPage` / `pageerror` (e.g. `expectMirrorAuthToggleFlow`) as the real
 * gate for those races; this helper is still useful for smoke and other failures.
 * Product fix for radios: `keepMounted` on Indicator (`components/ui/radio-group`).
 */

export const DOM_RACE_RE =
  /insertBefore|HierarchyRequestError|NotFoundError|The node before which/i;

function isDomRaceMessage(message: string): boolean {
  return DOM_RACE_RE.test(message);
}

/** Set by a test to skip the global afterEach DOM-race assertion (rare). */

export function consumeDomRaceAllowlist(): boolean {
  const g = globalThis as { __oxideanAllowDomRaces?: boolean };
  const allowed = g.__oxideanAllowDomRaces === true;
  g.__oxideanAllowDomRaces = false;
  return allowed;
}

export function trackDomErrors() {
  const errors: string[] = [];

  const onError = (ev: ErrorEvent) => {
    const msg = ev.message || (ev.error instanceof Error ? ev.error.message : String(ev.error));
    if (msg) errors.push(msg);
  };
  const onRejection = (ev: PromiseRejectionEvent) => {
    const reason = ev.reason;
    errors.push(reason instanceof Error ? reason.message : String(reason));
  };

  window.addEventListener("error", onError);
  window.addEventListener("unhandledrejection", onRejection);

  return {
    errors,
    /** Messages that indicate Octane/portal DOM hierarchy races. */
    domRaceErrors() {
      return errors.filter(isDomRaceMessage);
    },
    /** Fail the test with a clear message if any DOM race was recorded. */
    expectNoDomRaces() {
      const races = this.domRaceErrors();
      if (races.length > 0) {
        throw new Error(
          `Octane DOM race(s) during interaction:\n${races.map((m) => `  - ${m}`).join("\n")}`,
        );
      }
    },
    dispose() {
      window.removeEventListener("error", onError);
      window.removeEventListener("unhandledrejection", onRejection);
    },
  };
}

export type DomErrorTracker = ReturnType<typeof trackDomErrors>;
