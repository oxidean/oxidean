import { Link, useRouter } from "@octanejs/tanstack-router";
import type { ComponentProps } from "octane";

type AppLinkProps = ComponentProps<"a"> & {
  /** Preload behavior; defaults to "intent" (hover/focus/touch). */
  preload?: "intent" | "viewport" | "render" | false;
};

/**
 * Internal navigation link — the shared replacement for raw `<a>` elements
 * pointing at app routes.
 * Inside the app router it renders a router-aware anchor: client-side
 * navigation, intent preloading, and active-state attributes while still
 * emitting a normal `href` for SSR / middle-click / screen readers.
 * Without a router context (standalone component tests) it falls back to a
 * plain `<a>`, so RouterProvider-free harnesses keep working.
 *
 * Keep a plain `<a>` (annotated `raw-anchor-ok`) for external URLs,
 * downloads, `#` same-page anchors, and `/api/` endpoints that are not
 * router routes.
 */
export function AppLink({ href, preload, children, ...rest }: AppLinkProps) @{
  const router = useRouter({ warn: false });
  <>
    @if (router) {
      <Link href={href as string} preload={preload ?? "intent"} {...rest}>
        {children}
      </Link>;
    } @else {
      {
        /* raw-anchor-ok — intentional fallback when no RouterProvider (tests) */
      }
      <a href={href} {...rest}>
        {children}
      </a>;
    }
  </>;
}
