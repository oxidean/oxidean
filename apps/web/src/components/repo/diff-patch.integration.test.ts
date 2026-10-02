import { cleanup, render, screen, waitFor } from "@octanejs/testing-library";
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
    expect(add?.textContent).toBe("+new");
    expect(del?.textContent).toBe("-old");
    expect(hunk?.textContent).toBe("@@ -1,2 +1,2 @@");
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

    await waitFor(() => {
      const add = document.querySelector('[data-diff-kind="add"]');
      expect(add?.querySelector('span[style*="color"]')).toBeTruthy();
    });
  });

  it("renders SSR initialRows with token spans on first paint", async () => {
    const patch = ["@@ -1 +1 @@", "-const a = 1;", "+const b = 2;"].join("\n");
    const initialRows = await highlightDiffLines(parseUnifiedDiffLines(patch), {
      path: "util.ts",
      theme: "oxidean-light",
    });

    document.documentElement.classList.remove("dark");

    render(DiffPatch, {
      props: {
        path: "util.ts",
        status: "modified",
        patch,
        initialRows,
        highlightTheme: "oxidean-light",
      },
    });

    expect(await screen.findByText("util.ts")).toBeInTheDocument();
    const add = document.querySelector('[data-diff-kind="add"]');
    const firstHtml = add?.innerHTML ?? "";
    expect(add?.querySelector('span[style*="color"]')).toBeTruthy();

    // Hydrate must keep SSR HTML when document theme matches (no flicker).
    await new Promise((r) => setTimeout(r, 50));
    expect(document.querySelector('[data-diff-kind="add"]')?.innerHTML).toBe(firstHtml);
  });
});
