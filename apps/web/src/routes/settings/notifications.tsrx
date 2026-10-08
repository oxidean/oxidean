import { queryOptions, useQuery } from "@octanejs/tanstack-query";
import { AppPage } from "@/components/app-page";
import type { RepoPublic, UserPublic, WatchLevel } from "@oxidean/api-client";
import { useEffect, useState } from "octane";
import { AccountSettingsLayout, SettingsShell } from "@/components/settings/settings-shell";
import { AppLink } from "@/components/ui/app-link";
import { EmptyState } from "@/components/ui/empty-state";
import { List, ListRow } from "@/components/ui/list";
import { apiClient } from "@/lib/api-client";
import { fetchSessionMe, fetchWatchedRepos } from "@/lib/ssr-auth";

export type NotificationsSettingsLoaderData =
  | { kind: "unauthenticated" }
  | { kind: "error"; message: string }
  | {
      kind: "ready";
      user: UserPublic;
      repos: RepoPublic[];
      truncated: boolean;
    };

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";

function settingsNotificationsQueryOptions() {
  return queryOptions({
    queryKey: ["settings", "settingsNotifications"],
    queryFn: async (): Promise<NotificationsSettingsLoaderData> => {
      try {
        const res = await fetchSessionMe();
        if (!res.ok) {
          if (res.error.code === "auth.unauthenticated") {
            return { kind: "unauthenticated" };
          }
          return {
            kind: "error",
            message: res.error.message || "Could not load session.",
          };
        }
        const watched = await fetchWatchedRepos();
        if (!watched.ok) {
          return {
            kind: "error",
            message: watched.error.message || "Could not load watched repositories.",
          };
        }
        return {
          kind: "ready",
          user: res.data,
          repos: watched.data.repos,
          truncated: watched.data.truncated,
        };
      } catch {
        return { kind: "error", message: NETWORK_ERROR };
      }
    },
    retry: false,
    staleTime: 30_000,
  });
}

type Phase = { kind: "loading" } | { kind: "ready" } | { kind: "error"; message: string };

const LEVELS: { value: WatchLevel; label: string; hint: string }[] = [
  { value: "all", label: "All activity", hint: "Every issue and pull event" },
  {
    value: "participating",
    label: "Participating and @mentions",
    hint: "Only threads you participate in or are mentioned in",
  },
  { value: "ignore", label: "Ignore", hint: "Never notified for this repository" },
];

function levelOf(repo: RepoPublic): WatchLevel {
  return repo.viewer_watch_level ?? "all";
}

/** Exported for tests — `/settings/notifications` per-repo watch matrix (DEBT-06). */
export function NotificationsSettingsPage() @{
  <AppPage>
    <NotificationsSettingsContent />
  </AppPage>;
}

export function NotificationsSettingsContent() @{
  const loaderData = useQuery(settingsNotificationsQueryOptions()).data;

  useEffect(() => {
    if (loaderData?.kind === "unauthenticated") {
      window.location.assign("/login?returnTo=/settings/notifications");
    }
  }, [loaderData]);

  const [phase, setPhase] = useState<Phase>(() => {
    if (loaderData?.kind === "ready") return { kind: "ready" };
    if (loaderData?.kind === "error") {
      return { kind: "error", message: loaderData.message };
    }
    return { kind: "loading" };
  });
  const [repos, setRepos] = useState<RepoPublic[] | undefined>(() =>
    loaderData?.kind === "ready" ? loaderData.repos : undefined,
  );
  const [truncated, setTruncated] = useState(() =>
    loaderData?.kind === "ready" ? loaderData.truncated : false,
  );
  const [busy, setBusy] = useState<Record<string, boolean>>({});

  useEffect(() => {
    if (!loaderData) return;
    if (loaderData.kind === "ready") {
      setPhase({ kind: "ready" });
      setRepos(loaderData.repos);
      setTruncated(loaderData.truncated);
      return;
    }
    if (loaderData.kind === "error") {
      setPhase({ kind: "error", message: loaderData.message });
    }
  }, [loaderData]);

  useEffect(() => {
    if (loaderData) return;
    let cancelled = false;
    void (async () => {
      try {
        const res = await apiClient.auth.me();
        if (cancelled) return;
        if (!res.ok) {
          window.location.assign("/login?returnTo=/settings/notifications");
          return;
        }
        const all: RepoPublic[] = [];
        let hitCap = false;
        for (let offset = 0; offset < 500; offset += 50) {
          const watched = await apiClient.user.listWatched({ offset, limit: 50 });
          if (cancelled) return;
          if (!watched.ok) {
            setPhase({
              kind: "error",
              message: watched.error.message || "Could not load watched repositories.",
            });
            return;
          }
          all.push(...watched.data.repos);
          if (watched.data.repos.length < 50) {
            break;
          }
          hitCap = true;
        }
        setRepos(all);
        setTruncated(hitCap);
        setPhase({ kind: "ready" });
      } catch {
        if (cancelled) return;
        setPhase({ kind: "error", message: NETWORK_ERROR });
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [loaderData]);

  async function setLevel(repo: RepoPublic, next: WatchLevel) {
    if (busy[repo.id]) return;
    setBusy({ ...busy, [repo.id]: true });
    try {
      const res = await apiClient.repo.watch({
        owner: repo.owner_username,
        name: repo.name,
        level: next,
      });
      if (res.ok && res.data) {
        setRepos((repos ?? []).map((r) => (r.id === repo.id ? res.data : r)));
      }
    } catch {
      // keep prior level — select re-renders from state
    } finally {
      setBusy({ ...busy, [repo.id]: false });
    }
  }

  @if (phase.kind === "loading") {
    <></>;
  } @else {
    @if (phase.kind === "error") {
      <SettingsShell bare>
        <p role="alert" className="text-[16px] text-destructive">
          {phase.message as string}
        </p>
      </SettingsShell>;
    } @else {
      <AccountSettingsLayout active="notifications" data-testid="settings-notifications-page">
        <div className="min-w-0">
          <h1 className="font-[family-name:var(--font-display)] text-[24px] font-semibold leading-[1.2] text-foreground">
            Notifications
          </h1>
          <p className="mt-2 text-[16px] text-muted-foreground">
            Choose how much each repository you watch notifies you. These levels apply to issue,
            pull request, release, and workflow run activity.
          </p>
        </div>

        <section className="mt-10">
          <h2 className="text-[18px] font-semibold text-foreground">Watched repositories</h2>
          <p className="mt-1 text-[14px] text-muted-foreground">
            All activity sends every notification. Participating and @mentions limits notifications
            to threads you take part in or are mentioned in. Ignore silences the repository
            entirely.
          </p>
          @if ((repos ?? []).length === 0) {
            <EmptyState
              className="mt-4"
              title="No watched repositories"
              description="Watch a repository from its page to manage notifications here."
            />;
          } @else {
            <List className="mt-4">
              @for (const repo of repos ?? []; key repo.id) {
                <ListRow className="items-center gap-4 py-4">
                  <div className="min-w-0 flex-1">
                    <AppLink
                      href={("/" + repo.owner_username + "/" + repo.name) as string}
                      className="text-[15px] font-semibold text-primary no-underline hover:underline"
                    >
                      {(repo.owner_username + "/" + repo.name) as string}
                    </AppLink>
                    <p className="mt-0.5 text-[13px] text-muted-foreground">
                      {
                        (LEVELS.find((l) => l.value === levelOf(repo))?.hint ??
                          "") as string
                      }
                    </p>
                  </div>
                  <label className="flex items-center gap-2 text-[13px] text-muted-foreground">
                    <span className="sr-only">Notification level</span>
                    <select
                      className="h-9 rounded-md border border-input bg-card px-2.5 text-[13px] text-foreground"
                      aria-label={
                        ("Notifications for " +
                          repo.owner_username +
                          "/" +
                          repo.name) as string
                      }
                      value={levelOf(repo) as string}
                      disabled={busy[repo.id] === true}
                      suppressNativeChangeWarning
                      onChange={(e) => {
                        const next = (e.currentTarget as HTMLSelectElement)
                          .value as WatchLevel;
                        void setLevel(repo, next);
                      }}
                    >
                      @for (const level of LEVELS; key level.value) {
                        <option value={level.value as string}>
                          {level.label as string}
                        </option>;
                      }
                    </select>
                  </label>
                </ListRow>;
              }
            </List>;
          }
          @if (truncated) {
            <p className="mt-3 text-[13px] text-muted-foreground">
              Showing the first 500 watched repositories.
            </p>;
          }
        </section>
      </AccountSettingsLayout>;
    }
  }
}
