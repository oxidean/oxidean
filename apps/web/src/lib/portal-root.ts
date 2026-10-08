/**
 * Host element for every floating portal (menus, dialogs, selects, toasts).
 * Lives in AppShell's static body with `data-astro-transition-persist`, so
 * ClientRouter swaps carry the node — and every portal subtree inside it —
 * across navigations. Without it portals default to `document.body` children
 * that the swap drops while persisted chrome islands keep stale container
 * refs, leaving overlays rendering into detached DOM.
 */
export function portalRoot(): HTMLElement | undefined {
  if (typeof document === "undefined") return undefined;
  return document.getElementById("oxidean-portal-root") ?? undefined;
}
