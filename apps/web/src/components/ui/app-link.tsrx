import type { ComponentProps } from "octane";
import { cn } from "@/lib/utils";
import { usePathname } from "@/lib/pathname.tsrx";

type AppLinkProps = ComponentProps<"a"> & {
  /**
   * Document prefetch strategy — mapped to Astro's `data-astro-prefetch`
   * (`intent`→hover, `render`→load). `/api/` links always opt out.
   */
  preload?: "intent" | "viewport" | "render" | false;
  /**
   * Active-state control. `{ exact: true }` marks the link active only on an
   * exact path match; otherwise a path-prefix match marks it active.
   */
  activeOptions?: { exact?: boolean; includeSearch?: boolean; includeHash?: boolean };
  /** Class applied while the link is active (router-era `activeProps`). */
  activeProps?: { className?: string };
};

const PRELOAD_TO_PREFETCH = {
  intent: "hover",
  viewport: "viewport",
  render: "load",
} as const;

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
  preload,
  activeOptions,
  activeProps,
  children,
  ...rest
}: AppLinkProps) @{
  // Router-era active-state parity: callers pass activeOptions on nav tabs.
  // `usePathname` keeps the active flag correct inside `transition:persist`
  // islands (header/footer) that never remount across ClientRouter swaps.
  const pathname = usePathname();
  const wantsActive = !!(activeOptions || activeProps);
  let active = false;
  if (wantsActive && typeof href === "string") {
    const target = href.split(/[?#]/)[0]!;
    active = activeOptions?.exact
      ? pathname === target
      : pathname === target || pathname.startsWith(target + "/");
  }
  const mergedClass =
    active && activeProps?.className ? cn(rest.className, activeProps.className) : rest.className;
  const prefetch =
    preload === false
      ? "false"
      : typeof href === "string" && (href === "/api" || href.startsWith("/api/"))
        ? "false"
        : preload
          ? PRELOAD_TO_PREFETCH[preload]
          : undefined;
  // raw-anchor-ok — AppLink IS the sanctioned internal-link primitive.
  <a
    href={href}
    {...rest}
    className={mergedClass}
    {...(prefetch !== undefined ? { "data-astro-prefetch": prefetch } : {})}
    {...(active ? { "data-status": "active", "aria-current": "page" } : {})}
  >
    {children}
  </a>;
}
