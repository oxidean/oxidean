import { cleanup, screen, waitFor } from "@octanejs/testing-library";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderWithQueryClient } from "@/test/render-with-query";

/**
 * DEBT-05: PR detail "Checks" tab — commit-status viewer for the head SHA.
 */

const commitStatusListMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    repo: {
      commitStatus: {
        list: (...args: unknown[]) => commitStatusListMock(...args),
      },
    },
  },
}));

const HEAD_SHA = "0123456789abcdef0123456789abcdef01234567";

const sampleStatuses = [
  {
    id: "cs-1",
    repo_id: "r1",
    sha: HEAD_SHA,
    context: "CI / build",
    state: "success",
    description: "Completed successfully",
    target_url: "https://ci.example.com/runs/1",
    creator_id: "u1",
    created_at: "2026-09-14T00:00:00Z",
    updated_at: "2026-09-14T00:05:00Z",
  },
  {
    id: "cs-2",
    repo_id: "r1",
    sha: HEAD_SHA,
    context: "CI / test",
    state: "pending",
    description: "Waiting for a runner",
    target_url: null,
    creator_id: "u1",
    created_at: "2026-09-14T00:00:00Z",
    updated_at: "2026-09-14T00:01:00Z",
  },
  {
    id: "cs-3",
    repo_id: "r1",
    sha: HEAD_SHA,
    context: "deploy / preview",
    state: "failure",
    description: "Preview deploy failed",
    target_url: "https://ci.example.com/runs/2",
    creator_id: null,
    created_at: "2026-09-14T00:00:00Z",
    updated_at: "2026-09-14T00:03:00Z",
  },
];

beforeEach(() => {
  commitStatusListMock.mockReset();
  commitStatusListMock.mockResolvedValue({ ok: true, data: { statuses: sampleStatuses } });
});

afterEach(cleanup);

async function mountChecks() {
  const mod = await import("../components/repo/pull-checks");
  const Page = mod.PullChecks ?? mod.default;
  expect(Page, "PullChecks must be exported from pull-checks.tsrx").toBeTruthy();
  return renderWithQueryClient(Page, {
    props: { owner: "ada", repoName: "hello", sha: HEAD_SHA },
  });
}

describe("PR Checks tab (DEBT-05)", () => {
  it("detail route wires a Checks tab between Commits and Files changed", () => {
    const pullDetail = readFileSync(
      join(process.cwd(), "src/routes/$owner.$repo.pull.$n.tsrx"),
      "utf8",
    );
    expect(pullDetail).toMatch(/Checks/);
    expect(pullDetail).toMatch(/PullChecks/);
    expect(pullDetail).toMatch(/"checks"/);
    expect(pullDetail).toMatch(/head_sha/);
  });

  it("lists each status context with state, description, link, and time", async () => {
    await mountChecks();

    await waitFor(() => {
      expect(screen.getAllByTestId("pull-check-row")).toHaveLength(3);
    });

    expect(commitStatusListMock).toHaveBeenCalledWith({
      owner: "ada",
      name: "hello",
      sha: HEAD_SHA,
    });

    expect(screen.getByText("CI / build")).toBeInTheDocument();
    expect(screen.getByText("CI / test")).toBeInTheDocument();
    expect(screen.getByText("deploy / preview")).toBeInTheDocument();
    expect(screen.getByText("Completed successfully")).toBeInTheDocument();
    expect(screen.getByText("Waiting for a runner")).toBeInTheDocument();
    expect(screen.getByText("Preview deploy failed")).toBeInTheDocument();
    expect(screen.getByText("success")).toBeInTheDocument();
    expect(screen.getByText("pending")).toBeInTheDocument();
    expect(screen.getByText("failure")).toBeInTheDocument();

    const link = screen.getByRole("link", { name: "CI / build" });
    expect(link).toHaveAttribute("href", "https://ci.example.com/runs/1");
    expect(link).toHaveAttribute("target", "_blank");

    expect(screen.getByTestId("pull-checks-summary").textContent).toMatch(
      /3 checks on 0123456 · 1 successful, 1 pending, 1 failing/,
    );
  }, 30_000);

  it("shows an empty state when no statuses exist", async () => {
    commitStatusListMock.mockResolvedValue({ ok: true, data: { statuses: [] } });
    await mountChecks();

    await waitFor(() => {
      expect(screen.getByText("No checks")).toBeInTheDocument();
    });
  }, 30_000);

  it("prompts sign-in when statuses require a session", async () => {
    commitStatusListMock.mockResolvedValue({
      ok: false,
      error: { code: "auth.unauthenticated", message: "not authenticated" },
    });
    await mountChecks();

    await waitFor(() => {
      expect(screen.getByTestId("pull-checks-signin")).toBeInTheDocument();
    });
  }, 30_000);

  it("surfaces a generic load failure", async () => {
    commitStatusListMock.mockResolvedValue({
      ok: false,
      error: { code: "repo.internal", message: "repository operation failed" },
    });
    await mountChecks();

    await waitFor(() => {
      expect(screen.getByRole("alert")).toHaveTextContent("Could not load checks.");
    });
  }, 30_000);
});
