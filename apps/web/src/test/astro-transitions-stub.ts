/**
 * Vitest stub for `astro:transitions/client` — performs a real history
 * pushState + popstate dispatch so `usePathname`/`useSearchString` react the
 * same way they do after an Astro ClientRouter swap. Tests that need to spy on
 * navigation should `vi.mock("astro:transitions/client")` or assert on
 * `window.location` after `waitFor`.
 */
export async function navigate(href: string, _opts?: unknown): Promise<void> {
  window.history.pushState({}, "", href);
  window.dispatchEvent(new PopStateEvent("popstate"));
}
