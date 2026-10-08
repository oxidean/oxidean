/**
 * Issues/releases list leaves own their data + rendering (no layout parents
 * under Astro). Leaf chrome lives only on `$owner.$repo` (D-QH-01).
 */
import { describe, expect, it } from "vitest";

describe("issues/releases leaves", () => {
  it("issues list lives on index", async () => {
    const index = await import("./$owner.$repo.issues.index.tsrx?raw").then((m) =>
      String((m as { default: string }).default),
    );
    expect(index).toMatch(/IssuesListPage/);
    expect(index).toMatch(/fetchLabelListForRepo/);
    expect(index).toMatch(/fetchIssueList/);
  });

  it("releases list lives on index + release.list fetch", async () => {
    const index = await import("./$owner.$repo.releases.index.tsrx?raw").then((m) =>
      String((m as { default: string }).default),
    );
    expect(index).toMatch(/RepoReleasesPage/);
    expect(index).toMatch(/fetchReleaseList/);
  });

  it("issues/releases/settings leaves inherit layout chrome — no leaf RepoChrome (D-QH-01)", async () => {
    const sources = await Promise.all([
      import("./$owner.$repo.issues.index.tsrx?raw"),
      import("./$owner.$repo.issues.new.tsrx?raw"),
      import("./$owner.$repo.issues.$n.tsrx?raw"),
      import("./$owner.$repo.issues.labels.tsrx?raw"),
      import("./$owner.$repo.releases.index.tsrx?raw"),
      import("./$owner.$repo.releases.new.tsrx?raw"),
      import("./$owner.$repo.releases.$tag.tsrx?raw"),
      import("./$owner.$repo.settings.tsrx?raw"),
    ]);
    const names = [
      "issues.index",
      "issues.new",
      "issues.$n",
      "issues.labels",
      "releases.index",
      "releases.new",
      "releases.$tag",
      "settings",
    ];
    for (let i = 0; i < sources.length; i++) {
      const src = String((sources[i] as { default: string }).default);
      expect(src, `${names[i]} must not remount RepoChrome`).not.toMatch(/RepoChrome/);
    }
    const repoLayout = await import("./$owner.$repo.tsrx?raw").then((m) =>
      String((m as { default: string }).default),
    );
    expect(repoLayout).toMatch(/RepoLayoutChrome/);
    expect(repoLayout).toMatch(/RepoChrome/);
    expect(repoLayout).toMatch(/repoChromeActiveFromPath/);
  }, 30_000);
});
