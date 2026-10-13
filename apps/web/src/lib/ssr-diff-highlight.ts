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
 * Pre-highlight unified-diff patches for SSR (commit / compare). Rows come
 * back dual-theme (`light-dark()`), correct under either color scheme.
 * Soft caps live in {@link highlightDiffLines}.
 */
export async function ssrHighlightDiffFiles(
  files: ReadonlyArray<{ path: string; patch: string }>,
): Promise<SsrDiffHighlightResult> {
  if (files.length === 0) {
    return { byPath: {} };
  }

  const entries = await Promise.all(
    files.map(async (f) => {
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
