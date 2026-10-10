/**
 * Chromium gate for mobile (touch) navigation on internal links — issue #111.
 * Simulates the real tap sequence (pointerdown → touchstart → pointerup →
 * touchend → mousedown → mouseup → click) against AppLink and asserts the
 * click lands on the anchor. Under Astro, AppLink is a plain `<a>` and the
 * ClientRouter's document-level delegation drives the navigation — the tap
 * must reach the element as an unswallowed click event.
 */
import { afterEach, describe, expect, it } from "vitest";
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

  it("tap on AppLink delivers a click to the anchor", async () => {
    const tracker = trackDomErrors();
    try {
      let clicked: MouseEvent | null = null;
      function Host() {
        return createElement(
          "div",
          {},
          createElement(AppLink as never, {
            href: "/target",
            "data-testid": "nav-applink",
            onClick: (e: MouseEvent) => {
              clicked = e;
              // Prevent the anchor's default navigation — it would move the
              // vitest iframe off the test page.
              e.preventDefault();
            },
            children: "Go target",
          }),
        );
      }
      await mountComponent(Host as never);
      const link = await waitForTestId("nav-applink");
      expect(link.tagName).toBe("A");
      expect(link.getAttribute("href")).toBe("/target");
      dispatchTouchTap(link);
      expect(clicked).not.toBeNull();
      expectNoOctaneOverlayInDocument();
      tracker.expectNoDomRaces();
    } finally {
      tracker.dispose();
    }
  });
});
