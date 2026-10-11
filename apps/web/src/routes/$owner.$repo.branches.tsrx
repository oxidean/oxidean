import type { RepoPublic, RepoRefEntry } from "@oxidean/api-client";
import { useQueryClient, queryOptions, useQuery } from "@octanejs/tanstack-query";

import { matchPath } from "@/lib/route-params";
import { usePathname, useSearchString } from "@/lib/pathname.tsrx";
import { RepoLayout } from "@/routes/$owner.$repo.tsrx";
import { AppPage } from "@/components/app-page";
import { useEffect, useState } from "octane";
import { GitBranch, Plus } from "@octanejs/lucide";
import { RepoNotFound } from "@/components/repo/repo-not-found";
import { Badge } from "@/components/ui/badge";
import { EmptyState } from "@/components/ui/empty-state";
import { List, ListRow } from "@/components/ui/list";
import { PageHeader } from "@/components/ui/page-header";
import { Skeleton } from "@/components/ui/skeleton";
import {
  AlertDialog,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { apiClient } from "@/lib/api-client";
import { isBranchRef, shortOid, shortRefName, treeHref } from "@/lib/repo-browse";
import { formatRelative } from "@/lib/relative-time";
import { fetchRepoGet, fetchRepoRefs } from "@/lib/ssr-repo";
import { AppLink } from "@/components/ui/app-link";

export type BranchesLoaderData =
  | { kind: "not_found" }
  | { kind: "error"; message: string }
  | { kind: "ready"; repo: RepoPublic; branches: RepoRefEntry[] };

type Phase = { kind: "loading" } | BranchesLoaderData;

type DialogState =
  | { kind: "none" }
  | { kind: "create" }
  | { kind: "rename"; from: string }
  | { kind: "delete"; branch: string };

const DEFAULT_SOFT_PROTECT_TITLE = "Default branch can’t be renamed or deleted here.";
const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";

function sortBranches(refs: RepoRefEntry[]): RepoRefEntry[] {
  return refs
    .filter((r) => isBranchRef(r.name))
    .sort((a, b) =>
      shortRefName(a.name).localeCompare(shortRefName(b.name), undefined, {
        sensitivity: "base",
      }),
    );
}

async function loadBranchesPage(owner: string, repoName: string): Promise<BranchesLoaderData> {
  const got = await fetchRepoGet({ owner, name: repoName });
  if (!got.ok) {
    if (got.error.code === "repo.not_found") return { kind: "not_found" };
    return {
      kind: "error",
      message: got.error.message || "Could not load repository.",
    };
  }
  const refsRes = await fetchRepoRefs({ owner, name: repoName });
  if (!refsRes.ok) {
    if (refsRes.error.code === "repo.not_found") return { kind: "not_found" };
    return {
      kind: "error",
      message: refsRes.error.message || "Could not load branches.",
    };
  }
  return {
    kind: "ready",
    repo: got.data,
    branches: sortBranches(refsRes.data.refs),
  };
}

function repoBranchesQueryOptions(params: Record<string, string | undefined>, searchStr = "") {
  return queryOptions({
    queryKey: [
      "repo-page",
      "/$owner/$repo/branches",
      params.owner ?? "",
      params.repo ?? "",
      params._splat ?? "",
      params.n ?? "",
      params.sha ?? "",
      params.tag ?? "",
      params.run ?? "",
      searchStr,
    ],
    queryFn: async (): Promise<BranchesLoaderData> => {
      const owner = String((params as { owner?: string }).owner ?? "");
      const repoName = String((params as { repo?: string }).repo ?? "");
      try {
        return await loadBranchesPage(owner, repoName);
      } catch {
        return { kind: "error", message: NETWORK_ERROR };
      }
    },
    enabled: Boolean(params.owner && params.repo),
    retry: false,
    staleTime: 30_000,
  });
}

function RepoBranchesPageContent() @{
  const pathname = usePathname();
  const searchStr = useSearchString();
  const params = (matchPath("/$owner/$repo/branches", pathname) ?? {}) as {
    owner: string;
    repo: string;
  };
  const owner = params.owner ?? "";
  const repoName = params.repo ?? "";
  const routerQueryClient = useQueryClient();
  const loaderData = useQuery(repoBranchesQueryOptions(params, searchStr)).data;

  const [phase, setPhase] = useState<Phase>(() => loaderData ?? { kind: "loading" });
  const [dialog, setDialog] = useState<DialogState>({ kind: "none" });
  const [branchName, setBranchName] = useState("");
  const [startRef, setStartRef] = useState("");
  const [confirmName, setConfirmName] = useState("");
  const [pending, setPending] = useState(false);
  const [formError, setFormError] = useState("");

  useEffect(() => {
    if (loaderData) {
      setPhase(loaderData);
      return;
    }
    let cancelled = false;
    void (async () => {
      setPhase({ kind: "loading" });
      try {
        const next = await loadBranchesPage(owner, repoName);
        if (!cancelled) setPhase(next);
      } catch {
        if (!cancelled) setPhase({ kind: "error", message: NETWORK_ERROR });
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [owner, repoName, loaderData]);

  const canWrite = phase.kind === "ready" && !!phase.repo.can_write;

  function openCreate() {
    if (phase.kind !== "ready") return;
    setFormError("");
    setBranchName("");
    setStartRef(phase.repo.default_branch || "main");
    setDialog({ kind: "create" });
  }

  function openRename(from: string) {
    setFormError("");
    setBranchName(from);
    setDialog({ kind: "rename", from });
  }

  function openDelete(branch: string) {
    setFormError("");
    setConfirmName("");
    setDialog({ kind: "delete", branch });
  }

  function closeDialog() {
    if (pending) return;
    setDialog({ kind: "none" });
    setFormError("");
    setConfirmName("");
  }

  async function refreshBranches() {
    await routerQueryClient.invalidateQueries();
  }

  async function submitCreate() {
    if (phase.kind !== "ready") return;
    const name = branchName.trim();
    if (!name) {
      setFormError("Enter a branch name.");
      return;
    }
    setPending(true);
    setFormError("");
    try {
      const res = await apiClient.repo.branchCreate({
        owner,
        name: repoName,
        branch: name,
        start: startRef.trim() || phase.repo.default_branch,
      });
      if (!res.ok) {
        setFormError(res.error.message || "Could not create branch.");
        setPending(false);
        return;
      }
      setPending(false);
      setDialog({ kind: "none" });
      await refreshBranches();
    } catch {
      setFormError(NETWORK_ERROR);
      setPending(false);
    }
  }

  async function submitRename() {
    if (phase.kind !== "ready" || dialog.kind !== "rename") return;
    const to = branchName.trim();
    if (!to) {
      setFormError("Enter a new branch name.");
      return;
    }
    setPending(true);
    setFormError("");
    try {
      const res = await apiClient.repo.branchRename({
        owner,
        name: repoName,
        from: dialog.from,
        to,
      });
      if (!res.ok) {
        setFormError(res.error.message || "Could not rename branch.");
        setPending(false);
        return;
      }
      setPending(false);
      setDialog({ kind: "none" });
      await refreshBranches();
    } catch {
      setFormError(NETWORK_ERROR);
      setPending(false);
    }
  }

  async function submitDelete() {
    if (phase.kind !== "ready" || dialog.kind !== "delete") return;
    if (confirmName.trim() !== dialog.branch) return;
    setPending(true);
    setFormError("");
    try {
      const res = await apiClient.repo.branchDelete({
        owner,
        name: repoName,
        branch: dialog.branch,
      });
      if (!res.ok) {
        setFormError(res.error.message || "Could not delete branch.");
        setPending(false);
        return;
      }
      setPending(false);
      setDialog({ kind: "none" });
      setConfirmName("");
      await refreshBranches();
    } catch {
      setFormError(NETWORK_ERROR);
      setPending(false);
    }
  }

  @if (phase.kind === "not_found") {
    <RepoNotFound />;
  } @else if (phase.kind === "loading") {
    <>
      <PageHeader icon={<GitBranch aria-hidden size={22} />} title="Branches" />
      <List className="mt-6">
        @for (const i of [0, 1, 2]; key i) {
          <ListRow>
            <div className="flex flex-1 flex-col gap-2">
              <Skeleton className="h-4 w-48" />
              <Skeleton className="h-3 w-24" />
            </div>
          </ListRow>;
        }
      </List>
    </>;
  } @else if (phase.kind === "error") {
    <div className="flex flex-col gap-4">
      <p role="alert" className="text-[16px] text-destructive">
        {phase.message as string}
      </p>
      <Button type="button" variant="outline" onClick={() => void refreshBranches()}>
        Try again
      </Button>
    </div>;
  } @else {
    <>
      <PageHeader
        icon={<GitBranch aria-hidden size={22} />}
        title="Branches"
        actions={
          canWrite ? (
            <Button type="button" onClick={() => openCreate()}>
              <Plus aria-hidden size={15} />
              New branch
            </Button>
          ) : undefined
        }
      />

      @if (phase.branches.length === 0) {
        <EmptyState
          className="mt-6"
          icon={<GitBranch aria-hidden size={22} />}
          title="No branches yet"
          description="Push a branch to this repository and it will show up here."
        />;
      } @else {
        <List className="mt-6">
          @for (const b of phase.branches; key b.name) {
            <BranchRow
              owner={owner}
              repo={repoName}
              branch={b}
              defaultBranch={phase.repo.default_branch}
              canWrite={canWrite}
              onRename={(name) => openRename(name)}
              onDelete={(name) => openDelete(name)}
            />;
          }
        </List>;
      }

      <Dialog
        open={dialog.kind === "create" || dialog.kind === "rename"}
        onOpenChange={(open) => {
          if (!open) closeDialog();
        }}
      >
        <DialogContent showCloseButton={!pending}>
          <DialogHeader>
            <DialogTitle>
              {(dialog.kind === "rename" ? "Rename branch" : "New branch") as string}
            </DialogTitle>
            <DialogDescription>
              {
                (dialog.kind === "rename"
                  ? `Rename “${dialog.from}” to a new name.`
                  : "Create a branch from an existing ref.") as string
              }
            </DialogDescription>
          </DialogHeader>
          <div className="flex flex-col gap-3">
            <div className="flex flex-col gap-2">
              <Label htmlFor="branch-name-input">Branch name</Label>
              <Input
                id="branch-name-input"
                value={branchName}
                onInput={(e) => setBranchName((e.currentTarget as HTMLInputElement).value)}
                autoComplete="off"
                disabled={pending}
              />
            </div>
            @if (dialog.kind === "create") {
              <div className="flex flex-col gap-2">
                <Label htmlFor="branch-start-input">Start point</Label>
                <Input
                  id="branch-start-input"
                  value={startRef}
                  onInput={(e) => setStartRef((e.currentTarget as HTMLInputElement).value)}
                  autoComplete="off"
                  disabled={pending}
                />
              </div>;
            }
            @if (formError) {
              <p role="alert" className="text-[14px] text-destructive">
                {formError as string}
              </p>;
            }
          </div>
          <DialogFooter>
            <DialogClose
              disabled={pending}
              className="inline-flex h-11 items-center justify-center rounded-md border-2 border-secondary bg-secondary/5 px-4 text-[14px] font-semibold text-secondary"
            >
              Cancel
            </DialogClose>
            <Button
              type="button"
              disabled={pending}
              onClick={() => void (dialog.kind === "rename" ? submitRename() : submitCreate())}
            >
              {
                (pending
                  ? "Working…"
                  : dialog.kind === "rename"
                    ? "Rename branch"
                    : "Create branch") as string
              }
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <AlertDialog
        open={dialog.kind === "delete"}
        onOpenChange={(open) => {
          if (!open) closeDialog();
        }}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Delete branch?</AlertDialogTitle>
            <AlertDialogDescription>
              This deletes the branch from the repository. Type the branch name to confirm.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <div className="flex flex-col gap-2">
            <Label htmlFor="delete-branch-confirm">
              {
                (dialog.kind === "delete"
                  ? `Type ${dialog.branch} to confirm`
                  : "Confirm") as string
              }
            </Label>
            <Input
              id="delete-branch-confirm"
              value={confirmName}
              onInput={(e) => setConfirmName((e.currentTarget as HTMLInputElement).value)}
              autoComplete="off"
              disabled={pending}
            />
            @if (formError) {
              <p role="alert" className="text-[14px] text-destructive">
                {formError as string}
              </p>;
            }
          </div>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={pending}>Cancel</AlertDialogCancel>
            <Button
              type="button"
              variant="destructive"
              disabled={pending || dialog.kind !== "delete" || confirmName.trim() !== dialog.branch}
              onClick={() => void submitDelete()}
            >
              {(pending ? "Deleting…" : "Delete branch") as string}
            </Button>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>;
  }
}

function BranchRow({
  owner,
  repo,
  branch,
  defaultBranch,
  canWrite,
  onRename,
  onDelete,
}: {
  owner: string;
  repo: string;
  branch: RepoRefEntry;
  defaultBranch: string;
  canWrite: boolean;
  onRename: (name: string) => void;
  onDelete: (name: string) => void;
}) @{
  const name = shortRefName(branch.name);
  const isDefault = name === defaultBranch;
  const tip = shortOid(branch.oid);
  const href = treeHref(owner, repo, name, "");

  <ListRow className="flex-wrap gap-x-4 gap-y-2">
    <GitBranch aria-hidden size={16} className="shrink-0 text-muted-foreground" />
    <div className="min-w-0 flex-1">
      <div className="flex flex-wrap items-center gap-2">
        <AppLink
          href={href as string}
          className="truncate text-[15px] font-semibold text-primary no-underline hover:underline"
          title={name as string}
        >
          {name as string}
        </AppLink>
        @if (isDefault) {
          <Badge variant="primary">default</Badge>;
        }
      </div>
      <p className="mt-0.5 text-[12px] text-muted-foreground">
        @if (branch.tip_committed_at) {
          <>
            {
              `Updated ${formatRelative(branch.tip_committed_at)}${branch.tip_author_name ? ` by ${branch.tip_author_name}` : ""}` as string
            }
            <span className="mx-1.5">·</span>
          </>;
        }
        <span className="font-mono" title={branch.oid as string}>
          {tip as string}
        </span>
      </p>
    </div>
    <div className="flex flex-wrap items-center gap-2">
      @if (canWrite && !isDefault) {
        <AppLink
          href={`/${owner}/${repo}/pulls/new?head=${encodeURIComponent(name)}` as string}
          className="inline-flex h-8 items-center justify-center rounded-md border border-border bg-transparent px-3 text-[13px] font-medium text-foreground no-underline hover:bg-muted"
        >
          New pull request
        </AppLink>;
      }
      @if (canWrite) {
        <Button
          type="button"
          variant="outline"
          size="sm"
          disabled={isDefault}
          title={isDefault ? DEFAULT_SOFT_PROTECT_TITLE : undefined}
          onClick={() => {
            if (!isDefault) onRename(name);
          }}
        >
          Rename
        </Button>;
      }
      @if (canWrite) {
        <Button
          type="button"
          variant="destructive"
          size="sm"
          disabled={isDefault}
          title={isDefault ? DEFAULT_SOFT_PROTECT_TITLE : undefined}
          onClick={() => {
            if (!isDefault) onDelete(name);
          }}
        >
          Delete
        </Button>;
      }
    </div>
  </ListRow>;
}

/** Page root — RepoLayout (chrome + repo store) around the leaf content. */
export function RepoBranchesPage() @{
  <AppPage>
    <RepoLayout>
      <RepoBranchesPageContent />
    </RepoLayout>
  </AppPage>;
}
