import { createRouter } from "@octanejs/tanstack-router";
import { routeTree } from "./routeTree.gen";

export function getRouter() {
  return createRouter({
    routeTree,
    // Typed route tree is registered below — Link `to` / params infer from it.
    defaultPreload: "intent",
    defaultPreloadDelay: 50,
    // View Transitions stay off: the adapter defers the match commit to a
    // browser-scheduled callback (skipped callbacks can wedge pending commits),
    // the cross-fade flashes a light canvas behind dark pages, and the snapshot
    // swap visibly jumps the layout.
    defaultViewTransition: false,
    // Restore scroll on route changes; same-document hash jumps stay native.
    scrollRestoration: true,
  });
}

declare module "@octanejs/tanstack-router" {
  interface Register {
    router: ReturnType<typeof getRouter>;
  }
}
