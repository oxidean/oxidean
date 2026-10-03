import { createRouter } from "@octanejs/tanstack-router";
import { routeTree } from "./routeTree.gen";

export function getRouter() {
  return createRouter({
    routeTree,
    // Typed route tree is registered below — Link `to` / params infer from it.
    defaultPreload: "intent",
    defaultPreloadDelay: 50,
    // Loader staleness matches the QueryClient default (lib/query-client.ts) —
    // revisiting a route within 30s serves its loader from cache instead of
    // re-fetching, so client-side nav doesn't blank-and-refill.
    defaultStaleTime: 30_000,
    // Smooth cross-route transitions via the View Transitions API (Octane adapter
    // commits match updates inside startViewTransition).
    defaultViewTransition: true,
    // Restore scroll on route changes; same-document hash jumps stay native.
    scrollRestoration: true,
  });
}

declare module "@octanejs/tanstack-router" {
  interface Register {
    router: ReturnType<typeof getRouter>;
  }
}
