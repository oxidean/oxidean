import { useEffect, useRef, useState } from "octane";

/**
 * Incremental list-reveal state (issue #110): mount `pageSize` items, then
 * grow in batches when the sentinel scrolls into view or the user presses
 * "Load more". Keeps initial DOM bounded on long diff / blame lists.
 *
 * `resetKey` re-arms the count when the underlying collection identity
 * changes (e.g. navigating between blame paths without a remount).
 *
 * In happy-dom (integration tests) there is no IntersectionObserver — the
 * explicit `showMore` path is the only grow trigger there.
 */
export function useIncrementalList(
  total: number,
  pageSize: number,
  resetKey?: unknown,
): {
  count: number;
  remaining: number;
  sentinelRef: { current: HTMLDivElement | null };
  showMore: () => void;
} {
  const [count, setCount] = useState(pageSize);
  const sentinelRef = useRef<HTMLDivElement | null>(null);

  const shown = Math.min(count, total);
  const remaining = Math.max(total - shown, 0);

  function showMore() {
    setCount((c) => c + pageSize);
  }

  // Re-arm the batch when the collection identity changes (new file / ref).
  useEffect(() => {
    setCount(pageSize);
  }, [resetKey]);

  // Auto-append the next batch once the tail sentinel nears the viewport.
  useEffect(() => {
    if (remaining <= 0) return;
    const el = sentinelRef.current;
    if (!el) return;
    if (typeof IntersectionObserver === "undefined") return;
    const io = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (entry.isIntersecting) showMore();
        }
      },
      { rootMargin: "600px 0px" },
    );
    io.observe(el);
    return () => io.disconnect();
  }, [remaining, pageSize]);

  return { count: shown, remaining, sentinelRef, showMore };
}
