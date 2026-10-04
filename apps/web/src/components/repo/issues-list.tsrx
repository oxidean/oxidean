import { useForm } from "@octanejs/tanstack-form";
import { CircleCheck, CircleDot, MessageSquare } from "@octanejs/lucide";
import type { IssuePublic, LabelPublic, RepoPublic } from "@oxidean/api-client";
import { MemberLookup } from "@/components/org/member-lookup";
import { LabelChip } from "@/components/repo/label-chip";
import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/empty-state";
import { List, ListRow } from "@/components/ui/list";
import { Skeleton } from "@/components/ui/skeleton";
import { formatRelative } from "@/lib/relative-time";
import {
  repoListApplyClass,
  repoListFilterFieldClass,
  repoListFilterFormClass,
  repoListFilterLabelClass,
  repoListPrimaryCtaClass,
  repoListTabClass,
  repoListTablistClass,
} from "@/components/repo/repo-list-toolbar";
import { AppLink } from "@/components/ui/app-link";

export type IssueListStateFilter = "open" | "closed" | "all";

export type IssueListFilters = {
  state: IssueListStateFilter;
  author: string;
  label: string;
  assignee: string;
  q: string;
  offset: number;
  limit: number;
};

export type IssueFilterDraft = {
  author: string;
  label: string;
  assignee: string;
  q: string;
};

export const ISSUE_LIST_PAGE_SIZE = 25;

type IssuesListProps = {
  owner: string;
  repoName: string;
  repo: RepoPublic;
  issues: IssuePublic[];
  total: number;
  filters: IssueListFilters;
  labels: LabelPublic[];
  onStateChange: (next: IssueListStateFilter) => void;
  onApplyFilters: (draft: IssueFilterDraft) => void;
  onPrevPage: () => void;
  onNextPage: () => void;
  pending?: boolean;
};

/** Open/Closed/All + author/label/assignee/text filters + offset pages (D-ISS-16..19). */
export function IssuesList({
  owner,
  repoName,
  repo,
  issues,
  total,
  filters,
  labels,
  onStateChange,
  onApplyFilters,
  onPrevPage,
  onNextPage,
  pending = false,
}: IssuesListProps) @{
  const base = `/${owner}/${repoName}`;
  const canWrite = !!repo.can_write;
  const canAdmin = !!repo.can_admin;
  const emptyCopy =
    filters.state === "closed"
      ? "No closed issues"
      : filters.state === "all"
        ? "No issues"
        : "No open issues";
  const from = total === 0 ? 0 : filters.offset + 1;
  const to = Math.min(filters.offset + issues.length, total);
  const canPrev = filters.offset > 0;
  const canNext = filters.offset + filters.limit < total;

  const filterForm = useForm({
    defaultValues: {
      author: filters.author,
      label: filters.label,
      assignee: filters.assignee,
      q: filters.q,
    },
    onSubmit: ({ value }) => {
      onApplyFilters({
        author: value.author.trim(),
        label: value.label.trim(),
        assignee: value.assignee.trim(),
        q: value.q.trim(),
      });
    },
  });

  <div className="space-y-4">
    <div className="flex flex-wrap items-center justify-between gap-3">
      <div role="tablist" aria-label="Issue state" className={repoListTablistClass() as string}>
        <button
          type="button"
          role="tab"
          aria-selected={filters.state === "open"}
          className={repoListTabClass(filters.state === "open") as string}
          onClick={() => onStateChange("open")}
        >
          Open
        </button>
        <button
          type="button"
          role="tab"
          aria-selected={filters.state === "closed"}
          className={repoListTabClass(filters.state === "closed") as string}
          onClick={() => onStateChange("closed")}
        >
          Closed
        </button>
        <button
          type="button"
          role="tab"
          aria-selected={filters.state === "all"}
          className={repoListTabClass(filters.state === "all") as string}
          onClick={() => onStateChange("all")}
        >
          All
        </button>
      </div>
      <div className="flex flex-wrap items-center gap-3">
        @if (canAdmin) {
          <AppLink
            href={`${base}/issues/labels` as string}
            className="text-sm font-medium text-primary no-underline hover:underline"
          >
            Labels
          </AppLink>;
        }
        @if (canWrite) {
          <AppLink
            href={`${base}/issues/new` as string}
            className={repoListPrimaryCtaClass() as string}
          >
            New issue
          </AppLink>;
        }
      </div>
    </div>

    <form
      method="post"
      action="#"
      aria-label="Issue filters"
      className={repoListFilterFormClass() as string}
      onSubmit={(e) => {
        e.preventDefault();
        void filterForm.handleSubmit();
      }}
    >
      <filterForm.Field name="author">
        {(field) => (
          <MemberLookup
            id="issue-filter-author"
            name="author"
            label="Author"
            compact
            className="min-w-[9rem]"
            placeholder="username"
            value={field.state.value as string}
            onValueChange={field.handleChange}
          />
        )}
      </filterForm.Field>
      <filterForm.Field name="label">
        {(field) => (
          <label className={repoListFilterLabelClass() as string}>
            Label
            <select
              name="label"
              aria-label="Label"
              value={field.state.value as string}
              className={repoListFilterFieldClass() as string}
              onChange={(e) => field.handleChange((e.target as HTMLSelectElement).value)}
            >
              <option value="">Any label</option>
              @for (const lab of labels; key lab.id) {
                <option value={lab.id as string}>{lab.name as string}</option>;
              }
            </select>
          </label>
        )}
      </filterForm.Field>
      <filterForm.Field name="assignee">
        {(field) => (
          <MemberLookup
            id="issue-filter-assignee"
            name="assignee"
            label="Assignee"
            compact
            className="min-w-[9rem]"
            placeholder="username"
            value={field.state.value as string}
            onValueChange={field.handleChange}
          />
        )}
      </filterForm.Field>
      <filterForm.Field name="q">
        {(field) => (
          <label className={`${repoListFilterLabelClass()} min-w-[12rem] flex-1` as string}>
            Search
            <input
              type="search"
              name="q"
              autoComplete="off"
              placeholder="Search title or body"
              value={field.state.value as string}
              className={`${repoListFilterFieldClass()} w-full` as string}
              onBlur={field.handleBlur}
              onInput={(e) => field.handleChange((e.target as HTMLInputElement).value)}
            />
          </label>
        )}
      </filterForm.Field>
      <button type="submit" className={repoListApplyClass() as string}>
        Apply
      </button>
    </form>

    @if (pending) {
      <List>
        @for (const i of [0, 1, 2, 3, 4]; key i) {
          <ListRow className="items-start gap-3">
            <Skeleton className="mt-0.5 size-4 rounded-full" />
            <div className="flex flex-1 flex-col gap-2">
              <Skeleton className="h-4 w-2/3" />
              <Skeleton className="h-3 w-1/3" />
            </div>
          </ListRow>;
        }
      </List>;
    } @else {
      @if (issues.length === 0) {
        <EmptyState
          icon={<CircleDot aria-hidden size={22} />}
          title={emptyCopy}
          description={
            filters.state === "open"
              ? "Track bugs, ideas, and tasks for this repository here."
              : "Nothing to show for this filter."
          }
        />;
      } @else {
        <>
          <List>
            @for (const issue of issues; key issue.id) {
              <ListRow className="items-start gap-3 py-3">
                @if (issue.state === "open") {
                  <CircleDot
                    aria-hidden
                    size={16}
                    className="mt-0.5 shrink-0 text-success"
                  />;
                } @else {
                  <CircleCheck
                    aria-hidden
                    size={16}
                    className="mt-0.5 shrink-0 text-[#8250df] dark:text-[#a371f7]"
                  />;
                }
                <div className="min-w-0 flex-1">
                  <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
                    <AppLink
                      href={`${base}/issues/${issue.number}` as string}
                      className="text-[15px] font-semibold text-foreground no-underline hover:text-primary"
                    >
                      {issue.title as string}
                    </AppLink>
                    @for (const lab of issue.labels ?? []; key lab.id) {
                      <LabelChip label={lab} />;
                    }
                  </div>
                  <p className="mt-1 text-[12px] text-muted-foreground">
                    {
                      (issue.state === "open"
                        ? `#${issue.number} opened ${formatRelative(issue.created_at)} by @${issue.author_username}`
                        : `#${issue.number} closed ${formatRelative(issue.closed_at ?? issue.updated_at)} by @${issue.closed_by ?? issue.author_username}`) as string
                    }
                    @if (
                      issue.assignees &&
                      issue.assignees.length > 0
                    ) {
                      <>
                        <span className="mx-1.5">·</span>
                        <span>
                          {
                            `assigned ${issue.assignees.map((a) => `@${a.username}`).join(", ")}` as string
                          }
                        </span>
                      </>;
                    }
                  </p>
                </div>
                @if ((issue.comment_count ?? 0) > 0) {
                  <AppLink
                    href={`${base}/issues/${issue.number}` as string}
                    className="mt-0.5 flex shrink-0 items-center gap-1 text-[12px] text-muted-foreground no-underline hover:text-primary"
                    aria-label={
                      `${issue.comment_count} comment${issue.comment_count === 1 ? "" : "s"}` as string
                    }
                  >
                    <MessageSquare aria-hidden size={14} />
                    {issue.comment_count as number}
                  </AppLink>;
                }
              </ListRow>;
            }
          </List>
          <div className="flex flex-wrap items-center justify-between gap-3">
            <p className="text-[13px] text-muted-foreground">
              {`${from}–${to} of ${total} issue${total === 1 ? "" : "s"}` as string}
            </p>
            <div
              className="flex items-center gap-2"
              role="navigation"
              aria-label="Issue list pagination"
            >
              <Button
                type="button"
                variant="outline"
                size="sm"
                disabled={!canPrev}
                onClick={() => onPrevPage()}
              >
                Previous
              </Button>
              <Button
                type="button"
                variant="outline"
                size="sm"
                disabled={!canNext}
                onClick={() => onNextPage()}
              >
                Next
              </Button>
            </div>
          </div>
        </>;
      }
    }
  </div>;
}
