/**
 * Repo compare view — route coverage (D-QH-03).
 */
import { describe, expect, it } from "vitest";
import { RepoComparePage } from "./$owner.$repo.compare.$";

describe("/$owner/$repo/compare/$", () => {
  it("exports RepoComparePage", () => {
    expect(typeof RepoComparePage).toBe("function");
  });

  it("uses AppLink for internal navigation (client-side routing)", async () => {
    const src = await import("./$owner.$repo.compare.$.tsrx?raw").then((m) =>
      String((m as { default: string }).default),
    );
    expect(src).toMatch(/AppLink/);
    expect(src).not.toMatch(/<a\s+href=\{?["'`]\//);
  });
});
