/**
 * Repo commits view — route coverage (D-QH-03).
 */
import { describe, expect, it } from "vitest";
import { RepoCommitsPage } from "./$owner.$repo.commits.$";

describe("/$owner/$repo/commits/$", () => {
  it("exports RepoCommitsPage", () => {
    expect(typeof RepoCommitsPage).toBe("function");
  });

  it("uses AppLink for internal navigation (client-side routing)", async () => {
    const src = await import("./$owner.$repo.commits.$.tsrx?raw").then((m) =>
      String((m as { default: string }).default),
    );
    expect(src).toMatch(/AppLink/);
    expect(src).not.toMatch(/<a\s+href=\{?["'`]\//);
  });
});
