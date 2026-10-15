import { DIFF_LARGE_PATCH_CHARS, SSR_DIFF_HIGHLIGHT_MAX_FILES } from "@/lib/diff-budget";
import {
  highlightDiffLines,
  plainHighlightedDiffLines,
  type HighlightedDiffLine,
} from "@/lib/highlight-diff";
import { parseUnifiedDiffLines } from "@/lib/parse-unified-diff";

export type SsrDiffHighlightResult = {
  /** path → highlighted rows (soft-cap / plaintext stay escaped plain). */
  byPath: Record<string, HighlightedDiffLine[]>;
};

/**
 * Pre-highlight unified-diff patches for the first paint (commit / compare).
 * Rows come back dual-theme (`light-dark()`), correct under either color
 * scheme; soft caps live in {@link highlightDiffLines}.
 *
 * Issue #110: only the first few *small* files are highlighted eagerly — the
 * pass is a synchronous Shiki burst per file, and running it over every file
 * in a large diff stalls the main thread long enough to crash mobile
 * clients. Files past the budget (or above the large-diff gate) highlight
 * lazily inside `DiffPatch` once they scroll near the viewport.
 */
export async function ssrHighlightDiffFiles(
  files: ReadonlyArray<{ path: string; patch: string }>,
  maxFiles: number = SSR_DIFF_HIGHLIGHT_MAX_FILES,
): Promise<SsrDiffHighlightResult> {
  if (files.length === 0 || maxFiles <= 0) {
    return { byPath: {} };
  }

  const eager = files
    .filter((f) => f.patch.length > 0 && f.patch.length <= DIFF_LARGE_PATCH_CHARS)
    .slice(0, maxFiles);

  const entries = await Promise.all(
    eager.map(async (f) => {
      const lines = parseUnifiedDiffLines(f.patch);
      try {
        const rows = await highlightDiffLines(lines, { path: f.path });
        return [f.path, rows] as const;
      } catch {
        return [f.path, plainHighlightedDiffLines(lines)] as const;
      }
    }),
  );

  return { byPath: Object.fromEntries(entries) };
}
