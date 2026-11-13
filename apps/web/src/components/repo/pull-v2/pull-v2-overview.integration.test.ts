/**
 * pull-v2 overview (WI-0001, #263 GitHub parity): comment segmentation
 * (All / Unresolved / Mine) + oldest↔newest sort + count, the markdown
 * composer toolbar, the pending-drafts banner, and comment mutations —
 * all against a mocked api-client.
 */
import { cleanup, fireEvent, screen, waitFor } from "@octanejs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderWithQueryClient } from "@/test/render-with-query";

const commentsListMock = vi.fn();
const commentsCreateMock = vi.fn();
const commentsDeleteMock = vi.fn();
const commentsUpdateMock = vi.fn();
const commentsResolveMock = vi.fn();
const commentsHistoryMock = vi.fn();
const reviewsListMock = vi.fn();
const reviewRequestsListMock = vi.fn();
const commitsMock = vi.fn();
const branchStatusMock = vi.fn();
const runsListForRefMock = vi.fn();
const mergeSettingsGetMock = vi.fn();
const mergeQueueGetMock = vi.fn();
const assigneeCandidatesMock = vi.fn();
const labelsForRepoMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    pull: {
      commits: (...args: unknown[]) => commitsMock(...args),
      comments: {
        list: (...args: unknown[]) => commentsListMock(...args),
        create: (...args: unknown[]) => commentsCreateMock(...args),
        delete: (...args: unknown[]) => commentsDeleteMock(...args),
        update: (...args: unknown[]) => commentsUpdateMock(...args),
        resolve: (...args: unknown[]) => commentsResolveMock(...args),
        history: (...args: unknown[]) => commentsHistoryMock(...args),
      },
      reviews: {
        list: (...args: unknown[]) => reviewsListMock(...args),
      },
      reviewRequests: {
        list: (...args: unknown[]) => reviewRequestsListMock(...args),
      },
      branchStatus: (...args: unknown[]) => branchStatusMock(...args),
    },
    check: {
      runs: {
        listForRef: (...args: unknown[]) => runsListForRefMock(...args),
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

import { PullV2Overview } from "./pull-v2-overview";

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
  body: "the description",
};

const REPO = { name: "hello", can_write: true, archived: false };

const C1 = {
  id: "c1",
  pull_id: "p1",
  author_id: "u2",
  author_username: "grace",
  body: "first posted comment",
  pending: false,
  resolved: false,
  outdated: false,
  created_at: "2026-03-04T10:00:00Z",
  updated_at: "2026-03-04T10:00:00Z",
};

const C2_UNRESOLVED = {
  id: "c2",
  pull_id: "p1",
  author_id: "u3",
  author_username: "linus",
  body: "second on a file",
  path: "src/a.ts",
  line: 12,
  pending: false,
  resolved: false,
  outdated: false,
  created_at: "2026-03-05T10:00:00Z",
  updated_at: "2026-03-05T10:00:00Z",
};

const C3_MINE_RESOLVED = {
  id: "c3",
  pull_id: "p1",
  author_id: "u9",
  author_username: "me",
  body: "mine resolved",
  path: "src/a.ts",
  line: 40,
  pending: false,
  resolved: true,
  outdated: false,
  created_at: "2026-03-06T10:00:00Z",
  updated_at: "2026-03-06T10:00:00Z",
};

const C4_PENDING = {
  id: "c4",
  pull_id: "p1",
  author_id: "u9",
  author_username: "me",
  body: "pending draft",
  path: "src/a.ts",
  line: 8,
  pending: true,
  resolved: false,
  outdated: false,
  created_at: "2026-03-07T10:00:00Z",
  updated_at: "2026-03-07T10:00:00Z",
};

function renderOverview(comments: unknown[] = [], extra: Record<string, unknown> = {}) {
  commentsListMock.mockResolvedValue({ ok: true, data: { comments } });
  return renderWithQueryClient(PullV2Overview, {
    props: {
      owner: "ada",
      repoName: "hello",
      number: 42,
      repo: REPO as never,
      pull: PULL as never,
      canWrite: true,
      canInteract: true,
      isAuthor: true,
      currentUserId: "u9",
      ...extra,
    },
  });
}

afterEach(cleanup);

beforeEach(() => {
  for (const m of [
    commentsListMock,
    commentsCreateMock,
    commentsDeleteMock,
    commentsUpdateMock,
    commentsResolveMock,
    commentsHistoryMock,
    reviewsListMock,
    reviewRequestsListMock,
    commitsMock,
    branchStatusMock,
    runsListForRefMock,
    mergeSettingsGetMock,
    mergeQueueGetMock,
    assigneeCandidatesMock,
    labelsForRepoMock,
  ]) {
    m.mockReset();
  }
  commentsListMock.mockResolvedValue({ ok: true, data: { comments: [] } });
  commentsHistoryMock.mockResolvedValue({ ok: true, data: { revisions: [] } });
  reviewsListMock.mockResolvedValue({ ok: true, data: { reviews: [] } });
  reviewRequestsListMock.mockResolvedValue({ ok: true, data: { usernames: [] } });
  commitsMock.mockResolvedValue({ ok: true, data: { commits: [] } });
  branchStatusMock.mockResolvedValue({
    ok: true,
    data: { status: "up_to_date", ahead_count: 1, behind_count: 0 },
  });
  runsListForRefMock.mockResolvedValue({ ok: true, data: { check_runs: [] } });
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

describe("PullV2Overview conversation", () => {
  it("segments comments All / Unresolved / Mine with a live count", async () => {
    renderOverview([C1, C2_UNRESOLVED, C3_MINE_RESOLVED]);
    expect(await screen.findByTestId("pull-v2-conversation")).toBeTruthy();
    await waitFor(() => {
      expect(screen.getByTestId("pull-v2-comments-count").textContent).toContain("3 comments");
    });
    expect(screen.getAllByTestId("pull-comment").length).toBe(3);

    fireEvent.click(screen.getByTestId("pull-v2-overview-filter-unresolved"));
    await waitFor(() => {
      expect(screen.getAllByTestId("pull-comment").length).toBe(1);
    });
    expect(screen.getByTestId("pull-v2-comments-count").textContent).toContain("1 comment");
    // Unresolved keeps only path comments with resolved === false.
    expect(screen.getByText("src/a.ts:12")).toBeTruthy();

    fireEvent.click(screen.getByTestId("pull-v2-overview-filter-mine"));
    await waitFor(() => {
      expect(screen.getAllByTestId("pull-comment").length).toBe(1);
    });
    expect(screen.getByText("@me")).toBeTruthy();

    fireEvent.click(screen.getByTestId("pull-v2-overview-filter-all"));
    await waitFor(() => {
      expect(screen.getAllByTestId("pull-comment").length).toBe(3);
    });
  });

  it("sorts oldest/newest over the visible comments", async () => {
    renderOverview([C3_MINE_RESOLVED, C1, C2_UNRESOLVED]);
    await waitFor(() => {
      expect(screen.getAllByTestId("pull-comment").length).toBe(3);
    });
    // Default: oldest first — c1 (Mar 4) leads.
    let items = screen.getAllByTestId("pull-comment");
    expect(items[0]!.textContent).toContain("@grace");

    const sort = screen.getByTestId("pull-v2-comments-sort");
    fireEvent.change(sort, { target: { value: "newest" } });
    await waitFor(() => {
      items = screen.getAllByTestId("pull-comment");
      expect(items[0]!.textContent).toContain("@me");
    });
  });

  it("keeps pending drafts out of the list and shows the banner", async () => {
    renderOverview([C1, C4_PENDING]);
    await waitFor(() => {
      expect(screen.getAllByTestId("pull-comment").length).toBe(1);
    });
    const banner = screen.getByTestId("pull-v2-pending-banner");
    expect(banner.textContent).toContain("1 pending review comment");
    expect(banner.querySelector("a")!.getAttribute("href")).toBe("/ada/hello/pull/42/changes");
    expect(screen.queryByText("pending draft")).toBeNull();
  });

  it("wraps the textarea selection in markdown for each toolbar button", async () => {
    renderOverview([]);
    const ta = (await screen.findByTestId("pull-v2-comment-draft")) as HTMLTextAreaElement;
    fireEvent.input(ta, { target: { value: "hello" } });
    ta.setSelectionRange(0, 5);
    fireEvent.click(screen.getByTestId("pull-v2-md-bold"));
    expect(ta.value).toBe("**hello**");

    fireEvent.input(ta, { target: { value: "" } });
    fireEvent.click(screen.getByTestId("pull-v2-md-italic"));
    expect(ta.value).toBe("_italic text_");

    fireEvent.input(ta, { target: { value: "some words" } });
    ta.setSelectionRange(0, 10);
    fireEvent.click(screen.getByTestId("pull-v2-md-strike"));
    expect(ta.value).toBe("~~some words~~");

    fireEvent.input(ta, { target: { value: "x = 1" } });
    ta.setSelectionRange(0, 5);
    fireEvent.click(screen.getByTestId("pull-v2-md-code"));
    expect(ta.value).toBe("`x = 1`");

    fireEvent.input(ta, { target: { value: "quoted\nlines" } });
    ta.setSelectionRange(0, 12);
    fireEvent.click(screen.getByTestId("pull-v2-md-quote"));
    expect(ta.value).toBe("> quoted\n> lines");

    fireEvent.input(ta, { target: { value: "docs" } });
    ta.setSelectionRange(0, 4);
    fireEvent.click(screen.getByTestId("pull-v2-md-link"));
    expect(ta.value).toBe("[docs](url)");

    fireEvent.input(ta, { target: { value: "one\ntwo" } });
    ta.setSelectionRange(0, 7);
    fireEvent.click(screen.getByTestId("pull-v2-md-list"));
    expect(ta.value).toBe("- one\n- two");
  });

  it("posts a comment via pull.comments.create and patches the list", async () => {
    commentsCreateMock.mockResolvedValue({
      ok: true,
      data: { ...C1, id: "c9", body: "ship it" },
    });
    renderOverview([C1]);
    const ta = (await screen.findByTestId("pull-v2-comment-draft")) as HTMLTextAreaElement;
    fireEvent.input(ta, { target: { value: "ship it" } });
    fireEvent.click(screen.getByTestId("pull-v2-comment-submit"));
    await waitFor(() => {
      expect(commentsCreateMock).toHaveBeenCalledWith({
        owner: "ada",
        name: "hello",
        number: 42,
        body: "ship it",
      });
    });
    await waitFor(() => {
      expect(ta.value).toBe("");
    });
    await waitFor(() => {
      expect(screen.getByTestId("pull-v2-comments-count").textContent).toContain("2 comments");
    });
  });

  it("edits and deletes a comment through the item controls", async () => {
    commentsUpdateMock.mockResolvedValue({
      ok: true,
      data: { ...C3_MINE_RESOLVED, body: "edited body" },
    });
    commentsDeleteMock.mockResolvedValue({ ok: true, data: { ok: true } });
    renderOverview([C3_MINE_RESOLVED]);
    await waitFor(() => {
      expect(screen.getAllByTestId("pull-comment").length).toBe(1);
    });

    fireEvent.click(screen.getByText("Edit"));
    fireEvent.click(screen.getByTestId("pull-comment-edit-cancel-c3"));
    fireEvent.click(screen.getByText("Delete"));
    await waitFor(() => {
      expect(commentsDeleteMock).toHaveBeenCalledWith({
        owner: "ada",
        name: "hello",
        number: 42,
        commentId: "c3",
      });
    });
    await waitFor(() => {
      expect(screen.queryAllByTestId("pull-comment").length).toBe(0);
    });
  });

  it("renders compact readiness signals above the summary", async () => {
    runsListForRefMock.mockResolvedValue({
      ok: true,
      data: {
        check_runs: [
          { id: "r1", status: "in_progress", conclusion: null },
          { id: "r2", status: "queued", conclusion: null },
          { id: "r3", status: "completed", conclusion: "success" },
        ],
      },
    });
    reviewsListMock.mockResolvedValue({
      ok: true,
      data: {
        reviews: [
          {
            id: "rev1",
            pull_id: "p1",
            author_id: "u2",
            author_username: "grace",
            state: "approved",
            body: "",
            submitted_at: "2026-03-05T10:00:00Z",
          },
        ],
      },
    });
    renderOverview([C2_UNRESOLVED, C3_MINE_RESOLVED]);

    const signals = await screen.findByTestId("pull-v2-merge-signals");
    const checks = screen.getByTestId("pull-v2-checks-row");
    const comments = screen.getByTestId("pull-v2-comments-row");
    const approvals = screen.getByTestId("pull-v2-approvals-row");
    const summary = await screen.findByTestId("pull-v2-summary");

    expect(signals.className).toContain("grid-cols-1");
    expect(signals.className).toContain("divide-y");
    expect(signals.className).not.toContain("sm:grid-cols-3");
    expect(signals.contains(checks)).toBe(true);
    expect(signals.contains(comments)).toBe(true);
    expect(signals.contains(approvals)).toBe(true);
    expect(
      signals.compareDocumentPosition(summary) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    expect(checks.tagName).toBe("A");
    expect(checks.getAttribute("href")).toBe("/ada/hello/pull/42/checks");

    await waitFor(() => {
      expect(checks.textContent).toContain("2 pending");
      expect(comments.textContent).toContain("1 unresolved");
      expect(approvals.textContent).toContain("1 approval");
    });
    expect(screen.getByTestId("pull-v2-checks-bar")).toBeTruthy();
    expect(screen.getByTestId("pull-v2-comments-bar")).toBeTruthy();
    expect(screen.getByTestId("pull-v2-approvals-bar")).toBeTruthy();
  });

  it("surfaces requested reviewers in approvals", async () => {
    reviewRequestsListMock.mockResolvedValue({
      ok: true,
      data: { usernames: ["linus", "grace"] },
    });
    renderOverview([]);
    await waitFor(() => {
      expect(screen.getByTestId("pull-v2-approvals-row").textContent).toContain("2 requested");
    });
  });

  it("surfaces changes-requested reviews in approvals", async () => {
    reviewsListMock.mockResolvedValue({
      ok: true,
      data: {
        reviews: [
          {
            id: "rev2",
            pull_id: "p1",
            author_id: "u3",
            author_username: "linus",
            state: "changes_requested",
            body: "needs work",
            submitted_at: "2026-03-06T10:00:00Z",
          },
        ],
      },
    });
    renderOverview([]);
    await waitFor(() => {
      expect(screen.getByTestId("pull-v2-approvals-row").textContent).toContain(
        "Changes requested",
      );
    });
  });

  it("counts only each reviewer's latest review state", async () => {
    reviewsListMock.mockResolvedValue({
      ok: true,
      data: {
        reviews: [
          {
            id: "rev-old",
            pull_id: "p1",
            author_id: "u3",
            author_username: "linus",
            state: "changes_requested",
            body: "needs work",
            submitted_at: "2026-03-01T10:00:00Z",
          },
          {
            id: "rev-new",
            pull_id: "p1",
            author_id: "u3",
            author_username: "linus",
            state: "approved",
            body: "",
            submitted_at: "2026-03-06T10:00:00Z",
          },
          {
            id: "rev-other",
            pull_id: "p1",
            author_id: "u2",
            author_username: "grace",
            state: "approved",
            body: "",
            submitted_at: "2026-03-05T10:00:00Z",
          },
        ],
      },
    });
    renderOverview([]);
    await waitFor(() => {
      const approvals = screen.getByTestId("pull-v2-approvals-row").textContent;
      expect(approvals).toContain("2 approvals");
      expect(approvals).not.toContain("Changes requested");
    });
  });
});
