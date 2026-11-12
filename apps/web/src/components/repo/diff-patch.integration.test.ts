import { cleanup, fireEvent, render, screen, waitFor } from "@octanejs/testing-library";
import { afterEach, describe, expect, it, vi } from "vitest";
import { highlightDiffLines } from "@/lib/highlight-diff";
import { parseUnifiedDiffLines } from "@/lib/parse-unified-diff";
import { DiffPatch } from "./diff-patch";

function pullComment(overrides: Record<string, unknown> = {}) {
  return {
    id: "c1",
    pull_id: "p1",
    author_id: "u1",
    author_username: "ada",
    body: "note",
    path: "a.ts",
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

afterEach(cleanup);

describe("DiffPatch", { timeout: 60_000 }, () => {
  it("colorizes add/del/hunk lines from a unified patch", async () => {
    const patch = [
      "diff --git a/a.ts b/a.ts",
      "index 111..222 100644",
      "--- a/a.ts",
      "+++ b/a.ts",
      "@@ -1,2 +1,2 @@",
      " keep",
      "-old",
      "+new",
    ].join("\n");

    render(DiffPatch, {
      props: {
        path: "a.ts",
        status: "modified",
        patch,
      },
    });

    expect(await screen.findByText("a.ts")).toBeInTheDocument();
    expect(screen.getByText("(modified)")).toBeInTheDocument();

    const add = document.querySelector('[data-diff-kind="add"]');
    const del = document.querySelector('[data-diff-kind="del"]');
    const hunk = document.querySelector('[data-diff-kind="hunk"]');
    // Rows carry old/new gutter numbers before the prefix + content; the
    // " keep" ctx line consumes line 1 of both files, so del/add land on 2.
    expect(add?.textContent).toContain("+new");
    expect(del?.textContent).toContain("-old");
    expect(hunk?.textContent).toContain("@@ -1,2 +1,2 @@");
    expect(del?.textContent?.startsWith("2")).toBe(true);
    expect(add?.textContent?.startsWith("2")).toBe(true);
    expect(add?.className).toMatch(/diff-add/);
    expect(del?.className).toMatch(/diff-del/);
    expect(hunk?.className).toMatch(/diff-hunk/);
  });

  it("applies syntax token colors on code lines for known languages", async () => {
    const patch = ["@@ -1 +1 @@", "-const a = 1;", "+const b = 2;"].join("\n");

    render(DiffPatch, {
      props: {
        path: "util.ts",
        status: "modified",
        patch,
      },
    });

    await waitFor(
      () => {
        const add = document.querySelector('[data-diff-kind="add"]');
        expect(add?.querySelector('span[style*="color"]')).toBeTruthy();
      },
      { timeout: 30_000 },
    );
  });

  it("renders SSR initialRows with token spans on first paint", async () => {
    const patch = ["@@ -1 +1 @@", "-const a = 1;", "+const b = 2;"].join("\n");
    const initialRows = await highlightDiffLines(parseUnifiedDiffLines(patch), {
      path: "util.ts",
    });

    render(DiffPatch, {
      props: {
        path: "util.ts",
        status: "modified",
        patch,
        initialRows,
      },
    });

    expect(await screen.findByText("util.ts")).toBeInTheDocument();
    const add = document.querySelector('[data-diff-kind="add"]');
    const firstHtml = add?.innerHTML ?? "";
    expect(add?.querySelector('span[style*="color"]')).toBeTruthy();

    // Hydrate keeps dual-theme SSR rows verbatim — no re-highlight pass.
    await new Promise((r) => setTimeout(r, 50));
    expect(document.querySelector('[data-diff-kind="add"]')?.innerHTML).toBe(firstHtml);
  });
});

describe("DiffPatch — issue #110 mobile budgets", () => {
  it("gates large patches behind a Load diff button instead of mounting rows", async () => {
    const bigBody = Array.from({ length: 2000 }, (_, i) => `+line ${i}`).join("\n");
    const patch = `@@ -0,0 +1,2000 @@\n${bigBody}`;
    expect(patch.length).toBeGreaterThan(16_000);

    render(DiffPatch, {
      props: { path: "big.ts", status: "modified", patch },
    });

    expect(await screen.findByText("big.ts")).toBeInTheDocument();
    expect(screen.getByText("This diff is too large to show by default.")).toBeInTheDocument();
    // No diff rows mounted until the user asks for them.
    expect(document.querySelector('[data-diff-kind="add"]')).toBeNull();

    fireEvent.click(screen.getByRole("button", { name: "Load diff" }));

    await waitFor(
      () => {
        expect(document.querySelector('[data-diff-kind="add"]')).toBeTruthy();
      },
      { timeout: 30_000 },
    );
  });

  it("collapses and re-expands rows via the header toggle", async () => {
    const patch = ["@@ -1 +1 @@", "-old", "+new"].join("\n");

    render(DiffPatch, {
      props: { path: "a.ts", status: "modified", patch },
    });

    expect(await screen.findByText("a.ts")).toBeInTheDocument();
    expect(document.querySelector('[data-diff-kind="add"]')).toBeTruthy();

    // aria-controls resolves to the mounted collapsible region.
    const toggle = screen.getByRole("button", { name: "Toggle diff for a.ts" });
    const controlsId = toggle.getAttribute("aria-controls");
    expect(controlsId).toBeTruthy();
    expect(document.getElementById(controlsId!)).toBeTruthy();

    fireEvent.click(toggle);
    await waitFor(() => {
      expect(document.querySelector('[data-diff-kind="add"]')).toBeNull();
    });

    fireEvent.click(screen.getByRole("button", { name: "Toggle diff for a.ts" }));
    await waitFor(
      () => {
        expect(document.querySelector('[data-diff-kind="add"]')).toBeTruthy();
      },
      { timeout: 30_000 },
    );
  });

  it("shows per-file +/− stats in the header", async () => {
    const patch = ["@@ -1,2 +1,2 @@", " keep", "-old", "+new", "+extra"].join("\n");
    // widen the hunk so +extra lands inside the new-side budget
    const widePatch = patch.replace("+1,2", "+1,3");

    render(DiffPatch, {
      props: { path: "stats.ts", status: "modified", patch: widePatch },
    });

    expect(await screen.findByText("+2")).toBeInTheDocument();
    expect(screen.getByText("−1")).toBeInTheDocument();
  });

  it("re-renders rows when the patch prop changes on a mounted file", async () => {
    const { rerender } = render(DiffPatch, {
      props: {
        path: "a.ts",
        status: "modified",
        patch: ["@@ -1 +1 @@", "-const a = 1;", "+const first = 1;"].join("\n"),
      },
    });

    // Wait for the lazy highlight pass to land so the done-latch has engaged —
    // a stale latch is what freezes rows on a refetched patch (PR file lists
    // refetch after a push while the component stays mounted on `key path`).
    await waitFor(
      () => {
        const add = document.querySelector('[data-diff-kind="add"]');
        expect(add?.querySelector('span[style*="color"]')).toBeTruthy();
      },
      { timeout: 30_000 },
    );

    rerender({
      props: {
        path: "a.ts",
        status: "modified",
        patch: ["@@ -1 +1 @@", "-const a = 1;", "+const second = 2;"].join("\n"),
      },
    });

    await waitFor(() => {
      expect(document.querySelector('[data-diff-kind="add"]')?.textContent).toContain("second");
    });
    expect(document.querySelector('[data-diff-kind="add"]')?.textContent).not.toContain("first");
  });

  it("restores highlighted rows when the patch bounces back before a pending highlight lands", async () => {
    const patchA = ["@@ -1 +1 @@", "-const a = 1;", "+const first = 1;"].join("\n");
    const patchB = ["@@ -1 +1 @@", "-const a = 1;", "+const second = 2;"].join("\n");

    const { rerender } = render(DiffPatch, {
      props: { path: "a.ts", status: "modified", patch: patchA },
    });

    // A's highlight completes — the done-latch now holds A's rows.
    await waitFor(
      () => {
        const add = document.querySelector('[data-diff-kind="add"]');
        expect(add?.querySelector('span[style*="color"]')).toBeTruthy();
      },
      { timeout: 30_000 },
    );

    // Swap A→B→A inside the idle window: B's plain rows commit, but B's
    // scheduled highlight is cancelled by A's cleanup. The latch must restore
    // A's completed rows rather than leave B's plain content on screen.
    rerender({ props: { path: "a.ts", status: "modified", patch: patchB } });
    rerender({ props: { path: "a.ts", status: "modified", patch: patchA } });

    const add = document.querySelector('[data-diff-kind="add"]');
    expect(add?.textContent).toContain("first");
    expect(add?.textContent).not.toContain("second");
    expect(add?.querySelector('span[style*="color"]')).toBeTruthy();
  });
});

describe("DiffPatch — #262 inline comments", () => {
  const PATCH = ["@@ -1,3 +1,3 @@", " keep", "-old", "+new", " tail"].join("\n");

  it("stays read-only when onSubmitComment is not provided", async () => {
    render(DiffPatch, { props: { path: "a.ts", status: "modified", patch: PATCH } });
    expect(await screen.findByText("a.ts")).toBeInTheDocument();
    await waitFor(() => {
      expect(document.querySelector('[data-diff-kind="add"]')).toBeTruthy();
    });
    expect(document.querySelector('[data-testid="pull-line-comment-add"]')).toBeNull();
    expect(document.querySelector("[data-diff-gutter]")).toBeNull();
  });

  it("+ comment opens a single-line draft and submits the anchor", async () => {
    const onSubmit = vi.fn(async () => true);
    render(DiffPatch, {
      props: {
        path: "a.ts",
        status: "modified",
        patch: PATCH,
        canInteract: true,
        onSubmitComment: onSubmit,
      },
    });
    await waitFor(() => {
      expect(document.querySelector('[data-testid="pull-line-comment-add"]')).toBeTruthy();
    });
    // The "+" on the +new row (new-side line 2) anchors RIGHT/2; the del row
    // has a same-named button first in DOM order — take the second.
    const addBtn = screen.getAllByRole("button", { name: "Comment on line 2" })[1]!;
    fireEvent.click(addBtn);
    const box = document.querySelector('[data-testid="pull-line-draft"]');
    expect(box).toBeTruthy();
    fireEvent.input(box!.querySelector("textarea")!, { target: { value: "nit" } });
    fireEvent.click(screen.getByTestId("pull-line-draft-add"));
    await waitFor(() => {
      expect(onSubmit).toHaveBeenCalledWith(
        expect.objectContaining({ side: "RIGHT", line: 2, body: "nit" }),
      );
    });
    await waitFor(() => {
      expect(document.querySelector('[data-testid="pull-line-draft"]')).toBeNull();
    });
  });

  it("gutter drag selects a contiguous range and reports startLine", async () => {
    const onSubmit = vi.fn(async () => true);
    render(DiffPatch, {
      props: {
        path: "a.ts",
        status: "modified",
        patch: PATCH,
        canInteract: true,
        onSubmitComment: onSubmit,
      },
    });
    await waitFor(() => {
      expect(document.querySelectorAll("[data-diff-row]").length).toBeGreaterThan(0);
    });
    const rows = [...document.querySelectorAll("[data-diff-row]")];
    // Downward drag on the gutter: ctx " keep" (row 1, RIGHT/1) →
    // ctx " tail" (row 4, RIGHT/3).
    const first = rows[1] as HTMLElement;
    const last = rows[4] as HTMLElement;
    first
      .querySelector("[data-diff-gutter]")!
      .dispatchEvent(new MouseEvent("mousedown", { bubbles: true, button: 0 }));
    last.dispatchEvent(new MouseEvent("mouseover", { bubbles: true }));
    document.dispatchEvent(new MouseEvent("mouseup", { bubbles: true }));
    const box = await screen.findByTestId("pull-line-draft");
    expect(box.textContent).toContain("lines 1–3");
    fireEvent.input(box.querySelector("textarea")!, { target: { value: "range note" } });
    fireEvent.click(screen.getByTestId("pull-line-draft-add"));
    await waitFor(() => {
      expect(onSubmit).toHaveBeenCalledWith(
        expect.objectContaining({ side: "RIGHT", startLine: 1, line: 3 }),
      );
    });
  });

  it("upward drag normalizes to [start..end]", async () => {
    const onSubmit = vi.fn(async () => true);
    render(DiffPatch, {
      props: {
        path: "a.ts",
        status: "modified",
        patch: PATCH,
        canInteract: true,
        onSubmitComment: onSubmit,
      },
    });
    await waitFor(() => {
      expect(document.querySelectorAll("[data-diff-row]").length).toBeGreaterThan(0);
    });
    const rows = [...document.querySelectorAll("[data-diff-row]")];
    const top = rows[1] as HTMLElement;
    const bottom = rows[4] as HTMLElement;
    bottom
      .querySelector("[data-diff-gutter]")!
      .dispatchEvent(new MouseEvent("mousedown", { bubbles: true, button: 0 }));
    top.dispatchEvent(new MouseEvent("mouseover", { bubbles: true }));
    document.dispatchEvent(new MouseEvent("mouseup", { bubbles: true }));
    const box = await screen.findByTestId("pull-line-draft");
    expect(box.textContent).toContain("lines 1–3");
    fireEvent.input(box.querySelector("textarea")!, { target: { value: "up" } });
    fireEvent.click(screen.getByTestId("pull-line-draft-add"));
    await waitFor(() => {
      expect(onSubmit).toHaveBeenCalledWith(
        expect.objectContaining({ side: "RIGHT", startLine: 1, line: 3 }),
      );
    });
  });

  it("Escape cancels an in-progress drag without opening a draft", async () => {
    const onSubmit = vi.fn(async () => true);
    render(DiffPatch, {
      props: {
        path: "a.ts",
        status: "modified",
        patch: PATCH,
        canInteract: true,
        onSubmitComment: onSubmit,
      },
    });
    await waitFor(() => {
      expect(document.querySelectorAll("[data-diff-row]").length).toBeGreaterThan(0);
    });
    const rows = [...document.querySelectorAll("[data-diff-row]")];
    (rows[1] as HTMLElement)
      .querySelector("[data-diff-gutter]")!
      .dispatchEvent(new MouseEvent("mousedown", { bubbles: true, button: 0 }));
    (rows[3] as HTMLElement).dispatchEvent(new MouseEvent("mouseover", { bubbles: true }));
    document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    document.dispatchEvent(new MouseEvent("mouseup", { bubbles: true }));
    expect(document.querySelector('[data-testid="pull-line-draft"]')).toBeNull();
    expect(onSubmit).not.toHaveBeenCalled();
  });

  it("renders submitted + pending comments with a discard action", async () => {
    const onDiscard = vi.fn();
    render(DiffPatch, {
      props: {
        path: "a.ts",
        status: "modified",
        patch: PATCH,
        comments: [
          pullComment({ id: "sub1", body: "looks good" }),
          pullComment({ id: "pc7", body: "draft", pending: true }),
          pullComment({ id: "rng", body: "range", start_line: 1, line: 3 }),
        ],
        canInteract: true,
        onSubmitComment: vi.fn(async () => true),
        onDiscardComment: onDiscard,
      },
    });
    await waitFor(() => {
      expect(document.querySelectorAll('[data-testid="pull-line-comment"]').length).toBe(2);
    });
    expect(document.querySelectorAll('[data-testid="pull-pending-comment"]').length).toBe(1);
    // Header count includes everything.
    expect(screen.getByTestId("pull-file-comment-count").textContent).toContain("3");
    fireEvent.click(screen.getByTestId("pull-pending-discard"));
    expect(onDiscard).toHaveBeenCalledWith("pc7");
  });
});
