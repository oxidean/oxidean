import { beforeAll, describe, expect, it } from "vitest";
import { commands } from "vitest/browser";
import { requireStack } from "../stack/env";

declare module "vitest/browser" {
  interface BrowserCommands {
    expectRepoRenameTransferFlow: () => Promise<boolean>;
  }
}

describe("stack browser e2e: repo Danger zone rename/transfer (DEBT-11)", () => {
  beforeAll(() => {
    requireStack();
  });

  it("renames a repo via settings UI, resolves the old URL, then transfers to an org via type-confirm", async () => {
    const ok = await commands.expectRepoRenameTransferFlow();
    expect(ok).toBe(true);
  }, 180_000);
});
