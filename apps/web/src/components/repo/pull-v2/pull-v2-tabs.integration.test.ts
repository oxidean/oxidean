/**
 * pull-v2 tab bodies (WI-0001 stages 4–10): changes-tab file tree + toolbar,
 * per-file viewed state, pending-review flow, commits date grouping, and
 * checks suites sidebar — all against a mocked api-client.
 */
import { cleanup, fireEvent, screen, waitFor } from "@octanejs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderWithQueryClient } from "@/test/render-with-query";

const filesMock = vi.fn();
const filesSetViewedMock = vi.fn();
const filesListViewedMock = vi.fn();
const commentsListMock = vi.fn();
const commentsCreateMock = vi.fn();
const reviewsSubmitMock = vi.fn();
const commitsMock = vi.fn();
const suitesListMock = vi.fn();
const runsListForSuiteMock = vi.fn();
const runsGetMock = vi.fn();
const annotationsListMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    pull: {
      files: (...args: unknown[]) => filesMock(...args),
      filesViewed: {
        set: (...args: unknown[]) => filesSetViewedMock(...args),
        list: (...args: unknown[]) => filesListViewedMock(...args),
      },
      comments: {
        list: (...args: unknown[]) => commentsListMock(...args),
        create: (...args: unknown[]) => commentsCreateMock(...args),
      },
      reviews: {
        submit: (...args: unknown[]) => reviewsSubmitMock(...args),
      },
      commits: (...args: unknown[]) => commitsMock(...args),
    },
    check: {
      suites: {
        list: (...args: unknown[]) => suitesListMock(...args),
      },
      runs: {
        listForSuite: (...args: unknown[]) => runsListForSuiteMock(...args),
        get: (...args: unknown[]) => runsGetMock(...args),
      },
      annotations: {
        list: (...args: unknown[]) => annotationsListMock(...args),
      },
    },
  },
}));

import { PullV2Changes } from "./pull-v2-changes";
import { PullV2Checks } from "./pull-v2-checks";
import { PullV2Commits } from "./pull-v2-commits";

const FILES = {
  files: [
    {
      path: "src/a.ts",
      status: "modified",
      patch: "@@ -1,2 +1,3 @@\n line1\n-old\n+new1\n+new2\n line2",
    },
    {
      path: "docs/b.md",
      status: "added",
      patch: "@@ -0,0 +1 @@\n+hello",
    },
  ],
  empty: false,
};

const CHANGES_PROPS = {
  owner: "ada",
  repoName: "hello",
  number: 42,
  canInteract: true,
  isAuthor: false,
  currentUserId: "u2",
  headSha: "deadbeef0123",
};

function renderChanges(extra: Record<string, unknown> = {}) {
  return renderWithQueryClient(PullV2Changes, {
    props: { ...CHANGES_PROPS, ...extra },
  });
}

afterEach(cleanup);

beforeEach(() => {
  filesMock.mockReset();
  filesSetViewedMock.mockReset();
  filesListViewedMock.mockReset();
  commentsListMock.mockReset();
  commentsCreateMock.mockReset();
  reviewsSubmitMock.mockReset();
  commitsMock.mockReset();
  suitesListMock.mockReset();
  runsListForSuiteMock.mockReset();
  runsGetMock.mockReset();
  annotationsListMock.mockReset();

  filesMock.mockResolvedValue({ ok: true, data: FILES });
  filesListViewedMock.mockResolvedValue({ ok: true, data: { viewed_paths: [] } });
  commentsListMock.mockResolvedValue({ ok: true, data: { comments: [] } });
  commentsCreateMock.mockResolvedValue({ ok: true, data: { id: "c1" } });
  reviewsSubmitMock.mockResolvedValue({ ok: true, data: { id: "r1" } });
  commitsMock.mockResolvedValue({ ok: true, data: { commits: [] } });
  suitesListMock.mockResolvedValue({ ok: true, data: { suites: [] } });
  runsListForSuiteMock.mockResolvedValue({ ok: true, data: { check_runs: [] } });
  runsGetMock.mockResolvedValue({ ok: true, data: null });
  annotationsListMock.mockResolvedValue({ ok: true, data: { annotations: [] } });
});

describe("PullV2Changes", () => {
  it("renders the file tree with file names and the toolbar counts", async () => {
    renderChanges();

    expect(await screen.findByTestId("pull-v2-file-tree")).toBeTruthy();
    const rows = await screen.findAllByTestId("pull-v2-tree-file");
    expect(rows.length).toBe(2);
    // Basenames in the tree rows (tree is ordered by path, so assert order-free).
    const rowText = rows.map((r) => r.textContent ?? "");
    expect(rowText.some((t) => t.includes("a.ts"))).toBe(true);
    expect(rowText.some((t) => t.includes("b.md"))).toBe(true);
    // Directory grouping rows are rendered too.
    expect(screen.getByText("src")).toBeTruthy();
    expect(screen.getByText("docs")).toBeTruthy();

    expect(screen.getByTestId("pull-v2-viewed-progress").textContent).toContain(
      "0 of 2 files viewed",
    );
    expect(screen.getByTestId("pull-v2-file-filter")).toBeTruthy();
    expect(screen.getByTestId("pull-v2-expand-all")).toBeTruthy();
    expect(screen.getByTestId("pull-v2-collapse-all")).toBeTruthy();
    expect(screen.getByTestId("pull-v2-diff-settings")).toBeTruthy();
    // Both diff blocks mount.
    expect((await screen.findAllByTestId("pull-v2-diff-file")).length).toBe(2);
  });

  it("filters the file list by substring", async () => {
    renderChanges();
    await screen.findByTestId("pull-v2-file-tree");

    fireEvent.input(screen.getByTestId("pull-v2-file-filter"), {
      target: { value: "a.ts" },
    });
    await waitFor(() => {
      expect(screen.queryAllByTestId("pull-v2-tree-file").length).toBe(1);
    });
    expect(screen.getByTestId("pull-v2-filter-count").textContent).toContain("1 of 2");
  });

  it("calls pull.files.setViewed when a file is marked viewed", async () => {
    filesSetViewedMock.mockResolvedValue({
      ok: true,
      data: { viewed_paths: ["src/a.ts"] },
    });
    renderChanges();
    const rows = await screen.findAllByTestId("pull-v2-tree-file");
    const rowA = rows.find((r) => (r.textContent ?? "").includes("a.ts"))!;
    const toggle = rowA.querySelector('[data-testid="pull-v2-tree-viewed"]')!;
    fireEvent.click(toggle);

    await waitFor(() => {
      expect(filesSetViewedMock).toHaveBeenCalledWith({
        owner: "ada",
        name: "hello",
        number: 42,
        path: "src/a.ts",
        viewed: true,
      });
    });
    await waitFor(() => {
      expect(screen.getByTestId("pull-v2-viewed-progress").textContent).toContain(
        "1 of 2 files viewed",
      );
    });
  });

  it("collects a line comment into the pending review and submits it", async () => {
    renderChanges();
    await screen.findByTestId("pull-v2-file-tree");

    // First anchorable row in src/a.ts is the context line at RIGHT:1.
    const addButtons = await screen.findAllByTestId("pull-v2-line-comment-add");
    fireEvent.click(addButtons[0]!);

    const draft = await screen.findByTestId("pull-v2-line-draft");
    const textarea = draft.querySelector("textarea")!;
    fireEvent.input(textarea, { target: { value: "nit: rename this" } });
    fireEvent.click(screen.getByTestId("pull-v2-line-draft-add"));

    // Pending badge shows in-file.
    expect(await screen.findByTestId("pull-v2-pending-comment")).toBeTruthy();

    fireEvent.click(screen.getByTestId("pull-v2-review-submit"));

    await waitFor(() => {
      expect(reviewsSubmitMock).toHaveBeenCalledWith({
        owner: "ada",
        name: "hello",
        number: 42,
        state: "commented",
        body: null,
      });
    });
    await waitFor(() => {
      expect(commentsCreateMock).toHaveBeenCalledWith({
        owner: "ada",
        name: "hello",
        number: 42,
        body: "nit: rename this",
        path: "src/a.ts",
        side: "RIGHT",
        line: 1,
        commit_sha: "deadbeef0123",
      });
    });
    // Pending list cleared after submit.
    await waitFor(() => {
      expect(screen.queryByTestId("pull-v2-pending-comment")).toBeNull();
    });
  });

  it("disables Approve / Request changes for the pull author", async () => {
    renderChanges({ isAuthor: true });
    await screen.findByTestId("pull-v2-file-tree");
    // Base UI radios render as spans; disabled state is a data attribute.
    expect(screen.getByTestId("pull-v2-review-approve").hasAttribute("data-disabled")).toBe(true);
    expect(screen.getByTestId("pull-v2-review-request-changes").hasAttribute("data-disabled")).toBe(
      true,
    );
    expect(screen.getByTestId("pull-v2-review-comment").hasAttribute("data-disabled")).toBe(false);
  });

  it("shows the scoped-commit notice with a Show-all link", async () => {
    renderChanges({ changesSha: "abc123def456" });
    const notice = await screen.findByTestId("pull-v2-changes-sha-notice");
    expect(notice.textContent).toContain("abc123d");
    expect(screen.getByTestId("pull-v2-changes-show-all").getAttribute("href")).toBe(
      "/ada/hello/pull/42/changes",
    );
  });
});

describe("PullV2Commits", () => {
  it("groups commits under per-day headers and links shas to scoped changes", async () => {
    commitsMock.mockResolvedValue({
      ok: true,
      data: {
        commits: [
          {
            sha: "aaaa1111aaaa",
            short_sha: "aaaa111",
            subject: "first commit",
            author_name: "Ada",
            author_email: "ada@x",
            authored_at: "2026-03-04T10:00:00Z",
          },
          {
            sha: "bbbb2222bbbb",
            short_sha: "bbbb222",
            subject: "second commit",
            author_name: "Ada",
            author_email: "ada@x",
            authored_at: "2026-03-04T15:30:00Z",
          },
          {
            sha: "cccc3333cccc",
            short_sha: "cccc333",
            subject: "third commit",
            author_name: "Ada",
            author_email: "ada@x",
            authored_at: "2026-03-05T09:00:00Z",
          },
        ],
      },
    });
    renderWithQueryClient(PullV2Commits, {
      props: { owner: "ada", repoName: "hello", number: 42 },
    });

    const groups = await screen.findAllByTestId("pull-v2-commit-group");
    expect(groups.length).toBe(2);
    // Newest day first.
    expect(groups[0]!.textContent).toContain("Commits on Mar 5, 2026");
    expect(groups[1]!.textContent).toContain("Commits on Mar 4, 2026");
    // Sha links go to the scoped changes view.
    const shaLink = screen.getByText("aaaa111").closest("a")!;
    expect(shaLink.getAttribute("href")).toBe("/ada/hello/pull/42/changes/aaaa1111aaaa");
  });
});

describe("PullV2Checks", () => {
  const SUITE = {
    id: "s1",
    head_sha: "abc123",
    app: "github-actions",
    status: "completed",
    conclusion: "success",
    check_runs_count: 1,
    created_at: "2026-03-04T10:00:00Z",
    updated_at: "2026-03-04T10:05:00Z",
  };
  const RUN = {
    id: "r1",
    name: "build",
    head_sha: "abc123",
    status: "completed",
    conclusion: "success",
    check_suite_id: "s1",
    app: "github-actions",
    output: { title: "build ok", annotations_count: 1 },
    created_at: "2026-03-04T10:00:00Z",
    updated_at: "2026-03-04T10:05:00Z",
  };

  it("lists check suites in the sidebar and shows runs + annotations", async () => {
    suitesListMock.mockResolvedValue({ ok: true, data: { suites: [SUITE] } });
    runsListForSuiteMock.mockResolvedValue({ ok: true, data: { check_runs: [RUN] } });
    annotationsListMock.mockResolvedValue({
      ok: true,
      data: {
        annotations: [
          {
            id: "a1",
            check_run_id: "r1",
            path: "src/a.ts",
            start_line: 3,
            end_line: 3,
            annotation_level: "warning",
            message: "unused variable",
          },
        ],
      },
    });
    renderWithQueryClient(PullV2Checks, {
      props: { owner: "ada", repoName: "hello", sha: "abc123" },
    });

    const suite = await screen.findByTestId("pull-v2-check-suite");
    expect(suite.textContent).toContain("GitHub Actions");
    expect(suite.textContent).toContain("1 run");

    const run = await screen.findByTestId("pull-v2-check-run");
    expect(run.textContent).toContain("build");
    expect(runsListForSuiteMock).toHaveBeenCalledWith({
      owner: "ada",
      name: "hello",
      suite_id: "s1",
    });

    const annotation = await screen.findByTestId("pull-v2-check-annotation");
    expect(annotation.textContent).toContain("src/a.ts:3");
    expect(annotation.textContent).toContain("unused variable");
    expect(annotationsListMock).toHaveBeenCalledWith({
      owner: "ada",
      name: "hello",
      check_run_id: "r1",
    });
  });
});
