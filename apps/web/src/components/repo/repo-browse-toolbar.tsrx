import type { RepoRefEntry } from "@oxidean/api-client";
import { ChevronDown, FilePlus2, FolderPlus, Upload } from "@octanejs/lucide";
import { CloneBox } from "@/components/repo/clone-box";
import { PathBreadcrumb } from "@/components/repo/path-breadcrumb";
import { RefSelect } from "@/components/repo/ref-select";
import { buttonVariants } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuLinkItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { mkdirHref, newFileHref, uploadHref } from "@/lib/repo-browse";
import { cn } from "@/lib/utils";

type RepoBrowseToolbarProps = {
  owner: string;
  repo: string;
  refName: string;
  refs: RepoRefEntry[];
  /** Directory under ref — empty on repo home. */
  path?: string;
  /** Show path crumbs after the ref select (tree/blob pages). */
  showPathCrumbs?: boolean;
  empty?: boolean;
  /** Write+ on the repo — shows the web file-editing "Add file" menu (GIT-19). */
  canWrite?: boolean;
};

/**
 * Shared Code / tree toolbar — ref select, optional path crumbs, web file
 * editing ("Add file"), clone/download (matches repo home layout).
 */
export function RepoBrowseToolbar({
  owner,
  repo,
  refName,
  refs,
  path = "",
  showPathCrumbs = false,
  empty = false,
  canWrite = false,
}: RepoBrowseToolbarProps) @{
  const mode = showPathCrumbs ? "tree" : "home";

  <div className="mb-4 flex flex-wrap items-center gap-3" data-testid="repo-browse-toolbar">
    <RefSelect
      owner={owner}
      repo={repo}
      refs={refs}
      value={refName}
      pathUnderRef={path}
      mode={mode}
    />
    @if (showPathCrumbs) {
      <div className="min-w-0 flex-1">
        <PathBreadcrumb owner={owner} repo={repo} refName={refName} path={path} />
      </div>;
    }
    <div
      className={cn("flex items-center gap-2", showPathCrumbs ? "shrink-0 sm:ml-auto" : "ml-auto")}
    >
      @if (canWrite) {
        <DropdownMenu>
          <DropdownMenuTrigger
            className={cn(
              buttonVariants({ variant: "secondary" }),
              "h-8 gap-1.5 px-2.5 text-[13px]",
            )}
            data-testid="add-file-menu"
          >
            Add file
            <ChevronDown aria-hidden size={14} />
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            <DropdownMenuLinkItem
              data-testid="add-file-new"
              href={newFileHref(owner, repo, refName, path)}
            >
              <FilePlus2 aria-hidden size={14} />
              Create new file
            </DropdownMenuLinkItem>
            <DropdownMenuLinkItem
              data-testid="add-file-mkdir"
              href={mkdirHref(owner, repo, refName, path)}
            >
              <FolderPlus aria-hidden size={14} />
              Create directory
            </DropdownMenuLinkItem>
            <DropdownMenuLinkItem
              data-testid="add-file-upload"
              href={uploadHref(owner, repo, refName, path)}
            >
              <Upload aria-hidden size={14} />
              Upload files
            </DropdownMenuLinkItem>
          </DropdownMenuContent>
        </DropdownMenu>;
      }
      <CloneBox owner={owner} repo={repo} refName={refName} empty={empty} />
    </div>
  </div>;
}
