/**
 * Repo issue labels view — route coverage (D-QH-03).
 */
import { describe, expect, it } from "vitest";
import { RepoIssueLabelsPage } from "./$owner.$repo.issues.labels";

describe("/$owner/$repo/issues/labels", () => {
  it("exports RepoIssueLabelsPage", () => {
    expect(typeof RepoIssueLabelsPage).toBe("function");
  });

  it("uses AppLink for internal navigation (client-side routing)", async () => {
    const src = await import("./$owner.$repo.issues.labels.tsrx?raw").then((m) =>
      String((m as { default: string }).default),
    );
    expect(src).toMatch(/AppLink/);
    expect(src).not.toMatch(/<a\s+href=\{?["'`]\//);
  });
});
