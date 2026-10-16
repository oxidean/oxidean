import { useSyncExternalStore } from "octane";

function subscribe(onChange: () => void): () => void {
  const handler = () => onChange();
  window.addEventListener("popstate", handler);
  window.addEventListener("astro:after-swap", handler);
  return () => {
    window.removeEventListener("popstate", handler);
    window.removeEventListener("astro:after-swap", handler);
  };
}

/**
 * Reactive `location.pathname` — tracks Astro ClientRouter swaps and history
 * navigation. Lives in a `.tsrx` file so the compiler injects hook slots.
 *
 * The SSR snapshot returns `"/"` — the value every page was prerendered with —
 * so hydration output matches the static HTML, and Octane's post-hydration
 * store sync is what applies the real path (hydration attribute diffs are
 * never patched — this converge render is what corrects SSR-baked
 * `aria-current` / active-link markup). Any consumer whose rendered DOM or
 * query keys depend on the value MUST use this variant: with a live snapshot,
 * a warm query cache on soft-nav hydration would render resolved data against
 * the SSR'd skeleton and roll the island back.
 */
export function usePathname(): string {
  return useSyncExternalStore(
    subscribe,
    () => (typeof window !== "undefined" ? window.location.pathname : "/"),
    () => "/",
  );
}

/**
 * Live `location.pathname` even during hydration — the store-sync snapshot
 * matches the live read, so no post-hydration converge render fires for this
 * subscriber. Safe ONLY where the rendered DOM doesn't depend on the value at
 * hydration time: effect dependencies (redirects, menu close) and content
 * never server-rendered (portals, lazily-mounted overlays). Audit a callsite
 * before migrating — see `usePathname` for why the default stays divergent.
 */
export function useLivePathname(): string {
  return useSyncExternalStore(
    subscribe,
    () => (typeof window !== "undefined" ? window.location.pathname : "/"),
    () => (typeof window === "undefined" ? "/" : window.location.pathname),
  );
}

/** Reactive `location.search` (with `?`) — post-router `useSearch` sibling. */
export function useSearchString(): string {
  return useSyncExternalStore(
    subscribe,
    () => (typeof window !== "undefined" ? window.location.search : ""),
    () => "",
  );
}
