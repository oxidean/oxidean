import type { RepoCommitSummary, RepoTreeEntry } from "@oxidean/api-client";
import { FileCode2, Folder, Package } from "@octanejs/lucide";
import {
  blobHref,
  joinRepoPath,
  parentRepoPath,
  sortTreeEntries,
  treeHref,
} from "@/lib/repo-browse";
import { formatRelativeUpdated } from "@/lib/relative-time";
import { AppLink } from "@/components/ui/app-link";

type FileTreeProps = {
  owner: string;
  repo: string;
  refName: string;
  basePath: string;
  entries: RepoTreeEntry[];
  /** Optional last-commit map keyed by entry basename (issue #23). */
  lastCommits?: Record<string, RepoCommitSummary>;
};

function relativeCommitTime(iso: string): string {
  return formatRelativeUpdated(iso).replace(/^Updated\s+/i, "");
}

/** Sorted directory listing — dirs/gitlinks first; commit columns when provided (D-15, issue #23). */
export function FileTree({
  owner,
  repo,
  refName,
  basePath,
  entries,
  lastCommits,
}: FileTreeProps) @{
  const sorted = sortTreeEntries(entries);
  const nested = Boolean(basePath.replace(/^\/+|\/+$/g, ""));
  const parentHref = nested ? treeHref(owner, repo, refName, parentRepoPath(basePath)) : "";
  const hasHistory = lastCommits != null;
  const rounded = hasHistory
    ? "rounded-b-md border border-border"
    : "rounded-md border border-border";

  <ul
    role="list"
    aria-label="Directories and files"
    className={(rounded + " divide-y divide-border overflow-y-auto bg-card/40") as string}
    data-testid="repo-file-tree"
  >
    @if (nested) {
      <li className="flex min-h-11 items-center gap-3 px-3 py-2">
        <Folder aria-hidden size={16} className="shrink-0 text-muted-foreground" />
        <AppLink
          href={parentHref as string}
          className="min-w-0 flex-1 truncate text-[14px] font-normal text-muted-foreground no-underline hover:underline"
          title="Parent directory"
        >
          ..
        </AppLink>
      </li>;
    }
    @for (const entry of sorted; key entry.oid + entry.name) {
      const childPath = joinRepoPath(basePath, entry.name);
      const isTree = entry.kind === "tree";
      const isLink = entry.kind === "commit";
      const href = isTree
        ? treeHref(owner, repo, refName, childPath)
        : isLink
          ? treeHref(owner, repo, refName, childPath)
          : blobHref(owner, repo, refName, childPath);
      const last = lastCommits ? lastCommits[entry.name] : undefined;
      <li className="grid min-h-11 grid-cols-1 items-center gap-x-3 gap-y-1 px-3 py-2 md:grid-cols-[minmax(0,1fr)_minmax(0,1.2fr)_7rem]">
        <div className="flex min-w-0 items-center gap-3">
          @if (isTree) {
            <Folder aria-hidden size={16} className="shrink-0 text-muted-foreground" />;
          } @else {
            @if (isLink) {
              <Package
                aria-hidden
                size={16}
                className="shrink-0 text-muted-foreground"
              />;
            } @else {
              <FileCode2
                aria-hidden
                size={16}
                className="shrink-0 text-muted-foreground"
              />;
            }
          }
          <AppLink
            href={href as string}
            className="min-w-0 flex-1 truncate text-[14px] font-normal text-foreground no-underline hover:underline"
            title={
              isLink
                ? (`Submodule ${entry.name} @ ${entry.oid.slice(0, 7)}` as string)
                : (entry.name as string)
            }
          >
            {entry.name as string}
          </AppLink>
          @if (isLink) {
            <span className="shrink-0 font-mono text-[12px] text-muted-foreground md:hidden">
              {entry.oid.slice(0, 7) as string}
            </span>;
          }
        </div>
        @if (last) {
          <>
            <AppLink
              href={`/${owner}/${repo}/commit/${last.sha}` as string}
              className="hidden min-w-0 truncate text-[13px] text-muted-foreground no-underline hover:text-foreground hover:underline md:block"
              title={last.subject as string}
            >
              {last.subject as string}
            </AppLink>
            <time
              datetime={last.authored_at as string}
              className="text-[12px] text-muted-foreground md:text-right"
            >
              {relativeCommitTime(last.authored_at) as string}
            </time>
          </>;
        } @else {
          <>
            @if (hasHistory) {
              <>
                <span className="hidden md:block" />
                <span className="hidden md:block" />
              </>;
            }
            @if (isLink) {
              <span className="hidden shrink-0 font-mono text-[12px] text-muted-foreground md:block md:text-right">
                {entry.oid.slice(0, 7) as string}
              </span>;
            }
          </>;
        }
      </li>;
    } @empty {
      @if (!nested) {
        <li className="px-3 py-6 text-center text-[14px] text-muted-foreground">
          This directory is empty.
        </li>;
      }
    }
  </ul>;
}
