import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach } from "vitest";
import { consumeDomRaceAllowlist, trackDomErrors, type DomErrorTracker } from "./dom-errors";
import { getQueryClient } from "@/lib/query-client";

/**
 * Every happy-dom integration test fails on Octane insertBefore / hierarchy
 * races so page crashes (“Something went wrong!”) cannot slip through silent
 * catch paths. Opt out with `allowDomRacesInThisTest()` (rare).
 */
let suiteTracker: DomErrorTracker | null = null;

beforeEach(() => {
  suiteTracker?.dispose();
  suiteTracker = trackDomErrors();
  // The app's QueryClient is a module singleton (shared across chrome islands
  // and navigations); without a reset, staleTime serves the previous test's
  // mocked data.
  getQueryClient().clear();
});

afterEach(() => {
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
