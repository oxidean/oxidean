import type { PullPublic, RepoPublic } from "@oxidean/api-client";
import {
  GitMerge,
  GitPullRequest,
  GitPullRequestClosed,
  GitPullRequestDraft,
} from "@octanejs/lucide";
import { PullV2Changes } from "@/components/repo/pull-v2/pull-v2-changes";
import { PullV2Checks } from "@/components/repo/pull-v2/pull-v2-checks";
import { PullV2Commits } from "@/components/repo/pull-v2/pull-v2-commits";
import { PullV2Overview } from "@/components/repo/pull-v2/pull-v2-overview";
import { AppLink } from "@/components/ui/app-link";
import { Badge } from "@/components/ui/badge";
import { cn } from "@/lib/utils";

type PullV2Tab = "overview" | "commits" | "checks" | "changes";

/** Sub-path after `/pull/N` → tab. `changes/<sha>` is the files tab too. */
export function pullV2TabFromSplat(splat: string): PullV2Tab {
  if (splat === "commits") return "commits";
  if (splat === "checks") return "checks";
  if (splat === "changes" || splat.startsWith("changes/")) return "changes";
  return "overview";
}

type PullDetailV2Props = {
  owner: string;
  repoName: string;
  number: number;
  repo: RepoPublic;
  pull: PullPublic;
  canWrite: boolean;
  canInteract: boolean;
  isAuthor: boolean;
  currentUserId: string;
  tab: PullV2Tab;
  /** `changes/<sha>` scoped commit when the files tab carries one. */
  changesSha?: string;
};

function pullStateBadge(state: string, draft: boolean) @{
  @if (state === "merged") {
    <Badge variant="merged">
      <GitMerge aria-hidden size={12} />
      Merged
    </Badge>;
  } @else if (state === "closed") {
    <Badge variant="closed">
      <GitPullRequestClosed aria-hidden size={12} />
      Closed
    </Badge>;
  } @else {
    @if (draft) {
      <Badge variant="draft">
        <GitPullRequestDraft aria-hidden size={12} />
        Draft
      </Badge>;
    } @else {
      <Badge variant="open">
        <GitPullRequest aria-hidden size={12} />
        Open
      </Badge>;
    }
  }
}

const TABS: { id: PullV2Tab; label: string; path: (base: string) => string }[] = [
  { id: "overview", label: "Overview", path: (b) => b },
  { id: "commits", label: "Commits", path: (b) => `${b}/commits` },
  { id: "checks", label: "Checks", path: (b) => `${b}/checks` },
  { id: "changes", label: "Files changed", path: (b) => `${b}/changes` },
];

/**
 * pr-new-ui (WI-0001): GitHub-style pull chrome — sticky mini-header +
 * URL-backed tab nav. Tab bodies delegate to the pull-v2 surfaces
 * (stages 4–10); the legacy v1 components stay untouched for flag-off users.
 */
export function PullDetailV2(props: PullDetailV2Props) @{
  const base = `/${props.owner}/${props.repoName}/pull/${props.number}`;
  const headRef =
    props.pull.head_owner === props.owner && props.pull.head_name === props.repoName
      ? props.pull.head_ref
      : `${props.pull.head_owner}:${props.pull.head_ref}`;

  <div className="space-y-4">
    <div className="sticky top-0 z-20 -mx-4 border-b border-border bg-background/95 px-4 backdrop-blur sm:-mx-6 sm:px-6">
      <div className="flex flex-wrap items-center gap-2 py-2 text-[14px]">
        <span className="font-mono text-muted-foreground">#{props.number as number}</span>
        <h2 className="min-w-0 truncate font-[family-name:var(--font-display)] font-semibold">
          {props.pull.title as string}
        </h2>
        {pullStateBadge((props.pull.state as string) ?? "open", props.pull.draft)}
      </div>
      <nav className="flex gap-1 overflow-x-auto" aria-label="Pull request sections">
        @for (const t of TABS; key t.id) {
          <AppLink
            href={t.path(base) as string}
            className={
              cn(
                "border-b-2 px-3 pb-2 pt-1 text-sm no-underline",
                props.tab === t.id
                  ? "border-primary font-semibold text-foreground"
                  : "border-transparent text-muted-foreground hover:text-foreground",
              ) as string
            }
            aria-current={props.tab === t.id ? "page" : undefined}
            data-testid={`pull-v2-tab-${t.id}`}
          >
            {t.label as string}
          </AppLink>;
        }
      </nav>
    </div>

    <div className="flex flex-wrap items-center gap-2 text-sm text-muted-foreground">
      <span className="font-medium text-foreground">{props.pull.author_username as string}</span>
      <span>
        wants to merge <span className="font-mono">{headRef as string}</span> into{" "}
        <span className="font-mono">{props.pull.base_ref as string}</span>
      </span>
    </div>

    @if (props.tab === "overview") {
      <PullV2Overview
        owner={props.owner}
        repoName={props.repoName}
        number={props.number}
        repo={props.repo}
        pull={props.pull}
        canWrite={props.canWrite}
        canInteract={props.canInteract}
        isAuthor={props.isAuthor}
        currentUserId={props.currentUserId}
      />;
    }
    @if (props.tab === "commits") {
      <PullV2Commits owner={props.owner} repoName={props.repoName} number={props.number} />;
    }
    @if (props.tab === "checks") {
      <PullV2Checks
        owner={props.owner}
        repoName={props.repoName}
        sha={props.pull.head_sha}
      />;
    }
    @if (props.tab === "changes") {
      <PullV2Changes
        owner={props.owner}
        repoName={props.repoName}
        number={props.number}
        canInteract={props.canInteract}
        isAuthor={props.isAuthor}
        currentUserId={props.currentUserId}
        changesSha={props.changesSha}
        headSha={props.pull.head_sha}
      />;
    }
  </div>;
}
