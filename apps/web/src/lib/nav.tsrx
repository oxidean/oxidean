import { useEffect } from "octane";
import { navigate } from "astro:transitions/client";

/**
 * navigate() when `href` is non-null — the post-router replacement for
 * `throw redirect(...)` in loaders. Compiled `.tsrx` so the effect slot is
 * compiler-assigned. `replace` matches redirect semantics: the pre-redirect
 * URL shouldn't linger in history (back would just redirect again).
 */
export function usePageRedirect(href: string | null): void {
  useEffect(() => {
    if (href) void navigate(href, { history: "replace" });
  }, [href]);
}
