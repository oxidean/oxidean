/**
 * pull-v2 scaffold (WI-0001, stage 3): splat → tab mapping, sticky chrome,
 * URL-backed tab navigation, count badges, header actions (copy link /
 * submit review / merge chip / comments panel), inline title edit, and
 * per-tab delegation — all against a mocked api-client.
 */
import { cleanup, fireEvent, screen, waitFor } from "@octanejs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderWithQueryClient } from "@/test/render-with-query";

const commitsMock = vi.fn();
const filesMock = vi.fn();
const filesListViewedMock = vi.fn();
const filesSetViewedMock = vi.fn();
const commentsListMock = vi.fn();
const commentsCreateMock = vi.fn();
const commentsDeleteMock = vi.fn();
const commentsHistoryMock = vi.fn();
const reviewsListMock = vi.fn();
const branchStatusMock = vi.fn();
const updateMock = vi.fn();
const reviewRequestsListMock = vi.fn();
const suitesListMock = vi.fn();
const runsListForRefMock = vi.fn();
const runsListForSuiteMock = vi.fn();
const runsGetMock = vi.fn();
const annotationsListMock = vi.fn();
const mergeSettingsGetMock = vi.fn();
const mergeQueueGetMock = vi.fn();
const assigneeCandidatesMock = vi.fn();
const labelsForRepoMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    pull: {
      commits: (...args: unknown[]) => commitsMock(...args),
      files: (...args: unknown[]) => filesMock(...args),
      filesViewed: {
        list: (...args: unknown[]) => filesListViewedMock(...args),
        set: (...args: unknown[]) => filesSetViewedMock(...args),
      },
      comments: {
        list: (...args: unknown[]) => commentsListMock(...args),
        create: (...args: unknown[]) => commentsCreateMock(...args),
        delete: (...args: unknown[]) => commentsDeleteMock(...args),
        history: (...args: unknown[]) => commentsHistoryMock(...args),
      },
      reviews: {
        list: (...args: unknown[]) => reviewsListMock(...args),
      },
      reviewRequests: {
        list: (...args: unknown[]) => reviewRequestsListMock(...args),
      },
      branchStatus: (...args: unknown[]) => branchStatusMock(...args),
      update: (...args: unknown[]) => updateMock(...args),
    },
    check: {
      suites: {
        list: (...args: unknown[]) => suitesListMock(...args),
      },
      runs: {
        listForRef: (...args: unknown[]) => runsListForRefMock(...args),
        listForSuite: (...args: unknown[]) => runsListForSuiteMock(...args),
        get: (...args: unknown[]) => runsGetMock(...args),
      },
      annotations: {
        list: (...args: unknown[]) => annotationsListMock(...args),
      },
    },
    mergeSettings: {
      get: (...args: unknown[]) => mergeSettingsGetMock(...args),
    },
    mergeQueue: {
      get: (...args: unknown[]) => mergeQueueGetMock(...args),
    },
    issue: {
      assigneeCandidates: (...args: unknown[]) => assigneeCandidatesMock(...args),
    },
    label: {
      listForRepo: (...args: unknown[]) => labelsForRepoMock(...args),
    },
  },
}));

vi.mock("@/components/repo/pull-conversation", () => ({
  PullConversation: () => null,
}));
vi.mock("@/components/repo/pull-reviews", () => ({
  PullReviews: () => null,
}));
vi.mock("@/components/repo/pull-commits", () => ({
  PullCommits: () => null,
}));
vi.mock("@/components/repo/pull-checks", () => ({
  PullChecks: () => null,
}));
vi.mock("@/components/repo/pull-files", () => ({
  PullFiles: () => null,
}));

import { PullDetailV2, pullV2TabFromSplat } from "./pull-v2.tsrx";

const PULL = {
  number: 42,
  title: "Wire the thing",
  state: "open",
  draft: false,
  author_id: "u1",
  author_username: "ada",
  head_owner: "ada",
  head_name: "hello",
  head_ref: "feature/x",
  head_sha: "abc123",
  base_ref: "main",
  body: "",
};

const REPO = { name: "hello", can_write: true, archived: false };

const FILES = {
  files: [
    {
      path: "src/a.ts",
      status: "modified",
      patch: "@@ -1,2 +1,4 @@\n line1\n-old\n+new1\n+new2\n+new3\n line2",
    },
    {
      path: "docs/b.md",
      status: "added",
      patch: "@@ -0,0 +1 @@\n+hello",
    },
  ],
  empty: false,
};

const COMMITS = {
  commits: [
    {
      sha: "aaaa1111aaaa",
      short_sha: "aaaa111",
      subject: "one",
      author_name: "Ada",
      author_email: "a@x",
      authored_at: "2026-03-04T10:00:00Z",
    },
    {
      sha: "bbbb2222bbbb",
      short_sha: "bbbb222",
      subject: "two",
      author_name: "Ada",
      author_email: "a@x",
      authored_at: "2026-03-04T11:00:00Z",
    },
    {
      sha: "cccc3333cccc",
      short_sha: "cccc333",
      subject: "three",
      author_name: "Ada",
      author_email: "a@x",
      authored_at: "2026-03-05T09:00:00Z",
    },
  ],
};

function renderV2(
  tab: "overview" | "commits" | "checks" | "changes",
  overrides: Record<string, unknown> = {},
) {
  return renderWithQueryClient(PullDetailV2, {
    props: {
      owner: "ada",
      repoName: "hello",
      number: 42,
      repo: REPO as never,
      pull: PULL as never,
      canWrite: true,
      canInteract: true,
      isAuthor: true,
      currentUserId: "u1",
      tab,
      ...overrides,
    },
  });
}

afterEach(() => {
  cleanup();
  window.history.replaceState(null, "", "/");
});

beforeEach(() => {
  for (const m of [
    commitsMock,
    filesMock,
    filesListViewedMock,
    filesSetViewedMock,
    commentsListMock,
    commentsCreateMock,
    commentsDeleteMock,
    commentsHistoryMock,
    reviewsListMock,
    branchStatusMock,
    updateMock,
    reviewRequestsListMock,
    suitesListMock,
    runsListForRefMock,
    runsListForSuiteMock,
    runsGetMock,
    annotationsListMock,
    mergeSettingsGetMock,
    mergeQueueGetMock,
    assigneeCandidatesMock,
    labelsForRepoMock,
  ]) {
    m.mockReset();
  }

  commitsMock.mockResolvedValue({ ok: true, data: COMMITS });
  filesMock.mockResolvedValue({ ok: true, data: FILES });
  filesListViewedMock.mockResolvedValue({ ok: true, data: { viewed_paths: [] } });
  commentsListMock.mockResolvedValue({ ok: true, data: { comments: [] } });
  commentsDeleteMock.mockResolvedValue({ ok: true, data: { ok: true } });
  commentsHistoryMock.mockResolvedValue({ ok: true, data: { revisions: [] } });
  reviewsListMock.mockResolvedValue({ ok: true, data: { reviews: [] } });
  reviewRequestsListMock.mockResolvedValue({ ok: true, data: { usernames: [] } });
  branchStatusMock.mockResolvedValue({
    ok: true,
    data: { status: "up_to_date", ahead_count: 3, behind_count: 0 },
  });
  updateMock.mockResolvedValue({ ok: true, data: PULL });
  suitesListMock.mockResolvedValue({
    ok: true,
    data: {
      suites: [
        { id: "s1", check_runs_count: 2, status: "completed", conclusion: "success" },
        { id: "s2", check_runs_count: 1, status: "in_progress", conclusion: null },
      ],
    },
  });
  runsListForRefMock.mockResolvedValue({
    ok: true,
    data: {
      check_runs: [
        { id: "r1", status: "completed", conclusion: "success" },
        { id: "r2", status: "in_progress", conclusion: null },
      ],
    },
  });
  runsListForSuiteMock.mockResolvedValue({ ok: true, data: { check_runs: [] } });
  runsGetMock.mockResolvedValue({ ok: true, data: null });
  annotationsListMock.mockResolvedValue({ ok: true, data: { annotations: [] } });
  mergeSettingsGetMock.mockResolvedValue({
    ok: true,
    data: {
      allow_merge_commit: true,
      allow_squash_merge: true,
      allow_rebase_merge: true,
    },
  });
  mergeQueueGetMock.mockResolvedValue({ ok: true, data: { required: false } });
  assigneeCandidatesMock.mockResolvedValue({ ok: true, data: { users: [] } });
  labelsForRepoMock.mockResolvedValue({ ok: true, data: { labels: [] } });
});

describe("pullV2TabFromSplat", () => {
  it.each([
    ["", "overview"],
    ["commits", "commits"],
    ["checks", "checks"],
    ["changes", "changes"],
    ["changes/abc123", "changes"],
    ["files", "changes"],
    ["bogus", "overview"],
  ])("maps %s → %s", (splat, expected) => {
    expect(pullV2TabFromSplat(splat)).toBe(expected);
  });
});

describe("PullDetailV2 chrome", () => {
  it("renders the number, title, state, and URL-backed tabs", () => {
    renderV2("overview");
    expect(screen.getByText("#42")).toBeTruthy();
    // Title renders in both the mini-header and the H1 title block (GitHub layout).
    expect(screen.getAllByText("Wire the thing").length).toBeGreaterThan(0);
    expect(screen.getAllByText("Open").length).toBeGreaterThan(0);

    const overview = screen.getByTestId("pull-v2-tab-overview");
    expect(overview.getAttribute("href")).toBe("/ada/hello/pull/42");
    expect(overview.getAttribute("aria-current")).toBe("page");

    expect(screen.getByTestId("pull-v2-tab-commits").getAttribute("href")).toBe(
      "/ada/hello/pull/42/commits",
    );
    expect(screen.getByTestId("pull-v2-tab-checks").getAttribute("href")).toBe(
      "/ada/hello/pull/42/checks",
    );
    expect(screen.getByTestId("pull-v2-tab-changes").getAttribute("href")).toBe(
      "/ada/hello/pull/42/changes",
    );
  });

  it("marks the active tab with aria-current only", () => {
    renderV2("changes");
    expect(screen.getByTestId("pull-v2-tab-changes").getAttribute("aria-current")).toBe("page");
    expect(screen.getByTestId("pull-v2-tab-overview").getAttribute("aria-current")).toBeNull();
  });

  it("shows draft + merged states", () => {
    const draft = { ...PULL, draft: true };
    renderV2("overview", { pull: draft });
    expect(screen.getAllByText("Draft").length).toBeGreaterThan(0);
    cleanup();
    const merged = { ...PULL, state: "merged" };
    renderV2("overview", { pull: merged });
    expect(screen.getAllByText("Merged").length).toBeGreaterThan(0);
  });

  it("renders count pills on Commits / Checks / Files changed", async () => {
    renderV2("overview");
    expect((await screen.findByTestId("pull-v2-tab-count-commits")).textContent).toBe("3");
    expect((await screen.findByTestId("pull-v2-tab-count-checks")).textContent).toBe("3");
    expect((await screen.findByTestId("pull-v2-tab-count-changes")).textContent).toBe("2");
    expect(screen.queryByTestId("pull-v2-tab-count-overview")).toBeNull();
    // Same keys as the tab bodies — one fetch each, shared cache.
    expect(commitsMock).toHaveBeenCalledTimes(1);
    expect(suitesListMock).toHaveBeenCalledTimes(1);
    expect(filesMock).toHaveBeenCalledTimes(1);
    expect(filesMock).toHaveBeenCalledWith({ owner: "ada", name: "hello", number: 42 });
  });

  it("renders base/head branch chips and copies the ref name", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", {
      value: { writeText },
      configurable: true,
    });
    renderV2("overview");
    expect(screen.getByTestId("pull-v2-base-branch").textContent).toContain("main");
    expect(screen.getByTestId("pull-v2-head-branch").textContent).toContain("feature/x");
    fireEvent.click(screen.getByTestId("pull-v2-head-branch-copy"));
    await waitFor(() => {
      expect(writeText).toHaveBeenCalledWith("feature/x");
    });
  });

  it("shows the +additions/−deletions totals from the unscoped files query", async () => {
    renderV2("overview");
    const stat = await screen.findByTestId("pull-v2-diffstat");
    // FILES: 3 adds + 1 del in a.ts (old), 1 add in b.md → +4 / −1.
    expect(stat.textContent).toContain("+4");
    expect(stat.textContent).toContain("−1");
  });

  it("copies the page link and flips to a check", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", {
      value: { writeText },
      configurable: true,
    });
    renderV2("overview");
    fireEvent.click(screen.getByTestId("pull-v2-copy-link"));
    await waitFor(() => {
      expect(writeText).toHaveBeenCalledWith(window.location.href);
    });
  });

  it("shows Submit review when server-pending comments exist", async () => {
    commentsListMock.mockResolvedValue({
      ok: true,
      data: {
        comments: [
          {
            id: "c1",
            pull_id: "p1",
            author_id: "u1",
            author_username: "ada",
            body: "draft note",
            path: "src/a.ts",
            line: 4,
            pending: true,
            resolved: false,
            outdated: false,
            created_at: "2026-03-04T10:00:00Z",
            updated_at: "2026-03-04T10:00:00Z",
          },
        ],
      },
    });
    renderV2("overview");
    const link = await screen.findByTestId("pull-v2-submit-review");
    expect(link.getAttribute("href")).toBe("/ada/hello/pull/42/changes");
  });

  it("reflects merge state in the header chip", async () => {
    runsListForRefMock.mockResolvedValue({
      ok: true,
      data: { check_runs: [{ id: "r1", status: "completed", conclusion: "failure" }] },
    });
    renderV2("overview");
    const chip = await screen.findByTestId("pull-v2-merge-state");
    expect(chip.textContent).toContain("Checks failing");
  });

  it("opens the comments panel via the toggle and ?panel=comments", async () => {
    commentsListMock.mockResolvedValue({
      ok: true,
      data: {
        comments: [
          {
            id: "c1",
            pull_id: "p1",
            author_id: "u2",
            author_username: "grace",
            body: "looks good",
            pending: false,
            resolved: false,
            outdated: false,
            created_at: "2026-03-04T10:00:00Z",
            updated_at: "2026-03-04T10:00:00Z",
          },
        ],
      },
    });
    renderV2("overview");
    expect(screen.queryByTestId("pull-v2-comments-panel")).toBeNull();
    fireEvent.click(screen.getByTestId("pull-v2-panel-toggle"));
    expect(await screen.findByTestId("pull-v2-comments-panel")).toBeTruthy();
    expect(window.location.search).toContain("panel=comments");
    fireEvent.click(screen.getByTestId("pull-v2-comments-close"));
    await waitFor(() => {
      expect(screen.queryByTestId("pull-v2-comments-panel")).toBeNull();
    });
    expect(window.location.search).not.toContain("panel=comments");
  });

  it("opens the comments panel on load when ?panel=comments is in the URL", async () => {
    window.history.pushState(null, "", "/ada/hello/pull/42?panel=comments");
    renderV2("overview");
    expect(await screen.findByTestId("pull-v2-comments-panel")).toBeTruthy();
  });

  it("keeps the details rail on overview and opens the Description pane via the rail toggle", async () => {
    renderV2("overview");
    expect(screen.getByTestId("pull-v2-details-rail")).toBeTruthy();
    expect(screen.queryByTestId("pull-v2-pane")).toBeNull();

    fireEvent.click(screen.getByTestId("pull-v2-rail-toggle"));
    expect(await screen.findByTestId("pull-v2-pane")).toBeTruthy();
    expect(await screen.findByTestId("pull-v2-description-panel")).toBeTruthy();
    expect(window.location.search).toContain("panel=overview");
  });

  it("opens the Description pane on load when ?panel=overview is in the URL", async () => {
    window.history.pushState(null, "", "/ada/hello/pull/42?panel=overview");
    renderV2("overview", { pull: { ...PULL, body: "PR body markdown" } });
    const panel = await screen.findByTestId("pull-v2-description-panel");
    await waitFor(() => {
      expect(panel.textContent).toContain("PR body markdown");
    });
    expect(screen.getByTestId("pull-v2-details-rail")).toBeTruthy();
  });

  it("edits the title inline via pull.update and invalidates pull.get", async () => {
    renderV2("overview");
    fireEvent.click(screen.getByTestId("pull-v2-title-edit"));
    const input = await screen.findByTestId("pull-v2-title-input");
    fireEvent.input(input, { target: { value: "Renamed title" } });
    fireEvent.click(screen.getByTestId("pull-v2-title-save"));
    await waitFor(() => {
      expect(updateMock).toHaveBeenCalledWith({
        owner: "ada",
        name: "hello",
        number: 42,
        title: "Renamed title",
      });
    });
    await waitFor(() => {
      expect(screen.queryByTestId("pull-v2-title-input")).toBeNull();
    });
  });

  it("cancels title editing without calling pull.update", async () => {
    renderV2("overview");
    fireEvent.click(screen.getByTestId("pull-v2-title-edit"));
    fireEvent.input(await screen.findByTestId("pull-v2-title-input"), {
      target: { value: "Scratch" },
    });
    fireEvent.click(screen.getByTestId("pull-v2-title-cancel"));
    expect(screen.queryByTestId("pull-v2-title-input")).toBeNull();
    expect(updateMock).not.toHaveBeenCalled();
  });

  it("hides the title pencil for closed pulls or non-author read-only viewers", () => {
    renderV2("overview", { canWrite: false, isAuthor: false });
    expect(screen.queryByTestId("pull-v2-title-edit")).toBeNull();
    cleanup();
    // Authors keep edit rights on their own PR (can_edit_pull).
    renderV2("overview", { canWrite: false, isAuthor: true });
    expect(screen.queryByTestId("pull-v2-title-edit")).not.toBeNull();
    cleanup();
    renderV2("overview", { pull: { ...PULL, state: "closed" } });
    expect(screen.queryByTestId("pull-v2-title-edit")).toBeNull();
  });
});
