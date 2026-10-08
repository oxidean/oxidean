import { useEffect } from "octane";
import { navigate } from "astro:transitions/client";
import { QueryClientProvider, useQuery } from "@octanejs/tanstack-query";
import { SiteFooter, SiteHeader } from "@/components/chrome";
import { AppToaster } from "@/components/ui/toaster";
import { VerifyBanner } from "@/components/verify-banner";
import { getQueryClient } from "@/lib/query-client";
import { usePathname } from "@/lib/pathname.tsrx";
import { resolveAppAccessRedirect } from "@/lib/ssr-auth";
import { installEarlyNavBridge } from "@/lib/early-nav";
import { authBootstrapQueryOptions, authSessionQueryOptions } from "@/lib/session-queries";

/**
 * App-access gate the old root loader owned: needs-setup → /setup,
 * must-change-credentials → /setup/credentials (the web tier handles the fast
 * anonymous→/login case for protected routes; this covers the rest). Runs in the
 * persisted chrome island, so it re-checks on every client navigation via
 * `usePathname()`.
 */
function AppAccessGate() @{
  const pathname = usePathname();
  const meQ = useQuery(authSessionQueryOptions());
  const bootQ = useQuery(authBootstrapQueryOptions());

  useEffect(() => {
    if (meQ.isPending || bootQ.isPending) return;
    const target = resolveAppAccessRedirect({
      pathname,
      needsSetup: Boolean(bootQ.data?.needs_setup),
      mustChangeCredentials: Boolean(meQ.data?.must_change_credentials),
    });
    if (target) window.location.assign(target);
  });

  // Register SPA navigate for taps held by the early-nav boot listener (the
  // head script only resolves them once this bridge exists, which implies the
  // module graph — ClientRouter included — is up and safe to swap).
  useEffect(() => installEarlyNavBridge(({ href }) => navigate(href)), []);

  <></>;
}

/**
 * Persistent chrome island — mounts once and survives view-transition
 * navigations (`transition:persist` in AppShell). Owns the shared QueryClient
 * provider, the access gate, header, verify banner, and toasts.
 */
export function ChromeTop() @{
  <QueryClientProvider client={getQueryClient()}>
    <AppAccessGate />
    <AppToaster>
      <>
        <SiteHeader />
        <VerifyBanner />
      </>
    </AppToaster>
  </QueryClientProvider>;
}

/** Persistent footer island — below the page slot in AppShell. */
export function ChromeBottom() @{
  <QueryClientProvider client={getQueryClient()}>
    <SiteFooter />
  </QueryClientProvider>;
}

/**
 * Per-page content island — wraps route content with the same singleton
 * QueryClient (module-level, so the cache is shared with the chrome islands and
 * survives navigations even though this island remounts).
 */
export function AppPage(props: { children?: unknown }) @{
  <QueryClientProvider client={getQueryClient()}>
    <main className="oxidean-main flex-1">{props.children}</main>
  </QueryClientProvider>;
}
