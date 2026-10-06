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
 *    yet (`$$click` absent) are prevented and routed through
 *    `window.__oxideanEarlyNav` once the router registers it, or held in a
 *    closure-local pending slot until then. If the router never registers,
 *    a timer falls back to the native href navigation the anchor would have
 *    performed anyway.
 * 2. `installEarlyNavBridge` — called when the client router is created;
 *    registers the SPA navigate handler and flushes a queued tap. In a
 *    non-SSR mount (component tests) where the boot script never ran, it
 *    attaches the same listener directly.
 *
 * Once Octane binds an anchor's `$$click` slot the router's own `handleClick`
 * owns the event and this bridge steps aside.
 */

/** Octane delegated-event slot keys — presence means hydration wired the element. */
const CLICK_SLOT = "$$click";
const CAPTURE_CLICK_SLOT = "$$capture:click";

declare global {
  interface Window {
    __oxideanEarlyNav?: ((href: string) => void) | null;
    __oxideanEarlyNavReady?: (nav: (href: string) => void) => void;
  }
}

export type EarlyNavHandler = (opts: { href: string }) => unknown;

/**
 * Resolve the href this click should SPA-navigate to, or `undefined` when the
 * click must keep native semantics:
 *
 * - target isn't inside an `<a href>`;
 * - the anchor already owns a delegated click slot (hydrated router link or
 *   any component `onClick`);
 * - `target` other than `_self`, `download`, or explicit opt-out
 *   (`data-no-early-nav`);
 * - non-route paths: external/protocol-relative URLs and same-origin `/api/*`
 *   endpoints (asset downloads, RPC) that are not router routes.
 */
export function earlyNavHrefForTarget(target: EventTarget | null): string | undefined {
  if (!(target instanceof Element)) return undefined;
  const anchor = target.closest("a[href]");
  if (!(anchor instanceof HTMLAnchorElement)) return undefined;
  const bound = anchor as unknown as Record<string, unknown>;
  if (bound[CLICK_SLOT] != null || bound[CAPTURE_CLICK_SLOT] != null) return undefined;
  const href = anchor.getAttribute("href");
  if (!href || !href.startsWith("/") || href.startsWith("//")) return undefined;
  if (href === "/api" || href.startsWith("/api/")) return undefined;
  const targetAttr = anchor.getAttribute("target");
  if (targetAttr != null && targetAttr !== "_self") return undefined;
  if (anchor.hasAttribute("download") || anchor.hasAttribute("data-no-early-nav")) {
    return undefined;
  }
  return href;
}

/**
 * Parse-time boot listener, serialized into `RootShell`'s `<Head>` (same
 * channel as THEME_BOOT_SCRIPT). The predicate is embedded from
 * `earlyNavHrefForTarget`'s own source so the two halves can't drift.
 */
export const EARLY_NAV_BOOT_SCRIPT = `(function(){var CLICK_SLOT=${JSON.stringify(CLICK_SLOT)},CAPTURE_CLICK_SLOT=${JSON.stringify(CAPTURE_CLICK_SLOT)};var earlyNavHrefForTarget=${earlyNavHrefForTarget.toString()};var pending=null,timer=0;window.__oxideanEarlyNav=null;window.__oxideanEarlyNavReady=function(nav){window.__oxideanEarlyNav=nav;if(pending){var href=pending;pending=null;clearTimeout(timer);nav(href);}};document.addEventListener("click",function(event){if(event.defaultPrevented||event.button!==0||event.metaKey||event.altKey||event.ctrlKey||event.shiftKey)return;var href=earlyNavHrefForTarget(event.target);if(href===undefined)return;event.preventDefault();if(window.__oxideanEarlyNav){window.__oxideanEarlyNav(href);return;}pending=href;clearTimeout(timer);timer=setTimeout(function(){if(pending===href){pending=null;window.location.assign(href);}},4000);},true);})();`;

/**
 * Wire the router's navigate into the bridge (called once the client router
 * exists). `navigate` receives `{ href }` — the same shape router.navigate
 * takes. Returns a detach function.
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
