import { cleanup, fireEvent, screen, waitFor } from "@octanejs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { renderWithQueryClient } from "@/test/render-with-query";

/**
 * Phase 12 Pulls UI — tracer greened chrome/list/new/detail; later plans green the rest.
 */

const pullCreateMock = vi.fn();
const userLookupMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    pull: { create: (...args: unknown[]) => pullCreateMock(...args) },
    user: { lookup: (...args: unknown[]) => userLookupMock(...args) },
  },
}));

let pullsNewLoaderData: unknown = undefined;
let pullsSearchState: Record<string, unknown> = {};

vi.mock("@octanejs/tanstack-router", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@octanejs/tanstack-router")>();
  return {
    ...actual,
    useParams: () => ({ owner: "ada", repo: "hello" }),
    useLoaderData: () => pullsNewLoaderData,
    useSearch: () => pullsSearchState,
    useNavigate: () => vi.fn(),
  };
});

beforeEach(() => {
  pullsNewLoaderData = undefined;
  pullsSearchState = {};
  pullCreateMock.mockReset();
  userLookupMock.mockReset();
  userLookupMock.mockResolvedValue({ ok: true, data: { users: [] } });
});

afterEach(cleanup);

const chromeActive = readFileSync(join(process.cwd(), "src/lib/repo-chrome-active.ts"), "utf8");
const repoChrome = readFileSync(
  join(process.cwd(), "src/components/repo/repo-chrome.tsrx"),
  "utf8",
);
const pullsIndex = readFileSync(
  join(process.cwd(), "src/routes/$owner.$repo.pulls.index.tsrx"),
  "utf8",
);
const pullsNew = readFileSync(
  join(process.cwd(), "src/routes/$owner.$repo.pulls.new.tsrx"),
  "utf8",
);
const pullDetail = readFileSync(
  join(process.cwd(), "src/routes/$owner.$repo.pull.$n.tsrx"),
  "utf8",
);

describe("Phase 12 Pulls UI", () => {
  it("RepoChromeActive includes pulls and maps /pulls|/pull", () => {
    expect(chromeActive).toMatch(/"pulls"/);
    expect(chromeActive).toMatch(/pulls:\s*"pulls"/);
    expect(chromeActive).toMatch(/pull:\s*"pulls"/);
  });

  it("RepoChrome renders Pulls tab", () => {
    expect(repoChrome).toMatch(/Pulls/);
    expect(repoChrome).toMatch(/\/pulls/);
  });

  it("pulls list route defaults Open with Closed/All", () => {
    expect(pullsIndex).toMatch(/Open/);
    expect(pullsIndex).toMatch(/Closed/);
    expect(pullsIndex).toMatch(/All/);
  });

  it("New pull request gated by verified session (D-PR-29)", () => {
    expect(pullsIndex).toMatch(/email_verified/);
    expect(pullsNew).toMatch(/Create pull request/);
  });

  it("Author filter uses MemberLookup autocomplete", () => {
    expect(pullsIndex).toMatch(/MemberLookup/);
    expect(pullsIndex).toMatch(/pull-filter-author/);
  });

  it("pulls list filters include assignee and search", () => {
    expect(pullsIndex).toMatch(/pull-filter-assignee/);
    expect(pullsIndex).toMatch(/Search title or body/);
    expect(pullsIndex).toMatch(/assignee:/);
    expect(pullsIndex).toMatch(/\bq:/);
  });

  it("new PR head owner uses MemberLookup", () => {
    expect(pullsNew).toMatch(/MemberLookup/);
    expect(pullsNew).toMatch(/pull-new-head-owner/);
  });

  it("pulls list rows show assignees when present", () => {
    const pullsList = readFileSync(
      join(process.cwd(), "src/components/repo/pulls-list.tsrx"),
      "utf8",
    );
    expect(pullsList).toMatch(/assigned/);
    expect(pullsList).toMatch(/assignees/);
  });

  it("compare flow can create a PR", () => {
    const compare = readFileSync(
      join(process.cwd(), "src/routes/$owner.$repo.compare.$.tsrx"),
      "utf8",
    );
    expect(compare).toMatch(/Create pull request/);
    expect(compare).toMatch(/pulls\/new/);
  });

  it("detail tabs Conversation | Commits | Checks | Files changed", () => {
    expect(pullDetail).toMatch(/Conversation/);
    expect(pullDetail).toMatch(/Commits/);
    expect(pullDetail).toMatch(/Checks/);
    expect(pullDetail).toMatch(/Files changed/);
    expect(pullDetail).toMatch(/PullChecks/);
  });

  it("unified and split diff toggle", () => {
    const pullFiles = readFileSync(
      join(process.cwd(), "src/components/repo/pull-files.tsrx"),
      "utf8",
    );
    expect(pullFiles).toMatch(/Unified/);
    expect(pullFiles).toMatch(/Split/);
  });

  it("review actions Approve / Request changes / Comment", () => {
    const reviews = readFileSync(
      join(process.cwd(), "src/components/repo/pull-reviews.tsrx"),
      "utf8",
    );
    expect(reviews).toMatch(/Approve/);
    expect(reviews).toMatch(/Request changes/);
    expect(reviews).toMatch(/Comment/);
  });

  it("merge method picker + close/reopen", () => {
    expect(pullDetail).toMatch(/Close pull request/);
    expect(pullDetail).toMatch(/Reopen pull request/);
    expect(pullDetail).toMatch(/PullMergePanel/);
  });

  it("Admin merge strategy settings", () => {
    const settings = readFileSync(
      join(process.cwd(), "src/components/repo/merge-settings-panel.tsrx"),
      "utf8",
    );
    expect(settings).toMatch(/Allow merge commits/);
    expect(settings).toMatch(/Allow squash merging/);
    expect(settings).toMatch(/Allow rebase merging/);
  });

  it("Write|Preview on PR comments", () => {
    const conversation = readFileSync(
      join(process.cwd(), "src/components/repo/pull-conversation.tsrx"),
      "utf8",
    );
    expect(conversation).toMatch(/MarkdownWritePreview/);
    expect(conversation).toMatch(/Outdated/);
  });
});

describe("pulls/new template chooser (COL-02)", () => {
  const loaderWithTemplates = {
    kind: "ready",
    repo: { default_branch: "main", can_write: true },
    refs: [{ name: "refs/heads/main" }],
    templates: {
      issues: [],
      pulls: [
        {
          name: "Standard PR",
          description: "Default change checklist",
          body: "## Checklist\n\n- [ ] tests\n",
          filename: ".github/PULL_REQUEST_TEMPLATE/standard.md",
        },
        {
          name: "Hotfix",
          description: "Urgent fix",
          body: "## Hotfix\n\nWhat broke?\n",
          filename: ".github/PULL_REQUEST_TEMPLATE/hotfix.md",
        },
      ],
    },
  };

  it("multi-template repos show a chooser; picking prefills the body", async () => {
    pullsNewLoaderData = loaderWithTemplates;
    const mod = (await import(/* @vite-ignore */ "./$owner.$repo.pulls.new")) as Record<
      string,
      unknown
    >;
    const page = (mod.NewPullPage ?? mod.default) as never;
    expect(page, "NewPullPage must be exported").toBeTruthy();
    renderWithQueryClient(page);

    await waitFor(() => {
      expect(screen.getByTestId("pull-template-chooser")).toBeInTheDocument();
    });
    expect(screen.getByText("Standard PR")).toBeInTheDocument();
    expect(screen.getByText("Default change checklist")).toBeInTheDocument();
    expect(screen.getByText("Hotfix")).toBeInTheDocument();
    // Branch selectors stay available while choosing.
    expect(screen.getByText(/^Base$/i)).toBeInTheDocument();

    fireEvent.click(screen.getByText("Hotfix"));

    await waitFor(() => {
      expect(screen.queryByTestId("pull-template-chooser")).not.toBeInTheDocument();
    });
    const textarea = document.querySelector("textarea");
    expect(textarea?.value).toContain("## Hotfix");
  }, 15_000);

  it("template-less repos render the plain form (no chooser)", async () => {
    pullsNewLoaderData = {
      kind: "ready",
      repo: { default_branch: "main", can_write: true },
      refs: [{ name: "refs/heads/main" }],
      templates: { issues: [], pulls: [] },
    };
    const mod = (await import(/* @vite-ignore */ "./$owner.$repo.pulls.new")) as Record<
      string,
      unknown
    >;
    renderWithQueryClient((mod.NewPullPage ?? mod.default) as never);

    await waitFor(() => {
      expect(screen.getByRole("button", { name: /Create pull request/i })).toBeInTheDocument();
    });
    expect(screen.queryByTestId("pull-template-chooser")).not.toBeInTheDocument();
  }, 15_000);
});
