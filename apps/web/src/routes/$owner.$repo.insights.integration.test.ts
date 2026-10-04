import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const dir = dirname(fileURLToPath(import.meta.url));

describe("repo insights route", () => {
  it("wires the insights page with all three SSR + client sections", () => {
    const src = readFileSync(join(dir, "$owner.$repo.insights.tsrx"), "utf8");
    expect(src).toMatch(/createFileRoute\("\/\$owner\/\$repo\/insights"\)/);
    expect(src).toMatch(/export function RepoInsightsPage/);
    expect(src).toMatch(/fetchRepoGet/);
    expect(src).toMatch(/fetchRepoInsightsContributors/);
    expect(src).toMatch(/fetchRepoInsightsCommitActivity/);
    expect(src).toMatch(/fetchRepoInsightsForkNetwork/);
    // Client refresh via TanStack Query against the generated RPC client.
    expect(src).toMatch(/insightsContributors/);
    expect(src).toMatch(/insightsCommitActivity/);
    expect(src).toMatch(/insightsForkNetwork/);
  });

  it("isolates per-section failures so one bad RPC does not sink the page", () => {
    const src = readFileSync(join(dir, "$owner.$repo.insights.tsrx"), "utf8");
    expect(src).toMatch(/Promise\.all\(\[/);
    expect((src.match(/\.catch\(\(\) => null\)/g) ?? []).length).toBe(3);
    expect(src).toMatch(/contributorsQ\.isError/);
    expect(src).toMatch(/activityQ\.isError/);
    expect(src).toMatch(/networkQ\.isError/);
  });

  it("renders commit activity, contributors, and fork network sections", () => {
    const src = readFileSync(join(dir, "$owner.$repo.insights.tsrx"), "utf8");
    expect(src).toMatch(/data-testid="repo-insights"/);
    expect(src).toMatch(/data-testid="repo-insights-commit-activity"/);
    expect(src).toMatch(/data-testid="repo-insights-contributors"/);
    expect(src).toMatch(/data-testid="repo-insights-fork-network"/);
    // Fork list is rendered as an indented tree ordered by parent links.
    expect(src).toMatch(/orderNetworkRows/);
    // Surfaces truncation instead of implying the scan covered all history.
    expect(src).toMatch(/\.truncated/);
  });

  it("uses bounded request params for the git-backed sections", () => {
    const src = readFileSync(join(dir, "$owner.$repo.insights.tsrx"), "utf8");
    expect(src).toMatch(/limit: CONTRIBUTOR_LIMIT/);
    expect(src).toMatch(/weeks: ACTIVITY_WEEKS/);
    expect(src).toMatch(/limit: NETWORK_LIMIT/);
  });

  it("links Insights in the repo chrome navigation", () => {
    const src = readFileSync(join(dir, "../components/repo/repo-chrome.tsrx"), "utf8");
    expect(src).toMatch(/insightsHref/);
    expect(src).toMatch(/repo-chrome-insights/);
  });
});
