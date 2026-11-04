import { queryOptions, useQuery, useQueryClient } from "@octanejs/tanstack-query";
import { useState } from "octane";
import { BookMarked, GitFork, Star } from "@octanejs/lucide";
import type { RepoPublic } from "@oxidean/api-client";
import { apiClient } from "@/lib/api-client";
import { AppLink } from "@/components/ui/app-link";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Skeleton } from "@/components/ui/skeleton";

const MAX_PINS = 6;

function pinnedReposQueryOptions(username: string) {
  return queryOptions({
    queryKey: ["user", "pinnedRepos", username],
    queryFn: async (): Promise<RepoPublic[] | null> => {
      const res = await apiClient.user.listPinnedRepos({ username });
      return res.ok ? res.data.repos : null;
    },
    enabled: !!username,
    retry: false,
    staleTime: 60_000,
  });
}

/** Top language for a pinned card's language dot (GitHub card parity). */
function repoTopLanguageQueryOptions(owner: string, name: string) {
  return queryOptions({
    queryKey: ["repo", "languages", owner, name],
    queryFn: async (): Promise<{ name: string; color: string | null } | null> => {
      const res = await apiClient.repo.languages({ owner, name });
      const top = res.ok ? res.data.languages[0] : undefined;
      return top ? { name: top.name, color: top.color ?? null } : null;
    },
    enabled: !!owner && !!name,
    retry: false,
    staleTime: 5 * 60_000,
  });
}

function PinnedRepoCard(props: { repo: RepoPublic }) @{
  const slug = `${props.repo.owner_username}/${props.repo.name}`;
  const lang = useQuery(
    repoTopLanguageQueryOptions(props.repo.owner_username, props.repo.name),
  ).data;

  <div className="flex flex-col gap-2 rounded-md border border-border p-4">
    <div className="flex items-center gap-2">
      <BookMarked aria-hidden size={16} className="shrink-0 text-muted-foreground" />
      <AppLink
        href={`/${slug}`}
        className="truncate text-[14px] font-semibold text-primary no-underline hover:underline"
      >
        {slug as string}
      </AppLink>
      @if (props.repo.visibility === "private") {
        <Badge variant="outline" className="ml-auto shrink-0">
          Private
        </Badge>;
      }
    </div>
    <p className="line-clamp-2 min-h-[2.5em] text-[12px] text-muted-foreground">
      {(props.repo.description?.trim() || "") as string}
    </p>
    <div className="mt-auto flex items-center gap-4 text-[12px] text-muted-foreground">
      @if (lang) {
        <span className="flex items-center gap-1.5">
          <span
            aria-hidden
            className="inline-block size-3 rounded-full border border-border"
            style={{ backgroundColor: lang.color ?? "var(--muted-foreground)" }}
          />
          {lang.name as string}
        </span>;
      }
      <span className="flex items-center gap-1">
        <Star aria-hidden size={13} />
        {String(props.repo.star_count) as string}
      </span>
      <span className="flex items-center gap-1">
        <GitFork aria-hidden size={13} />
        {String(props.repo.fork_count) as string}
      </span>
    </div>
  </div>;
}

/** "Customize your pins" dialog — checkbox list of the owner's repos. */
function CustomizePinsDialog(props: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  username: string;
  candidates: RepoPublic[];
  pinnedIds: string[];
}) @{
  const queryClient = useQueryClient();
  const [selected, setSelected] = useState<string[]>(props.pinnedIds);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Re-seed the working selection each time the dialog opens on fresh pins.
  const [seededFor, setSeededFor] = useState<string | null>(null);
  const seedKey = `${props.open}:${props.pinnedIds.join(",")}`;
  if (seededFor !== seedKey) {
    setSeededFor(seedKey);
    setSelected(props.pinnedIds);
    setError(null);
  }

  function toggle(id: string, checked: boolean) {
    setSelected((cur) => (checked ? [...cur, id] : cur.filter((i) => i !== id)));
    setError(null);
  }

  async function save() {
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      const res = await apiClient.user.setPinnedRepos({ repo_ids: selected });
      if (res.ok) {
        await queryClient.invalidateQueries({
          queryKey: ["user", "pinnedRepos", props.username],
        });
        props.onOpenChange(false);
      } else {
        setError(res.error.message || "Could not save pinned repositories");
      }
    } catch {
      setError("Could not save pinned repositories");
    } finally {
      setBusy(false);
    }
  }

  const atCap = selected.length >= MAX_PINS;

  <Dialog open={props.open} onOpenChange={props.onOpenChange}>
    <DialogContent className="sm:max-w-lg" data-testid="customize-pins-dialog">
      <DialogHeader>
        <DialogTitle>Edit pinned repositories</DialogTitle>
        <DialogDescription>
          {`Select up to ${MAX_PINS} repositories to show on your profile.` as string}
        </DialogDescription>
      </DialogHeader>
      <div className="max-h-80 overflow-y-auto rounded-md border border-border">
        @for (const repo of props.candidates; key repo.id) {
          <label className="flex cursor-pointer items-center gap-3 border-b border-border px-3 py-2.5 last:border-b-0 hover:bg-muted/50">
            <Checkbox
              checked={selected.includes(repo.id)}
              disabled={!selected.includes(repo.id) && atCap}
              onCheckedChange={(checked) => toggle(repo.id, checked === true)}
              aria-label={`Pin ${repo.name}`}
            />
            <span className="min-w-0 flex-1">
              <span className="block truncate text-[14px] font-semibold text-foreground">
                {repo.name as string}
              </span>
              @if (repo.description?.trim()) {
                <span className="block truncate text-[12px] text-muted-foreground">
                  {repo.description as string}
                </span>;
              }
            </span>
            @if (repo.visibility === "private") {
              <Badge variant="outline" className="shrink-0">
                Private
              </Badge>;
            }
          </label>;
        } @empty {
          <p className="px-3 py-6 text-center text-[13px] text-muted-foreground">
            No repositories yet — create one before pinning.
          </p>;
        }
      </div>
      <div className="flex items-center justify-between text-[12px] text-muted-foreground">
        <span data-testid="pins-selected-count">
          {`${selected.length} of ${MAX_PINS} selected` as string}
        </span>
        @if (error) {
          <span className="text-destructive" role="alert">{error as string}</span>;
        }
      </div>
      <DialogFooter>
        <Button
          type="button"
          variant="default"
          disabled={busy}
          data-testid="save-pins-button"
          onClick={() => void save()}
        >
          {(busy ? "Saving…" : "Save pins") as string}
        </Button>
      </DialogFooter>
    </DialogContent>
  </Dialog>;
}

/**
 * GitHub-style "Pinned" section for `/{username}` (COL-04). Hidden when the
 * user has no pins — GitHub shows nothing then too. `isSelf` unlocks the
 * "Customize your pins" dialog; `candidates` is the profile payload's repo
 * list (owner-visible set).
 */
export function PinnedReposSection(props: {
  username: string;
  isSelf: boolean;
  candidates: RepoPublic[];
}) @{
  const q = useQuery(pinnedReposQueryOptions(props.username));
  const pinned = q.data;
  const [customizeOpen, setCustomizeOpen] = useState(false);

  @if (q.isPending) {
    <section className="mt-8" aria-busy="true" aria-label="Loading pinned repositories">
      <Skeleton className="h-5 w-20" />
      <div className="mt-4 grid gap-4 sm:grid-cols-2">
        @for (const i of [0, 1]; key i) {
          <Skeleton className="h-28 w-full" />;
        }
      </div>
    </section>;
  } @else if (pinned && pinned.length > 0) {
    <section className="mt-8" aria-label="Pinned" data-testid="pinned-repos">
      <div className="flex items-center justify-between">
        <h2 className="text-[16px] font-semibold text-foreground">Pinned</h2>
        @if (props.isSelf) {
          <button
            type="button"
            className="text-[13px] text-muted-foreground underline-offset-4 hover:text-foreground hover:underline"
            data-testid="customize-pins"
            onClick={() => setCustomizeOpen(true)}
          >
            Customize your pins
          </button>;
        }
      </div>
      <div className="mt-4 grid gap-4 sm:grid-cols-2">
        @for (const repo of pinned; key repo.id) {
          <PinnedRepoCard repo={repo} />;
        }
      </div>
      <CustomizePinsDialog
        open={customizeOpen}
        onOpenChange={setCustomizeOpen}
        username={props.username}
        candidates={props.candidates}
        pinnedIds={pinned.map((r) => r.id)}
      />
    </section>;
  } @else if (props.isSelf && pinned) {
    // Owner with zero pins: keep the customize entry point reachable —
    // hiding the whole section would make pins un-settable.
    <section className="mt-8" aria-label="Pinned" data-testid="pinned-repos">
      <div className="flex items-center justify-between">
        <h2 className="text-[16px] font-semibold text-foreground">Pinned</h2>
        <button
          type="button"
          className="text-[13px] text-muted-foreground underline-offset-4 hover:text-foreground hover:underline"
          data-testid="customize-pins"
          onClick={() => setCustomizeOpen(true)}
        >
          Customize your pins
        </button>
      </div>
      <CustomizePinsDialog
        open={customizeOpen}
        onOpenChange={setCustomizeOpen}
        username={props.username}
        candidates={props.candidates}
        pinnedIds={[]}
      />
    </section>;
  } @else {
    <></>;
  }
}
