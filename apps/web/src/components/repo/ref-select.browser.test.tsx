/**
 * Chromium gate for RefSelect — grouped branch/tag popup mounts and dismisses
 * through the Base UI portal without DOM races.
 */
import { afterEach, describe, expect, it, vi } from "vitest";
import type { RepoRefEntry } from "@oxidean/api-client";
import { RefSelect } from "@/components/repo/ref-select";
import {
  cleanupBrowserMount,
  clickAriaLabel,
  debugBody,
  expectNoOctaneOverlayInDocument,
  mountComponent,
} from "@/test/browser-mount";
import { trackDomErrors } from "@/test/dom-errors";

const REFS: RepoRefEntry[] = [
  { name: "refs/heads/main", oid: "a".repeat(40) },
  { name: "refs/heads/feature/two", oid: "b".repeat(40) },
  { name: "refs/tags/v1.0.0", oid: "c".repeat(40) },
];

async function waitForSelector(selector: string, ms = 5_000): Promise<Element> {
  const deadline = Date.now() + ms;
  while (Date.now() < deadline) {
    const el = document.querySelector(selector);
    if (el) return el;
    await new Promise((r) => setTimeout(r, 50));
  }
  throw new Error(`"${selector}" never appeared. ${debugBody()}`);
}

// Base UI keeps the popup mounted but hidden after close (data-closed+hidden).
async function waitForClosed(ms = 5_000): Promise<void> {
  const deadline = Date.now() + ms;
  while (Date.now() < deadline) {
    const positioner = document.querySelector('[data-slot="select-positioner"]');
    if (positioner?.hasAttribute("hidden")) return;
    await new Promise((r) => setTimeout(r, 50));
  }
  throw new Error(`select popup never closed. ${debugBody()}`);
}

afterEach(async () => {
  await cleanupBrowserMount();
});

describe("RefSelect browser DOM races", () => {
  it("opens grouped popup and dismisses without insertBefore", async () => {
    const tracker = trackDomErrors();
    try {
      await mountComponent(RefSelect, {
        owner: "ada",
        repo: "hello",
        refs: REFS,
        value: "main",
        pathUnderRef: "src",
        mode: "tree",
      });

      await waitForSelector('[aria-label="Switch branch or tag"]');
      await clickAriaLabel("Switch branch or tag");

      // Group labels + items mount inside the portalled popup.
      await waitForSelector('[role="listbox"]');
      const labels = [...document.querySelectorAll('[data-slot="select-group-label"]')].map((el) =>
        el.textContent?.trim(),
      );
      expect(labels).toContain("Branches");
      expect(labels).toContain("Tags");
      const options = [...document.querySelectorAll<HTMLElement>('[role="option"]')].map((el) =>
        el.textContent?.trim(),
      );
      expect(options).toEqual(expect.arrayContaining(["main", "feature/two", "v1.0.0"]));

      // Escape dismisses — portal teardown must not race the select root.
      document.activeElement?.dispatchEvent(
        new KeyboardEvent("keydown", { key: "Escape", bubbles: true }),
      );
      await waitForClosed();

      // Reopen once — the second portal cycle is where Octane races surface.
      // (Popup stays mounted when closed, so wait for `hidden` to clear.)
      await clickAriaLabel("Switch branch or tag");
      await vi.waitFor(() => {
        expect(
          document.querySelector('[data-slot="select-positioner"]')?.hasAttribute("hidden"),
        ).toBe(false);
      });

      tracker.expectNoDomRaces();
      expectNoOctaneOverlayInDocument();
    } finally {
      tracker.dispose();
    }
  }, 20_000);
});
