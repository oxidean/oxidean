/**
 * Chromium gate for mobile (touch) navigation on router links — issue #111.
 * Simulates the real tap sequence (pointerdown → touchstart → pointerup →
 * touchend → mousedown → mouseup → click) against a memory-history router and
 * asserts navigation commits. `preload="intent"` fires on touchstart, so the
 * click lands while a preload is in flight — the path desktop never takes.
 */
import { afterEach, describe, expect, it } from "vitest";
import {
  createMemoryHistory,
  createRootRoute,
  createRoute,
  createRouter,
  Link,
  Outlet,
  RouterProvider,
} from "@octanejs/tanstack-router";
import { createElement } from "octane";
import { AppLink } from "@/components/ui/app-link";
import {
  cleanupBrowserMount,
  debugBody,
  expectNoOctaneOverlayInDocument,
  mountComponent,
} from "@/test/browser-mount";
import { trackDomErrors } from "@/test/dom-errors";

function dispatchTouchTap(el: Element): void {
  const init = { bubbles: true, cancelable: true, composed: true };
  el.dispatchEvent(new PointerEvent("pointerdown", { ...init, pointerType: "touch", button: 0 }));
  el.dispatchEvent(
    new TouchEvent("touchstart", {
      ...init,
      touches: [new Touch({ identifier: 0, target: el })],
    }),
  );
  el.dispatchEvent(new PointerEvent("pointerup", { ...init, pointerType: "touch", button: 0 }));
  el.dispatchEvent(new TouchEvent("touchend", init));
  el.dispatchEvent(new MouseEvent("mousedown", { ...init, button: 0 }));
  el.dispatchEvent(new MouseEvent("mouseup", { ...init, button: 0 }));
  el.dispatchEvent(new MouseEvent("click", { ...init, button: 0 }));
}

function makeRouter() {
  const rootRoute = createRootRoute({
    component: () => createElement(Outlet as never),
  });
  const indexRoute = createRoute({
    getParentRoute: () => rootRoute,
    path: "/",
    component: function Index() {
      return createElement(
        "div",
        {},
        createElement(AppLink as never, {
          href: "/target",
          "data-testid": "nav-applink",
          children: "Go target",
        }),
        createElement(Link as never, {
          to: "/target",
          preload: "intent",
          "data-testid": "nav-link",
          children: "Go target link",
        }),
      );
    },
  });
  const targetRoute = createRoute({
    getParentRoute: () => rootRoute,
    path: "/target",
    component: function Target() {
      return createElement("div", { "data-testid": "target-page" }, "Target");
    },
  });
  const routeTree = rootRoute.addChildren([indexRoute, targetRoute]);
  return createRouter({
    routeTree,
    history: createMemoryHistory({ initialEntries: ["/"] }),
    defaultPreload: "intent",
    defaultPreloadDelay: 50,
  });
}

async function waitForTestId(testId: string, ms = 10_000): Promise<Element> {
  const deadline = Date.now() + ms;
  while (Date.now() < deadline) {
    const el = document.querySelector(`[data-testid="${testId}"]`);
    if (el) return el;
    await new Promise((r) => setTimeout(r, 50));
  }
  throw new Error(`[data-testid="${testId}"] never appeared. ${debugBody()}`);
}

describe("mobile touch navigation (issue #111)", () => {
  afterEach(async () => {
    await cleanupBrowserMount();
  });

  it("tap on AppLink navigates", async () => {
    const tracker = trackDomErrors();
    try {
      const router = makeRouter();
      await mountComponent(RouterProvider, { router });
      const link = await waitForTestId("nav-applink");
      dispatchTouchTap(link);
      await waitForTestId("target-page");
      expect(router.state.location.pathname).toBe("/target");
      expectNoOctaneOverlayInDocument();
      tracker.expectNoDomRaces();
    } finally {
      tracker.dispose();
    }
  });

  it("tap on Link with preload=intent navigates", async () => {
    const tracker = trackDomErrors();
    try {
      const router = makeRouter();
      await mountComponent(RouterProvider, { router });
      const link = await waitForTestId("nav-link");
      dispatchTouchTap(link);
      await waitForTestId("target-page");
      expect(router.state.location.pathname).toBe("/target");
      expectNoOctaneOverlayInDocument();
      tracker.expectNoDomRaces();
    } finally {
      tracker.dispose();
    }
  });
});
