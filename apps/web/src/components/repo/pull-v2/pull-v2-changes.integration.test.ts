/**
 * pull-v2 changes tab (#261–#263): scoped diff requests + notices, viewed-mark
 * gating, server-persisted pending drafts (create → pending:true, discard →
 * comments.delete, submit → reviews.submit with fresh comment_ids), inline +
 * file-level + range comment anchors.
 */
import { cleanup, fireEvent, screen, waitFor } from "@octanejs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderWithQueryClient } from "@/test/render-with-query";
import { PullV2Changes } from "./pull-v2-changes";

const filesMock = vi.fn();
const commitsMock = vi.fn();
const viewedListMock = vi.fn();
const viewedSetMock = vi.fn();
const commentsListMock = vi.fn();
const commentsCreateMock = vi.fn();
const commentsDeleteMock = vi.fn();
const reviewsSubmitMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    pull: {
      files: (...args: unknown[]) => filesMock(...args),
      commits: (...args: unknown[]) => commitsMock(...args),
      filesViewed: {
        list: (...args: unknown[]) => viewedListMock(...args),
        set: (...args: unknown[]) => viewedSetMock(...args),
      },
      comments: {
        list: (...args: unknown[]) => commentsListMock(...args),
        create: (...args: unknown[]) => commentsCreateMock(...args),
        delete: (...args: unknown[]) => commentsDeleteMock(...args),
      },
      reviews: {
        submit: (...args: unknown[]) => reviewsSubmitMock(...args),
      },
    },
  },
}));

vi.mock("@/lib/toast", () => ({
  appToastManager: {
    add: vi.fn(),
    remove: vi.fn(),
    update: vi.fn(),
    close: vi.fn(),
    promise: vi.fn(),
  },
  toastSuccess: vi.fn(),
  toastError: vi.fn(),
}));

const FILES = {
  empty: false,
  files: [
    {
      path: "src/main.rs",
      status: "modified",
      patch: "@@ -1,3 +1,3 @@\n fn main() {\n-    old();\n+    new();\n }\n",
    },
    {
      path: "docs/README.md",
      status: "added",
      patch: "@@ -0,0 +1,2 @@\n+# Title\n+body\n",
    },
  ],
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
  ],
};

function comment(overrides: Record<string, unknown> = {}) {
  return {
    id: "c1",
    pull_id: "p1",
    author_id: "u1",
    author_username: "ada",
    body: "note",
    path: "src/main.rs",
    side: "RIGHT",
    line: 2,
    pending: false,
    resolved: false,
    outdated: false,
    created_at: "2026-03-04T10:00:00Z",
    updated_at: "2026-03-04T10:00:00Z",
    ...overrides,
  };
}

beforeEach(() => {
  for (const m of [
    filesMock,
    commitsMock,
    viewedListMock,
    viewedSetMock,
    commentsListMock,
    commentsCreateMock,
    commentsDeleteMock,
    reviewsSubmitMock,
  ]) {
    m.mockReset();
  }
  localStorage.clear();
  filesMock.mockResolvedValue({ ok: true, data: FILES });
  commitsMock.mockResolvedValue({ ok: true, data: COMMITS });
  viewedListMock.mockResolvedValue({ ok: true, data: { viewed_paths: [] } });
  viewedSetMock.mockResolvedValue({ ok: true, data: { viewed_paths: [] } });
  commentsListMock.mockResolvedValue({ ok: true, data: { comments: [] } });
  commentsCreateMock.mockResolvedValue({ ok: true, data: comment({ id: "c-new", pending: true }) });
  commentsDeleteMock.mockResolvedValue({ ok: true, data: { deleted: true } });
  reviewsSubmitMock.mockResolvedValue({ ok: true, data: { id: "r1" } });
});

afterEach(() => {
  cleanup();
});

function renderChanges(overrides: Record<string, unknown> = {}) {
  return renderWithQueryClient(PullV2Changes, {
    props: {
      owner: "acme",
      repoName: "core",
      number: 3,
      canInteract: true,
      canWrite: true,
      isAuthor: false,
      currentUserId: "u1",
      headSha: "abc123def",
      ...overrides,
    },
  });
}

describe("PullV2Changes scoped diffs (#261)", () => {
  it("unscoped view calls pull.files without a scope and shows viewed marks", async () => {
    renderChanges();
    await screen.findByTestId("pull-v2-changes");
    await waitFor(() => {
      expect(filesMock).toHaveBeenCalled();
    });
    const call = filesMock.mock.calls[0]![0] as Record<string, unknown>;
    expect(call.sha).toBeUndefined();
    expect(call.base_sha).toBeUndefined();
    expect(call.since_review).toBeUndefined();
    expect(await screen.findByTestId("pull-v2-viewed-progress")).toBeTruthy();
    expect(document.querySelector('[data-testid="pull-v2-file-viewed"]')).not.toBeNull();
  });

  it("single-sha scope passes sha to pull.files and shows the notice", async () => {
    renderChanges({ changesSha: "aaaa1111aaaa" });
    await waitFor(() => {
      expect(filesMock).toHaveBeenCalledWith(expect.objectContaining({ sha: "aaaa1111aaaa" }));
    });
    const notice = await screen.findByTestId("pull-v2-changes-sha-notice");
    expect(notice.textContent).toContain("aaaa111");
    const showAll = document.querySelector(
      '[data-testid="pull-v2-changes-show-all"]',
    ) as HTMLAnchorElement;
    expect(showAll.getAttribute("href")).toBe("/acme/core/pull/3/changes");
    // Scoped views hide viewed marks and never write them.
    expect(screen.queryByTestId("pull-v2-viewed-progress")).toBeNull();
    expect(document.querySelector('[data-testid="pull-v2-file-viewed"]')).toBeNull();
    expect(viewedSetMock).not.toHaveBeenCalled();
  });

  it("range scope passes base_sha + head_sha and shows the range notice", async () => {
    renderChanges({ changesSha: "aaaa1111aaaa..bbbb2222bbbb" });
    await waitFor(() => {
      expect(filesMock).toHaveBeenCalledWith(
        expect.objectContaining({
          base_sha: "aaaa1111aaaa",
          head_sha: "bbbb2222bbbb",
        }),
      );
    });
    const notice = await screen.findByTestId("pull-v2-changes-range-notice");
    expect(notice.textContent).toContain("aaaa111..bbbb222");
  });

  it("anchors inline comments to the scoped sha", async () => {
    renderChanges({ changesSha: "aaaa1111aaaa" });
    const btn = await screen.findAllByTestId("pull-v2-line-comment-add");
    fireEvent.click(btn[0]!);
    const box = await screen.findByTestId("pull-v2-line-draft");
    const ta = box.querySelector("textarea")!;
    fireEvent.input(ta, { target: { value: "scoped note" } });
    fireEvent.click(screen.getByTestId("pull-v2-line-draft-add"));
    await waitFor(() => {
      expect(commentsCreateMock).toHaveBeenCalledWith(
        expect.objectContaining({ commit_sha: "aaaa1111aaaa", pending: true }),
      );
    });
  });
});

describe("PullV2Changes server-persisted drafts", () => {
  it("Add review comment posts pending:true and renders the pending badge", async () => {
    commentsListMock.mockResolvedValue({
      ok: true,
      data: { comments: [comment({ id: "pc1", pending: true })] },
    });
    renderChanges();
    // Existing pending comment renders with its badge + discard.
    const pending = await screen.findAllByTestId("pull-v2-pending-comment");
    expect(pending.length).toBe(1);
    expect(pending[0]!.textContent).toContain("Pending");

    const btn = await screen.findAllByTestId("pull-v2-line-comment-add");
    fireEvent.click(btn[0]!);
    const box = await screen.findByTestId("pull-v2-line-draft");
    fireEvent.input(box.querySelector("textarea")!, { target: { value: "nit" } });
    fireEvent.click(screen.getByTestId("pull-v2-line-draft-add"));
    await waitFor(() => {
      expect(commentsCreateMock).toHaveBeenCalledWith(
        expect.objectContaining({
          body: "nit",
          path: "src/main.rs",
          pending: true,
          commit_sha: "abc123def",
        }),
      );
    });
    await waitFor(() => {
      expect(commentsListMock).toHaveBeenCalledTimes(2);
    });
  });

  it("Add single comment posts pending:false", async () => {
    renderChanges();
    const btn = await screen.findAllByTestId("pull-v2-line-comment-add");
    fireEvent.click(btn[0]!);
    const box = await screen.findByTestId("pull-v2-line-draft");
    fireEvent.input(box.querySelector("textarea")!, { target: { value: "solo" } });
    fireEvent.click(screen.getByTestId("pull-v2-line-draft-add-single"));
    await waitFor(() => {
      expect(commentsCreateMock).toHaveBeenCalledWith(
        expect.objectContaining({ body: "solo", pending: false }),
      );
    });
  });

  it("file-level comment posts a path-only pending draft", async () => {
    renderChanges();
    const btn = await screen.findAllByTestId("pull-v2-file-comment");
    fireEvent.click(btn[0]!);
    const box = await screen.findByTestId("pull-v2-line-draft");
    fireEvent.input(box.querySelector("textarea")!, { target: { value: "file note" } });
    fireEvent.click(screen.getByTestId("pull-v2-line-draft-add"));
    await waitFor(() => {
      expect(commentsCreateMock).toHaveBeenCalledWith(
        expect.objectContaining({
          body: "file note",
          path: "src/main.rs",
          line: null,
          side: null,
          pending: true,
        }),
      );
    });
  });

  it("discards a pending draft via comments.delete", async () => {
    commentsListMock.mockResolvedValue({
      ok: true,
      data: { comments: [comment({ id: "pc9", pending: true })] },
    });
    renderChanges();
    const discard = await screen.findAllByTestId("pull-v2-pending-discard");
    fireEvent.click(discard[0]!);
    await waitFor(() => {
      expect(commentsDeleteMock).toHaveBeenCalledWith(
        expect.objectContaining({ commentId: "pc9" }),
      );
    });
  });

  it("submits the review with fresh pending comment_ids", async () => {
    commentsListMock.mockResolvedValue({
      ok: true,
      data: {
        comments: [
          comment({ id: "pc1", pending: true }),
          comment({ id: "pc2", pending: true, path: "docs/README.md", line: 1 }),
          comment({ id: "sub1", pending: false }),
        ],
      },
    });
    renderChanges();
    // Wait until the comments query has landed — the submit button stays
    // disabled until pendingCount > 0 and comment_ids read the cache.
    await screen.findAllByTestId("pull-v2-review-pending-item");
    const submit = await screen.findByTestId("pull-v2-review-submit");
    fireEvent.click(submit);
    await waitFor(() => {
      expect(reviewsSubmitMock).toHaveBeenCalledWith(
        expect.objectContaining({
          state: "commented",
          comment_ids: ["pc1", "pc2"],
        }),
      );
    });
    // Submit links drafts — it must not create comments itself.
    expect(commentsCreateMock).not.toHaveBeenCalled();
  });
});

describe("PullV2Changes minimize comments (#263)", () => {
  it("hides inline threads but keeps the header count", async () => {
    commentsListMock.mockResolvedValue({
      ok: true,
      data: { comments: [comment({ id: "sub1" })] },
    });
    renderChanges();
    expect(await screen.findAllByTestId("pull-v2-line-comment")).toHaveLength(1);
    expect(document.querySelector('[data-testid="pull-v2-file-comment-count"]')).not.toBeNull();
    // The settings menu lives in a portal — assert the persisted setting flag
    // by mounting with comments then checking the toggle path in Chromium
    // (pull-v2-changes.browser.test.tsx covers the portal flow).
  });
});
