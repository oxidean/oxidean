import { queryOptions, useQuery } from "@octanejs/tanstack-query";
import { useEffect, useState } from "octane";
import { BookMarked, Star } from "@octanejs/lucide";
import { ReadmePanel } from "@/components/repo/readme-panel";
import { AvatarPreview } from "@/components/avatar-preview";
import { Badge } from "@/components/ui/badge";
import { Button, buttonVariants } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/empty-state";
import { List, ListRow } from "@/components/ui/list";
import { apiClient } from "@/lib/api-client";
import { AppPage } from "@/components/app-page";
import { RepoNotFound } from "@/components/repo/repo-not-found";
import { usePathname } from "@/lib/pathname.tsrx";
import { matchPath } from "@/lib/route-params";
import { fetchOrgOverview, type OrgOverviewPayload } from "@/lib/ssr-org";
import { fetchUserProfile, type UserProfilePayload } from "@/lib/ssr-user-profile";
import { useChromeAccountState } from "@/lib/use-chrome-account";
import { cn } from "@/lib/utils";
import { AppLink } from "@/components/ui/app-link";

export type OwnerIndexLoaderData =
  | { kind: "org"; org: OrgOverviewPayload }
  | { kind: "user"; user: UserProfilePayload };

/**
 * `/{owner}` resolution — org overview first, then user profile; null when
 * neither exists (was `throw notFound()`).
 */
export function ownerIndexQueryOptions(owner: string) {
  return queryOptions({
    queryKey: ["owner", "index", owner],
    queryFn: async (): Promise<OwnerIndexLoaderData | null> => {
      const org = await fetchOrgOverview({ owner });
      if (org) return { kind: "org", org };
      const user = await fetchUserProfile({ username: owner });
      if (user) return { kind: "user", user };
      return null;
    },
    enabled: !!owner,
    retry: false,
    staleTime: 30_000,
  });
}

function ownerFromPathname(pathname: string): string {
  return matchPath("/$owner", pathname)?.owner ?? "";
}

/** Entity-aware title + atom feed link (shell carries the static fallback). */
function useOwnerIndexMeta(data: OwnerIndexLoaderData | null | undefined, owner: string): void {
  useEffect(() => {
    let title = `${owner} · Oxidean`;
    if (data?.kind === "org" && data.org.org) {
      title = `${data.org.org.display_name || data.org.org.slug} · Oxidean`;
    } else if (data?.kind === "user" && data.user.profile) {
      const p = data.user.profile;
      title = `${p.display_name || p.username} · Oxidean`;
    }
    document.title = title;

    let link: HTMLLinkElement | null = null;
    if (data?.kind === "user" && data.user.profile) {
      link = document.createElement("link");
      link.rel = "alternate";
      link.type = "application/atom+xml";
      link.title = `${data.user.profile.username} activity feed`;
      link.href = `/api/users/${data.user.profile.username}/activity.atom`;
      document.head.appendChild(link);
    }
    return () => {
      link?.remove();
    };
  }, [data, owner]);
}

/** `/{owner}` — org overview or user profile (title via effect above). */
export function OwnerIndexPage() @{
  <AppPage>
    <OwnerIndexContent />
  </AppPage>;
}

export function OwnerIndexContent() @{
  const pathname = usePathname();
  const owner = ownerFromPathname(pathname);
  const dataQ = useQuery(ownerIndexQueryOptions(owner));
  const data = dataQ.data;
  useOwnerIndexMeta(data, owner);

  @if (data === null) {
    <div className="mx-auto max-w-6xl px-4 py-16">
      <RepoNotFound />
    </div>;
  } @else if (data?.kind === "org") {
    <OrgOverviewPage />;
  } @else {
    <UserProfilePage />;
  }
}

/** Org overview at exact `/{owner}` (D-ORG-06). */
export function OrgOverviewPage() @{
  const pathname = usePathname();
  const data = useQuery(ownerIndexQueryOptions(ownerFromPathname(pathname))).data;
  const orgData = data?.kind === "org" ? data.org : undefined;

  const org = orgData?.org;
  const slug = org?.slug ?? ownerFromPathname(pathname);
  const display = org?.display_name?.trim() || slug;
  const memberCount = orgData?.memberCount;
  const repos = orgData?.repos ?? [];
  const canAdmin = orgData?.canAdmin === true;
  const profileReadme = orgData?.profileReadme ?? null;

  @if (!org) {
    <div className="mx-auto max-w-6xl px-4 py-16">
      <p className="text-[16px] text-muted-foreground">Organization not found.</p>
    </div>;
  } @else {
    <div className="mx-auto max-w-6xl px-4 py-12">
      <div className="flex flex-wrap items-start justify-between gap-4">
        <div>
          <h1 className="font-[family-name:var(--font-display)] text-[28px] font-semibold leading-[1.2] text-foreground">
            {display as string}
          </h1>
          <p className="mt-1 text-[16px] text-muted-foreground">{"@" + (slug as string)}</p>
          @if (memberCount != null) {
            <p className="mt-3 text-[14px] text-muted-foreground">
              {(memberCount === 1 ? "1 member" : `${memberCount} members`) as string}
            </p>;
          }
        </div>
        @if (canAdmin) {
          <div className="flex flex-wrap gap-2">
            <AppLink
              href={`/${slug}/settings`}
              className={cn(buttonVariants({ variant: "secondary" }), "h-11")}
            >
              Settings
            </AppLink>
            <AppLink
              href={`/${slug}/settings/members`}
              className={cn(buttonVariants({ variant: "outline" }), "h-11")}
            >
              Members
            </AppLink>
            <AppLink
              href={`/${slug}/settings/labels`}
              className={cn(buttonVariants({ variant: "outline" }), "h-11")}
            >
              Labels
            </AppLink>
          </div>;
        }
      </div>

      @if (profileReadme) {
        <ReadmePanel html={profileReadme.html} fileName={profileReadme.name} />;
      }

      <section className="mt-10" aria-labelledby="org-repos-heading">
        <h2
          id="org-repos-heading"
          className="font-[family-name:var(--font-display)] text-[20px] font-semibold text-foreground"
        >
          Repositories
        </h2>
        @if (repos.length === 0) {
          <EmptyState
            className="mt-4"
            icon={<BookMarked aria-hidden size={22} />}
            title="No public repositories yet"
            description="Repositories owned by this organization will appear here."
          />;
        } @else {
          <List className="mt-4">
            @for (const repo of repos; key repo.id) {
              <ListRow className="items-start gap-3 py-4">
                <BookMarked
                  aria-hidden
                  size={18}
                  className="mt-0.5 shrink-0 text-muted-foreground"
                />
                <div className="min-w-0 flex-1">
                  <div className="flex flex-wrap items-center gap-2">
                    <AppLink
                      href={`/${slug as string}/${repo.name as string}`}
                      className="text-[16px] font-semibold text-primary no-underline hover:underline"
                    >
                      {repo.name as string}
                    </AppLink>
                    <Badge
                      variant={repo.visibility === "private" ? "outline" : "success"}
                    >
                      {(repo.visibility === "private" ? "Private" : "Public") as string}
                    </Badge>
                  </div>
                  @if (repo.description?.trim()) {
                    <p className="mt-1 text-[14px] text-muted-foreground">
                      {repo.description as string}
                    </p>;
                  }
                </div>
              </ListRow>;
            }
          </List>;
        }
      </section>
    </div>;
  }
}

/** Public user profile at `/{username}` (D-SOC-05…08). */
export function UserProfilePage() @{
  const data = useQuery(ownerIndexQueryOptions(ownerFromPathname(usePathname()))).data;
  const payload = data?.kind === "user" ? data.user : undefined;
  const profile = payload?.profile;
  const repos = payload?.repos ?? [];
  const starred = payload?.starred ?? [];
  const isSelf = payload?.isSelf === true;
  const profileReadme = payload?.profileReadme ?? null;
  const [tab, setTab] = useState<"repos" | "stars">("repos");
  const account = useChromeAccountState();
  const signedIn = !!account.user;
  const [followBusy, setFollowBusy] = useState(false);
  const [followOverride, setFollowOverride] = useState<{
    following: boolean;
    followerCount: number;
  } | null>(null);

  const display = profile?.display_name?.trim() || profile?.username || "";
  const username = profile?.username ?? "";
  const followingNow = followOverride?.following ?? !!profile?.viewer_is_following;
  const followerCount = followOverride?.followerCount ?? profile?.follower_count ?? 0;
  const followingCount = profile?.following_count ?? 0;

  /** Follow/unfollow the profile owner — optimistic (DEBT-06). */
  async function toggleFollow() {
    if (!signedIn || followBusy || !username || isSelf) return;
    setFollowBusy(true);
    const prev = { following: followingNow, followerCount };
    setFollowOverride({
      following: !prev.following,
      followerCount: Math.max(0, prev.followerCount + (prev.following ? -1 : 1)),
    });
    try {
      const res = prev.following
        ? await apiClient.user.unfollow({ username })
        : await apiClient.user.follow({ username });
      if (res.ok && res.data) {
        setFollowOverride({
          following: !!res.data.viewer_is_following,
          followerCount: res.data.follower_count ?? 0,
        });
      } else {
        setFollowOverride(prev);
      }
    } catch {
      setFollowOverride(prev);
    } finally {
      setFollowBusy(false);
    }
  }

  @if (!profile) {
    <div className="mx-auto max-w-6xl px-4 py-16">
      <p className="text-[16px] text-muted-foreground">User not found.</p>
    </div>;
  } @else {
    <div className="mx-auto max-w-6xl px-4 py-12">
      <div className="flex flex-wrap items-start gap-4">
        <AvatarPreview user={profile} size={80} />
        <div>
          <h1 className="font-[family-name:var(--font-display)] text-[28px] font-semibold leading-[1.2] text-foreground">
            {display as string}
          </h1>
          @if (display !== username) {
            <p className="mt-1 text-[16px] text-muted-foreground">
              {"@" + (username as string)}
            </p>;
          }
          @if (profile.bio?.trim()) {
            <p className="mt-3 max-w-xl text-[16px] text-muted-foreground">
              {profile.bio as string}
            </p>;
          }
          <p className="mt-3 text-[14px] text-muted-foreground" data-testid="profile-follow-counts">
            <span className="font-semibold text-foreground">{String(followerCount)}</span>
            {(followerCount === 1 ? " follower · " : " followers · ") as string}
            <span className="font-semibold text-foreground">{String(followingCount)}</span>
            {" following" as string}
          </p>
        </div>
        @if (signedIn && !isSelf) {
          <div className="ml-auto">
            <Button
              type="button"
              variant="secondary"
              className="h-9 px-4 text-[14px]"
              disabled={followBusy}
              aria-pressed={followingNow}
              data-testid="profile-follow-button"
              onClick={() => void toggleFollow()}
            >
              {(followingNow ? "Following" : "Follow") as string}
            </Button>
          </div>;
        }
      </div>

      @if (profileReadme) {
        <ReadmePanel html={profileReadme.html} fileName={profileReadme.name} />;
      }

      <div className="mt-8 flex gap-4 border-b border-border">
        <button
          type="button"
          className={
            (tab === "repos"
              ? "border-b-2 border-primary pb-2 text-[14px] font-semibold text-foreground"
              : "pb-2 text-[14px] text-muted-foreground") as string
          }
          onClick={() => setTab("repos")}
        >
          Repositories
        </button>
        @if (isSelf) {
          <button
            type="button"
            className={
              (tab === "stars"
                ? "border-b-2 border-primary pb-2 text-[14px] font-semibold text-foreground"
                : "pb-2 text-[14px] text-muted-foreground") as string
            }
            onClick={() => setTab("stars")}
          >
            Stars
          </button>;
        }
      </div>

      @if (tab === "stars" && isSelf) {
        <section className="mt-6" aria-label="Starred repositories">
          @if (starred.length === 0) {
            <EmptyState
              icon={<Star aria-hidden size={22} />}
              title="No starred repositories yet"
              description="Repositories you star will be collected here for quick access."
            />;
          } @else {
            <List>
              @for (const repo of starred; key repo.id) {
                <ListRow className="items-start gap-3 py-4">
                  <Star
                    aria-hidden
                    size={18}
                    className="mt-0.5 shrink-0 text-muted-foreground"
                  />
                  <div className="min-w-0 flex-1">
                    <AppLink
                      href={
                        ("/" + repo.owner_username + "/" + repo.name) as string
                      }
                      className="text-[16px] font-semibold text-primary no-underline hover:underline"
                    >
                      {(repo.owner_username + "/" + repo.name) as string}
                    </AppLink>
                    @if (
                      repo.description?.trim()
                    ) {
                      <p className="mt-1 text-[14px] text-muted-foreground">
                        {repo.description as string}
                      </p>;
                    }
                  </div>
                </ListRow>;
              }
            </List>;
          }
        </section>;
      } @else {
        <section className="mt-6" aria-label="Repositories">
          @if (repos.length === 0) {
            <EmptyState
              icon={<BookMarked aria-hidden size={22} />}
              title="No repositories yet"
              description={
                isSelf
                  ? "Create a repository to start hosting your code on Oxidean."
                  : "This user hasn't published any repositories yet."
              }
              action={
                isSelf ? (
                  <AppLink
                    href="/new"
                    className={cn(buttonVariants({ variant: "default", size: "sm" }))}
                  >
                    New repository
                  </AppLink>
                ) : undefined
              }
            />;
          } @else {
            <List>
              @for (const repo of repos; key repo.id) {
                <ListRow className="items-start gap-3 py-4">
                  <BookMarked
                    aria-hidden
                    size={18}
                    className="mt-0.5 shrink-0 text-muted-foreground"
                  />
                  <div className="min-w-0 flex-1">
                    <div className="flex flex-wrap items-center gap-2">
                      <AppLink
                        href={`/${username as string}/${repo.name as string}`}
                        className="text-[16px] font-semibold text-primary no-underline hover:underline"
                      >
                        {repo.name as string}
                      </AppLink>
                      <Badge
                        variant={
                          repo.visibility === "private" ? "outline" : "success"
                        }
                      >
                        {
                          (repo.visibility === "private"
                            ? "Private"
                            : "Public") as string
                        }
                      </Badge>
                    </div>
                    @if (
                      repo.description?.trim()
                    ) {
                      <p className="mt-1 text-[14px] text-muted-foreground">
                        {repo.description as string}
                      </p>;
                    }
                    @if (repo.star_count) {
                      <div className="mt-2 flex items-center gap-1.5 text-[12px] text-muted-foreground">
                        <Star aria-hidden size={13} />
                        <span>{String(repo.star_count)}</span>
                      </div>;
                    }
                  </div>
                </ListRow>;
              }
            </List>;
          }
        </section>;
      }
    </div>;
  }
}
