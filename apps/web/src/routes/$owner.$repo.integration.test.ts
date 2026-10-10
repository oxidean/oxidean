import { cleanup, screen, waitFor } from "@octanejs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderWithQueryClient } from "@/test/render-with-query";

/**
 * Code / tree / blob browse (D-15, D-17, D-25 / GIT-05 UI).
 * Layout chrome mount (D-QH-01).
 */

const getMock = vi.fn();
const treeMock = vi.fn();
const blobMock = vi.fn();
const refsMock = vi.fn();
const commitsMock = vi.fn();
const pathLastMock = vi.fn();
const countMock = vi.fn();
const releasesMock = vi.fn();
const packagesMock = vi.fn();
const contribMock = vi.fn();
const langMock = vi.fn();
const meMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    auth: {
      me: (...args: unknown[]) => meMock(...args),
    },
    repo: {
      get: (...args: unknown[]) => getMock(...args),
      tree: (...args: unknown[]) => treeMock(...args),
      blob: (...args: unknown[]) => blobMock(...args),
      refs: (...args: unknown[]) => refsMock(...args),
      commits: (...args: unknown[]) => commitsMock(...args),
      pathLastCommits: (...args: unknown[]) => pathLastMock(...args),
      commitCount: (...args: unknown[]) => countMock(...args),
      releases: (...args: unknown[]) => releasesMock(...args),
      packages: (...args: unknown[]) => packagesMock(...args),
      contributorsList: (...args: unknown[]) => contribMock(...args),
      languages: (...args: unknown[]) => langMock(...args),
    },
  },
}));

vi.mock("@/lib/use-chrome-account", () => ({
  useChromeAccountState: () => ({
    pending: false,
    user: {
      id: "u1",
      email: "ada@example.com",
      username: "ada",
      display_name: "Ada",
      bio: "",
      role: "user",
      profile_incomplete: false,
      email_verified: true,
      must_change_credentials: false,
    },
    needsSetup: false,
    allowSignup: true,
  }),
  resolveAllowSignup: () => true,
}));

beforeEach(() => {
  window.history.pushState({}, "", "/ada/hello");
  getMock.mockReset();
  treeMock.mockReset();
  blobMock.mockReset();
  refsMock.mockReset();
  commitsMock.mockReset();
  pathLastMock.mockReset();
  countMock.mockReset();
  releasesMock.mockReset();
  packagesMock.mockReset();
  contribMock.mockReset();
  langMock.mockReset();
  meMock.mockReset();
  meMock.mockResolvedValue({
    ok: false,
    error: { code: "auth.unauthenticated", message: "n" },
  });
  commitsMock.mockResolvedValue({ ok: true, data: { commits: [] } });
  pathLastMock.mockResolvedValue({ ok: true, data: { last_commits: {} } });
  countMock.mockResolvedValue({ ok: true, data: { count: 0 } });
  releasesMock.mockResolvedValue({ ok: true, data: { releases: [] } });
  packagesMock.mockResolvedValue({ ok: true, data: { packages: [] } });
  contribMock.mockResolvedValue({ ok: true, data: { contributors: [] } });
  langMock.mockResolvedValue({ ok: true, data: { languages: [] } });
});

afterEach(cleanup);

describe("/$owner/$repo layout chrome (D-QH-01)", () => {
  it("ok-status layout mounts RepoChrome above Outlet from useRepoStore", async () => {
    const src = await import("./$owner.$repo.tsrx?raw").then((m) =>
      String((m as { default: string }).default),
    );
    expect(src).toMatch(/RepoChrome/);
    expect(src).toMatch(/useRepoStore/);
    expect(src).toMatch(/repoChromeActiveFromPath/);
    // Layout is a wrapper component now — children slot replaces <Outlet/>.
    expect(src).toMatch(/props\.children|children/);
    expect(src).toMatch(/RepoLayoutChrome/);
  }, 30_000);

  it("code home leaf has no RepoChrome — chrome is layout-owned (D-QH-01)", async () => {
    const src = await import("./$owner.$repo.index.tsrx?raw").then((m) =>
      String((m as { default: string }).default),
    );
    expect(src).not.toMatch(/RepoChrome/);
    expect(src).toMatch(/QuickSetup/);
  }, 30_000);

  it("RepoChrome exposes Code tab + active code contract (D-QH-01)", async () => {
    const chrome = await import("../components/repo/repo-chrome.tsrx?raw").then((m) =>
      String((m as { default: string }).default),
    );
    expect(chrome).toMatch(/>\s*Code\s*</);
    expect(chrome).toMatch(/active === "code"/);
    expect(chrome).toMatch(/RepoChromeActive/);
  }, 30_000);
});

describe("/{owner}/{repo} Code home (D-15, D-25)", () => {
  it("empty repo Code home shows Quick setup (not tree)", async () => {
    getMock.mockResolvedValue({
      ok: true,
      data: {
        id: "r1",
        owner_id: "u1",
        owner_type: "user",
        owner_username: "ada",
        name: "hello",
        description: "",
        visibility: "public",
        default_branch: "main",
        updated_at: "2026-09-12T00:00:00Z",
        can_admin: true,
      },
    });
    treeMock.mockResolvedValue({
      ok: true,
      data: {
        empty: true,
        ref: "main",
        path: "",
        entries: [],
      },
    });
    refsMock.mockResolvedValue({ ok: true, data: { refs: [] } });

    const { RepoCodeHome } = await import("./$owner.$repo.index");
    renderWithQueryClient(RepoCodeHome as never);

    await waitFor(() => {
      expect(screen.getByText("Quick setup")).toBeInTheDocument();
    });
    expect(screen.queryByText("src")).not.toBeInTheDocument();
    expect(screen.queryByText("Page not found")).not.toBeInTheDocument();
    // Settings lives in layout RepoChrome (D-QH-01), not the code-home leaf.
    expect(screen.queryByRole("link", { name: "Settings" })).not.toBeInTheDocument();
  }, 20000);

  it("tree route lists dirs first when commits exist", async () => {
    getMock.mockResolvedValue({
      ok: true,
      data: {
        id: "r1",
        owner_id: "u1",
        owner_type: "user",
        owner_username: "ada",
        name: "hello",
        description: "A sample repo",
        visibility: "public",
        default_branch: "main",
        updated_at: "2026-09-12T00:00:00Z",
      },
    });
    treeMock.mockResolvedValue({
      ok: true,
      data: {
        empty: false,
        ref: "main",
        path: "",
        entries: [
          { mode: "100644", kind: "blob", oid: "a", name: "README.md" },
          { mode: "040000", kind: "tree", oid: "b", name: "src" },
          { mode: "100644", kind: "blob", oid: "c", name: "package.json" },
        ],
      },
    });
    blobMock.mockResolvedValue({
      ok: true,
      data: {
        path: "README.md",
        ref: "main",
        size: 12,
        truncated: false,
        is_binary: false,
        encoding: "utf-8",
        content: "# Hello\n",
        soft_max_bytes: 1048576,
      },
    });
    refsMock.mockResolvedValue({
      ok: true,
      data: {
        refs: [{ name: "refs/heads/main", oid: "abc" }],
      },
    });

    const { RepoCodeHome } = await import("./$owner.$repo.index");
    renderWithQueryClient(RepoCodeHome as never);

    await waitFor(() => {
      expect(screen.getByText("src")).toBeInTheDocument();
    });
    expect(screen.queryByText("Quick setup")).not.toBeInTheDocument();

    const names = ["src", "README.md", "package.json"].map((n) =>
      screen.getByRole("link", { name: n }),
    );
    const labels = names.map((el) => el.textContent?.trim() ?? "");
    // Re-query in document order via tree list links
    const treeLinks = screen
      .getByRole("list", { name: /directories|files|tree/i })
      .querySelectorAll("a");
    const ordered = [...treeLinks].map((a) => a.textContent?.trim() ?? "");
    expect(ordered[0]).toBe("src");
    expect(ordered).toContain("README.md");
    expect(ordered).toContain("package.json");
    expect(ordered.indexOf("src")).toBeLessThan(ordered.indexOf("README.md"));
    expect(labels).toContain("src");
  }, 20000);

  it("private non-owner (or missing) shows Not found — same copy", async () => {
    getMock.mockResolvedValue({
      ok: false,
      error: { code: "repo.not_found", message: "Repository not found" },
    });

    const { RepoCodeHome } = await import("./$owner.$repo.index");
    renderWithQueryClient(RepoCodeHome as never);

    await waitFor(() => {
      expect(screen.getByText("Page not found")).toBeInTheDocument();
    });
    expect(screen.getByText("We couldn't find that page.")).toBeInTheDocument();
    expect(screen.queryByText("Quick setup")).not.toBeInTheDocument();
    expect(screen.queryByText("ada / hello")).not.toBeInTheDocument();
  }, 20000);
});
