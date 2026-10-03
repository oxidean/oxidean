/**
 * Chromium gate for RefSelect — the branch/tag Select popup must mount and
 * commit a pick without insertBefore races, and the pick routes through
 * useAppNavigate with the tree href (client-side nav, issue #104).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { RepoRefEntry } from "@oxidean/api-client";
import {
  cleanupBrowserMount,
  clickAriaLabel,
  debugBody,
  expectNoOctaneOverlayInDocument,
  mountComponent,
} from "@/test/browser-mount";
import { trackDomErrors } from "@/test/dom-errors";
import { act } from "octane";

const navMock = vi.fn();

// No RouterProvider in this mount — useAppNavigate would fall back to
// window.location.assign and actually navigate the Chromium iframe. Spy instead.
vi.mock("@/lib/app-navigate", () => ({
  useAppNavigate: () => navMock,
}));

import { RefSelect } from "@/components/repo/ref-select";

const refs: RepoRefEntry[] = [
  { name: "refs/heads/main", oid: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa" },
  { name: "refs/heads/develop", oid: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb" },
  { name: "refs/tags/v1.0.0", oid: "cccccccccccccccccccccccccccccccccccccccc" },
];

beforeEach(() => {
  navMock.mockReset();
  navMock.mockResolvedValue(undefined);
});

afterEach(async () => {
  await cleanupBrowserMount();
});

async function pickOptionByText(text: string, ms = 2_000): Promise<void> {
  const deadline = Date.now() + ms;
  let option: Element | null = null;
  while (Date.now() < deadline) {
    option = Array.from(document.querySelectorAll('[role="option"]')).find(
      (el) => (el.textContent ?? "").trim() === text,
    );
    if (option) break;
    await new Promise((r) => setTimeout(r, 50));
  }
  if (!option) {
    throw new Error(`option "${text}" never attached. ${debugBody()}`);
  }
  await act(async () => {
    option!.dispatchEvent(
      new PointerEvent("pointerdown", { bubbles: true, cancelable: true, pointerType: "mouse" }),
    );
    (option as HTMLElement).click();
  });
}

describe("RefSelect browser DOM races", () => {
  it("Select pick commits a client-side navigate to the picked ref's tree", async () => {
    const tracker = trackDomErrors();
    try {
      await mountComponent(RefSelect, {
        owner: "ada",
        repo: "hello",
        refs,
        value: "main",
      });

      await clickAriaLabel("Switch branch or tag");
      await pickOptionByText("develop");

      expect(navMock).toHaveBeenCalledWith("/ada/hello/tree/develop");

      tracker.expectNoDomRaces();
      expectNoOctaneOverlayInDocument();
    } finally {
      tracker.dispose();
    }
  }, 30_000);

  it("Select pick under a blob path keeps the path and mode", async () => {
    const tracker = trackDomErrors();
    try {
      await mountComponent(RefSelect, {
        owner: "ada",
        repo: "hello",
        refs,
        value: "main",
        pathUnderRef: "src/app.ts",
        mode: "blob",
      });

      await clickAriaLabel("Switch branch or tag");
      await pickOptionByText("v1.0.0");

      expect(navMock).toHaveBeenCalledWith("/ada/hello/blob/v1.0.0/src/app.ts");

      tracker.expectNoDomRaces();
      expectNoOctaneOverlayInDocument();
    } finally {
      tracker.dispose();
    }
  }, 30_000);
});
