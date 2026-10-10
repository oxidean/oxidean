import { queryOptions, useQuery } from "@octanejs/tanstack-query";
import type { OrgPublic } from "@oxidean/api-client";
import { useEffect } from "octane";
import { RepoNotFound } from "@/components/repo/repo-not-found";
import { OrgSettingsLayout, SettingsShell } from "@/components/settings/settings-shell";
import type { OrgSettingsNavActive } from "@/components/org/org-settings-nav";
import { usePageRedirect } from "@/lib/nav.tsrx";
import { usePathname } from "@/lib/pathname.tsrx";
import { matchPath } from "@/lib/route-params";
import { fetchSessionMe } from "@/lib/ssr-auth";
import { fetchOrgGet, fetchOrgListMine } from "@/lib/ssr-org";

/**
 * `/{owner}/settings/*` access resolution — session → org → admin membership.
 * Formerly the `$owner.settings` layout loader; one queryKey shared by the
 * shell and leaf pages.
 */
type OrgSettingsAccess =
  | { kind: "unauthenticated" }
  | { kind: "not-found" }
  | { kind: "forbidden"; org: OrgPublic }
  | { kind: "error"; message: string }
  | { kind: "ready"; org: OrgPublic; canAdmin: boolean };

export function orgSettingsAccessQueryOptions(owner: string) {
  return queryOptions({
    queryKey: ["org", "settingsAccess", owner],
    queryFn: async (): Promise<OrgSettingsAccess> => {
      try {
        const me = await fetchSessionMe();
        if (!me.ok) {
          if (me.error.code === "auth.unauthenticated") return { kind: "unauthenticated" };
          return { kind: "error", message: me.error.message || "Could not load session." };
        }
        const got = await fetchOrgGet({ slug: owner });
        if (!got.ok) return { kind: "not-found" };
        let canAdmin = false;
        const mine = await fetchOrgListMine();
        if (mine.ok) {
          const entry = mine.data.orgs.find((o) => o.slug === got.data.slug);
          canAdmin = entry?.role === "owner" || entry?.role === "admin";
        }
        if (!canAdmin) return { kind: "forbidden", org: got.data };
        return { kind: "ready", org: got.data, canAdmin: true };
      } catch {
        return {
          kind: "error",
          message: "Can't reach Oxidean. Check your connection and try again.",
        };
      }
    },
    enabled: !!owner,
    retry: false,
    staleTime: 30_000,
  });
}

/** Owner segment for `/{owner}/settings/…` paths (incl. deeper leaves). */
export function orgSettingsOwner(pathname: string): string {
  return (
    matchPath("/$owner/settings/$", pathname)?.owner ??
    matchPath("/$owner/settings", pathname)?.owner ??
    ""
  );
}

/**
 * Shared `/{owner}/settings/*` shell — session + org + admin gate, sidebar
 * layout. Leaf pages render inside; redirects fire client-side.
 */
export function OrgSettingsShell(props: {
  active: OrgSettingsNavActive;
  children?: unknown;
}) @{
  const pathname = usePathname();
  const owner = orgSettingsOwner(pathname);
  const accessQ = useQuery(orgSettingsAccessQueryOptions(owner));
  const access = accessQ.data;

  const redirectTo =
    access?.kind === "unauthenticated"
      ? `/login?returnTo=${encodeURIComponent(pathname)}`
      : access?.kind === "forbidden"
        ? `/${access.org.slug}`
        : null;
  usePageRedirect(redirectTo);

  const org = access?.kind === "ready" || access?.kind === "forbidden" ? access.org : null;
  const slug = org?.slug ?? owner;

  // Owner-aware document title (shell carries the static fallback).
  useEffect(() => {
    const name = org?.display_name || org?.slug || owner;
    if (name) document.title = `Settings · ${name} · Oxidean`;
  }, [org?.display_name, org?.slug, owner]);

  @if (
    !access ||
    accessQ.isPending ||
    access.kind === "unauthenticated" ||
    access.kind === "forbidden"
  ) {
    <SettingsShell bare>
      <p className="text-[16px] text-muted-foreground">Loading…</p>
    </SettingsShell>;
  } @else if (access.kind === "not-found") {
    <SettingsShell bare>
      <RepoNotFound />
    </SettingsShell>;
  } @else if (access.kind === "error") {
    <SettingsShell bare>
      <p className="text-[16px] text-muted-foreground">{access.message as string}</p>
    </SettingsShell>;
  } @else {
    <OrgSettingsLayout
      slug={slug}
      active={props.active}
      title="Organization settings"
      subtitle={"@" + slug}
      data-testid="org-settings-layout"
    >
      {props.children}
    </OrgSettingsLayout>;
  }
}
