import { cleanup, fireEvent, screen, waitFor } from "@octanejs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderWithQueryClient } from "@/test/render-with-query";

const searchMock = vi.fn();

vi.mock("@/lib/ssr-repo", () => ({
  fetchRepoSearch: (opts: { data: unknown }) => searchMock(opts.data),
}));

const navigateMock = vi.fn();
let searchState = { q: "UNIQUE_HIT", type: "code" };

vi.mock("@octanejs/tanstack-router", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@octanejs/tanstack-router")>();
  return {
    ...actual,
    useParams: () => ({ owner: "ada", repo: "hello" }),
    useSearch: () => searchState,
    useNavigate: () => navigateMock,
  };
});

import { RepoSearchPage } from "./$owner.$repo.search";

afterEach(() => {
  cleanup();
  searchMock.mockReset();
  navigateMock.mockReset();
  searchState = { q: "UNIQUE_HIT", type: "code" };
});

beforeEach(() => {
  searchMock.mockResolvedValue({
    ok: true,
    data: {
      type: "code",
      q: "UNIQUE_HIT",
      truncated: true,
      offset: 0,
      limit: 30,
      hits: [
        {
          kind: "code",
          path: "src/needle.txt",
          line: 2,
          content: "UNIQUE_HIT line",
          html: '<pre class="shiki" data-language="text"><code><span style="color:#cf222e">UNIQUE_HIT line</span></code></pre>',
        },
      ],
    },
    highlightTheme: "oxidean-light",
  });
});

describe("repo search route (GIT-18 / D-SRCH-02 / D-SRCH-15)", () => {
  it("renders query input for /{owner}/{repo}/search", async () => {
    renderWithQueryClient(RepoSearchPage);
    expect(await screen.findByLabelText(/search query/i)).toBeTruthy();
  });

  it("shows code hits from repo.search type=code", async () => {
    renderWithQueryClient(RepoSearchPage);
    await waitFor(() => {
      expect(searchMock).toHaveBeenCalled();
    });
    expect(await screen.findByText(/src\/needle\.txt/)).toBeTruthy();
    expect(screen.getByText(/UNIQUE_HIT line/)).toBeTruthy();
  });

  it("exposes type tabs: Code, Commits, Issues, Pull requests", async () => {
    renderWithQueryClient(RepoSearchPage);
    expect(await screen.findByTestId("search-tab-code")).toBeTruthy();
    expect(screen.getByTestId("search-tab-commits")).toBeTruthy();
    expect(screen.getByTestId("search-tab-issues")).toBeTruthy();
    expect(screen.getByTestId("search-tab-pulls")).toBeTruthy();
  });

  it("switches type query param and refetches on tab change", async () => {
    renderWithQueryClient(RepoSearchPage);
    const commitsTab = await screen.findByTestId("search-tab-commits");
    fireEvent.click(commitsTab);
    expect(navigateMock).toHaveBeenCalled();
    const arg = navigateMock.mock.calls[0]?.[0] as { search?: { type?: string } };
    expect(arg?.search?.type).toBe("commits");
  });

  it("renders empty and truncated states", async () => {
    renderWithQueryClient(RepoSearchPage);
    expect(await screen.findByTestId("search-truncated")).toBeTruthy();

    searchMock.mockResolvedValueOnce({
      ok: true,
      data: {
        type: "code",
        q: "UNIQUE_HIT",
        truncated: false,
        offset: 0,
        limit: 30,
        hits: [],
      },
      highlightTheme: "oxidean-light",
    });
    cleanup();
    renderWithQueryClient(RepoSearchPage);
    expect(await screen.findByTestId("search-empty")).toBeTruthy();
  });

  it("renders language browse hits as file links without line/content", async () => {
    searchState = { q: "language:Rust", type: "code" };
    searchMock.mockResolvedValueOnce({
      ok: true,
      data: {
        type: "code",
        q: "language:Rust",
        truncated: false,
        offset: 0,
        limit: 30,
        hits: [
          { kind: "code", path: "src/main.rs", line: 0, content: "" },
          { kind: "code", path: "crates/lib.rs", line: 0, content: "" },
        ],
      },
      highlightTheme: "oxidean-light",
    });
    renderWithQueryClient(RepoSearchPage);
    const link = await screen.findByText("src/main.rs");
    expect(link.closest("a")?.getAttribute("href")).toBe("/ada/hello/blob/src/main.rs");
    expect(screen.queryByText(":0")).toBeNull();
    expect(screen.getByText("crates/lib.rs")).toBeTruthy();
    // No <pre> body for browse hits (empty content).
    const li = link.closest("li");
    expect(li?.querySelector("pre")).toBeNull();
  });

  it("repo chrome search entry navigates to /search (D-SRCH-03)", async () => {
    const chrome = await import("../components/repo/repo-chrome.tsrx?raw").then((m) =>
      String((m as { default: string }).default),
    );
    expect(chrome).toMatch(/RepoSearchEntry/);
    const entry = await import("../components/repo/repo-search-entry.tsrx?raw").then((m) =>
      String((m as { default: string }).default),
    );
    expect(entry).toMatch(/\/search/);
    // Header GlobalSearch is a multi-entity omnibar that lands on /search (not public-only /explore).
    const global = await import("../components/global-search.tsrx?raw").then((m) =>
      String((m as { default: string }).default),
    );
    expect(global).toMatch(/\/search/);
    expect(global).toMatch(/fetchSearchSuggestions|global-search-suggest/);
    expect(global).not.toMatch(/goExplore/);
  });
});
