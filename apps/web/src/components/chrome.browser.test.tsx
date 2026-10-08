/**
 * Chromium gate for the header's pending account cluster — the island's SSR
 * emits both skeleton candidates and CSS selects by the `data-oxidean-session`
 * stamp on <html> (set by oxidean-web's session-check, presence-fallback
 * otherwise). A wrong pick paints the opposite shape and CLS-swaps on
 * hydrate; happy-dom can't see computed display, so this lives in browser
 * mode. Companion to the happy-dom contracts in chrome.integration.test.ts.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanupBrowserMount,
  clickAriaLabel,
  debugBody,
  expectNoOctaneOverlayInDocument,
  mountWithQueryClient,
} from "@/test/browser-mount";
import { trackDomErrors } from "@/test/dom-errors";

const meMock = vi.fn();
const bootstrapMock = vi.fn();
const providerConfigMock = vi.fn();
const orgListMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    auth: {
      me: (...args: unknown[]) => meMock(...args),
      bootstrapStatus: (...args: unknown[]) => bootstrapMock(...args),
      providerConfig: (...args: unknown[]) => providerConfigMock(...args),
      logout: vi.fn(async () => ({ ok: true, data: { ok: true } })),
    },
    org: {
      listMine: (...args: unknown[]) => orgListMock(...args),
    },
  },
}));

import { SiteHeader } from "./chrome";

const anonymous = { ok: false, error: { code: "auth.unauthenticated", message: "" } };

beforeEach(() => {
  meMock.mockReset();
  bootstrapMock.mockReset();
  bootstrapMock.mockResolvedValue({ ok: true, data: { needs_setup: false } });
  providerConfigMock.mockReset();
  providerConfigMock.mockResolvedValue({
    ok: true,
    data: { mode: "local", allow_signup: true },
  });
  orgListMock.mockReset();
  orgListMock.mockResolvedValue({ ok: true, data: { orgs: [] } });
  window.history.replaceState({}, "", "/");
  document.documentElement.removeAttribute("data-oxidean-session");
});

afterEach(async () => {
  document.documentElement.removeAttribute("data-oxidean-session");
  await cleanupBrowserMount();
});

function deferred<T>() {
  let resolve!: (v: T) => void;
  const promise = new Promise<T>((res) => {
    resolve = res;
  });
  return { promise, resolve };
}

async function waitForSelector(selector: string, ms = 10_000): Promise<HTMLElement> {
  const deadline = Date.now() + ms;
  while (Date.now() < deadline) {
    const el = document.querySelector(selector);
    if (el) return el as HTMLElement;
    await new Promise((r) => setTimeout(r, 50));
  }
  throw new Error(`${selector} never appeared. ${debugBody()}`);
}

describe("header pending account cluster", () => {
  it("pending emits both skeleton candidates; no stamp → anon paints", async () => {
    const me = deferred<unknown>();
    meMock.mockReturnValue(me.promise);

    const tracker = trackDomErrors();
    try {
      await mountWithQueryClient(SiteHeader, {});

      const anon = await waitForSelector("[data-header-skeleton-anon]");
      const authed = await waitForSelector("[data-header-skeleton-authed]");
      // Without the session stamp the anon button-shaped skeleton paints.
      expect(getComputedStyle(anon).display).not.toBe("none");
      expect(getComputedStyle(authed).display).toBe("none");

      me.resolve(anonymous);
      await waitForSelector("a[href='/login']");

      // Resolved header still toggles the mobile sheet without DOM races —
      // the pending→anon swap must not leave the menu button detached.
      await clickAriaLabel("Open menu");
      await waitForSelector("#mobile-nav");
      await clickAriaLabel("Close menu");

      tracker.expectNoDomRaces();
      expectNoOctaneOverlayInDocument();
    } finally {
      me.resolve(anonymous);
      tracker.dispose();
    }
  });

  it("session stamp selects the authed skeleton candidate pre-resolution", async () => {
    document.documentElement.setAttribute("data-oxidean-session", "1");
    const me = deferred<unknown>();
    meMock.mockReturnValue(me.promise);

    const tracker = trackDomErrors();
    try {
      await mountWithQueryClient(SiteHeader, {});

      const authed = await waitForSelector("[data-header-skeleton-authed]");
      const anon = await waitForSelector("[data-header-skeleton-anon]");
      expect(getComputedStyle(authed).display).toBe("flex");
      expect(getComputedStyle(anon).display).toBe("none");

      me.resolve(anonymous);
      await waitForSelector("a[href='/login']");
      tracker.expectNoDomRaces();
      expectNoOctaneOverlayInDocument();
    } finally {
      me.resolve(anonymous);
      tracker.dispose();
    }
  });
});
