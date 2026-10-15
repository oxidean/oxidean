import { cleanup, fireEvent, render, screen, waitFor } from "@octanejs/testing-library";
import { afterEach, describe, expect, it } from "vitest";
import { highlightDiffLines } from "@/lib/highlight-diff";
import { parseUnifiedDiffLines } from "@/lib/parse-unified-diff";
import { DiffPatch } from "./diff-patch";

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

    fireEvent.click(screen.getByRole("button", { name: "Toggle diff for a.ts" }));
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
});
