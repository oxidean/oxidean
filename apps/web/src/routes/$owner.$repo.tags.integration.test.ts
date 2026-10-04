/**
 * Repo tags view — route coverage (D-QH-03).
 */
import { describe, expect, it } from "vitest";
import { RepoTagsPage } from "./$owner.$repo.tags";

describe("/$owner/$repo/tags", () => {
  it("exports RepoTagsPage", () => {
    expect(typeof RepoTagsPage).toBe("function");
  });

  it("uses AppLink for internal navigation (client-side routing)", async () => {
    const src = await import("./$owner.$repo.tags.tsrx?raw").then((m) =>
      String((m as { default: string }).default),
    );
    expect(src).toMatch(/AppLink/);
    expect(src).not.toMatch(/<a\s+href=\{?["'`]\//);
  });
});
