import { queryOptions, useQuery } from "@octanejs/tanstack-query";
import type { OrgPublic } from "@oxidean/api-client";
import { useEffect } from "octane";
import { RepoNotFound } from "@/components/repo/repo-not-found";
import { OrgSettingsLayout, SettingsShell } from "@/components/settings/settings-shell";
import type { OrgSettingsNavActive } from "@/components/org/org-settings-nav";
import { usePageRedirect } from "@/lib/nav.tsrx";
import { usePathname } from "@/lib/pathname.tsrx";
import { matchPath } from "@/lib/route-params";
import { apiClient } from "@/lib/api-client";
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
  | { kind: "ready"; org: OrgPublic; canAdmin: boolean; canManageTeam: boolean };

export function orgSettingsAccessQueryOptions(
  owner: string,
  teamSlug?: string,
  allowOrgMember = false,
) {
  return queryOptions({
    queryKey: ["org", "settingsAccess", owner, teamSlug ?? null, allowOrgMember],
    queryFn: async (): Promise<OrgSettingsAccess> => {
      try {
        const me = await fetchSessionMe();
        if (!me.ok) {
          if (me.error.code === "auth.unauthenticated") return { kind: "unauthenticated" };
          return { kind: "error", message: me.error.message || "Could not load session." };
        }
        const got = await fetchOrgGet({ slug: owner });
        if (!got.ok) return { kind: "not-found" };
        const mine = await fetchOrgListMine();
        if (!mine.ok)
          return { kind: "error", message: mine.error.message || "Could not load organizations." };
        const entry = mine.data.orgs.find((o) => o.slug === got.data.slug);
        if (!entry) return { kind: "forbidden", org: got.data };
        const canAdmin = entry.role === "owner" || entry.role === "admin";
        if (!canAdmin) {
          if (!teamSlug) {
            return allowOrgMember
              ? { kind: "ready", org: got.data, canAdmin: false, canManageTeam: false }
              : { kind: "forbidden", org: got.data };
          }
          const members = await apiClient.org.teams.members.list({
            slug: got.data.slug,
            team_slug: teamSlug,
          });
          if (!members.ok) {
            if (members.error.code === "org.team_not_found") return { kind: "not-found" };
            if (members.error.code === "org.forbidden") return { kind: "forbidden", org: got.data };
            return {
              kind: "error",
              message: members.error.message || "Could not load team access.",
            };
          }
          const managesTeam = members.data.members.some(
            (member) => member.user_id === me.data.id && member.role === "maintainer",
          );
          if (!managesTeam) return { kind: "forbidden", org: got.data };
          return { kind: "ready", org: got.data, canAdmin: false, canManageTeam: true };
        }
        return { kind: "ready", org: got.data, canAdmin: true, canManageTeam: !!teamSlug };
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
  teamSlug?: string;
  allowOrgMember?: boolean;
  children?: unknown;
}) @{
  const pathname = usePathname();
  const owner = orgSettingsOwner(pathname);
  const accessQ = useQuery(
    orgSettingsAccessQueryOptions(owner, props.teamSlug, props.allowOrgMember),
  );
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
