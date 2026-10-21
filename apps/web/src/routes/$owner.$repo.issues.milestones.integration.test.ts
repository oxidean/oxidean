/**
 * Repo issue milestones view — route coverage + COL-13 unit toggle.
 */
import { cleanup, waitFor } from "@octanejs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderWithQueryClient } from "@/test/render-with-query";
import { RepoIssueMilestonesPage } from "./$owner.$repo.issues.milestones";

describe("/$owner/$repo/issues/milestones", () => {
  it("exports RepoIssueMilestonesPage", () => {
    expect(typeof RepoIssueMilestonesPage).toBe("function");
  });

  it("uses AppLink for internal navigation (client-side routing)", async () => {
    const src = await import("./$owner.$repo.issues.milestones.tsrx?raw").then((m) =>
      String((m as { default: string }).default),
    );
    expect(src).toMatch(/AppLink/);
    expect(src).not.toMatch(/<a\s+href=\{?["'`]\//);
  });
});

const repoGetMock = vi.fn();
const milestoneListMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    repo: {
      get: (...args: unknown[]) => repoGetMock(...args),
    },
    milestone: {
      list: (...args: unknown[]) => milestoneListMock(...args),
      create: vi.fn(),
      update: vi.fn(),
      delete: vi.fn(),
    },
  },
}));

const writeRepo = {
  id: "r1",
  owner_id: "u1",
  owner_type: "user" as const,
  owner_username: "ada",
  name: "hello",
  description: "",
  visibility: "public" as const,
  default_branch: "main",
  updated_at: "2026-10-02T00:00:00Z",
  can_admin: true,
  can_write: true,
  issues_enabled: true,
  pulls_enabled: false,
};

const disabledRepo = { ...writeRepo, issues_enabled: false };

beforeEach(() => {
  window.history.pushState({}, "", "/ada/hello/issues/milestones");
  repoGetMock.mockReset();
  milestoneListMock.mockReset();
  repoGetMock.mockResolvedValue({ ok: true, data: writeRepo });
  milestoneListMock.mockResolvedValue({
    ok: true,
    data: {
      milestones: [
        {
          id: "m1",
          repo_id: "r1",
          number: 1,
          title: "v1.0",
          description: "First release",
          state: "open",
          due_on: null,
          creator_id: "u1",
          creator_username: "ada",
          closed_at: null,
          created_at: "2026-10-02T00:00:00Z",
          updated_at: "2026-10-02T00:00:00Z",
          open_issues: 2,
          closed_issues: 1,
        },
      ],
    },
  });
});

afterEach(() => {
  cleanup();
});

describe("RepoIssueMilestonesPage", () => {
  it("renders milestones with issue counts and progress", async () => {
    renderWithQueryClient(RepoIssueMilestonesPage);
    await waitFor(() => {
      expect(document.body.textContent).toMatch(/v1\.0/);
    });
    expect(document.body.textContent).toMatch(/1 Open/);
    expect(document.body.textContent).toMatch(/2 open · 1 closed/);
  }, 30_000);

  it("renders the unit-disabled state when issues is off", async () => {
    repoGetMock.mockResolvedValue({ ok: true, data: disabledRepo });
    renderWithQueryClient(RepoIssueMilestonesPage);
    await waitFor(() => {
      expect(document.body.textContent).toMatch(/disabled/i);
    });
    expect(document.querySelector("#repo-milestone-title")).toBeNull();
  });
});
