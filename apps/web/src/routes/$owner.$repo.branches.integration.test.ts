/**
 * Repo branches view — route coverage (D-QH-03).
 */
import { describe, expect, it } from "vitest";
import { RepoBranchesPage } from "./$owner.$repo.branches";

describe("/$owner/$repo/branches", () => {
  it("exports RepoBranchesPage", () => {
    expect(typeof RepoBranchesPage).toBe("function");
  });

  it("uses AppLink for internal navigation (client-side routing)", async () => {
    const src = await import("./$owner.$repo.branches.tsrx?raw").then((m) =>
      String((m as { default: string }).default),
    );
    expect(src).toMatch(/AppLink/);
    expect(src).not.toMatch(/<a\s+href=\{?["'`]\//);
  });
});
