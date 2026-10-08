import { useEffect } from "octane";
import { navigate } from "astro:transitions/client";

/**
 * navigate() when `href` is non-null — the post-router replacement for
 * `throw redirect(...)` in loaders. Compiled `.tsrx` so the effect slot is
 * compiler-assigned.
 */
export function usePageRedirect(href: string | null): void {
  useEffect(() => {
    if (href) void navigate(href);
  }, [href]);
}
