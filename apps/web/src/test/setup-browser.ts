import { afterEach, beforeEach } from "vitest";
import { cleanupBrowserMount } from "./browser-mount";
import { consumeDomRaceAllowlist, trackDomErrors, type DomErrorTracker } from "./dom-errors";

/**
 * Chromium browser-mode suite: fail on Octane insertBefore / hierarchy races.
 * This is the gate happy-dom cannot provide for Base UI Indicator mount races.
 *
 * Deliberately does **not** load `@testing-library/jest-dom` — its `aria-query`
 * CJS named exports break under Vitest browser ESM.
 */
let suiteTracker: DomErrorTracker | null = null;

beforeEach(() => {
  suiteTracker?.dispose();
  suiteTracker = trackDomErrors();
});

afterEach(async () => {
  await cleanupBrowserMount();
  const tracker = suiteTracker;
  suiteTracker = null;
  if (!tracker) return;
  try {
    if (!consumeDomRaceAllowlist()) {
      tracker.expectNoDomRaces();
    }
  } finally {
    tracker.dispose();
  }
});
