import { DIFF_FILES_PAGE_SIZE } from "@/lib/diff-budget";
import type { HighlightedDiffLine } from "@/lib/highlight-diff";
import { useIncrementalList } from "@/lib/incremental-list.tsrx";
import { DiffPatch } from "@/components/repo/diff-patch";

type DiffFileEntry = {
  path: string;
  status: string;
  patch: string;
};

/**
 * File-by-file diff list (issue #110): mounts the first
 * {@link DIFF_FILES_PAGE_SIZE} file blocks, then appends the next batch when
 * the tail sentinel nears the viewport or the user presses "Load more files".
 * Keeps initial DOM bounded on commits / compares touching many files.
 */
export function DiffFileList(props: {
  files: ReadonlyArray<DiffFileEntry>;
  highlightedByPath?: Record<string, HighlightedDiffLine[]>;
}) @{
  const list = useIncrementalList(props.files.length, DIFF_FILES_PAGE_SIZE, props.files);
  const shown = props.files.slice(0, list.count);

  <div className="flex flex-col gap-6">
    @for (const f of shown; key f.path) {
      <DiffPatch
        path={f.path as string}
        status={f.status as string}
        patch={f.patch as string}
        initialRows={props.highlightedByPath?.[f.path] ?? null}
      />;
    }
    @if (list.remaining > 0) {
      <div ref={list.sentinelRef} className="flex justify-center py-2">
        <button
          type="button"
          className="rounded-md border border-border px-3 py-1.5 text-[14px] text-foreground hover:bg-muted"
          onClick={() => list.showMore()}
        >
          {`Load more files (${list.remaining} remaining)` as string}
        </button>
      </div>;
    }
  </div>;
}
