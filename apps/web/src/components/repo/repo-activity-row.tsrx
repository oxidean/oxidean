import type { RepoActivityItem } from "@oxidean/api-client";
import { commitHref } from "@/lib/repo-browse";
import { formatRelativeUpdated } from "@/lib/relative-time";
import { ListRow } from "@/components/ui/list";
import { AppLink } from "@/components/ui/app-link";

function relativeActivityTime(iso: string): string {
  return formatRelativeUpdated(iso).replace(/^Updated\s+/i, "");
}

function actionCopy(item: RepoActivityItem): string {
  const branch = item.ref_short || item.ref_name;
  switch (item.push_type) {
    case "branch_creation":
      return `created branch ${branch}`;
    case "branch_deletion":
      return `deleted branch ${branch}`;
    case "branch_rename":
      if (item.commit_message) {
        return `renamed branch ${item.commit_message} to ${branch}`;
      }
      return `renamed branch to ${branch}`;
    case "force_push":
      return `force-pushed to ${branch}`;
    case "pr_merge":
      if (item.pr_number != null) {
        return `merged pull request #${item.pr_number} into ${branch}`;
      }
      return `merged into ${branch}`;
    default:
      return `pushed to ${branch}`;
  }
}

function actorInitial(item: RepoActivityItem): string {
  const source = item.pusher.name || item.pusher.login || "?";
  return source.slice(0, 1).toUpperCase();
}

function hasCommitOid(oid: string): boolean {
  return oid.length > 0 && !oid.split("").every((c) => c === "0");
}

type RepoActivityRowProps = {
  owner: string;
  repoName: string;
  item: RepoActivityItem;
};

/** One chronological activity feed row (dense). */
export function RepoActivityRow(props: RepoActivityRowProps) @{
  const { owner, repoName, item } = props;
  const action = actionCopy(item);
  const when = relativeActivityTime(item.pushed_at);
  const initial = actorInitial(item);
  const showCommitLink = !!item.commit_message && hasCommitOid(item.after);
  const commitCountLabel = item.commits_count > 1 ? `${String(item.commits_count)} commits` : "";

  <ListRow className="items-start gap-3 py-3">
    <AppLink
      href={item.pusher.path as string}
      className="inline-flex h-8 w-8 shrink-0 items-center justify-center overflow-hidden rounded-full border border-border bg-muted text-[11px] font-semibold text-foreground no-underline"
      title={item.pusher.login as string}
    >
      @if (item.pusher.avatar_url) {
        <img
          src={item.pusher.avatar_url as string}
          alt=""
          className="block h-full w-full object-cover"
        />;
      } @else {
        <span aria-hidden className="flex size-full items-center justify-center leading-none">
          {initial as string}
        </span>;
      }
    </AppLink>
    <div className="min-w-0 flex-1 text-[14px] leading-5">
      <div className="text-foreground">
        <AppLink
          href={item.pusher.path as string}
          className="font-semibold text-foreground no-underline hover:underline"
        >
          {item.pusher.login as string}
        </AppLink>
        <span className="text-muted-foreground">{" " + action}</span>
      </div>
      @if (item.commit_message) {
        <div className="mt-0.5 truncate text-[13px] text-muted-foreground">
          @if (showCommitLink) {
            <AppLink
              href={commitHref(owner, repoName, item.after) as string}
              className="text-primary no-underline hover:underline"
            >
              {item.commit_message as string}
            </AppLink>;
          } @else {
            <span>{item.commit_message as string}</span>;
          }
        </div>;
      }
      @if (commitCountLabel) {
        <div className="mt-0.5 text-[12px] text-muted-foreground">
          {commitCountLabel as string}
        </div>;
      }
    </div>
    <time
      className="shrink-0 text-[12px] text-muted-foreground"
      dateTime={item.pushed_at as string}
      title={item.pushed_at as string}
    >
      {when as string}
    </time>
  </ListRow>;
}
