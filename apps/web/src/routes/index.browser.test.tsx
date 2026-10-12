/**
 * Chromium gate for `/` — the pending dual-render (anon landing + signed-in
 * skeleton candidates) swaps to the resolved tree once `auth.me` settles, and
 * the swap replaces the `.oct-reveal` nodes. The reveal effect must re-observe
 * the live nodes — a stale observer leaves sections at opacity:0 forever.
 * Companion to the happy-dom contracts in index.integration.test.ts.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanupBrowserMount,
  debugBody,
  expectNoOctaneOverlayInDocument,
  mountWithQueryClient,
} from "@/test/browser-mount";
import { trackDomErrors } from "@/test/dom-errors";
import { getQueryClient } from "@/lib/query-client";

const meMock = vi.fn();
const providerConfigMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    auth: {
      me: (...args: unknown[]) => meMock(...args),
      providerConfig: (...args: unknown[]) => providerConfigMock(...args),
    },
  },
}));

import { HomePage } from "./index";

beforeEach(() => {
  meMock.mockReset();
  providerConfigMock.mockReset();
  providerConfigMock.mockResolvedValue({
    ok: true,
    data: { mode: "local", allow_signup: true },
  });
  window.history.replaceState({}, "", "/");
  document.documentElement.removeAttribute("data-oxidean-session");
  // HomePage rides the module-level QueryClient singleton — clear it so each
  // mount starts from the pending state instead of a stale cached session.
  getQueryClient().clear();
});

afterEach(async () => {
  document.documentElement.removeAttribute("data-oxidean-session");
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

      me.resolve(anonymous);
      await waitForSelector("#explore");
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
      // data-anon-landing). Resolving swaps to the bare AnonLanding tree and
      // replaces the .oct-reveal nodes — the effect must re-observe them.
      await waitForSelector("[data-anon-landing] .oct-reveal");
      me.resolve(anonymous);
      await waitForSelector("#explore");

      for (const el of Array.from(document.querySelectorAll<HTMLElement>(".oct-reveal"))) {
        el.scrollIntoView({ block: "center" });
      }
      // Re-query the node that gained the class — earlier refs may have been
      // replaced by follow-up renders (providerConfig settling). Then wait out
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
      // observer holds the swapped-in nodes, not the detached pending ones.
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
});
