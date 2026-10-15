/**
 * DiffFileList — issue #110: incremental file-list mounting for
 * commit / compare diffs (bounded initial DOM on many-file diffs).
 */
import { cleanup, fireEvent, render, screen, waitFor } from "@octanejs/testing-library";
import { afterEach, describe, expect, it } from "vitest";
import { DIFF_FILES_PAGE_SIZE } from "@/lib/diff-budget";
import { DiffFileList } from "./diff-file-list";

afterEach(cleanup);

function makeFiles(count: number) {
  return Array.from({ length: count }, (_, i) => ({
    path: `src/file-${i}.ts`,
    status: "modified",
    patch: `@@ -1 +1 @@\n-old-${i}\n+new-${i}`,
  }));
}

describe("DiffFileList", { timeout: 60_000 }, () => {
  it("mounts only the first page of files, then grows on Load more", async () => {
    const files = makeFiles(DIFF_FILES_PAGE_SIZE + 3);

    render(DiffFileList, { props: { files } });

    // First page mounted; file past the page size is not in the DOM.
    expect(await screen.findByText("src/file-0.ts")).toBeInTheDocument();
    expect(screen.getByText(`src/file-${DIFF_FILES_PAGE_SIZE - 1}.ts`)).toBeInTheDocument();
    expect(screen.queryByText(`src/file-${DIFF_FILES_PAGE_SIZE}.ts`)).toBeNull();

    const more = screen.getByRole("button", { name: /Load more files/ });
    expect(more.textContent).toContain("3 remaining");

    fireEvent.click(more);

    await waitFor(() => {
      expect(screen.getByText(`src/file-${DIFF_FILES_PAGE_SIZE + 2}.ts`)).toBeInTheDocument();
    });
    // All files mounted — the Load more control is gone.
    expect(screen.queryByRole("button", { name: /Load more files/ })).toBeNull();
  });

  it("mounts every file without a Load more control under the page size", async () => {
    const files = makeFiles(3);

    render(DiffFileList, { props: { files } });

    expect(await screen.findByText("src/file-2.ts")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Load more files/ })).toBeNull();
  });
});
