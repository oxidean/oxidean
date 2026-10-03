import { Link } from "@octanejs/tanstack-router";
import { Lock, Plus } from "@octanejs/lucide";
import { useState } from "octane";
import type { RepoPublic, UserPublic } from "@oxidean/api-client";
import { Badge } from "@/components/ui/badge";
import { buttonVariants } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { formatRelativeUpdated } from "@/lib/relative-time";
import { cn } from "@/lib/utils";
import { AppLink } from "@/components/ui/app-link";

type SignedInHomeProps = {
  user: UserPublic;
  /** SSR / loader payload — no client listMine round-trip. */
  repos: RepoPublic[];
  reposError?: string | null;
};

const TOP_REPOS_INITIAL = 7;

/** Signed-in home — classic two-column dashboard (repos + feed). */
export function SignedInHome({ user, repos, reposError = null }: SignedInHomeProps) @{
  const [repoFilter, setRepoFilter] = useState("");
  const [showAllRepos, setShowAllRepos] = useState(false);

  const empty = !reposError && repos.length === 0;
  const filter = repoFilter.trim().toLowerCase();
  const filtered = filter
    ? repos.filter((r) => {
        const q = `${r.owner_username}/${r.name}`.toLowerCase();
        return q.includes(filter);
      })
    : repos;
  const visibleRepos =
    showAllRepos || filtered.length <= TOP_REPOS_INITIAL
      ? filtered
      : filtered.slice(0, TOP_REPOS_INITIAL);
  const hasMore = !showAllRepos && filtered.length > TOP_REPOS_INITIAL;

  <div className="mx-auto max-w-7xl px-4 py-8 sm:px-6 lg:py-10" data-testid="signed-in-home">
    @if (user.profile_incomplete) {
      <p
        role="status"
        className="mb-6 rounded-md border border-primary/25 bg-primary/5 px-4 py-3 text-[14px] text-foreground"
      >
        Choose a username to finish setup.{" "}
        <AppLink
          href="/settings/profile"
          className="font-semibold text-primary underline-offset-4 hover:underline"
        >
          Profile
        </AppLink>
      </p>;
    }

    <div className="grid gap-8 lg:grid-cols-[17.5rem_minmax(0,1fr)] lg:gap-12">
      <aside aria-label="Top repositories" className="min-w-0" data-testid="home-top-repos">
        <div className="mb-3 flex items-center justify-between gap-2">
          <h2 className="text-[13px] font-semibold tracking-tight text-foreground">
            Top repositories
          </h2>
          @if (user.email_verified) {
            <Link
              to="/new"
              preload="intent"
              className={cn(
                buttonVariants({ variant: "default" }),
                "h-8 shrink-0 gap-1 px-2.5 text-[13px]",
              )}
            >
              <Plus aria-hidden size={14} />
              New
            </Link>;
          } @else {
            <button
              type="button"
              disabled
              aria-disabled="true"
              title="Verify your email to create a repository."
              className={cn(
                buttonVariants({ variant: "default" }),
                "h-8 shrink-0 px-2.5 text-[13px]",
              )}
            >
              New
            </button>;
          }
        </div>

        <Input
          type="search"
          value={repoFilter}
          onInput={(e) => setRepoFilter((e.currentTarget as HTMLInputElement).value)}
          placeholder="Find a repository…"
          aria-label="Find a repository"
          className="mb-3 h-9 border-border/80 bg-background"
        />

        @if (!user.email_verified) {
          <p className="mb-3 text-[13px] leading-snug text-muted-foreground">
            Verify your email to create a repository.
          </p>;
        }

        @if (reposError) {
          <p role="status" className="text-[13px] text-muted-foreground">
            Repository list unavailable.
          </p>;
        } @else if (empty) {
          <div className="rounded-md border border-dashed border-border bg-card/40 px-3 py-8 text-center">
            <p className="text-[14px] text-muted-foreground">No repositories yet.</p>
            @if (user.email_verified) {
              <Link
                to="/new"
                preload="intent"
                className={cn(
                  buttonVariants({ variant: "secondary" }),
                  "mt-4 h-9 px-3",
                )}
              >
                New repository
              </Link>;
            }
          </div>;
        } @else if (filtered.length === 0) {
          <p className="px-1 text-[14px] text-muted-foreground">No repositories match.</p>;
        } @else {
          <>
            <ul className="flex flex-col gap-0.5">
              @for (const repo of visibleRepos; key repo.id) {
                <li>
                  <AppLink
                    href={"/" + repo.owner_username + "/" + repo.name}
                    className="group flex items-center gap-2 rounded-md px-2 py-1.5 text-[14px] text-foreground no-underline transition-colors hover:bg-muted"
                  >
                    <span className="min-w-0 flex-1 truncate font-medium group-hover:text-primary">
                      {repo.owner_username + "/" + repo.name}
                    </span>
                    @if (repo.visibility === "private") {
                      <Lock
                        aria-hidden
                        size={12}
                        className="shrink-0 text-muted-foreground"
                      />;
                    }
                  </AppLink>
                </li>;
              }
            </ul>
            @if (hasMore) {
              <button
                type="button"
                className="mt-2 px-2 text-[13px] font-medium text-primary hover:underline"
                onClick={() => setShowAllRepos(true)}
              >
                Show more
              </button>;
            }
          </>;
        }
      </aside>

      <section aria-label="Home feed" className="min-w-0" data-testid="home-feed">
        <div className="mb-5 flex flex-wrap items-end justify-between gap-3 border-b border-border pb-3">
          <div>
            <h1 className="font-[family-name:var(--font-display)] text-[26px] font-semibold tracking-tight text-foreground">
              Home
            </h1>
            <p className="mt-1 text-[14px] text-muted-foreground">
              Recent repositories and activity on this instance.
            </p>
          </div>
        </div>

        @if (empty && !reposError) {
          <div className="relative overflow-hidden rounded-lg border border-border bg-card px-6 py-14 text-center">
            <div
              aria-hidden
              className="pointer-events-none absolute inset-0 bg-[radial-gradient(ellipse_at_top,color-mix(in_srgb,var(--primary)_14%,transparent),transparent_55%)]"
            />
            <div className="relative">
              <h2 className="font-[family-name:var(--font-display)] text-[22px] font-semibold tracking-tight text-foreground">
                Create your first repository
              </h2>
              <p className="mx-auto mt-3 max-w-md text-[15px] leading-relaxed text-muted-foreground">
                Host your code on Oxidean — public or private, cloud or self-hosted.
              </p>
              <div className="mt-7 flex justify-center">
                @if (user.email_verified) {
                  <Link
                    to="/new"
                    preload="intent"
                    className={cn(buttonVariants({ variant: "default" }), "h-11 px-5")}
                  >
                    New repository
                  </Link>;
                } @else {
                  <button
                    type="button"
                    disabled
                    aria-disabled="true"
                    title="Verify your email to create a repository."
                    className={cn(buttonVariants({ variant: "default" }), "h-11 px-5")}
                  >
                    New repository
                  </button>;
                }
              </div>
            </div>
          </div>;
        } @else {
          <div className="overflow-hidden rounded-lg border border-border bg-card">
            <div className="border-b border-border bg-muted/40 px-5 py-3">
              <h2 className="text-[13px] font-semibold tracking-tight text-foreground">
                Your repositories
              </h2>
              <p className="mt-0.5 text-[13px] text-muted-foreground">
                Sorted by recent updates. A richer activity feed lands later.
              </p>
            </div>
            @if (reposError) {
              <p
                role="alert"
                className="px-5 py-8 text-center text-[14px] text-destructive"
              >
                {reposError as string}
              </p>;
            } @else {
              <ul className="divide-y divide-border">
                @for (const repo of repos.slice(0, 8); key repo.id) {
                  <li className="flex flex-wrap items-center gap-3 px-5 py-3.5 transition-colors hover:bg-muted/35">
                    <AppLink
                      href={"/" + repo.owner_username + "/" + repo.name}
                      className="min-w-0 flex-1 truncate text-[15px] font-semibold text-primary no-underline hover:underline"
                    >
                      {repo.owner_username + "/" + repo.name}
                    </AppLink>
                    <Badge variant="outline">
                      {
                        (repo.visibility === "private"
                          ? "Private"
                          : "Public") as string
                      }
                    </Badge>
                    <span className="text-[13px] text-muted-foreground">
                      {formatRelativeUpdated(repo.updated_at) as string}
                    </span>
                  </li>;
                }
              </ul>;
            }
          </div>;
        }
      </section>
    </div>
  </div>;
}
