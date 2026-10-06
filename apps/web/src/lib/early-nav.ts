/**
 * Early-navigation bridge (issue #111).
 *
 * Octane wires component `onClick` handlers lazily: each element gets its
 * delegated `$$<event>` slot when hydration reaches it. Until then a tap on
 * an internal `<a href="/...">` falls through to native navigation — a full
 * document reload that throws the page back into the same multi-second
 * hydration window. On mobile devices hydration is slow enough that users
 * stay trapped in that reload loop: every tap reloads the page again, and
 * taps that land while a reload is in flight appear completely dead.
 *
 * Two cooperating halves:
 *
 * 1. `EARLY_NAV_BOOT_SCRIPT` — an inline `<head>` script emitted by the SSR
 *    shell, so the capture listener exists from document parse (before any
 *    module loads). Clicks on internal anchors that hydration has not wired
 *    yet (`$$click` absent) are prevented and held as a pending tap. A ticker
 *    then resolves the pending tap the safest way available:
 *
 *    - once Octane binds the anchor's `$$click` slot, the tap is replayed as
 *      a synthetic `click()` on the real element — identical to a post-
 *      hydration tap, routing through the router's own `handleClick`;
 *    - once the app root has hydrated (`#__app` gained its delegation
 *      `onclick` marker) but the anchor never wired, the tap is SPA-
 *      navigated via `window.__oxideanEarlyNav` (registered by
 *      `installEarlyNavBridge`) — hydration is finished at that point, so
 *      `router.navigate` can no longer race the initial mount;
 *    - if hydration never runs, a bounded timer falls back to the native
 *      href navigation the anchor would have performed anyway.
 *
 *    The pending tap is never resolved by calling `router.navigate` while
 *    hydration could still be in progress — committing a route change mid-
 *    hydration corrupts the render (Octane "Something went wrong" boundary).
 * 2. `installEarlyNavBridge` — called when the client router is created;
 *    registers the SPA navigate handler the boot ticker uses for
 *    hydrated-but-never-wired anchors. In a non-SSR mount (component tests)
 *    where the boot script never ran, it attaches the same capture listener
 *    directly — with no hydration pending there, `navigate` is immediate.
 *
 * Once Octane binds an anchor's `$$click` slot the router's own `handleClick`
 * owns the event and this bridge steps aside.
 */

/** Octane delegated-event slot keys — presence means hydration wired the element. */
const CLICK_SLOT = "$$click";
const CAPTURE_CLICK_SLOT = "$$capture:click";

/** Id of the SSR mount container (`<div id="__app">` in the app shell). */
const ROOT_CONTAINER_ID = "__app";

/** Upper bound on holding a pre-hydration tap before falling back to native nav. */
const PENDING_MAX_MS = 4_000;
/** Poll interval for the pending-tap resolver. */
const RESOLVE_TICK_MS = 40;

declare global {
  interface Window {
    __oxideanEarlyNav?: ((href: string) => void) | null;
    __oxideanEarlyNavReady?: (nav: (href: string) => void) => void;
  }
}

export type EarlyNavHandler = (opts: { href: string }) => unknown;

/**
 * Resolve the `<a href>` this click should be bridged through, or `null` when
 * the click must keep native semantics:
 *
 * - target isn't inside an `<a href>`;
 * - the anchor already owns a delegated click slot (hydrated router link or
 *   any component `onClick`);
 * - `target` other than `_self`, `download`, or explicit opt-out
 *   (`data-no-early-nav`);
 * - non-route paths: external/protocol-relative URLs and same-origin `/api/*`
 *   endpoints (asset downloads, RPC) that are not router routes.
 */
export function earlyNavAnchorForTarget(target: EventTarget | null): HTMLAnchorElement | null {
  if (!(target instanceof Element)) return null;
  const anchor = target.closest("a[href]");
  if (!(anchor instanceof HTMLAnchorElement)) return null;
  const bound = anchor as unknown as Record<string, unknown>;
  if (bound[CLICK_SLOT] != null || bound[CAPTURE_CLICK_SLOT] != null) return null;
  const href = anchor.getAttribute("href");
  if (!href || !href.startsWith("/") || href.startsWith("//")) return null;
  if (href === "/api" || href.startsWith("/api/")) return null;
  const targetAttr = anchor.getAttribute("target");
  if (targetAttr != null && targetAttr !== "_self") return null;
  if (anchor.hasAttribute("download") || anchor.hasAttribute("data-no-early-nav")) {
    return null;
  }
  return anchor;
}

/** Same predicate as {@link earlyNavAnchorForTarget}, returning the href. */
export function earlyNavHrefForTarget(target: EventTarget | null): string | undefined {
  return earlyNavAnchorForTarget(target)?.getAttribute("href") ?? undefined;
}

/**
 * Parse-time boot listener, serialized into `RootShell`'s `<Head>` (same
 * channel as THEME_BOOT_SCRIPT). The predicate is embedded from
 * `earlyNavAnchorForTarget`'s own source so the two halves can't drift.
 */
export const EARLY_NAV_BOOT_SCRIPT = `(function(){var CLICK_SLOT=${JSON.stringify(CLICK_SLOT)},CAPTURE_CLICK_SLOT=${JSON.stringify(CAPTURE_CLICK_SLOT)},ROOT_ID=${JSON.stringify(ROOT_CONTAINER_ID)},PENDING_MAX=${PENDING_MAX_MS},TICK=${RESOLVE_TICK_MS};var earlyNavAnchorForTarget=${earlyNavAnchorForTarget.toString()};var pending=null,timer=0;window.__oxideanEarlyNav=null;window.__oxideanEarlyNavReady=function(nav){window.__oxideanEarlyNav=nav;};function wired(el){return el[CLICK_SLOT]!=null||el[CAPTURE_CLICK_SLOT]!=null;}function appHydrated(){var root=document.getElementById(ROOT_ID);return root!=null&&root.onclick!=null;}function fallback(href){try{window.location.assign(href);}catch(e){}}function resolve(){if(!pending)return;var el=pending.el,href=pending.href;if(!el.isConnected){pending=null;fallback(href);return;}if(wired(el)){pending=null;try{el.click();}catch(e){fallback(href);}return;}if(appHydrated()){pending=null;var nav=window.__oxideanEarlyNav;if(nav){try{nav(href);return;}catch(e){}}fallback(href);return;}if(Date.now()-pending.ts>=PENDING_MAX){pending=null;fallback(href);}}document.addEventListener("click",function(event){if(event.defaultPrevented||event.button!==0||event.metaKey||event.altKey||event.ctrlKey||event.shiftKey)return;var anchor=earlyNavAnchorForTarget(event.target);if(anchor===null)return;event.preventDefault();pending={el:anchor,href:anchor.getAttribute("href"),ts:Date.now()};resolve();if(pending&&!timer){timer=setInterval(function(){resolve();if(!pending&&timer){clearInterval(timer);timer=0;}},TICK);}},true);})();`;

/**
 * Wire the router's navigate into the bridge (called once the client router
 * exists). `navigate` receives `{ href }` — the same shape router.navigate
 * takes. Returns a detach function.
 *
 * The registered handler only fires for taps on anchors that stayed unwired
 * *after* hydration completed — the boot ticker never calls it while
 * hydration can still be in progress, because committing a route change
 * mid-hydration corrupts the render.
 */
export function installEarlyNavBridge(navigate: EarlyNavHandler): () => void {
  if (typeof document === "undefined") return () => {};
  const nav = (href: string) => {
    try {
      navigate({ href });
    } catch {
      window.location.assign(href);
    }
  };
  const ready = window.__oxideanEarlyNavReady;
  if (typeof ready === "function") {
    ready(nav);
    return () => {
      if (window.__oxideanEarlyNav === nav) window.__oxideanEarlyNav = null;
    };
  }
  // Non-SSR mount (component test harnesses): no boot listener — attach the
  // same capture listener directly.
  const onClick = (event: MouseEvent) => {
    if (
      event.defaultPrevented ||
      event.button !== 0 ||
      event.metaKey ||
      event.altKey ||
      event.ctrlKey ||
      event.shiftKey
    ) {
      return;
    }
    const href = earlyNavHrefForTarget(event.target);
    if (href === undefined) return;
    event.preventDefault();
    nav(href);
  };
  document.addEventListener("click", onClick, true);
  return () => document.removeEventListener("click", onClick, true);
}
