import { Archive, Check, Eye, Lock, Star } from "@octanejs/lucide";
import { useState } from "octane";
import type { RepoPublic, WatchLevel } from "@oxidean/api-client";
import { Badge } from "@/components/ui/badge";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { RepoSearchEntry } from "@/components/repo/repo-search-entry";
import { SyncForkPanel } from "@/components/repo/sync-fork-panel";
import { Button } from "@/components/ui/button";
import { apiClient } from "@/lib/api-client";
import type { RepoChromeActive } from "@/lib/repo-chrome-active";
import { useChromeAccountState } from "@/lib/use-chrome-account";
import { useRepoStore } from "@/lib/repo-store";
import { AppLink } from "@/components/ui/app-link";

type RepoChromeProps = {
  repo: RepoPublic;
  active: RepoChromeActive;
  /** Force Settings nav (e.g. settings page). Admins also see it via can_admin. */
  showSettings?: boolean;
};

/** Title row + Code/Commits/Branches/Tags/Issues/Releases/Packages/(Settings) link-row. */
export function RepoChrome({ repo, active, showSettings = false }: RepoChromeProps) @{
  // Gate Settings on API can_admin — not personal username equality (D-REL-07 / D-REL-09).
  const settingsVisible = showSettings || !!repo.can_admin;
  const account = useChromeAccountState();
  const storeRepo = useRepoStore((s) => s.repo);
  const setRepo = useRepoStore((s) => s.setRepo);
  // Store is source of truth so About rail + chrome stay in sync.
  const live = storeRepo && storeRepo.id === repo.id ? storeRepo : repo;
  const [starBusy, setStarBusy] = useState(false);
  const [watchBusy, setWatchBusy] = useState(false);

  const starCount = live.star_count ?? 0;
  const starred = !!live.viewer_has_starred;
  const watchCount = live.watch_count ?? 0;
  // Subscription row exists at any level — "ignore" rows are still subscriptions.
  const watchLevel = live.viewer_watch_level ?? null;
  const subscribed = watchLevel != null;
  const watching = !!live.viewer_is_watching;
  const forkCount = live.fork_count ?? 0;
  // COL-13: repo admin unit toggles hide the Issues / Pulls tabs (list
  // endpoints may omit the flags → default enabled).
  const issuesEnabled = live.issues_enabled !== false;
  const pullsEnabled = live.pulls_enabled !== false;

  const owner = live.owner_username;
  const name = live.name;
  const base = `/${owner}/${name}`;
  const codeHref = base;
  const commitsHref = `${base}/commits/${encodeURIComponent(live.default_branch)}`;
  const branchesHref = `${base}/branches`;
  const tagsHref = `${base}/tags`;
  const issuesHref = `${base}/issues`;
  const pullsHref = `${base}/pulls`;
  const releasesHref = `${base}/releases`;
  const packagesHref = `${base}/packages`;
  const actionsHref = `${base}/actions`;
  const insightsHref = `${base}/insights`;
  const settingsHref = `${base}/settings`;
  const forkHref = `${base}/fork`;
  const signedIn = !!account.user;

  async function toggleStar() {
    if (!signedIn || starBusy) return;
    setStarBusy(true);
    const prev = live;
    const nextStarred = !starred;
    // Optimistic — mirrors GitHub header ↔ About live update.
    setRepo({
      ...prev,
      viewer_has_starred: nextStarred,
      star_count: Math.max(0, (prev.star_count ?? 0) + (nextStarred ? 1 : -1)),
    });
    try {
      const res = nextStarred
        ? await apiClient.repo.star({ owner, name })
        : await apiClient.repo.unstar({ owner, name });
      if (res.ok && res.data) {
        setRepo(res.data);
      } else {
        setRepo(prev);
      }
    } catch {
      setRepo(prev);
    } finally {
      setStarBusy(false);
    }
  }

  /** Upsert the viewer's subscription at `next` level — optimistic (DEBT-06). */
  async function setWatchLevel(next: WatchLevel) {
    if (!signedIn || watchBusy) return;
    setWatchBusy(true);
    const prev = live;
    const wasActive = prev.viewer_watch_level != null && prev.viewer_watch_level !== "ignore";
    const nextActive = next !== "ignore";
    setRepo({
      ...prev,
      viewer_watch_level: next,
      viewer_is_watching: nextActive,
      watch_count: Math.max(0, (prev.watch_count ?? 0) + Number(nextActive) - Number(wasActive)),
    });
    try {
      const res = await apiClient.repo.watch({ owner, name, level: next });
      if (res.ok && res.data) {
        setRepo(res.data);
      } else {
        setRepo(prev);
      }
    } catch {
      setRepo(prev);
    } finally {
      setWatchBusy(false);
    }
  }

  /** Remove the subscription row entirely — optimistic. */
  async function unwatchRepo() {
    if (!signedIn || watchBusy) return;
    setWatchBusy(true);
    const prev = live;
    const wasActive = prev.viewer_watch_level != null && prev.viewer_watch_level !== "ignore";
    setRepo({
      ...prev,
      viewer_watch_level: null,
      viewer_is_watching: false,
      watch_count: Math.max(0, (prev.watch_count ?? 0) - Number(wasActive)),
    });
    try {
      const res = await apiClient.repo.unwatch({ owner, name });
      if (res.ok && res.data) {
        setRepo(res.data);
      } else {
        setRepo(prev);
      }
    } catch {
      setRepo(prev);
    } finally {
      setWatchBusy(false);
    }
  }

  function watchLabel(): string {
    if (watchLevel === "ignore") return "Ignored";
    return watching ? "Watching" : "Watch";
  }

  function navClass(isActive: boolean): string {
    return isActive
      ? "border-b-2 border-primary pb-2 text-[14px] font-semibold text-foreground no-underline"
      : "pb-2 text-[14px] font-normal text-muted-foreground no-underline hover:text-foreground";
  }

  <div className="mb-8">
    <div data-testid="repo-header-row" className="mb-4 flex flex-wrap items-center gap-3">
      <h1 className="font-[family-name:var(--font-display)] text-[24px] font-semibold tracking-tight leading-[1.2] text-foreground">
        <AppLink href={"/" + owner} className="text-primary no-underline hover:underline">
          {owner as string}
        </AppLink>
        <span className="mx-1 text-muted-foreground">/</span>
        <span>{name as string}</span>
      </h1>
      <Badge variant="outline">
        @if (live.visibility === "private") {
          <Lock aria-hidden size={12} />;
        }
        {(live.visibility === "private" ? "Private" : "Public") as string}
      </Badge>
      @if (live.archived) {
        <span data-testid="repo-archived-badge">
          <Badge variant="secondary">
            <Archive aria-hidden size={12} />
            Archived
          </Badge>
        </span>;
      }
      <div className="ml-auto flex flex-wrap items-center gap-2">
        <RepoSearchEntry owner={owner as string} repo={name as string} />
        @if (signedIn) {
          <>
            <DropdownMenu>
              <DropdownMenuTrigger
                className="inline-flex h-8 items-center gap-1.5 rounded-md border border-border bg-card px-2.5 text-[13px] font-medium text-foreground outline-none transition-colors hover:bg-muted focus-visible:ring-2 focus-visible:ring-ring"
                disabled={watchBusy}
                aria-label="Watch options"
                data-testid="repo-watch-menu"
              >
                <Eye
                  aria-hidden
                  size={14}
                  className={watching ? "text-secondary" : undefined}
                />
                <span>{watchLabel() as string}</span>
                <span className="text-muted-foreground">{String(watchCount)}</span>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end" className="min-w-60">
                <DropdownMenuItem
                  onClick={() => void setWatchLevel("all")}
                  data-testid="repo-watch-level-all"
                >
                  <Check
                    aria-hidden
                    size={14}
                    className={watchLevel === "all" ? "text-primary" : "invisible"}
                  />
                  <span className="flex flex-col">
                    <span>All activity</span>
                    <span className="text-[12px] text-muted-foreground">
                      Every issue and pull request event
                    </span>
                  </span>
                </DropdownMenuItem>
                <DropdownMenuItem
                  onClick={() => void setWatchLevel("participating")}
                  data-testid="repo-watch-level-participating"
                >
                  <Check
                    aria-hidden
                    size={14}
                    className={watchLevel === "participating" ? "text-primary" : "invisible"}
                  />
                  <span className="flex flex-col">
                    <span>Participating and @mentions</span>
                    <span className="text-[12px] text-muted-foreground">
                      Only threads you take part in
                    </span>
                  </span>
                </DropdownMenuItem>
                <DropdownMenuItem
                  onClick={() => void setWatchLevel("ignore")}
                  data-testid="repo-watch-level-ignore"
                >
                  <Check
                    aria-hidden
                    size={14}
                    className={watchLevel === "ignore" ? "text-primary" : "invisible"}
                  />
                  <span className="flex flex-col">
                    <span>Ignore</span>
                    <span className="text-[12px] text-muted-foreground">
                      Never notified for this repository
                    </span>
                  </span>
                </DropdownMenuItem>
                @if (subscribed) {
                  <>
                    <DropdownMenuSeparator />
                    <DropdownMenuItem
                      onClick={() => void unwatchRepo()}
                      data-testid="repo-watch-unwatch"
                    >
                      <Check aria-hidden size={14} className="invisible" />
                      <span>Unwatch</span>
                    </DropdownMenuItem>
                  </>;
                }
              </DropdownMenuContent>
            </DropdownMenu>
            <Button
              type="button"
              variant="secondary"
              className="h-8 gap-1.5 px-2.5 text-[13px]"
              disabled={starBusy}
              aria-pressed={starred}
              onClick={() => void toggleStar()}
            >
              <Star
                aria-hidden
                size={14}
                className={starred ? "fill-current text-secondary" : undefined}
              />
              <span>{(starred ? "Starred" : "Star") as string}</span>
              <span className="text-muted-foreground">{String(starCount)}</span>
            </Button>
          </>;
        } @else {
          <>
            <AppLink
              href={("/login?next=" + encodeURIComponent(base)) as string}
              className="inline-flex items-center gap-1.5 rounded-md border border-border px-2.5 py-1.5 text-[13px] text-foreground no-underline hover:bg-muted"
            >
              <Eye aria-hidden size={14} />
              <span>Watch</span>
              <span className="text-muted-foreground">{String(watchCount)}</span>
            </AppLink>
            <AppLink
              href={("/login?next=" + encodeURIComponent(base)) as string}
              className="inline-flex items-center gap-1.5 rounded-md border border-border px-2.5 py-1.5 text-[13px] text-foreground no-underline hover:bg-muted"
            >
              <Star aria-hidden size={14} />
              <span>Star</span>
              <span className="text-muted-foreground">{String(starCount)}</span>
            </AppLink>
          </>;
        }
        @if (signedIn && live.visibility === "public") {
          <AppLink
            href={forkHref as string}
            className="inline-flex items-center gap-1.5 rounded-md border border-border px-2.5 py-1.5 text-[13px] text-foreground no-underline hover:bg-muted"
          >
            Fork
            <span className="text-muted-foreground">{String(forkCount)}</span>
          </AppLink>;
        } @else {
          @if (!signedIn) {
            <AppLink
              href={("/login?next=" + encodeURIComponent(forkHref)) as string}
              className="inline-flex items-center gap-1.5 rounded-md border border-border px-2.5 py-1.5 text-[13px] text-foreground no-underline hover:bg-muted"
            >
              Fork
              <span className="text-muted-foreground">{String(forkCount)}</span>
            </AppLink>;
          }
        }
      </div>
    </div>
    @if (live.archived) {
      <div
        data-testid="repo-archived-banner"
        role="status"
        className="mb-4 flex items-start gap-2 rounded-md border border-amber-500/40 bg-amber-500/10 px-3 py-2 text-[14px] text-amber-800 dark:text-amber-300"
      >
        <Archive aria-hidden size={16} className="mt-0.5 shrink-0" />
        <span>
          This repository is archived and read-only. Browsing and cloning still work; pushes,
          issues, pull requests, and releases are disabled until it is unarchived.
        </span>
      </div>;
    }
    @if (live.forked_from) {
      <>
        <p className="mt-2 text-[14px] text-muted-foreground">
          Forked from{" "}
          <AppLink
            href={("/" + live.forked_from.owner + "/" + live.forked_from.name) as string}
            className="text-primary no-underline hover:underline"
          >
            {(live.forked_from.owner + "/" + live.forked_from.name) as string}
          </AppLink>
        </p>
        <SyncForkPanel owner={owner} repoName={name} canWrite={!!live.can_write} />
      </>;
    }

    <nav className="mt-6 flex flex-wrap gap-4 border-b border-border pb-0" aria-label="Repository">
      <AppLink
        href={codeHref as string}
        className={navClass(active === "code") as string}
        aria-current={active === "code" ? "page" : undefined}
      >
        Code
      </AppLink>
      <AppLink
        href={commitsHref as string}
        className={navClass(active === "commits") as string}
        aria-current={active === "commits" ? "page" : undefined}
      >
        Commits
      </AppLink>
      <AppLink
        href={branchesHref as string}
        className={navClass(active === "branches") as string}
        aria-current={active === "branches" ? "page" : undefined}
      >
        Branches
      </AppLink>
      <AppLink
        href={tagsHref as string}
        className={navClass(active === "tags") as string}
        aria-current={active === "tags" ? "page" : undefined}
      >
        Tags
      </AppLink>
      @if (issuesEnabled) {
        <AppLink
          href={issuesHref as string}
          className={navClass(active === "issues") as string}
          aria-current={active === "issues" ? "page" : undefined}
          data-testid="repo-chrome-issues"
        >
          Issues
          @if ((live.open_issue_count ?? 0) > 0) {
            <span className="ml-1 inline-flex min-w-5 items-center justify-center rounded-full bg-muted px-1.5 py-0.5 text-[11px] font-medium text-muted-foreground">
              {String(live.open_issue_count)}
            </span>;
          }
        </AppLink>;
      }
      @if (pullsEnabled) {
        <AppLink
          href={pullsHref as string}
          className={navClass(active === "pulls") as string}
          aria-current={active === "pulls" ? "page" : undefined}
          data-testid="repo-chrome-pulls"
        >
          Pulls
          @if ((live.open_pull_count ?? 0) > 0) {
            <span className="ml-1 inline-flex min-w-5 items-center justify-center rounded-full bg-muted px-1.5 py-0.5 text-[11px] font-medium text-muted-foreground">
              {String(live.open_pull_count)}
            </span>;
          }
        </AppLink>;
      }
      <AppLink
        href={releasesHref as string}
        className={navClass(active === "releases") as string}
        aria-current={active === "releases" ? "page" : undefined}
      >
        Releases
      </AppLink>
      <AppLink
        href={packagesHref as string}
        className={navClass(active === "packages") as string}
        aria-current={active === "packages" ? "page" : undefined}
      >
        Packages
      </AppLink>
      <AppLink
        href={actionsHref as string}
        className={navClass(active === "actions") as string}
        aria-current={active === "actions" ? "page" : undefined}
        data-testid="repo-chrome-actions"
      >
        Actions
      </AppLink>
      <AppLink
        href={insightsHref as string}
        className={navClass(active === "insights") as string}
        aria-current={active === "insights" ? "page" : undefined}
        data-testid="repo-chrome-insights"
      >
        Insights
      </AppLink>
      @if (settingsVisible) {
        <AppLink
          href={settingsHref as string}
          className={navClass(active === "settings") as string}
          aria-current={active === "settings" ? "page" : undefined}
        >
          Settings
        </AppLink>;
      }
    </nav>
  </div>;
}
