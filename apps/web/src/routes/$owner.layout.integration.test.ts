/**
 * Regression: `/{owner}` resolves orgs to the overview page and users to the
 * profile page — no eager notFound, or `/{user}/{repo}` never renders.
 * Under Astro there is no `/$owner` layout module; the index page owns the
 * org-vs-user branch via `ownerIndexQueryOptions`.
 */
import { cleanup, screen, waitFor } from "@octanejs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderWithQueryClient } from "@/test/render-with-query";

const orgOverviewMock = vi.fn();
const userProfileMock = vi.fn();

vi.mock("@/lib/ssr-org", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/lib/ssr-org")>();
  return {
    ...actual,
    fetchOrgOverview: (...args: unknown[]) => orgOverviewMock(...args),
  };
});

vi.mock("@/lib/ssr-user-profile", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/lib/ssr-user-profile")>();
  return {
    ...actual,
    fetchUserProfile: (...args: unknown[]) => userProfileMock(...args),
  };
});

import { OrgOverviewPage } from "./$owner.index";

const ORG_PAYLOAD = {
  org: {
    id: "o1",
    slug: "acme",
    display_name: "Acme Corp",
    member_base_permission: "read",
    created_at: "2026-01-01T00:00:00Z",
    updated_at: "2026-01-01T00:00:00Z",
  },
  memberCount: 3,
  repos: [
    {
      id: "r1",
      owner_id: "o1",
      owner_type: "org",
      owner_username: "acme",
      name: "demo",
      description: "Demo repo",
      visibility: "public",
      default_branch: "main",
      updated_at: "2026-09-14T00:00:00Z",
      can_admin: true,
      can_write: true,
    },
  ],
  canAdmin: true,
  profileReadme: null,
};

beforeEach(() => {
  orgOverviewMock.mockReset();
  userProfileMock.mockReset();
  orgOverviewMock.mockResolvedValue(ORG_PAYLOAD);
  userProfileMock.mockResolvedValue(null);
  window.history.pushState({}, "", "/acme");
});

afterEach(cleanup);

describe("/$owner index resolution", () => {
  it("index owns the org overview — no separate layout gate", async () => {
    const index = await import("./$owner.index.tsrx?raw").then((m) =>
      String((m as { default: string }).default),
    );
    expect(index).toMatch(/fetchOrgOverview/);
    expect(index).toMatch(/fetchUserProfile/);
    expect(index).toMatch(/OrgOverviewPage/);
    // No unconditional notFound in the owner resolution — data === null is a
    // render branch, not a loader throw. Comments are stripped first: the
    // doc block mentions the old `throw notFound()` behavior by name.
    const code = index.replace(/\/\*[\s\S]*?\*\/|\/\/[^\n]*/g, "");
    expect(code).not.toMatch(/notFound\(/);
  });

  it("renders org overview shell for org payload (G-11.1-15)", async () => {
    renderWithQueryClient(OrgOverviewPage);

    await waitFor(
      () => {
        expect(screen.getByRole("heading", { name: "Acme Corp" })).toBeTruthy();
        expect(screen.getByText("@acme")).toBeTruthy();
        expect(screen.getByText("3 members")).toBeTruthy();
        expect(screen.getByRole("heading", { name: "Repositories" })).toBeTruthy();
        expect(screen.getByText("demo")).toBeTruthy();
      },
      { timeout: 10_000 },
    );
  }, 15_000);
});
