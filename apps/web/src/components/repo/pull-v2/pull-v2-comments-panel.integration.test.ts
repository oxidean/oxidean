/**
 * pull-v2 comments panel (#263): server-persisted pending drafts group under
 * a "Pending" header with a badge + discard affordance (pull.comments.delete),
 * while posted threads keep the existing All / Unresolved / Mine filtering.
 */
import { cleanup, fireEvent, screen, waitFor } from "@octanejs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderWithQueryClient } from "@/test/render-with-query";

const commentsDeleteMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    pull: {
      comments: {
        delete: (...args: unknown[]) => commentsDeleteMock(...args),
      },
    },
  },
}));

import { PullV2CommentsPanel } from "./pull-v2-comments-panel";

const POSTED = {
  id: "c1",
  pull_id: "p1",
  author_id: "u2",
  author_username: "grace",
  body: "looks good",
  path: "src/a.ts",
  line: 12,
  pending: false,
  resolved: false,
  outdated: false,
  created_at: "2026-03-04T10:00:00Z",
  updated_at: "2026-03-04T10:00:00Z",
};

const PENDING = {
  id: "c2",
  pull_id: "p1",
  author_id: "u9",
  author_username: "me",
  body: "pending draft note",
  path: "src/b.ts",
  line: 3,
  pending: true,
  resolved: false,
  outdated: false,
  created_at: "2026-03-05T10:00:00Z",
  updated_at: "2026-03-05T10:00:00Z",
};

function renderPanel(overrides: Record<string, unknown> = {}) {
  return renderWithQueryClient(PullV2CommentsPanel, {
    props: {
      comments: [POSTED, PENDING] as never,
      currentUserId: "u9",
      owner: "ada",
      repoName: "hello",
      number: 42,
      canInteract: true,
      onClose: vi.fn(),
      onJump: vi.fn(),
      ...overrides,
    },
  });
}

afterEach(cleanup);

beforeEach(() => {
  commentsDeleteMock.mockReset();
  commentsDeleteMock.mockResolvedValue({ ok: true, data: { ok: true } });
});

describe("PullV2CommentsPanel", () => {
  it("groups server-pending drafts under a Pending header with badges", async () => {
    renderPanel();
    const group = await screen.findByTestId("pull-v2-pending-group");
    expect(group.textContent).toContain("Pending (1)");
    expect(screen.getByText("Pending")).toBeTruthy();
    // Both rows render — the pending draft keeps its path:line jump link.
    expect(screen.getAllByTestId("pull-v2-panel-comment").length).toBe(2);
    expect(screen.getByText("src/b.ts:3")).toBeTruthy();
    expect(screen.getByText("src/a.ts:12")).toBeTruthy();
  });

  it("discards a pending draft via pull.comments.delete", async () => {
    renderPanel();
    fireEvent.click(await screen.findByTestId("pull-v2-panel-discard"));
    await waitFor(() => {
      expect(commentsDeleteMock).toHaveBeenCalledWith({
        owner: "ada",
        name: "hello",
        number: 42,
        commentId: "c2",
      });
    });
  });

  it("surfaces a delete failure without removing the row", async () => {
    commentsDeleteMock.mockResolvedValue({
      ok: false,
      error: { code: "pull.comments.forbidden", message: "nope" },
    });
    renderPanel();
    fireEvent.click(await screen.findByTestId("pull-v2-panel-discard"));
    const err = await screen.findByTestId("pull-v2-panel-error");
    expect(err.textContent).toContain("nope");
  });

  it("hides discard controls without repo coordinates or interaction rights", async () => {
    renderPanel({ canInteract: false });
    await screen.findByTestId("pull-v2-pending-group");
    expect(screen.queryByTestId("pull-v2-panel-discard")).toBeNull();
    cleanup();
    renderPanel({ owner: undefined, repoName: undefined, number: undefined });
    await screen.findByTestId("pull-v2-pending-group");
    expect(screen.queryByTestId("pull-v2-panel-discard")).toBeNull();
  });

  it("still filters posted comments All / Unresolved / Mine", async () => {
    renderPanel();
    await screen.findAllByTestId("pull-v2-panel-comment");
    // "mine" → only the pending draft is mine; pending rows stay grouped.
    fireEvent.click(screen.getByTestId("pull-v2-comments-filter-mine"));
    await waitFor(() => {
      expect(screen.getAllByTestId("pull-v2-panel-comment").length).toBe(1);
    });
    expect(screen.getByText("src/b.ts:3")).toBeTruthy();
    // "unresolved" → path comments with resolved === false (posted + pending).
    fireEvent.click(screen.getByTestId("pull-v2-comments-filter-unresolved"));
    await waitFor(() => {
      expect(screen.getAllByTestId("pull-v2-panel-comment").length).toBe(2);
    });
  });

  it("renders file-level comments as bare paths without a line suffix", async () => {
    renderPanel({
      comments: [
        {
          ...POSTED,
          id: "c3",
          line: null,
        },
      ] as never,
    });
    await screen.findByTestId("pull-v2-comments-panel");
    expect(screen.getByText("src/a.ts")).toBeTruthy();
    expect(screen.queryByText("src/a.ts:")).toBeNull();
  });
});
