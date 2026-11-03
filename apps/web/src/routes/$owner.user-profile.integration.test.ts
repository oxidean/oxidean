import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { cleanup, screen, waitFor } from "@octanejs/testing-library";
import { afterEach, describe, expect, it, vi } from "vitest";
import { renderWithQueryClient } from "@/test/render-with-query";
import {
  buildContributionWeeks,
  contributionDayTooltip,
  contributionLevel,
  contributionMonthHeading,
  contributionRangeHref,
  contributionSectionSummary,
  resolveContributionRange,
} from "@/lib/user-contributions";

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    user: {
      getContributions: vi.fn(),
    },
  },
}));

import { apiClient } from "@/lib/api-client";
import { ContributionGraph } from "@/components/profile/contribution-graph";
import { UserContributionsSection } from "@/components/profile/user-contributions-section";

afterEach(cleanup);

const dir = dirname(fileURLToPath(import.meta.url));

describe("$owner user profile (SOC-02)", () => {
  it("branches user vs org on owner index", () => {
    const src = readFileSync(join(dir, "$owner.index.tsrx"), "utf8");
    expect(src).toMatch(/export function UserProfilePage/);
    expect(src).toMatch(/export function OrgOverviewPage/);
    expect(src).toMatch(/fetchUserProfile/);
    expect(src).toMatch(/fetchOrgOverview/);
  });

  it("wires profile README panel on user and org overview", () => {
    const src = readFileSync(join(dir, "$owner.index.tsrx"), "utf8");
    expect(src).toMatch(/ReadmePanel/);
    expect(src).toMatch(/profileReadme/);

    const userSsr = readFileSync(join(dir, "../lib/ssr-user-profile.ts"), "utf8");
    expect(userSsr).toMatch(/fetchUserProfileReadme/);
    expect(userSsr).toMatch(/profileReadme/);

    const orgSsr = readFileSync(join(dir, "../lib/ssr-org.ts"), "utf8");
    expect(orgSsr).toMatch(/fetchOrgProfileReadme/);
    expect(orgSsr).toMatch(/profileReadme/);
  });

  it("wires the contributions section on the user profile (COL-12)", () => {
    const src = readFileSync(join(dir, "$owner.index.tsrx"), "utf8");
    expect(src).toMatch(/UserContributionsSection/);

    const section = readFileSync(
      join(dir, "../components/profile/user-contributions-section.tsrx"),
      "utf8",
    );
    expect(section).toMatch(/apiClient\.user\.getContributions/);
    // Zero-contribution profiles hide the whole section (GitHub parity).
    expect(section).toMatch(/c\.total > 0/);
    expect(section).toMatch(/includes_private/);
    expect(section).toMatch(/data-testid="contribution-years"/);

    const graph = readFileSync(join(dir, "../components/profile/contribution-graph.tsrx"), "utf8");
    expect(graph).toMatch(/aria-label=\{contributionDayTooltip/);
    expect(graph).toMatch(/data-testid="contribution-graph"/);

    const activity = readFileSync(
      join(dir, "../components/profile/contribution-activity.tsrx"),
      "utf8",
    );
    expect(activity).toMatch(/Contribution activity/);
    expect(activity).toMatch(/data-testid="contribution-month"/);
    expect(activity).toMatch(/truncated/);
  });
});

describe("contribution helpers (COL-12)", () => {
  it("resolves the default rolling range and year links", () => {
    expect(resolveContributionRange("")).toEqual({ selected: "last_year" });
    expect(resolveContributionRange("?from=2025-01-01&to=2025-12-31")).toEqual({
      from: "2025-01-01",
      to: "2025-12-31",
      selected: 2025,
    });
    // Malformed input falls back to the default rather than erroring.
    expect(resolveContributionRange("?from=nope")).toEqual({ selected: "last_year" });
    expect(contributionRangeHref("octo", "last_year")).toBe("/octo");
    expect(contributionRangeHref("octo", 2024)).toBe("/octo?from=2024-01-01&to=2024-12-31");
  });

  it("scales intensity to the busiest day and formats tooltips", () => {
    expect(contributionLevel(0, 10)).toBe(0);
    expect(contributionLevel(10, 10)).toBe(4);
    expect(contributionLevel(1, 10)).toBe(1);
    expect(contributionDayTooltip("2026-01-05", 0)).toBe("No contributions on January 5, 2026");
    expect(contributionDayTooltip("2026-01-05", 3)).toBe("3 contributions on January 5, 2026");
    expect(contributionMonthHeading("2026-11")).toBe("November 2026");
    expect(contributionSectionSummary("commits", 4, 2)).toBe("Created 4 commits in 2 repositories");
  });

  it("pads the first week so days land on their weekday row", () => {
    // 2026-01-01 is a Thursday (getUTCDay 4) → 4 leading null cells.
    const weeks = buildContributionWeeks([
      { date: "2026-01-01", count: 1 },
      { date: "2026-01-02", count: 0 },
      { date: "2026-01-03", count: 2 },
    ]);
    expect(weeks[0].slice(0, 4)).toEqual([null, null, null, null]);
    expect(weeks[0][4]?.date).toBe("2026-01-01");
    expect(weeks[0][6]?.count).toBe(2);
  });
});

describe("contribution components (COL-12)", () => {
  const days = [
    { date: "2026-01-01", count: 1 },
    { date: "2026-01-02", count: 0 },
    { date: "2026-01-03", count: 5 },
  ];

  it("renders the heatmap with accessible day cells", () => {
    const { container } = renderWithQueryClient(ContributionGraph, {
      props: { days },
    });
    expect(container.querySelector('[data-testid="contribution-graph"]')).toBeInTheDocument();
    expect(screen.getByLabelText("5 contributions on January 3, 2026")).toBeInTheDocument();
    expect(screen.getByText("Less")).toBeInTheDocument();
    expect(screen.getByText("More")).toBeInTheDocument();
  });

  it("hides the section for zero-contribution profiles", async () => {
    vi.mocked(apiClient.user.getContributions).mockResolvedValueOnce({
      ok: true,
      data: {
        total: 0,
        from: "2025-01-01",
        to: "2026-01-01",
        includes_private: false,
        days: [],
        by_type: { commits: 0, pull_requests: 0, issues: 0, reviews: 0 },
        months: [],
        years: [2025, 2026],
      },
    } as never);
    const { container } = renderWithQueryClient(UserContributionsSection, {
      props: { username: "octo" },
    });
    await waitFor(() =>
      expect(container.querySelector('[aria-busy="true"]')).not.toBeInTheDocument(),
    );
    expect(container.querySelector('[data-testid="user-contributions"]')).not.toBeInTheDocument();
  });

  it("renders graph, breakdown, year rail, and month sections", async () => {
    vi.mocked(apiClient.user.getContributions).mockResolvedValueOnce({
      ok: true,
      data: {
        total: 3,
        from: "2026-01-01",
        to: "2026-01-07",
        includes_private: true,
        days,
        by_type: { commits: 2, pull_requests: 1, issues: 0, reviews: 0 },
        months: [
          {
            month: "2026-01",
            sections: [
              {
                kind: "commits",
                total: 2,
                repo_count: 1,
                repos: [{ slug: "octo/app", count: 2 }],
                items: [],
                truncated: false,
              },
              {
                kind: "pull_requests",
                total: 1,
                repo_count: 1,
                repos: [{ slug: "octo/app", count: 1 }],
                items: [
                  {
                    href: "/octo/app/pull/7",
                    slug: "octo/app",
                    number: 7,
                    title: "Fix it",
                  },
                ],
                truncated: false,
              },
            ],
          },
        ],
        years: [2026, 2025],
      },
    } as never);
    const { container } = renderWithQueryClient(UserContributionsSection, {
      props: { username: "octo" },
    });
    await waitFor(() =>
      expect(container.querySelector('[data-testid="user-contributions"]')).toBeInTheDocument(),
    );
    expect(screen.getByText("3 contributions in the last year")).toBeInTheDocument();
    expect(screen.getByText("Includes private contributions")).toBeInTheDocument();
    expect(container.querySelector('[data-testid="contribution-years"]')).toBeInTheDocument();
    expect(container.querySelector('[data-testid="contribution-month"]')).toBeInTheDocument();
    expect(screen.getByText("Contribution activity")).toBeInTheDocument();
    expect(screen.getByText("January 2026")).toBeInTheDocument();
    expect(screen.getByText("Commits 67%")).toBeInTheDocument();
  });
});
