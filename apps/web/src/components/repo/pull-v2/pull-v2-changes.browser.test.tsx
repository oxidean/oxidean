/**
 * Chromium proof for the pull-v2 changes tab — the file-tree sidebar toggle,
 * toolbar filter, and per-file viewed checkbox re-render large sibling
 * subtrees; Base UI + Octane reconciliation must not race them.
 */
import { beforeEach, describe, expect, it, vi } from "vitest";
import { PullV2Changes } from "@/components/repo/pull-v2/pull-v2-changes";
import {
  clickTestId,
  debugBody,
  expectNoOctaneOverlayInDocument,
  mountWithQueryClient,
} from "@/test/browser-mount";
import { trackDomErrors } from "@/test/dom-errors";
import { act } from "octane";

const filesMock = vi.fn();
const viewedListMock = vi.fn();
const viewedSetMock = vi.fn();
const commentsListMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    pull: {
      files: (...args: unknown[]) => filesMock(...args),
      filesViewed: {
        list: (...args: unknown[]) => viewedListMock(...args),
        set: (...args: unknown[]) => viewedSetMock(...args),
      },
      comments: {
        list: (...args: unknown[]) => commentsListMock(...args),
      },
      reviews: {
        submit: vi.fn(async () => ({ ok: true, data: { id: "r1" } })),
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
      patch: "@@ -1,2 +1,3 @@\n fn main() {\n+    run();\n }\n",
    },
    {
      path: "docs/README.md",
      status: "added",
      patch: "@@ -0,0 +1,2 @@\n+# Title\n+body\n",
    },
  ],
};

async function settle(ms = 250) {
  await act(async () => {
    await new Promise((r) => setTimeout(r, ms));
  });
}

beforeEach(() => {
  filesMock.mockResolvedValue({ ok: true, data: FILES });
  viewedListMock.mockResolvedValue({ ok: true, data: { viewed_paths: [] } });
  viewedSetMock.mockResolvedValue({ ok: true, data: { viewed_paths: ["src/main.rs"] } });
  commentsListMock.mockResolvedValue({ ok: true, data: { comments: [] } });
});

async function mount() {
  await mountWithQueryClient(PullV2Changes, {
    owner: "acme",
    repoName: "core",
    number: 3,
    canInteract: true,
    isAuthor: false,
    currentUserId: "u1",
    headSha: "abc123",
  });
  await settle();
}

describe("pull-v2 changes browser DOM races", () => {
  it("sidebar toggle + filter typing re-render without insertBefore races", async () => {
    const tracker = trackDomErrors();
    try {
      await mount();
      const root = document.querySelector('[data-testid="pull-v2-changes"]');
      if (!root) throw new Error(`changes root missing. ${debugBody()}`);

      // Sidebar hide/show flips a large @if block next to the diff column.
      await clickTestId("pull-v2-tree-toggle");
      await settle(60);
      await clickTestId("pull-v2-tree-toggle");
      await settle(60);

      // Filter narrows both the tree and the diff list at once.
      const filter = document.querySelector(
        '[data-testid="pull-v2-file-filter"] input, input[data-testid="pull-v2-file-filter"]',
      ) as HTMLInputElement | null;
      if (!filter) throw new Error(`filter input missing. ${debugBody()}`);
      await act(async () => {
        filter.value = "main";
        filter.dispatchEvent(new Event("input", { bubbles: true }));
      });
      await settle(60);
      const count = document.querySelector('[data-testid="pull-v2-filter-count"]');
      expect(count?.textContent).toContain("1");

      await act(async () => {
        filter.value = "";
        filter.dispatchEvent(new Event("input", { bubbles: true }));
      });
      await settle(60);

      expectNoOctaneOverlayInDocument();
    } finally {
      tracker.dispose();
    }
  });

  it("comments side panel toggles without unmounting the diff column", async () => {
    const tracker = trackDomErrors();
    try {
      await mount();
      await clickTestId("pull-v2-comments-toggle");
      await settle(60);
      expect(document.querySelector('[data-testid="pull-v2-comments-panel"]')).not.toBeNull();
      await clickTestId("pull-v2-comments-toggle");
      await settle(60);
      expectNoOctaneOverlayInDocument();
    } finally {
      tracker.dispose();
    }
  });
});
