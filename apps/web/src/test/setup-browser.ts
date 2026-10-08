import { afterEach, beforeEach } from "vitest";
import { cleanupBrowserMount } from "./browser-mount";
import { consumeDomRaceAllowlist, trackDomErrors, type DomErrorTracker } from "./dom-errors";
import { getQueryClient } from "@/lib/query-client";

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
  // Same singleton-cache reset as setup-integration.ts — pages mount AppPage,
  // which provides the shared getQueryClient(), not the mount-time client.
  getQueryClient().clear();
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
