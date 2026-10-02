/**
 * /search — grouped sitewide results page (DEBT-03).
 * Renders loader-shaped data through the real page (no RPC calls; the
 * `search.global` handler is covered by Rust integration tests).
 */
import { cleanup, render, screen } from "@octanejs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

type Group<T> = { hits: T[]; total: number; truncated: boolean };

function group<T>(hits: T[], total?: number, truncated = false): Group<T> {
  return { hits, total: total ?? hits.length, truncated };
}

type LoaderShape = {
  q: string;
  type: string;
  offset: number;
  results: {
    q: string;
    repositories: Group<Record<string, unknown>>;
    users: Group<Record<string, unknown>>;
    organizations: Group<Record<string, unknown>>;
    issues: Group<Record<string, unknown>>;
    pulls: Group<Record<string, unknown>>;
    commits: Group<Record<string, unknown>>;
    code: Group<Record<string, unknown>>;
  } | null;
  error: string | null;
  signedIn: boolean;
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

let loaderData: LoaderShape;

vi.mock("@octanejs/tanstack-router", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@octanejs/tanstack-router")>();
  return {
    ...actual,
    useLoaderData: () => loaderData,
    createFileRoute: () => (opts: { component?: unknown }) => opts,
  };
});

import { SearchPage } from "./search";

afterEach(cleanup);

beforeEach(() => {
  loaderData = {
    q: "kernel",
    type: "overview",
    offset: 0,
    results: {
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
          },
        ],
        1,
      ),
      users: group([{ username: "ada", display_name: "Ada Lovelace", avatar_url: null }]),
      organizations: group([{ slug: "acme", display_name: "Acme Corp" }]),
    },
    error: null,
    signedIn: true,
  };
});

describe("/search overview", () => {
  it("renders grouped sections for every populated kind", () => {
    render(SearchPage as never);

    expect(screen.getByTestId("search-page")).toBeInTheDocument();
    expect(screen.getByTestId("search-section-repositories")).toBeInTheDocument();
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

    // Users + orgs.
    expect(screen.getByRole("link", { name: "@ada" })).toHaveAttribute("href", "/ada");
    expect(screen.getByRole("link", { name: "acme" })).toHaveAttribute("href", "/acme");

    // Tab badges carry group totals; truncated scan groups render "N+".
    expect(screen.getByRole("link", { name: /Issues 1/ })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /Commits 3\+/ })).toBeInTheDocument();
  });

  it("shows a friendly empty state when nothing matched", () => {
    loaderData = {
      q: "zzz-nomatch",
      type: "overview",
      offset: 0,
      results: { ...emptyResults, q: "zzz-nomatch" },
      error: null,
      signedIn: true,
    };
    render(SearchPage as never);
    expect(screen.getByText("No results")).toBeInTheDocument();
  });

  it("prompts for a query when q is empty", () => {
    loaderData = {
      q: "",
      type: "overview",
      offset: 0,
      results: null,
      error: null,
      signedIn: false,
    };
    render(SearchPage as never);
    expect(screen.getByText("Search Oxidean")).toBeInTheDocument();
  });
});

describe("/search scoped tabs", () => {
  it("renders the issues tab list and hides inactive group hits", () => {
    loaderData = { ...loaderData, type: "issues", results: loaderData.results };
    render(SearchPage as never);
    expect(screen.getByRole("link", { name: "kernel panic on boot" })).toBeInTheDocument();
    expect(screen.queryByTestId("search-section-repositories")).not.toBeInTheDocument();
  });

  it("shows a sign-in hint for anonymous users on the users tab", () => {
    loaderData = {
      ...loaderData,
      type: "users",
      signedIn: false,
      results: { ...emptyResults },
    };
    render(SearchPage as never);
    expect(screen.getByRole("link", { name: "Sign in" })).toHaveAttribute(
      "href",
      "/login?returnTo=/search",
    );
    expect(screen.getByText(/to search users/)).toBeInTheDocument();
  });

  it("flags the bounded code scan on the code tab", () => {
    loaderData = { ...loaderData, type: "code" };
    render(SearchPage as never);
    expect(screen.getByTestId("search-scan-note")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "ada/kernel:src/sched.rs" })).toBeInTheDocument();
  });

  it("surfaces the error message when the RPC fails", () => {
    loaderData = {
      ...loaderData,
      type: "repositories",
      results: null,
      error: "search failed",
    };
    render(SearchPage as never);
    expect(screen.getByTestId("search-error")).toHaveTextContent("search failed");
  });

  it("renders next-page pagination when the group has more hits", () => {
    loaderData = {
      ...loaderData,
      type: "repositories",
      results: {
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
      },
    };
    render(SearchPage as never);
    const next = screen.getByRole("link", { name: /Older/ });
    expect(next).toHaveAttribute("href", "/search?q=kernel&type=repositories&offset=20");
  });
});
