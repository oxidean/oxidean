/**
 * Chromium proof for the pull-v2 detail chrome — the comments-panel toggle
 * mounts/unmounts a large sibling subtree beside the tab body, and the inline
 * title edit swaps the header row; neither may race Octane reconciliation.
 */
import { beforeEach, describe, expect, it, vi } from "vitest";
import { PullDetailV2 } from "@/components/repo/pull-v2/pull-v2";
import {
  clickTestId,
  debugBody,
  expectNoOctaneOverlayInDocument,
  mountWithQueryClient,
} from "@/test/browser-mount";
import { trackDomErrors } from "@/test/dom-errors";
import { act } from "octane";

const commitsMock = vi.fn();
const filesMock = vi.fn();
const filesListViewedMock = vi.fn();
const commentsListMock = vi.fn();
const commentsDeleteMock = vi.fn();
const reviewsListMock = vi.fn();
const branchStatusMock = vi.fn();
const updateMock = vi.fn();
const reviewRequestsListMock = vi.fn();
const suitesListMock = vi.fn();
const runsListForRefMock = vi.fn();
const runsListForSuiteMock = vi.fn();
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
      },
      comments: {
        list: (...args: unknown[]) => commentsListMock(...args),
        delete: (...args: unknown[]) => commentsDeleteMock(...args),
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

async function settle(ms = 250) {
  await act(async () => {
    await new Promise((r) => setTimeout(r, ms));
  });
}

beforeEach(() => {
  commitsMock.mockResolvedValue({
    ok: true,
    data: {
      commits: [
        {
          sha: "a1",
          subject: "wire the thing",
          author_username: "ada",
          author_name: "Ada",
          committed_at: "2026-03-04T10:00:00Z",
        },
      ],
    },
  });
  filesMock.mockResolvedValue({
    ok: true,
    data: { files: [{ path: "src/a.ts", status: "modified", patch: "@@ -1 +1 @@\n-x\n+y" }] },
  });
  filesListViewedMock.mockResolvedValue({ ok: true, data: { viewed_paths: [] } });
  commentsListMock.mockResolvedValue({
    ok: true,
    data: {
      comments: [
        {
          id: "c1",
          pull_id: "p1",
          author_id: "u1",
          author_username: "ada",
          body: "draft",
          path: "src/a.ts",
          line: 1,
          pending: true,
          resolved: false,
          outdated: false,
          created_at: "2026-03-04T10:00:00Z",
          updated_at: "2026-03-04T10:00:00Z",
        },
      ],
    },
  });
  commentsDeleteMock.mockResolvedValue({ ok: true, data: { ok: true } });
  reviewsListMock.mockResolvedValue({ ok: true, data: { reviews: [] } });
  reviewRequestsListMock.mockResolvedValue({ ok: true, data: { usernames: [] } });
  branchStatusMock.mockResolvedValue({
    ok: true,
    data: { status: "up_to_date", ahead_count: 1, behind_count: 0 },
  });
  updateMock.mockResolvedValue({ ok: true, data: PULL });
  suitesListMock.mockResolvedValue({ ok: true, data: { suites: [] } });
  runsListForRefMock.mockResolvedValue({ ok: true, data: { check_runs: [] } });
  runsListForSuiteMock.mockResolvedValue({ ok: true, data: { check_runs: [] } });
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

async function mount() {
  await mountWithQueryClient(PullDetailV2, {
    owner: "ada",
    repoName: "hello",
    number: 42,
    repo: REPO,
    pull: PULL,
    canWrite: true,
    canInteract: true,
    isAuthor: true,
    currentUserId: "u1",
    tab: "overview",
  });
  await settle();
}

describe("pull-v2 detail chrome browser DOM races", () => {
  it("comments-panel toggle mounts/unmounts beside the tab body cleanly", async () => {
    const tracker = trackDomErrors();
    try {
      await mount();
      if (!document.querySelector('[data-testid="pull-v2-tab-overview"]')) {
        throw new Error(`chrome missing. ${debugBody()}`);
      }

      await clickTestId("pull-v2-panel-toggle");
      await settle(60);
      expect(document.querySelector('[data-testid="pull-v2-comments-panel"]')).not.toBeNull();
      expect(window.location.search).toContain("panel=comments");

      await clickTestId("pull-v2-comments-close");
      await settle(60);
      expect(document.querySelector('[data-testid="pull-v2-comments-panel"]')).toBeNull();
      expect(window.location.search).not.toContain("panel=comments");

      await clickTestId("pull-v2-panel-toggle");
      await settle(60);
      await clickTestId("pull-v2-panel-toggle");
      await settle(60);

      expectNoOctaneOverlayInDocument();
    } finally {
      tracker.dispose();
      window.history.replaceState(null, "", "/");
    }
  });

  it("inline title edit swaps the header row and saves without races", async () => {
    const tracker = trackDomErrors();
    try {
      await mount();
      await clickTestId("pull-v2-title-edit");
      const input = document.querySelector(
        '[data-testid="pull-v2-title-input"]',
      ) as HTMLInputElement | null;
      if (!input) throw new Error(`title input missing. ${debugBody()}`);

      await act(async () => {
        input.value = "Renamed title";
        input.dispatchEvent(new Event("input", { bubbles: true }));
      });
      await settle(60);
      await clickTestId("pull-v2-title-save");
      await settle(120);
      expect(updateMock).toHaveBeenCalledWith({
        owner: "ada",
        name: "hello",
        number: 42,
        title: "Renamed title",
      });
      expect(document.querySelector('[data-testid="pull-v2-title-input"]')).toBeNull();

      expectNoOctaneOverlayInDocument();
    } finally {
      tracker.dispose();
    }
  });

  it("tab count badges populate from the shared queries", async () => {
    const tracker = trackDomErrors();
    try {
      await mount();
      const commits = document.querySelector('[data-testid="pull-v2-tab-count-commits"]');
      const changes = document.querySelector('[data-testid="pull-v2-tab-count-changes"]');
      expect(commits?.textContent).toBe("1");
      expect(changes?.textContent).toBe("1");
      expect(document.querySelector('[data-testid="pull-v2-submit-review"]')).not.toBeNull();

      expectNoOctaneOverlayInDocument();
    } finally {
      tracker.dispose();
    }
  });

  it("pull-v2-activity-pane: pane selector mounts the activity feed cleanly", async () => {
    const tracker = trackDomErrors();
    try {
      await mount();
      await clickTestId("pull-v2-pane-select");
      await settle(80);
      await clickTestId("pull-v2-pane-activity");
      await settle(120);

      const pane = document.querySelector('[data-testid="pull-v2-activity-pane"]');
      if (!pane) throw new Error(`activity pane missing. ${debugBody()}`);
      expect(pane.textContent).toContain("wire the thing");

      await clickTestId("pull-v2-pane-close");
      await settle(60);
      expect(document.querySelector('[data-testid="pull-v2-activity-pane"]')).toBeNull();

      expectNoOctaneOverlayInDocument();
    } finally {
      tracker.dispose();
    }
  });
});
