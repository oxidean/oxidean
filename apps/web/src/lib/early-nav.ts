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
 *    - once the app registers its navigate handler (`window.__oxideanEarlyNav`
 *      set by `installEarlyNavBridge` — under Astro that runs inside an
 *      island effect, which means module JS is up and the ClientRouter swap
 *      path is safe), the tap goes through `astro:transitions` navigate;
 *    - if neither ever arrives, a bounded timer falls back to the native
 *      href navigation the anchor would have performed anyway.
 *
 *    The script also swallows the ClientRouter's benign transition aborts:
 *    Astro attaches `.finally()` (not `.catch()`) to `viewTransition.ready` /
 *    `.finished`, so a transition aborted by `skipTransition()` or a viewport
 *    resize (mobile URL-bar collapse) surfaces as an unhandled rejection even
 *    though the DOM swap already committed. The filter matches only that
 *    DOMException — genuine render errors still reject loudly.
 * 2. `installEarlyNavBridge` — called from the persistent chrome island;
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
 * Parse-time boot listener, serialized into `AppShell`'s `<head>` (same
 * channel as THEME_BOOT_SCRIPT). The predicate is embedded from
 * `earlyNavAnchorForTarget`'s own source so the two halves can't drift.
 *
 * Ready check: `window.__oxideanEarlyNav` set by `installEarlyNavBridge` —
 * it runs in an island effect, so its presence means module JS is up and
 * `navigate` is safe. `appHydrated()` from the Octane-root era is gone:
 * the `#__app` container no longer exists.
 */
export const EARLY_NAV_BOOT_SCRIPT = `(function(){var CLICK_SLOT=${JSON.stringify(CLICK_SLOT)},CAPTURE_CLICK_SLOT=${JSON.stringify(CAPTURE_CLICK_SLOT)},PENDING_MAX=${PENDING_MAX_MS},TICK=${RESOLVE_TICK_MS};var earlyNavAnchorForTarget=${earlyNavAnchorForTarget.toString()};var pending=null,timer=0;window.__oxideanEarlyNav=null;window.__oxideanEarlyNavReady=function(nav){window.__oxideanEarlyNav=nav;};function wired(el){return el[CLICK_SLOT]!=null||el[CAPTURE_CLICK_SLOT]!=null;}function fallback(href){try{window.location.assign(href);}catch(e){}}function resolve(){if(!pending)return;var el=pending.el,href=pending.href;if(!el.isConnected){pending=null;fallback(href);return;}if(wired(el)){pending=null;try{el.click();}catch(e){fallback(href);}return;}var nav=window.__oxideanEarlyNav;if(nav){pending=null;try{nav(href);return;}catch(e){fallback(href);return;}}if(Date.now()-pending.ts>=PENDING_MAX){pending=null;fallback(href);}}document.addEventListener("click",function(event){if(event.defaultPrevented||event.button!==0||event.metaKey||event.altKey||event.ctrlKey||event.shiftKey)return;var anchor=earlyNavAnchorForTarget(event.target);if(anchor===null)return;event.preventDefault();pending={el:anchor,href:anchor.getAttribute("href"),ts:Date.now()};resolve();if(pending&&!timer){timer=setInterval(function(){resolve();if(!pending&&timer){clearInterval(timer);timer=0;}},TICK);}},true);window.addEventListener("unhandledrejection",function(event){var reason=event&&event.reason;if(reason&&reason.name==="InvalidStateError"&&/Transition was aborted/.test(String(reason.message||reason))){event.preventDefault();}});})();`;

/**
 * Wire the SPA navigate into the bridge (called from the persistent chrome
 * island's effect, so `__oxideanEarlyNav` being set implies module JS — and
 * the ClientRouter — is up). `navigate` receives `{ href }`; returns a
 * detach function.
 *
 * The registered handler only fires for taps on anchors that stayed unwired
 * after the bridge registered — the boot ticker never calls it before that,
 * because committing a route change while module JS is still loading can
 * corrupt the render.
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
