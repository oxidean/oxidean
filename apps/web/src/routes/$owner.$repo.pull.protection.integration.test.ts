import { describe, expect, it } from "vitest";

/**
 * PR-08 / D-22 / D-24: PR detail merge-blocker display.
 */

describe("PR merge protection blockers (PR-08 / D-24)", () => {
  it("PullMergePanel surfaces structured protection reasons", async () => {
    const panel = await import("../components/repo/pull-merge-panel");
    expect(panel.PullMergePanel ?? panel.default).toBeTruthy();
    const src = await import("../components/repo/pull-merge-panel.tsrx?raw").then((m) =>
      String((m as { default: string }).default),
    );
    expect(src).toMatch(/pull-merge-blockers/);
    expect(src).toMatch(/pull\.merge_blocked|Merge blocked by branch protection/);
    expect(src).toMatch(/reviews/);
    expect(src).toMatch(/checks/);
    expect(src).toMatch(/conversations/);
    expect(src).toMatch(/up_to_date/);
    expect(src).toMatch(/locked/);
    expect(src).toMatch(/<PullCodeownersStatus status=\{props\.codeownersStatus\} \/>/);
    expect(src).toMatch(
      /props\.codeownersStatus\.file_present \|\| props\.codeownersStatus\.errors\.length > 0/,
    );
    const permissionGuard = src.indexOf('@if (!props.canWrite || props.pullState !== "open")');
    const readOnlyStatus = src.indexOf("<PullCodeownersStatus", permissionGuard);
    const writeBranch = src.indexOf("} @else {", permissionGuard);
    expect(permissionGuard).toBeGreaterThanOrEqual(0);
    expect(readOnlyStatus).toBeGreaterThan(permissionGuard);
    expect(readOnlyStatus).toBeLessThan(writeBranch);
  }, 30_000);
});
