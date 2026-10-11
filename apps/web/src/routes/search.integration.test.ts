/**
 * /search — grouped sitewide results page (DEBT-03).
 * Renders query-shaped data through the real page (no RPC calls; the
 * `search.global` handler is covered by Rust integration tests).
 */
import { cleanup, screen, waitFor } from "@octanejs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderWithQueryClient } from "@/test/render-with-query";

type Group<T> = { hits: T[]; total: number; truncated: boolean };

function group<T>(hits: T[], total?: number, truncated = false): Group<T> {
  return { hits, total: total ?? hits.length, truncated };
}

type ResultsShape = {
  q: string;
  repositories: Group<Record<string, unknown>>;
  users: Group<Record<string, unknown>>;
  organizations: Group<Record<string, unknown>>;
  issues: Group<Record<string, unknown>>;
  pulls: Group<Record<string, unknown>>;
  commits: Group<Record<string, unknown>>;
  code: Group<Record<string, unknown>>;
};

const emptyResults = {
  q: "kernel",
  repositories: group([]),
  users: group([]),
  organizations: group([]),
  issues: group([]),
  pulls: group([]),
  commits: group([]),
  code: group([]),
};

const searchGlobalMock = vi.fn();
const authMeMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    auth: {
      me: (...args: unknown[]) => authMeMock(...args),
    },
    search: {
      global: (...args: unknown[]) => searchGlobalMock(...args),
    },
  },
}));

function setLocation(path: string) {
  window.history.pushState({}, "", path);
}

import { SearchPage } from "./search";

afterEach(cleanup);

describe("/search route query", () => {
  it("keys the query on the full search param set (q + type + offset)", async () => {
    const src = await import("./search.tsrx?raw").then((m) => String(m.default));
    // queryKey carries all of q/type/offset so tab + paging variants refetch
    // instead of sharing one cached match (was loaderDeps).
    const m2 = src.match(/queryKey:\s*\[([^\]]*)\]/s);
    expect(m2, "search page must declare a queryKey").toBeTruthy();
    const key = m2![1]!;
    expect(key).toContain("q");
    expect(key).toContain("type");
    expect(key).toContain("offset");
  });
});

const signedInUser = {
  id: "u1",
  email: "ada@example.com",
  username: "ada",
  display_name: "Ada",
  bio: "",
  avatar_url: null,
  role: "user",
  profile_incomplete: false,
  email_verified: true,
  must_change_credentials: false,
};

function setResults(results: ResultsShape | null, opts?: { error?: string }) {
  if (opts?.error) {
    searchGlobalMock.mockResolvedValue({
      ok: false,
      error: { code: "search.failed", message: opts.error },
    });
    return;
  }
  searchGlobalMock.mockResolvedValue({ ok: true, data: results });
}

beforeEach(() => {
  setLocation("/search?q=kernel");
  searchGlobalMock.mockReset();
  authMeMock.mockReset();
  authMeMock.mockResolvedValue({ ok: true, data: signedInUser });
  setResults({
    ...emptyResults,
    repositories: group(
      [
        {
          owner: "ada",
          owner_type: "user",
          name: "kernel",
          description: "kernel hacking",
          visibility: "public",
          star_count: 3,
          updated_at: "2026-01-01T00:00:00Z",
        },
        {
          owner: "acme",
          owner_type: "org",
          name: "kernel-fork",
          description: "",
          visibility: "private",
          star_count: 0,
          updated_at: "2026-01-02T00:00:00Z",
        },
      ],
      7,
    ),
    issues: group([
      {
        repo_owner: "ada",
        repo_name: "kernel",
        number: 12,
        title: "kernel panic on boot",
        state: "open",
        author_username: "ada",
        comment_count: 4,
        updated_at: "2026-01-03T00:00:00Z",
      },
    ]),
    pulls: group([
      {
        repo_owner: "ada",
        repo_name: "kernel",
        number: 5,
        title: "fix scheduler",
        state: "merged",
        draft: false,
        author_username: "ada",
        comment_count: 0,
        updated_at: "2026-01-04T00:00:00Z",
      },
    ]),
    commits: group(
      [
        {
          repo_owner: "ada",
          repo_name: "kernel",
          sha: "0123456789abcdef",
          short_sha: "0123456",
          subject: "kernel: tune scheduler",
          author_name: "Ada",
          authored_at: "2026-01-05T00:00:00Z",
        },
      ],
      3,
      true,
    ),
    code: group(
      [
        {
          repo_owner: "ada",
          repo_name: "kernel",
          ref: "main",
          path: "src/sched.rs",
          line: 42,
          content: "fn kernel_main() {}",
          html: '<pre class="shiki" data-language="rust"><code><span style="color:#cf222e">fn</span> kernel_main() {}</code></pre>',
        },
      ],
      1,
    ),
    users: group([{ username: "ada", display_name: "Ada Lovelace", avatar_url: null }]),
    organizations: group([{ slug: "acme", display_name: "Acme Corp" }]),
  });
});

describe("/search overview", () => {
  it("renders grouped sections for every populated kind", async () => {
    renderWithQueryClient(SearchPage);

    await waitFor(
      () => {
        expect(screen.getByTestId("search-section-repositories")).toBeInTheDocument();
      },
      { timeout: 15_000 },
    );
    expect(screen.getByTestId("search-page")).toBeInTheDocument();
    expect(screen.getByTestId("search-section-issues")).toBeInTheDocument();
    expect(screen.getByTestId("search-section-pulls")).toBeInTheDocument();
    expect(screen.getByTestId("search-section-commits")).toBeInTheDocument();
    expect(screen.getByTestId("search-section-code")).toBeInTheDocument();
    expect(screen.getByTestId("search-section-users")).toBeInTheDocument();
    expect(screen.getByTestId("search-section-organizations")).toBeInTheDocument();

    // Repo rows link to /{owner}/{name} and mark visibility.
    const repoLink = screen.getByRole("link", { name: "ada/kernel" });
    expect(repoLink).toHaveAttribute("href", "/ada/kernel");
    expect(screen.getByText("Private")).toBeInTheDocument();

    // Issue + PR rows link to the repo-scoped detail pages.
    expect(screen.getByRole("link", { name: "kernel panic on boot" })).toHaveAttribute(
      "href",
      "/ada/kernel/issues/12",
    );
    expect(screen.getByRole("link", { name: "fix scheduler" })).toHaveAttribute(
      "href",
      "/ada/kernel/pull/5",
    );

    // Commit + code rows link to commit/blob pages.
    expect(screen.getByRole("link", { name: "0123456" })).toHaveAttribute(
      "href",
      "/ada/kernel/commit/0123456789abcdef",
    );
    expect(screen.getByRole("link", { name: "ada/kernel:src/sched.rs" })).toHaveAttribute(
      "href",
      "/ada/kernel/blob/src/sched.rs",
    );

    // Code hits render highlighted Shiki markup on first paint.
    await waitFor(() => {
      const snippet = document.querySelector("pre[data-language='rust']");
      expect(snippet).toBeInTheDocument();
      expect(snippet?.textContent).toContain("fn kernel_main()");
      expect(snippet?.querySelector("span[style*='color']")).not.toBeNull();
    });

    // Users + orgs.
    expect(screen.getByRole("link", { name: "@ada" })).toHaveAttribute("href", "/ada");
    expect(screen.getByRole("link", { name: "acme" })).toHaveAttribute("href", "/acme");

    // Tab badges carry group totals; truncated scan groups render "N+".
    expect(screen.getByRole("link", { name: /Issues 1/ })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /Commits 3\+/ })).toBeInTheDocument();
  });

  it("shows a friendly empty state when nothing matched", async () => {
    setLocation("/search?q=zzz-nomatch");
    setResults({ ...emptyResults, q: "zzz-nomatch" });
    renderWithQueryClient(SearchPage);
    expect(await screen.findByText("No results")).toBeInTheDocument();
  });

  it("prompts for a query when q is empty", async () => {
    setLocation("/search");
    renderWithQueryClient(SearchPage);
    expect(await screen.findByText("Search Oxidean")).toBeInTheDocument();
  });
});

describe("/search scoped tabs", () => {
  it("renders the issues tab list and hides inactive group hits", async () => {
    setLocation("/search?q=kernel&type=issues");
    renderWithQueryClient(SearchPage);
    expect(await screen.findByRole("link", { name: "kernel panic on boot" })).toBeInTheDocument();
    expect(screen.queryByTestId("search-section-repositories")).not.toBeInTheDocument();
  });

  it("shows a sign-in hint for anonymous users on the users tab", async () => {
    setLocation("/search?q=kernel&type=users");
    authMeMock.mockResolvedValue({
      ok: false,
      error: { code: "auth.unauthenticated", message: "not signed in" },
    });
    setResults({ ...emptyResults });
    renderWithQueryClient(SearchPage);
    await waitFor(() => {
      expect(screen.getByRole("link", { name: "Sign in" })).toHaveAttribute(
        "href",
        "/login?returnTo=/search",
      );
    });
    expect(screen.getByText(/to search users/)).toBeInTheDocument();
  });

  it("flags the bounded code scan on the code tab", async () => {
    setLocation("/search?q=kernel&type=code");
    renderWithQueryClient(SearchPage);
    expect(await screen.findByTestId("search-scan-note")).toBeInTheDocument();
    expect(
      await screen.findByRole("link", { name: "ada/kernel:src/sched.rs" }),
    ).toBeInTheDocument();
  });

  it("surfaces the error message when the RPC fails", async () => {
    setLocation("/search?q=kernel&type=repositories");
    setResults(null, { error: "search failed" });
    renderWithQueryClient(SearchPage);
    await waitFor(() => {
      expect(screen.getByTestId("search-error")).toHaveTextContent("search failed");
    });
  });

  it("renders next-page pagination when the group has more hits", async () => {
    setLocation("/search?q=kernel&type=repositories");
    setResults({
      ...emptyResults,
      repositories: group(
        [
          {
            owner: "ada",
            owner_type: "user",
            name: "kernel",
            description: "",
            visibility: "public",
            star_count: 0,
            updated_at: "2026-01-01T00:00:00Z",
          },
        ],
        40,
      ),
    });
    renderWithQueryClient(SearchPage);
    const next = await screen.findByRole("link", { name: /Older/ });
    expect(next).toHaveAttribute("href", "/search?q=kernel&type=repositories&offset=20");
  });
});
