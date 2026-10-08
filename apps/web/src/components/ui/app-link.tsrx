import type { ComponentProps } from "octane";
import { cn } from "@/lib/utils";

type AppLinkProps = ComponentProps<"a"> & {
  /** No-op under Astro MPA — kept for call-site compatibility during the port. */
  preload?: "intent" | "viewport" | "render" | false;
  /**
   * Active-state control. `{ exact: true }` marks the link active only on an
   * exact path match; otherwise a path-prefix match marks it active.
   */
  activeOptions?: { exact?: boolean; includeSearch?: boolean; includeHash?: boolean };
  /** Class applied while the link is active (router-era `activeProps`). */
  activeProps?: { className?: string };
};

/**
 * Internal navigation link — a plain anchor now that Astro's ClientRouter
 * drives view-transition navigations off document-level click delegation
 * (every same-origin `<a>` navigates with a morph + transition).
 *
 * Keep a plain `<a>` (annotated `raw-anchor-ok`) for external URLs,
 * downloads, `#` same-page anchors, and `/api/` endpoints.
 */
export function AppLink({
  href,
  preload: _preload,
  activeOptions,
  activeProps,
  children,
  ...rest
}: AppLinkProps) @{
  // Router-era active-state parity: callers pass activeOptions on nav tabs.
  const wantsActive = !!(activeOptions || activeProps);
  let active = false;
  if (wantsActive && typeof href === "string" && typeof window !== "undefined") {
    const path = window.location.pathname;
    const target = href.split(/[?#]/)[0]!;
    active = activeOptions?.exact
      ? path === target
      : path === target || path.startsWith(target + "/");
  }
  const mergedClass =
    active && activeProps?.className ? cn(rest.className, activeProps.className) : rest.className;
  // raw-anchor-ok — AppLink IS the sanctioned internal-link primitive.
  <a
    href={href}
    {...rest}
    className={mergedClass}
    {...(active ? { "data-status": "active", "aria-current": "page" } : {})}
  >
    {children}
  </a>;
}
