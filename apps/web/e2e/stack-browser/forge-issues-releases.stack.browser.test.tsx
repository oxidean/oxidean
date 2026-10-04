import { beforeAll, describe, expect, it } from "vitest";
import { commands } from "vitest/browser";
import { requireStack } from "../stack/env";

declare module "vitest/browser" {
  interface BrowserCommands {
    expectForgeIssuesCrudFlow: () => Promise<boolean>;
    expectForgeReleasesCrudFlow: () => Promise<boolean>;
    expectReleaseAssetFlow: () => Promise<boolean>;
  }
}

describe("stack browser e2e: forge issues + releases CRUD (D-QH-03)", () => {
  beforeAll(() => {
    requireStack();
  });

  it("creates an issue via UI and closes it", async () => {
    const ok = await commands.expectForgeIssuesCrudFlow();
    expect(ok).toBe(true);
  }, 120_000);

  it("creates a release from a seeded tag via UI", async () => {
    const ok = await commands.expectForgeReleasesCrudFlow();
    expect(ok).toBe(true);
  }, 120_000);

  it("uploads a release asset and serves its download link (DEBT-11)", async () => {
    const ok = await commands.expectReleaseAssetFlow();
    expect(ok).toBe(true);
  }, 120_000);
});
