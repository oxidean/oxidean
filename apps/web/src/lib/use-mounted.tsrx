import { useEffect, useState } from "octane";

/**
 * `false` during SSR and hydration, `true` after the first mounted effect.
 * Use to gate DOM branches that depend on client-warm state (e.g. a
 * QueryClient cache populated before hydration): the hydration render then
 * always matches the prerendered HTML, so Octane never sees the structural
 * mismatch that would roll back and remount the whole island. Lives in a
 * `.tsrx` file so the compiler injects hook slots.
 */
export function useMounted(): boolean {
  const [mounted, setMounted] = useState(false);
  useEffect(() => setMounted(true), []);
  return mounted;
}
