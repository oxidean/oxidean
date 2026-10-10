import { beforeAll } from "vitest";
import { webOrigin } from "../stack/env";

/**
 * Stack-browser commands open pages via `newGuardedPage` (dom-race-guard.ts),
 * which fails the flow on Octane insertBefore / any uncaught pageerror.
 */
beforeAll(async () => {
  // requireStack() lives in env.ts and is browser-safe (no bare `process`).
  const { requireStack } = await import("../stack/env");
  requireStack();

  const web = webOrigin();
  // Cross-origin ping from the vitest browser context: oxidean-web serves no
  // CORS headers (Vite dev used to), so probe with no-cors — an opaque
  // response still proves the server is alive.
  const res = await fetch(web, { mode: "no-cors" }).catch(() => null);
  if (!res || (res.type !== "opaque" && !res.ok)) {
    throw new Error(`E2E_STACK=1 but web origin failed at ${web} — run make test-e2e-stack`);
  }
});
