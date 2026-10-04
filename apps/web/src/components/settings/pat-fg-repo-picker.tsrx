import type { RepoPublic } from "@oxidean/api-client";
import { Badge } from "@/components/ui/badge";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { Skeleton } from "@/components/ui/skeleton";
import { cn } from "@/lib/utils";
import { AppLink } from "@/components/ui/app-link";

function repoFullName(repo: RepoPublic): string {
  return `${repo.owner_username}/${repo.name}`;
}

export type PatFgRepoPickerProps = {
  visible: boolean;
  pending: boolean;
  reposLoading: boolean;
  reposError: string;
  ownedRepos: RepoPublic[];
  filteredRepos: RepoPublic[];
  selectedIds: string[];
  repoFilter: string;
  onRepoFilterChange: (value: string) => void;
  onToggleRepo: (id: string, checked: boolean) => void;
};

/** Selected-repos panel — always mounted; parent toggles `visible` via CSS. */
export function PatFgRepoPicker({
  visible,
  pending,
  reposLoading,
  reposError,
  ownedRepos,
  filteredRepos,
  selectedIds,
  repoFilter,
  onRepoFilterChange,
  onToggleRepo,
}: PatFgRepoPickerProps) @{
  <div
    className={cn(
      "mt-1 flex max-h-64 flex-col gap-2 overflow-y-auto rounded-md border border-border bg-card p-3",
      !visible && "hidden",
    )}
    data-testid="fg-repo-picker"
    aria-hidden={!visible}
  >
    <Input
      type="search"
      value={repoFilter}
      placeholder="Filter repositories…"
      aria-label="Filter repositories"
      disabled={pending || !visible}
      className="h-9"
      data-testid="fg-repo-filter"
      onInput={(e) => onRepoFilterChange((e.currentTarget as HTMLInputElement).value)}
    />
    <p className="text-[13px] text-muted-foreground" data-testid="fg-repo-selected-count">
      {
        (selectedIds.length === 0
          ? "No repositories selected"
          : `${selectedIds.length} selected`) as string
      }
    </p>

    @if (reposLoading) {
      <>
        <Skeleton className="h-8 w-full" />
        <Skeleton className="h-8 w-full" />
        <Skeleton className="h-8 w-3/4" />
      </>;
    } @else {
      @if (reposError) {
        <p role="alert" className="text-[14px] text-destructive">
          {reposError as string}
        </p>;
      } @else {
        @if (ownedRepos.length === 0) {
          <>
            <p className="text-[16px] text-muted-foreground">
              You don’t have any repositories yet.
            </p>
            <AppLink
              href="/new"
              className="text-[14px] font-semibold text-primary no-underline hover:underline"
            >
              New repository
            </AppLink>
          </>;
        } @else {
          @if (filteredRepos.length === 0) {
            <p className="text-[14px] text-muted-foreground">No repositories match.</p>;
          } @else {
            @for (const repo of filteredRepos; key repo.id) {
              <label className="flex min-w-0 cursor-pointer items-center gap-3">
                <Checkbox
                  checked={selectedIds.includes(repo.id)}
                  disabled={pending || !visible}
                  aria-label={repoFullName(repo)}
                  data-testid={`fg-repo-item-${repo.id}`}
                  onCheckedChange={(checked) => onToggleRepo(repo.id, checked === true)}
                />
                <span
                  className="min-w-0 flex-1 truncate text-[14px] text-foreground"
                  title={repoFullName(repo) as string}
                >
                  {repoFullName(repo) as string}
                </span>
                <Badge variant="outline">
                  {(repo.visibility === "private" ? "Private" : "Public") as string}
                </Badge>
              </label>;
            }
          }
        }
      }
    }
  </div>;
}
