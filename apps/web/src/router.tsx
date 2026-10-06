import { createRouter } from "@octanejs/tanstack-router";
import { installEarlyNavBridge } from "./lib/early-nav";
import { routeTree } from "./routeTree.gen";

let earlyNavBridgeInstalled = false;

export function getRouter() {
  const router = createRouter({
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
  // #111: on mobile the hydration window lasts seconds; until Octane binds
  // each anchor's `$$click` slot every tap is a native reload that restarts
  // hydration (the reload-loop that makes navigation feel dead). Bridge
  // unwired internal clicks to router.navigate from bundle-eval onward.
  if (!earlyNavBridgeInstalled) {
    earlyNavBridgeInstalled = true;
    installEarlyNavBridge((opts) => router.navigate(opts));
  }
  return router;
}

declare module "@octanejs/tanstack-router" {
  interface Register {
    router: ReturnType<typeof getRouter>;
  }
}
