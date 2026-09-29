import { beforeAll, describe, expect, it } from "vitest";
import { commands } from "vitest/browser";
import { requireStack } from "../stack/env";

declare module "vitest/browser" {
  interface BrowserCommands {
    expectPatMintClickThroughFlow: () => Promise<boolean>;
  }
}

describe("stack browser e2e: PAT mint click-through", () => {
  beforeAll(() => {
    requireStack();
  });

  it("clicks classic + fine-grained mint controls without insertBefore", async () => {
    const ok = await commands.expectPatMintClickThroughFlow();
    expect(ok).toBe(true);
  }, 120_000);
});
