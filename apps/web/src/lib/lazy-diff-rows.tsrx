import { useEffect, useRef, useState } from "octane";
import {
  highlightDiffLines,
  highlightedDiffRowsEqual,
  plainHighlightedDiffLines,
  type HighlightedDiffLine,
} from "@/lib/highlight-diff";
import { parseUnifiedDiffLines } from "@/lib/parse-unified-diff";

/** Defer work to an idle slice when the platform supports it. */
function scheduleIdle(cb: () => void): () => void {
  if (typeof requestIdleCallback === "function") {
    const id = requestIdleCallback(cb, { timeout: 1500 });
    return () => cancelIdleCallback(id);
  }
  const t = setTimeout(cb, 0);
  return () => clearTimeout(t);
}

/**
 * Lazy diff rows (issue #110):
 *
 * - Plain (escaped) rows render immediately for cheap first paint — unless
 *   `deferPlainRows` (large gated diffs parse only once the user loads them).
 * - Shiki highlighting runs only once the block nears the viewport
 *   (IntersectionObserver) AND the body is open, inside an idle slice — never
 *   an eager multi-file tokenize burst on mount.
 * - `initialRows` (pre-highlighted loader rows) are taken verbatim.
 *
 * Attach `rootRef` to the file block's root element. Zero-layout environments
 * (happy-dom — its IntersectionObserver is inert) count as visible so tests
 * still exercise the highlight path.
 */
export function useLazyDiffRows(options: {
  patch: string;
  path: string;
  /** Whether the diff body is mounted (expanded and not gated). */
  open: boolean;
  /** Parse only on first open — for large patches gated behind "Load diff". */
  deferPlainRows?: boolean;
  initialRows?: HighlightedDiffLine[] | null;
}): { rows: HighlightedDiffLine[]; rootRef: { current: HTMLElement | null } } {
  const { patch, path, open } = options;
  const initialRows = options.initialRows ?? null;
  const [rows, setRows] = useState<HighlightedDiffLine[]>(() => {
    if (options.deferPlainRows) return [];
    if (initialRows && initialRows.length > 0) return initialRows;
    return plainHighlightedDiffLines(parseUnifiedDiffLines(patch));
  });
  const rootRef = useRef<HTMLElement | null>(null);
  const [nearViewport, setNearViewport] = useState(false);
  // Latch on the diff *source* (patch + path + provided rows), not a plain
  // boolean: a refetched patch for the same mounted file (e.g. PR files
  // refreshed after a push, keyed `key path`) must re-parse and re-highlight
  // instead of freezing on the first source forever.
  const doneSourceRef = useRef<{
    patch: string;
    path: string;
    initialRows: HighlightedDiffLine[] | null;
  } | null>(null);

  useEffect(() => {
    const el = rootRef.current;
    if (!el) return;
    if (typeof IntersectionObserver === "undefined") {
      setNearViewport(true);
      return;
    }
    // Zero-layout environments (happy-dom ships an inert IntersectionObserver
    // stub that never fires) report a zero rect — treat the block as visible
    // so the highlight path stays exercised there and never stalls.
    const rect = el.getBoundingClientRect();
    if (rect.width === 0 && rect.height === 0) {
      setNearViewport(true);
      return;
    }
    const io = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (entry.isIntersecting) {
            setNearViewport(true);
            io.disconnect();
          }
        }
      },
      { rootMargin: "800px 0px" },
    );
    io.observe(el);
    return () => io.disconnect();
  }, []);

  useEffect(() => {
    if (!open || !nearViewport) return;
    const done = doneSourceRef.current;
    if (done && done.patch === patch && done.path === path && done.initialRows === initialRows) {
      return;
    }
    let cancelled = false;

    // SSR / loader rows are dual-theme (`light-dark()`) — already correct for
    // either scheme; take them as-is and skip the re-highlight pass.
    if (initialRows && initialRows.length > 0) {
      doneSourceRef.current = { patch, path, initialRows };
      setRows((prev) => (highlightedDiffRowsEqual(prev, initialRows) ? prev : initialRows));
      return;
    }

    const lines = parseUnifiedDiffLines(patch);
    setRows((prev) => {
      const plain = plainHighlightedDiffLines(lines);
      return highlightedDiffRowsEqual(prev, plain) ? prev : plain;
    });

    async function run() {
      try {
        const next = await highlightDiffLines(lines, { path });
        if (cancelled) return;
        doneSourceRef.current = { patch, path, initialRows };
        setRows((prev) => (highlightedDiffRowsEqual(prev, next) ? prev : next));
      } catch {
        /* highlighter unavailable — plain rows already rendered */
      }
    }

    const unschedule = scheduleIdle(() => {
      void run();
    });

    return () => {
      cancelled = true;
      unschedule();
    };
  }, [open, nearViewport, patch, path, initialRows]);

  return { rows, rootRef };
}
