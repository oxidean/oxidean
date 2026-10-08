/**
 * Repo commit detail view — route coverage (D-QH-03) + render mount.
 */
import { cleanup, screen, waitFor } from "@octanejs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderWithQueryClient } from "@/test/render-with-query";
import { RepoCommitPage } from "./$owner.$repo.commit.$sha";

const repoGetMock = vi.fn();
const commitGetMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    auth: {
      me: vi.fn(async () => ({
        ok: false,
        error: { code: "auth.unauthenticated", message: "anonymous" },
      })),
    },
    repo: {
      get: (...args: unknown[]) => repoGetMock(...args),
      commit: (...args: unknown[]) => commitGetMock(...args),
    },
  },
}));

// Keep the mount in happy-dom: the real helper lazily loads Shiki.
vi.mock("@/lib/ssr-diff-highlight", () => ({
  ssrHighlightDiffFiles: vi.fn(async () => ({ theme: null, byPath: {} })),
}));

const SHA = "0123456789abcdef0123456789abcdef01234567";

const repo = {
  id: "r1",
  owner_id: "u1",
  owner_type: "user" as const,
  owner_username: "ada",
  name: "hello",
  description: "",
  visibility: "public" as const,
  default_branch: "main",
  updated_at: "2026-10-02T00:00:00Z",
  can_admin: false,
  can_write: false,
  issues_enabled: true,
  pulls_enabled: true,
};

const commit = {
  sha: SHA,
  short_sha: SHA.slice(0, 7),
  subject: "fix: keep mobile taps alive through hydration",
  body: "",
  author_name: "Ada Lovelace",
  author_email: "ada@example.com",
  authored_at: "2026-10-02T12:00:00Z",
  author_username: "ada",
  parents: ["deadbeef"],
  files: [
    {
      path: "src/lib/early-nav.ts",
      status: "modified",
      patch: "@@ -1,2 +1,2 @@\n-old\n+new",
    },
  ],
  truncated: false,
};

beforeEach(() => {
  window.history.pushState({}, "", `/ada/hello/commit/${SHA}`);
  repoGetMock.mockReset();
  commitGetMock.mockReset();
  repoGetMock.mockResolvedValue({ ok: true, data: repo });
  commitGetMock.mockResolvedValue({ ok: true, data: commit });
});

afterEach(cleanup);

describe("/$owner/$repo/commit/$sha", () => {
  it("mounts the commit detail and renders subject, author, and changed files", async () => {
    renderWithQueryClient(RepoCommitPage);

    await waitFor(() => {
      expect(screen.getByText("fix: keep mobile taps alive through hydration")).toBeInTheDocument();
    });

    expect(commitGetMock).toHaveBeenCalledWith({
      owner: "ada",
      name: "hello",
      sha: SHA,
    });
    expect(screen.getByText("src/lib/early-nav.ts")).toBeInTheDocument();
  });
});
