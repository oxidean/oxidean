import { Link, useRouter } from "@octanejs/tanstack-router";
import type { ComponentProps } from "octane";

type AppLinkProps = ComponentProps<"a"> & {
  /** Preload behavior; defaults to "intent" (hover/focus/touch). */
  preload?: "intent" | "viewport" | "render" | false;
  /**
   * Router active-state control, forwarded to Link. Pass `{ exact: true }` for
   * same-path destinations that differ only in search/hash (e.g. tab bars) —
   * the default fuzzy match would otherwise mark every variant active.
   */
  activeOptions?: { exact?: boolean; includeSearch?: boolean; includeHash?: boolean };
};

/**
 * Split an app-internal href into the router-facing pieces (`to`, raw
 * `search`, `hash`) — the same decomposition `navigate({ href })` performs
 * internally in `buildAndCommitLocation`. `Link`'s rendered `href`, active
 * state, and preload target are computed by `buildLocation`, which reads
 * `to`/`search`/`hash` and ignores a raw `href` prop — so passing these
 * explicitly is what makes a fully-built path behave like a router link.
 *
 * Returns `undefined` for anything that is not an absolute-path internal
 * URL (relative paths, protocol-relative, absolute external URLs, bare
 * `#fragment`s, missing href) — those keep native anchor semantics.
 */
function splitInternalHref(
  href: string | undefined,
): { to: string; search: string; hash: string | undefined } | undefined {
  if (typeof href !== "string" || !href.startsWith("/") || href.startsWith("//")) {
    return undefined;
  }
  try {
    const url = new URL(href, "http://internal.invalid");
    return {
      to: url.pathname,
      search: url.search,
      hash: url.hash.slice(1) || undefined,
    };
  } catch {
    return undefined;
  }
}

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
export function AppLink({
  href,
  preload,
  activeOptions,
  children,
  ...rest
}: AppLinkProps) @{
  const router = useRouter({ warn: false });
  const dest = splitInternalHref(typeof href === "string" ? href : undefined);
  <>
    @if (router && dest) {
      <Link
        to={dest.to}
        search={router.options.parseSearch(dest.search) as Record<string, unknown>}
        hash={dest.hash}
        preload={preload ?? "intent"}
        activeOptions={activeOptions}
        {...rest}
      >
        {children}
      </Link>;
    } @else {
      {
        /* raw-anchor-ok — no RouterProvider (tests) or non-route href */
      }
      <a href={href} {...rest}>
        {children}
      </a>;
    }
  </>;
}
