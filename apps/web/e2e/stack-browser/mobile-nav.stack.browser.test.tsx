import { beforeAll, describe, expect, it } from "vitest";
import { commands } from "vitest/browser";
import { requireStack } from "../stack/env";

declare module "vitest/browser" {
  interface BrowserCommands {
    expectMobileNavTapFlow: () => Promise<boolean>;
  }
}

/**
 * Issue #111 — mobile taps on repo chrome tabs and file-tree rows must
 * SPA-navigate (no document reload) even before hydration binds the anchors.
 */
describe("stack browser e2e: mobile tap navigation (#111)", () => {
  beforeAll(() => {
    requireStack();
  });

  it("taps chrome tabs and file-tree rows without document reloads", async () => {
    const ok = await commands.expectMobileNavTapFlow();
    expect(ok).toBe(true);
  }, 90_000);
});
