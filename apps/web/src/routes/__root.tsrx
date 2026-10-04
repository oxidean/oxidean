import {
  Body,
  Head,
  HeadContent,
  Html,
  Outlet,
  Scripts,
  createRootRoute,
  redirect,
  useLoaderData,
} from "@octanejs/tanstack-router";
import {
  dehydrate,
  HydrationBoundary,
  QueryClientProvider,
  type DehydratedState,
} from "@octanejs/tanstack-query";
import type { BootstrapStatus, ProviderConfigPublic, UserPublic } from "@oxidean/api-client";
import { SiteFooter, SiteHeader } from "@/components/chrome";
import { AppToaster } from "@/components/ui/toaster";
import { VerifyBanner } from "@/components/verify-banner";
import { createAppQueryClient, getQueryClient } from "@/lib/query-client";
import {
  authBootstrapQueryKey,
  authProviderConfigQueryKey,
  authMeQueryKey,
} from "@/lib/session-queries";
import {
  fetchBootstrapStatus,
  fetchProviderConfig,
  fetchSessionMe,
  resolveAppAccessRedirect,
  ssrResolvedTheme,
} from "@/lib/ssr-auth";
import {
  THEME_BOOT_SCRIPT,
  FONT_WARM_SCRIPT,
  resolveThemeForSsr,
  themePreferenceFromCookieHeader,
  resolvedColorSchemeFromCookieHeader,
} from "@/lib/theme";
import { buildSwRegisterScript } from "@/lib/sw-register";
// The stylesheet href must be the file the client build actually emitted:
// a bare side-effect import goes through the positionally-keyed managed
// head-asset pipeline (removed/re-inserted per route, flashing unstyled
// content) and a direct `?url` import resolves per-build, so the SSR bundle
// can point at a filename that was never deployed. The plugin hands the SSR
// bundle the emitted filename and leaves client/dev on `?url`.
import appStylesHref from "virtual:oxidean-app-styles-href";

// Build id is Vite-defined at compile time (commit SHA / local stamp). Script
// body stays free of user input (T-03-19).
const SW_REGISTER_SCRIPT = buildSwRegisterScript(
  (import.meta.env.VITE_OXIDEAN_SW_BUILD as string | undefined) ?? "dev",
);

const PROVIDER_CONFIG_LOCKED: ProviderConfigPublic = {
  mode: "local",
  allow_signup: false,
};

export type RootLoaderData = {
  dehydratedState: DehydratedState;
  /** Resolved theme for the SSR `<html class>` — keeps reloads flash-free. */
  ssrTheme: "light" | "dark";
};

/**
 * SSR gate + seed auth chrome queries (me / bootstrap / providerConfig) so
 * SiteHeader paints without a client-only skeleton flash.
 */
export const Route = createRootRoute({
  component: RootComponent,
  shellComponent: RootShell,
  head: ({ loaderData }) => ({
    meta: [
      { title: "Oxidean" },
      // Pinned per request: the UA canvas (pre-CSS gap + scrollbars/controls)
      // matches the resolved theme, so document loads don't flash light.
      {
        name: "color-scheme",
        content: (loaderData as RootLoaderData | undefined)?.ssrTheme ?? "light",
      },
    ],
  }),
  loader: async ({ location }): Promise<RootLoaderData> => {
    const qc = createAppQueryClient();

    let needsSetup = false;
    let boot: BootstrapStatus = { needs_setup: false };
    try {
      const status = await fetchBootstrapStatus();
      if (status.ok) {
        boot = status.data;
        needsSetup = Boolean(status.data.needs_setup);
      }
    } catch {
      /* API unreachable — do not lock the tree */
    }
    qc.setQueryData(authBootstrapQueryKey, boot);

    let me: UserPublic | null = null;
    let mustChangeCredentials = false;
    if (!needsSetup) {
      try {
        const meRes = await fetchSessionMe();
        if (meRes.ok) {
          me = meRes.data;
          mustChangeCredentials = Boolean(me.must_change_credentials);
        }
      } catch {
        /* anonymous / network */
      }
    }
    qc.setQueryData(authMeQueryKey, me);

    let cfg: ProviderConfigPublic = PROVIDER_CONFIG_LOCKED;
    try {
      const cfgRes = await fetchProviderConfig();
      if (cfgRes.ok) {
        cfg = cfgRes.data;
      }
    } catch {
      /* fail closed */
    }
    qc.setQueryData(authProviderConfigQueryKey, cfg);

    const target = resolveAppAccessRedirect({
      pathname: location.pathname,
      needsSetup,
      mustChangeCredentials,
    });
    if (target) {
      throw redirect({ href: target });
    }

    // Resolve the theme from the same cookies the FOUC boot script mirrors so
    // the SSR <html> already carries `dark` — no post-paint theme flip on
    // full-document loads. The request-header read lives behind a
    // createServerOnlyFn (start import-protection); client-side loader reruns
    // read document.cookie, which holds the same mirrored values.
    const ssrTheme =
      typeof document === "undefined"
        ? ssrResolvedTheme()
        : resolveThemeForSsr(
            themePreferenceFromCookieHeader(document.cookie),
            null,
            resolvedColorSchemeFromCookieHeader(document.cookie),
          );

    return { dehydratedState: dehydrate(qc), ssrTheme };
  },
  // Documents must revalidate every load: HTML is unpinned while /assets/*
  // is content-hashed, so a cached page would reference a deleted bundle.
  headers: () => ({ "cache-control": "no-cache" }),
});

function RootShell({ children }: { children?: unknown }) @{
  <Html lang="en">
    <Head>
      {/* `precedence` marks this a hoisted stylesheet resource: it emits with
          the earliest head bytes and is deduplicated by href on the client, so
          it is never removed and re-inserted like positionally-keyed managed
          assets were (that churn flashed unstyled content on navigation). */}
      <link rel="stylesheet" href={appStylesHref} precedence="app" />
      <script dangerouslySetInnerHTML={{ __html: THEME_BOOT_SCRIPT }} />
      <script dangerouslySetInnerHTML={{ __html: FONT_WARM_SCRIPT }} />
      <meta charSet="utf-8" />
      <meta name="viewport" content="width=device-width, initial-scale=1" />
      <link
        rel="preload"
        href="/fonts/sora-latin.woff2"
        as="font"
        type="font/woff2"
        crossOrigin="anonymous"
      />
      <link
        rel="preload"
        href="/fonts/source-sans-3-latin.woff2"
        as="font"
        type="font/woff2"
        crossOrigin="anonymous"
      />
      <link rel="icon" href="/favicon.ico" sizes="any" />
      <link rel="icon" type="image/png" sizes="32x32" href="/favicon-32.png" />
      <link rel="icon" type="image/png" sizes="16x16" href="/favicon-16.png" />
      <link rel="apple-touch-icon" href="/apple-touch-icon.png" />
      <link rel="manifest" href="/manifest.webmanifest" />
      <meta name="theme-color" content="#ffffff" media="(prefers-color-scheme: light)" />
      <meta name="theme-color" content="#121212" media="(prefers-color-scheme: dark)" />
      <meta name="apple-mobile-web-app-title" content="Oxidean" />
      <HeadContent />
    </Head>
    <Body>
      {children}
      <Scripts />
      <script dangerouslySetInnerHTML={{ __html: SW_REGISTER_SCRIPT }} />
    </Body>
  </Html>;
}

function RootComponent() @{
  const data = useLoaderData({ from: "__root__" }) as RootLoaderData | undefined;
  const queryClient = getQueryClient();

  <QueryClientProvider client={queryClient}>
    <HydrationBoundary state={data?.dehydratedState}>
      <AppToaster>
        <div className="flex min-h-screen flex-col">
          <SiteHeader />
          <main className="oxidean-main flex-1">
            <Outlet />
          </main>
          <SiteFooter />
          <VerifyBanner />
        </div>
      </AppToaster>
    </HydrationBoundary>
  </QueryClientProvider>;
}
