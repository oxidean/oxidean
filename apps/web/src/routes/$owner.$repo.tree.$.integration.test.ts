/**
 * Repo tree page — route coverage (D-QH-03) + render mount.
 *
 * Regression: a manual useEffect fallback wrapped its fetchers as
 * `({ data }) => apiClient.repo.X(data)` while `loadTreePage` passes
 * `{ owner, name, ... }` flat — `data` destructured to `undefined`, so the
 * first render fired `repo.get(undefined)` and briefly rendered
 * "invalid repo.get input: missing field `owner`" until the TanStack query
 * resolved. The fallback also duplicated the whole load plan.
 */
import { cleanup, screen, waitFor } from "@octanejs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderWithQueryClient } from "@/test/render-with-query";
import { RepoTreePage } from "./$owner.$repo.tree.$";

const repoGetMock = vi.fn();
const repoTreeMock = vi.fn();
const repoRefsMock = vi.fn();
const repoBlobMock = vi.fn();
const repoCommitsMock = vi.fn();
const repoPathLastMock = vi.fn();
const repoCommitCountMock = vi.fn();
const repoContributorsMock = vi.fn();
const repoLanguagesMock = vi.fn();
const releaseListMock = vi.fn();
const packagesListMock = vi.fn();

const err = (code: string) => ({ ok: false, error: { code, message: code } });

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    auth: {
      me: vi.fn(async () => err("auth.unauthenticated")),
    },
    repo: {
      get: (...args: unknown[]) => repoGetMock(...args),
      tree: (...args: unknown[]) => repoTreeMock(...args),
      refs: (...args: unknown[]) => repoRefsMock(...args),
      blob: (...args: unknown[]) => repoBlobMock(...args),
      commits: (...args: unknown[]) => repoCommitsMock(...args),
      pathLastCommits: (...args: unknown[]) => repoPathLastMock(...args),
      commitCount: (...args: unknown[]) => repoCommitCountMock(...args),
      contributorsList: (...args: unknown[]) => repoContributorsMock(...args),
      languages: (...args: unknown[]) => repoLanguagesMock(...args),
    },
    release: {
      list: (...args: unknown[]) => releaseListMock(...args),
    },
    packages: {
      list: (...args: unknown[]) => packagesListMock(...args),
    },
  },
}));

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

beforeEach(() => {
  window.history.pushState({}, "", "/ada/hello/tree/main");
  for (const m of [
    repoGetMock,
    repoTreeMock,
    repoRefsMock,
    repoBlobMock,
    repoCommitsMock,
    repoPathLastMock,
    repoCommitCountMock,
    repoContributorsMock,
    repoLanguagesMock,
    releaseListMock,
    packagesListMock,
  ]) {
    m.mockReset();
    m.mockResolvedValue(err("not.used"));
  }
  repoGetMock.mockResolvedValue({ ok: true, data: repo });
  repoRefsMock.mockResolvedValue({
    ok: true,
    data: { refs: [{ name: "refs/heads/main", oid: "abc123" }] },
  });
  repoTreeMock.mockResolvedValue({
    ok: true,
    data: {
      empty: false,
      ref: "refs/heads/main",
      path: "",
      entries: [{ mode: "100644", kind: "blob", oid: "deadbeef", name: "main.ts" }],
    },
  });
});

afterEach(cleanup);

describe("/$owner/$repo/tree/$", () => {
  it("renders the tree without ever calling repo.get with missing input", async () => {
    renderWithQueryClient(RepoTreePage);

    await waitFor(() => {
      expect(screen.getByText("main.ts")).toBeInTheDocument();
    });

    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(repoGetMock.mock.calls.length).toBeGreaterThan(0);
    for (const [input] of repoGetMock.mock.calls) {
      expect(input).toMatchObject({ owner: "ada", name: "hello" });
    }
    // Layout + content queries only — the removed effect fallback used to add a
    // third (malformed) call.
    expect(repoGetMock.mock.calls.length).toBeLessThanOrEqual(2);
  });
});
