/**
 * Chromium gate for `/` — the pending dual-render (anon landing + signed-in
 * skeleton candidates) settles to the resolved tree once `auth.me` resolves.
 * The landing subtree must keep its DOM identity across the settle (no hero
 * remount / `oct-rise` restart, issue #143), the reveal effect must keep
 * observing the live nodes, and the CSS-stamped signup CTA must surface via
 * a `<html>` attribute sync — never a node replacement.
 * Companion to the happy-dom contracts in index.integration.test.ts.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createElement } from "octane";
import { QueryClientProvider } from "@octanejs/tanstack-query";
import {
  cleanupBrowserMount,
  debugBody,
  expectNoOctaneOverlayInDocument,
  mountComponent,
  mountWithQueryClient,
} from "@/test/browser-mount";
import { trackDomErrors } from "@/test/dom-errors";
import { getQueryClient } from "@/lib/query-client";
import { SiteHeader } from "@/components/chrome";

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
    },
    org: {
      listMine: (...args: unknown[]) => orgListMock(...args),
    },
  },
}));

import { HomePage } from "./index";

/**
 * The stamp sync under test lives in the chrome island (`use-chrome-account`),
 * which the real AppShell mounts alongside the page island — mirror that so
 * the first-visit CTA flow can actually run in these tests.
 */
function HomePageWithChrome(): unknown {
  return createElement(
    QueryClientProvider as never,
    { client: getQueryClient() } as never,
    createElement(SiteHeader as never),
    createElement(HomePage as never),
  );
}

async function mountHomePageWithChrome(): Promise<unknown> {
  return mountComponent(HomePageWithChrome as never);
}

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
  // Simulate a first visit: no per-request `data-oxidean-signup` stamp from
  // the web tier (the client-side stamp sync is what the CTA tests assert).
  document.documentElement.removeAttribute("data-oxidean-signup");
  // HomePage rides the module-level QueryClient singleton — clear it so each
  // mount starts from the pending state instead of a stale cached session.
  getQueryClient().clear();
});

afterEach(async () => {
  document.documentElement.removeAttribute("data-oxidean-session");
  document.documentElement.removeAttribute("data-oxidean-signup");
  await cleanupBrowserMount();
});

function deferred<T>() {
  let resolve!: (v: T) => void;
  let reject!: (e: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
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

async function waitForClass(selector: string, cls: string, ms = 10_000): Promise<HTMLElement> {
  const deadline = Date.now() + ms;
  while (Date.now() < deadline) {
    const el = document.querySelector(selector);
    if (el?.classList.contains(cls)) return el as HTMLElement;
    await new Promise((r) => setTimeout(r, 50));
  }
  throw new Error(`${selector} never gained .${cls}. ${debugBody()}`);
}

const anonymous = { ok: false, error: { code: "auth.unauthenticated", message: "" } };

describe("index `/` render gates", () => {
  it("pending emits both anon-landing and home-skeleton candidates", async () => {
    const me = deferred<unknown>();
    meMock.mockReturnValue(me.promise);

    const tracker = trackDomErrors();
    try {
      await mountWithQueryClient(HomePage, {});

      await waitForSelector("[data-anon-landing]");
      await waitForSelector("[data-home-skeleton]");
      // Anon landing visible, skeleton hidden without the session stamp.
      expect(getComputedStyle(document.querySelector("[data-anon-landing]")!).display).not.toBe(
        "none",
      );
      expect(getComputedStyle(document.querySelector("[data-home-skeleton]")!).display).toBe(
        "none",
      );

      // The hero element must survive the settle: the landing wrapper is
      // retained in the marketing branch, so Octane keeps the subtree in
      // place (no remount, no `oct-rise` restart — issue #143).
      const heroH1 = document.querySelector("[data-anon-landing] h1");
      expect(heroH1).toBeTruthy();

      me.resolve(anonymous);
      // Settle signal: the skeleton candidate is dropped by the swap.
      await vi.waitFor(() => {
        expect(document.querySelector("[data-home-skeleton]")).toBeNull();
      });
      // The landing wrapper is retained by the marketing branch, and the hero
      // is the very same element — no remount, no `oct-rise` restart (#143).
      expect(document.querySelector("[data-anon-landing]")).not.toBeNull();
      expect(document.querySelector("[data-anon-landing] h1")).toBe(heroH1);
      tracker.expectNoDomRaces();
      expectNoOctaneOverlayInDocument();
    } finally {
      me.resolve(anonymous);
      tracker.dispose();
    }
  });

  it("session stamp selects the skeleton candidate pre-resolution", async () => {
    document.documentElement.setAttribute("data-oxidean-session", "1");
    const me = deferred<unknown>();
    meMock.mockReturnValue(me.promise);

    const tracker = trackDomErrors();
    try {
      await mountWithQueryClient(HomePage, {});

      await waitForSelector("[data-home-skeleton]");
      expect(getComputedStyle(document.querySelector("[data-home-skeleton]")!).display).toBe(
        "block",
      );
      expect(getComputedStyle(document.querySelector("[data-anon-landing]")!).display).toBe("none");

      me.resolve(anonymous);
      tracker.expectNoDomRaces();
      expectNoOctaneOverlayInDocument();
    } finally {
      me.resolve(anonymous);
      tracker.dispose();
    }
  });

  it("scroll-reveal observes the live nodes after the pending→marketing swap", async () => {
    const me = deferred<unknown>();
    meMock.mockReturnValue(me.promise);

    const tracker = trackDomErrors();
    try {
      await mountWithQueryClient(HomePage, {});

      // Observer attaches while pending (wrapped landing inside
      // data-anon-landing). The marketing branch keeps the same wrapper and
      // nodes in place — the effect's epoch re-run must re-observe them so
      // reveals still fire.
      await waitForSelector("[data-anon-landing] .oct-reveal");
      me.resolve(anonymous);
      await vi.waitFor(() => {
        expect(document.querySelector("[data-home-skeleton]")).toBeNull();
      });

      for (const el of Array.from(document.querySelectorAll<HTMLElement>(".oct-reveal"))) {
        el.scrollIntoView({ block: "center" });
      }
      // Re-query the node that gained the class — earlier refs may have been
      // replaced by follow-up renders (e.g. chrome re-renders). Then wait out
      // the 320ms transition: sampling computed opacity mid-flight reads ~0.
      await waitForClass("#explore", "oct-reveal-in");
      const deadline = Date.now() + 10_000;
      let opacity = "0";
      while (Date.now() < deadline) {
        opacity = getComputedStyle(document.querySelector("#explore")!).opacity;
        if (opacity === "1") break;
        await new Promise((r) => setTimeout(r, 50));
      }
      expect(opacity).toBe("1");

      // Every reveal section ends revealed once scrolled through — proves the
      // observer holds the live nodes (same nodes across the settle), not a
      // stale set from the pending render.
      const stuck = Array.from(document.querySelectorAll<HTMLElement>(".oct-reveal")).filter(
        (n) => !n.classList.contains("oct-reveal-in"),
      );
      expect(stuck, `stuck hidden reveal nodes: ${stuck.length}`).toHaveLength(0);

      tracker.expectNoDomRaces();
      expectNoOctaneOverlayInDocument();
    } finally {
      me.resolve(anonymous);
      tracker.dispose();
    }
  });

  it("first-visit CTA: unstamped at mount, appears on the same node once signup opens", async () => {
    // beforeEach leaves <html> unstamped (first visit: no cookie, no server
    // stamp) and providerConfig resolves with signup open.
    const me = deferred<unknown>();
    meMock.mockReturnValue(me.promise);

    const tracker = trackDomErrors();
    try {
      await mountHomePageWithChrome();

      const cta = await waitForSelector("[data-anon-signup-cta]");
      // Always in the DOM, CSS-hidden while <html> is unstamped.
      expect(getComputedStyle(cta).display).toBe("none");

      me.resolve(anonymous);
      await vi.waitFor(() => {
        expect(document.querySelector("[data-home-skeleton]")).toBeNull();
      });

      // The chrome's stamp sync (fresh providerConfig) stamps <html> once —
      // the CTA surfaces via the attribute, never via a node replacement.
      await vi.waitFor(() => {
        expect(document.documentElement.getAttribute("data-oxidean-signup")).toBe("1");
      });
      expect(getComputedStyle(cta).display).not.toBe("none");
      tracker.expectNoDomRaces();
      expectNoOctaneOverlayInDocument();
    } finally {
      me.resolve(anonymous);
      tracker.dispose();
    }
  });

  it("signup closed: CTA stays hidden and a stale server stamp is cleared", async () => {
    providerConfigMock.mockResolvedValue({
      ok: true,
      data: { mode: "local", allow_signup: false },
    });
    const me = deferred<unknown>();
    meMock.mockReturnValue(me.promise);
    // Stale per-request stamp from an older response: the fresh RPC wins.
    document.documentElement.setAttribute("data-oxidean-signup", "1");

    const tracker = trackDomErrors();
    try {
      await mountHomePageWithChrome();
      await waitForSelector("[data-anon-signup-cta]");
      me.resolve(anonymous);
      await vi.waitFor(() => {
        expect(document.querySelector("[data-home-skeleton]")).toBeNull();
      });

      await vi.waitFor(() => {
        expect(document.documentElement.getAttribute("data-oxidean-signup")).toBeNull();
      });
      const cta = document.querySelector("[data-anon-signup-cta]")!;
      expect(getComputedStyle(cta).display).toBe("none");
      tracker.expectNoDomRaces();
      expectNoOctaneOverlayInDocument();
    } finally {
      me.resolve(anonymous);
      tracker.dispose();
    }
  });
});
