import { useSyncExternalStore } from "octane";

function subscribe(onChange: () => void): () => void {
  const handler = () => onChange();
  window.addEventListener("popstate", handler);
  window.addEventListener("astro:after-swap", handler);
  window.addEventListener("astro:page-load", handler);
  return () => {
    window.removeEventListener("popstate", handler);
    window.removeEventListener("astro:after-swap", handler);
    window.removeEventListener("astro:page-load", handler);
  };
}

/**
 * Reactive `location.pathname` — tracks Astro ClientRouter swaps and history
 * navigation. Lives in a `.tsrx` file so the compiler injects hook slots.
 * Third arg is the SSR snapshot (prerender has no `window`).
 */
export function usePathname(): string {
  return useSyncExternalStore(
    subscribe,
    () => (typeof window !== "undefined" ? window.location.pathname : "/"),
    () => "/",
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
