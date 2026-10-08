import type { HighlightTheme } from "@/lib/highlight";
import {
  highlightDiffLines,
  plainHighlightedDiffLines,
  type HighlightedDiffLine,
} from "@/lib/highlight-diff";
import { parseUnifiedDiffLines } from "@/lib/parse-unified-diff";
import { clientHighlightTheme } from "@/lib/ssr-auth";

export type SsrDiffHighlightResult = {
  theme: HighlightTheme | null;
  /** path → highlighted rows (soft-cap / plaintext stay escaped plain). */
  byPath: Record<string, HighlightedDiffLine[]>;
};

/**
 * Pre-highlight unified-diff patches for SSR (commit / compare), matching blob
 * viewer’s `resolveSsrHighlightTheme` + Shiki path. Soft caps live in
 * {@link highlightDiffLines}.
 */
export async function ssrHighlightDiffFiles(
  files: ReadonlyArray<{ path: string; patch: string }>,
): Promise<SsrDiffHighlightResult> {
  if (files.length === 0) {
    return { theme: null, byPath: {} };
  }

  let theme: HighlightTheme;
  try {
    theme = clientHighlightTheme();
  } catch {
    return { theme: null, byPath: {} };
  }

  const entries = await Promise.all(
    files.map(async (f) => {
      const lines = parseUnifiedDiffLines(f.patch);
      try {
        const rows = await highlightDiffLines(lines, { path: f.path, theme });
        return [f.path, rows] as const;
      } catch {
        return [f.path, plainHighlightedDiffLines(lines)] as const;
      }
    }),
  );

  return { theme, byPath: Object.fromEntries(entries) };
}
