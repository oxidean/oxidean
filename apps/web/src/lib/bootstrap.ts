import { apiClient } from "@/lib/api-client";

/**
 * Progressive enhancement only — not the first-paint / security lock.
 * Shared root SSR loader (ssr-auth resolveAppAccessRedirect) owns D-09/D-10.
 * Kept for client navigations that skip a full document request.
 */
export async function redirectIfNeedsSetup(): Promise<boolean> {
  try {
    const status = await apiClient.auth.bootstrapStatus();
    if (status.ok && status.data.needs_setup) {
      if (window.location.pathname !== "/setup") {
        // raw-nav-ok — boot guard runs before/outside the router; full document
        // navigation to /setup is the intended behavior
        window.location.assign("/setup");
      }
      return true;
    }
  } catch {
    /* network — leave page alone */
  }
  return false;
}
