import { useRouter } from "@octanejs/tanstack-router";
import { useCallback } from "octane";

/**
 * Imperative client-side navigation for internal app hrefs — the programmatic
 * counterpart of <AppLink> (components/ui/app-link.tsrx).
 *
 * Inside the app router this calls `router.navigate({ href })`: the router
 * splits the href into `to`/`search`/`hash` and commits a client-side
 * transition (absolute external URLs still reload the document, which is the
 * correct behavior for them).
 *
 * Without a RouterProvider (standalone component tests that render pages bare)
 * it falls back to a full document navigation — same contract as AppLink's
 * plain <a> fallback, so existing `location.assign` spy assertions keep working.
 *
 * Deliberate full reloads (session-creating/destroying transitions, /api/
 * downloads, SSO start endpoints) should keep `window.location.assign` with a
 * `// raw-nav-ok` justification marker — see scripts/check-internal-nav.ts.
 */
export function useAppNavigate(): (href: string) => Promise<void> {
  const router = useRouter({ warn: false });
  // Memoized — callers list navigateApp in useEffect deps; a fresh closure per
  // render would re-fire effects that write to the Query cache → update loop.
  return useCallback(
    (href: string) => {
      if (router) {
        return router.navigate({ href });
      }
      // raw-nav-ok — no RouterProvider (tests); mirrors <AppLink>'s <a> fallback
      window.location.assign(href);
      return Promise.resolve();
    },
    [router],
  );
}
