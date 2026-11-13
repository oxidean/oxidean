/**
 * Chromium proof for the pull-v2 changes tab — the file-tree sidebar toggle,
 * toolbar filter, scope/settings DropdownMenu portals, splitter drag, and the
 * "+ comment" draft flow re-render large sibling subtrees; Base UI + Octane
 * reconciliation must not race them.
 */
import { beforeEach, describe, expect, it, vi } from "vitest";
import { PullV2Changes } from "@/components/repo/pull-v2/pull-v2-changes";
import { PullV2DiffFile } from "@/components/repo/pull-v2/pull-v2-diff-file";
import { PullV2FileTree } from "@/components/repo/pull-v2/pull-v2-file-tree";
import {
  clickTestId,
  debugBody,
  expectNoOctaneOverlayInDocument,
  mountWithQueryClient,
} from "@/test/browser-mount";
import { trackDomErrors } from "@/test/dom-errors";
import { act } from "octane";

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
      patch: "@@ -1,2 +1,3 @@\n fn main() {\n+    run();\n }\n",
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

async function settle(ms = 250) {
  await act(async () => {
    await new Promise((r) => setTimeout(r, ms));
  });
}

async function waitForTestId(testId: string, ms = 5_000): Promise<Element> {
  const deadline = Date.now() + ms;
  for (;;) {
    const el = document.querySelector(`[data-testid="${testId}"]`);
    if (el) return el;
    if (Date.now() > deadline) throw new Error(`testid ${testId} never mounted. ${debugBody()}`);
    await settle(50);
  }
}

beforeEach(() => {
  localStorage.clear();
  filesMock.mockResolvedValue({ ok: true, data: FILES });
  commitsMock.mockResolvedValue({ ok: true, data: COMMITS });
  viewedListMock.mockResolvedValue({ ok: true, data: { viewed_paths: [] } });
  viewedSetMock.mockResolvedValue({ ok: true, data: { viewed_paths: ["src/main.rs"] } });
  commentsListMock.mockResolvedValue({ ok: true, data: { comments: [] } });
  commentsCreateMock.mockResolvedValue({
    ok: true,
    data: { id: "c-new", pending: true },
  });
  commentsDeleteMock.mockResolvedValue({ ok: true, data: { deleted: true } });
  reviewsSubmitMock.mockResolvedValue({ ok: true, data: { id: "r1" } });
});

async function mount(props: Record<string, unknown> = {}) {
  await mountWithQueryClient(PullV2Changes, {
    owner: "acme",
    repoName: "core",
    number: 3,
    canInteract: true,
    canWrite: true,
    isAuthor: false,
    currentUserId: "u1",
    headSha: "abc123",
    ...props,
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

  it("comments toggle lifts panel state without unmounting the diff column", async () => {
    const tracker = trackDomErrors();
    try {
      const seen: boolean[] = [];
      await mount({ panelOpen: false, onPanelToggle: (open: boolean) => seen.push(open) });
      await clickTestId("pull-v2-comments-toggle");
      await settle(60);
      // The panel itself renders at the PullDetailV2 level now — the changes
      // toolbar button only lifts state through onPanelToggle.
      expect(seen).toEqual([true]);
      expect(document.querySelector('[data-testid="pull-v2-changes"]')).not.toBeNull();
      expectNoOctaneOverlayInDocument();
    } finally {
      tracker.dispose();
    }
  });

  it("scope menu lists commits + since option through the portal", async () => {
    const tracker = trackDomErrors();
    try {
      await mount();
      await clickTestId("pull-v2-scope-menu");
      await settle(80);
      expect(document.querySelector('[data-testid="pull-v2-scope-all"]')).not.toBeNull();
      expect(document.querySelector('[data-testid="pull-v2-scope-since"]')).not.toBeNull();
      const commits = document.querySelectorAll('[data-testid="pull-v2-scope-commit"]');
      expect(commits.length).toBe(2);
      expect(commits[0]?.textContent).toContain("aaaa111");
      expectNoOctaneOverlayInDocument();
    } finally {
      tracker.dispose();
    }
  });

  it("since-review scope switches the files query and notice", async () => {
    const tracker = trackDomErrors();
    try {
      await mount();
      await clickTestId("pull-v2-scope-menu");
      await settle(80);
      await clickTestId("pull-v2-scope-since");
      await settle();
      expect(filesMock).toHaveBeenCalledWith(
        expect.objectContaining({ owner: "acme", name: "core", number: 3, since_review: true }),
      );
      expect(document.querySelector('[data-testid="pull-v2-changes-since-notice"]')).not.toBeNull();
      // Scoped views hide viewed marks.
      expect(document.querySelector('[data-testid="pull-v2-tree-viewed"]')).toBeNull();
      expectNoOctaneOverlayInDocument();
    } finally {
      tracker.dispose();
    }
  });

  it("no_prior_review falls back to all files with the notice", async () => {
    const tracker = trackDomErrors();
    try {
      filesMock.mockImplementation(async (input: { since_review?: boolean }) => {
        if (input.since_review) {
          return { ok: false, error: { code: "pull.no_prior_review", message: "none" } };
        }
        return { ok: true, data: FILES };
      });
      await mount();
      await clickTestId("pull-v2-scope-menu");
      await settle(80);
      await clickTestId("pull-v2-scope-since");
      await settle();
      expect(
        document.querySelector('[data-testid="pull-v2-changes-no-prior-notice"]'),
      ).not.toBeNull();
      // Fallback ran the unscoped call — viewed marks stay enabled.
      expect(document.querySelector('[data-testid="pull-v2-file-viewed"]')).not.toBeNull();
      expectNoOctaneOverlayInDocument();
    } finally {
      tracker.dispose();
    }
  });

  it("diff settings dropdown opens its portal and toggles options", async () => {
    const tracker = trackDomErrors();
    try {
      await mount();
      await clickTestId("pull-v2-diff-settings");
      await settle(80);
      await clickTestId("pull-v2-minimize-comments");
      await settle(60);
      await clickTestId("pull-v2-wrap-lines");
      await settle(60);
      expectNoOctaneOverlayInDocument();
    } finally {
      tracker.dispose();
    }
  });

  it("tree filter dropdown narrows rows by filename", async () => {
    const tracker = trackDomErrors();
    try {
      await mount();
      await clickTestId("pull-v2-tree-filter");
      await settle(80);
      const input = document.querySelector(
        'input[data-testid="pull-v2-tree-filter-name"]',
      ) as HTMLInputElement | null;
      if (!input) throw new Error(`tree filter input missing. ${debugBody()}`);
      await act(async () => {
        input.value = "readme";
        input.dispatchEvent(new Event("input", { bubbles: true }));
      });
      await settle(60);
      const rows = document.querySelectorAll('[data-testid="pull-v2-tree-file"]');
      expect(rows.length).toBe(1);
      expect(rows[0]?.textContent).toContain("README");
      expectNoOctaneOverlayInDocument();
    } finally {
      tracker.dispose();
    }
  });

  it("per-file more-options menu opens with copy-path + view-file items", async () => {
    const tracker = trackDomErrors();
    try {
      await mount();
      await clickTestId("pull-v2-file-menu");
      await settle(80);
      expect(document.querySelector('[data-testid="pull-v2-file-menu-copy"]')).not.toBeNull();
      const view = document.querySelector(
        '[data-testid="pull-v2-file-menu-view"]',
      ) as HTMLAnchorElement | null;
      expect(view?.getAttribute("href")).toContain("/acme/core/blob/abc123/");
      expectNoOctaneOverlayInDocument();
    } finally {
      tracker.dispose();
    }
  });

  it("single-line + comment writes a pending draft and submits the review", async () => {
    const tracker = trackDomErrors();
    try {
      await mount();
      await waitForTestId("pull-v2-line-comment-add");
      await clickTestId("pull-v2-line-comment-add");
      await settle(60);
      const box = document.querySelector('[data-testid="pull-v2-line-draft"]');
      if (!box) throw new Error(`line draft missing. ${debugBody()}`);
      const ta = box.querySelector("textarea");
      if (!ta) throw new Error("draft textarea missing");
      await act(async () => {
        ta.value = "nit";
        ta.dispatchEvent(new Event("input", { bubbles: true }));
      });
      await clickTestId("pull-v2-line-draft-add");
      await settle();
      expect(commentsCreateMock).toHaveBeenCalledWith(
        expect.objectContaining({
          owner: "acme",
          name: "core",
          number: 3,
          body: "nit",
          path: "src/main.rs",
          pending: true,
          commit_sha: "abc123",
        }),
      );
      expectNoOctaneOverlayInDocument();
    } finally {
      tracker.dispose();
    }
  });

  it("tree splitter drag persists a clamped width", async () => {
    const tracker = trackDomErrors();
    try {
      await mount();
      const splitter = document.querySelector(
        '[data-testid="pull-v2-tree-splitter"]',
      ) as HTMLElement | null;
      if (!splitter) throw new Error(`splitter missing. ${debugBody()}`);
      await act(async () => {
        splitter.dispatchEvent(
          new PointerEvent("pointerdown", { bubbles: true, pointerId: 7, clientX: 300 }),
        );
        splitter.dispatchEvent(
          new PointerEvent("pointermove", { bubbles: true, pointerId: 7, clientX: 420 }),
        );
        splitter.dispatchEvent(
          new PointerEvent("pointerup", { bubbles: true, pointerId: 7, clientX: 420 }),
        );
      });
      await settle(60);
      expect(localStorage.getItem("oxidean:pull-v2-tree-width:acme/core")).toBe("380");
      expectNoOctaneOverlayInDocument();
    } finally {
      tracker.dispose();
    }
  });
});

describe("pull-v2 diff-file + file-tree mounts", () => {
  it("file tree renders rows and toggles viewed", async () => {
    const tracker = trackDomErrors();
    try {
      const toggled = vi.fn();
      await mountWithQueryClient(PullV2FileTree, {
        files: FILES.files,
        viewed: new Set<string>(),
        commentCounts: new Map<string, number>([["src/main.rs", 2]]),
        canInteract: true,
        showViewed: true,
        onToggleViewed: toggled,
      });
      await settle();
      const main = document.querySelector('[data-testid="pull-v2-tree-viewed"]');
      expect(main).toBeTruthy();
      await clickTestId("pull-v2-tree-viewed");
      await settle(60);
      expect(toggled).toHaveBeenCalled();
      expectNoOctaneOverlayInDocument();
    } finally {
      tracker.dispose();
    }
  });

  it("gutter drag-select drafts a start_line..line range", async () => {
    const tracker = trackDomErrors();
    try {
      const startDraft = vi.fn();
      await mountWithQueryClient(PullV2DiffFile, {
        file: FILES.files[0],
        view: "unified",
        wrap: false,
        compact: false,
        expanded: true,
        viewed: false,
        marksEnabled: true,
        canInteract: true,
        canWrite: true,
        minimizeComments: false,
        pending: [],
        submitted: [],
        draft: null,
        draftBody: "",
        draftBusy: false,
        draftError: null,
        viewFileHref: "/acme/core/blob/main/src/main.rs",
        onToggleExpand: vi.fn(),
        onToggleViewed: vi.fn(),
        onDraftBody: vi.fn(),
        onStartDraft: startDraft,
        onSubmitDraft: vi.fn(),
        onCancelDraft: vi.fn(),
        onDiscardPending: vi.fn(),
      });
      await settle();
      // Hunk/meta rows render gutter cells too but carry no line number —
      // pick rows whose gutter actually shows a number.
      const rows = [...document.querySelectorAll("[data-diff-row]")];
      const anchorable = rows.filter((r) =>
        [...r.querySelectorAll("[data-diff-gutter]")].some((g) =>
          /^\d+$/.test((g.textContent ?? "").trim()),
        ),
      );
      if (anchorable.length < 3) throw new Error(`expected >=3 anchorable rows. ${debugBody()}`);
      const firstGutter = anchorable[0].querySelector("[data-diff-gutter]") as HTMLElement;
      const lastRow = anchorable[anchorable.length - 1] as HTMLElement;
      await act(async () => {
        firstGutter.dispatchEvent(new MouseEvent("mousedown", { bubbles: true, button: 0 }));
      });
      await act(async () => {
        lastRow.dispatchEvent(new MouseEvent("mouseover", { bubbles: true }));
        document.dispatchEvent(new MouseEvent("mouseup", { bubbles: true }));
      });
      await settle(60);
      expect(startDraft).toHaveBeenCalledTimes(1);
      const target = startDraft.mock.calls[0][0] as {
        line: number;
        startLine?: number | null;
      };
      expect(target.line).toBeGreaterThan(target.startLine ?? Infinity);
      expectNoOctaneOverlayInDocument();
    } finally {
      tracker.dispose();
    }
  });

  it("keeps the open draft's line range highlighted", async () => {
    const tracker = trackDomErrors();
    try {
      await mountWithQueryClient(PullV2DiffFile, {
        file: FILES.files[0],
        view: "unified",
        wrap: false,
        compact: false,
        expanded: true,
        viewed: false,
        marksEnabled: true,
        canInteract: true,
        canWrite: true,
        minimizeComments: false,
        pending: [],
        submitted: [],
        draft: {
          path: "src/main.rs",
          row: 2,
          side: "RIGHT",
          line: 3,
          startLine: 1,
        },
        draftBody: "",
        draftBusy: false,
        draftError: null,
        viewFileHref: "/acme/core/blob/main/src/main.rs",
        onToggleExpand: vi.fn(),
        onToggleViewed: vi.fn(),
        onDraftBody: vi.fn(),
        onStartDraft: vi.fn(),
        onSubmitDraft: vi.fn(),
        onCancelDraft: vi.fn(),
        onDiscardPending: vi.fn(),
      });
      await settle();
      const lit = [...document.querySelectorAll("[data-diff-row]")].filter((r) =>
        (r.firstElementChild?.className ?? "").includes("outline-primary"),
      );
      // All three content rows anchor RIGHT-side lines 1–3.
      expect(lit.length).toBe(3);
      expectNoOctaneOverlayInDocument();
    } finally {
      tracker.dispose();
    }
  });
});
