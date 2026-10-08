import { queryOptions, useQuery, useQueryClient } from "@octanejs/tanstack-query";
import { navigate as astroNavigate } from "astro:transitions/client";
import { useRepoLayoutData } from "@/lib/repo-store.tsrx";
import { matchPath, routeHref } from "@/lib/route-params";
import { usePathname, useSearchString } from "@/lib/pathname.tsrx";
import { RepoLayout } from "@/routes/$owner.$repo.tsrx";
import { AppPage } from "@/components/app-page";
import type { IssuePublic, RepoPublic } from "@oxidean/api-client";
import { useEffect, useState } from "octane";
import { CircleCheck, CircleDot } from "@octanejs/lucide";
import { AuthErrorBanner } from "@/components/auth-shell";
import { IssueAssigneesPanel } from "@/components/repo/issue-assignees-panel";
import { IssueComments } from "@/components/repo/issue-comments";
import { IssueDeleteDialog } from "@/components/repo/issue-delete-dialog";
import { IssueHistoryPanel } from "@/components/repo/issue-history";
import { IssueLabelsPanel } from "@/components/repo/issue-labels-panel";
import { IssueLinkedPrsPanel } from "@/components/repo/issue-linked-prs";
import { IssueReactionsBar } from "@/components/repo/issue-reactions";
import { MarkdownHtml } from "@/components/repo/markdown-body";
import { MarkdownWritePreview } from "@/components/repo/markdown-write-preview";
import { RepoNotFound } from "@/components/repo/repo-not-found";
import { RepoUnitDisabled } from "@/components/repo/repo-unit-disabled";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { apiClient } from "@/lib/api-client";
import { renderGfm } from "@/lib/markdown";
import { formatRelative } from "@/lib/relative-time";

import { useChromeAccountState } from "@/lib/use-chrome-account";
import { fetchIssueGet, fetchRepoGet } from "@/lib/ssr-repo";

export type IssueDetailLoaderData =
  | { kind: "not_found" }
  | { kind: "error"; message: string }
  // Issues unit disabled for this repo (COL-13).
  | { kind: "disabled"; repo: RepoPublic }
  | { kind: "ready"; repo: RepoPublic; issue: IssuePublic };

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";

async function loadIssueDetail(
  owner: string,
  repoName: string,
  number: number,
): Promise<IssueDetailLoaderData> {
  const got = await fetchRepoGet({ owner, name: repoName });
  if (!got.ok) {
    if (got.error.code === "repo.not_found") return { kind: "not_found" };
    return {
      kind: "error",
      message: got.error.message || "Could not load repository.",
    };
  }
  // Unit toggle (COL-13): Issues off → disabled view, skip issue.get.
  if (got.data.issues_enabled === false) {
    return { kind: "disabled", repo: got.data };
  }
  const issueRes = await fetchIssueGet({ owner, name: repoName, number });
  if (!issueRes.ok) {
    if (issueRes.error.code === "repo.issues.disabled") {
      return { kind: "disabled", repo: got.data };
    }
    if (issueRes.error.code === "repo.not_found" || issueRes.error.code === "issue.not_found") {
      return { kind: "not_found" };
    }
    return {
      kind: "error",
      message: issueRes.error.message || "Could not load issue.",
    };
  }
  return { kind: "ready", repo: got.data, issue: issueRes.data };
}

function repoIssuesNQueryOptions(params: Record<string, string | undefined>, searchStr = "") {
  return queryOptions({
    queryKey: [
      "repo-page",
      "/$owner/$repo/issues/$n",
      params.owner ?? "",
      params.repo ?? "",
      params._splat ?? "",
      params.n ?? "",
      params.sha ?? "",
      params.tag ?? "",
      params.run ?? "",
      searchStr,
    ],
    queryFn: async (): Promise<IssueDetailLoaderData> => {
      const owner = String((params as { owner?: string }).owner ?? "");
      const repoName = String((params as { repo?: string }).repo ?? "");
      const n = Number((params as { n?: string }).n ?? "0");
      try {
        return await loadIssueDetail(owner, repoName, n);
      } catch {
        return { kind: "error", message: NETWORK_ERROR };
      }
    },
    enabled: Boolean(params.owner && params.repo),
    retry: false,
    staleTime: 30_000,
  });
}

/** Issue detail — edit/close/reopen/history/Admin delete (ISS-01 / D-ISS-02..04). */
function IssueDetailPageContent() @{
  const pathname = usePathname();
  const searchStr = useSearchString();
  const params = (matchPath("/$owner/$repo/issues/$n", pathname) ?? {}) as {
    owner?: string;
    repo?: string;
    n?: string;
  };
  const owner = params.owner ?? "";
  const repoName = params.repo ?? "";
  const number = Number(params.n ?? "0");
  const navigate = astroNavigate;
  const queryClient = useQueryClient();
  const account = useChromeAccountState();
  const layout = useRepoLayoutData();
  const routeLoader = useQuery(repoIssuesNQueryOptions(params, searchStr)).data;

  const [editing, setEditing] = useState(false);
  const [editTitle, setEditTitle] = useState("");
  const [editBody, setEditBody] = useState("");
  const [bodyHtml, setBodyHtml] = useState("");
  const [actionError, setActionError] = useState("");
  const [pending, setPending] = useState(false);
  const [deleteOpen, setDeleteOpen] = useState(false);
  const [deletePending, setDeletePending] = useState(false);
  const [deleteError, setDeleteError] = useState("");

  const repoQ = useQuery({
    queryKey: ["repo", "get", owner, repoName] as const,
    queryFn: async () => {
      const res = await apiClient.repo.get({ owner, name: repoName });
      if (!res.ok) throw new Error(res.error.message || NETWORK_ERROR);
      return res.data;
    },
    enabled: !!owner && !!repoName,
    initialData:
      routeLoader?.kind === "ready"
        ? routeLoader.repo
        : layout?.status === "ok"
          ? (layout.repo ?? undefined)
          : undefined,
  });

  const issueQ = useQuery({
    queryKey: ["issue", "get", owner, repoName, number] as const,
    queryFn: async () => {
      const res = await apiClient.issue.get({
        owner,
        name: repoName,
        number,
      });
      if (!res.ok) throw new Error(res.error.message || NETWORK_ERROR);
      return res.data;
    },
    enabled:
      !!owner &&
      !!repoName &&
      number > 0 &&
      routeLoader?.kind !== "disabled" &&
      (layout?.status === "ok" ? layout.repo?.issues_enabled !== false : true),
    initialData: routeLoader?.kind === "ready" ? routeLoader.issue : undefined,
  });

  const repo =
    repoQ.data ??
    (routeLoader?.kind === "disabled" ? routeLoader.repo : null) ??
    (layout?.status === "ok" ? layout.repo : null) ??
    null;
  const issue = issueQ.data ?? null;
  const unitDisabled = routeLoader?.kind === "disabled" || repo?.issues_enabled === false;
  const userId = account.user?.id ?? "";
  // GIT-20: archived repos hide all write affordances (edit/comment/close included).
  const notArchived = !repo?.archived;
  const canEdit =
    !!issue && notArchived && (!!repo?.can_write || (!!userId && issue.author_id === userId));
  const canWrite = !!repo?.can_write && notArchived;
  const canAdmin = !!repo?.can_admin;
  // Verified signed-in users participate on any repo they can view (D-ISS-20).
  const canInteract = !!account.user?.email_verified;
  const showClose = canEdit && issue?.state === "open";
  const showReopen = canEdit && issue?.state === "closed";
  const notFound =
    layout?.status === "not_found" ||
    routeLoader?.kind === "not_found" ||
    (!repoQ.isPending && !repo) ||
    (!issueQ.isPending && !issue && !issueQ.isError);
  const loading = !notFound && (!repo || !issue);

  useEffect(() => {
    if (!issue || editing) return;
    let cancelled = false;
    void (async () => {
      const html = await renderGfm(issue.body || "", {
        owner,
        repo: repoName,
      });
      if (!cancelled) setBodyHtml(html);
    })();
    return () => {
      cancelled = true;
    };
  }, [issue?.id, issue?.body, editing, owner, repoName]);

  async function invalidateIssueCaches() {
    await queryClient.invalidateQueries({
      queryKey: ["issue", "get", owner, repoName, number],
    });
    await queryClient.invalidateQueries({
      queryKey: ["issue", "history", owner, repoName, number],
    });
    await queryClient.invalidateQueries({
      queryKey: ["issue", "list", owner, repoName],
    });
  }

  function startEdit() {
    if (!issue) return;
    setEditTitle(issue.title);
    setEditBody(issue.body);
    setActionError("");
    setEditing(true);
  }

  async function saveEdit() {
    if (!issue) return;
    setActionError("");
    if (!editTitle.trim()) {
      setActionError("Title is required.");
      return;
    }
    setPending(true);
    try {
      const res = await apiClient.issue.update({
        owner,
        name: repoName,
        number: issue.number,
        title: editTitle.trim(),
        body: editBody,
      });
      if (!res.ok) {
        setActionError(res.error.message || "Could not update issue.");
        setPending(false);
        return;
      }
      await invalidateIssueCaches();
      setEditing(false);
      setPending(false);
    } catch {
      setActionError(NETWORK_ERROR);
      setPending(false);
    }
  }

  async function closeIssue() {
    if (!issue) return;
    setActionError("");
    setPending(true);
    try {
      const res = await apiClient.issue.close({
        owner,
        name: repoName,
        number: issue.number,
      });
      if (!res.ok) {
        setActionError(res.error.message || "Could not close issue.");
        setPending(false);
        return;
      }
      await invalidateIssueCaches();
      setPending(false);
    } catch {
      setActionError(NETWORK_ERROR);
      setPending(false);
    }
  }

  async function reopenIssue() {
    if (!issue) return;
    setActionError("");
    setPending(true);
    try {
      const res = await apiClient.issue.reopen({
        owner,
        name: repoName,
        number: issue.number,
      });
      if (!res.ok) {
        setActionError(res.error.message || "Could not reopen issue.");
        setPending(false);
        return;
      }
      await invalidateIssueCaches();
      setPending(false);
    } catch {
      setActionError(NETWORK_ERROR);
      setPending(false);
    }
  }

  async function confirmDelete(confirmNumber: number) {
    if (!issue) return;
    setDeleteError("");
    setDeletePending(true);
    try {
      const res = await apiClient.issue.delete({
        owner,
        name: repoName,
        number: issue.number,
        confirmNumber,
      });
      if (!res.ok) {
        setDeleteError(res.error.message || "Could not delete issue.");
        setDeletePending(false);
        return;
      }
      await queryClient.invalidateQueries({
        queryKey: ["issue", "list", owner, repoName],
      });
      setDeleteOpen(false);
      setDeletePending(false);
      await navigate(routeHref("/$owner/$repo/issues", { owner, repo: repoName }));
    } catch {
      setDeleteError(NETWORK_ERROR);
      setDeletePending(false);
    }
  }

  @if (notFound) {
    <RepoNotFound />;
  } @else {
    @if (unitDisabled) {
      <RepoUnitDisabled unit="issues" canAdmin={canAdmin} />;
    } @else if (loading) {
      <p className="text-[16px] text-muted-foreground">Loading issue…</p>;
    } @else {
      <>
        <div className="grid gap-8 lg:grid-cols-[minmax(0,1fr)_16rem]">
          <div className="flex flex-col gap-4">
            @if (actionError) {
              <AuthErrorBanner message={actionError} />;
            }
            <div className="flex flex-wrap items-start justify-between gap-3">
              <div className="min-w-0 flex-1">
                @if (editing) {
                  <div className="flex flex-col gap-2">
                    <Label htmlFor="issue-edit-title">Title</Label>
                    <Input
                      id="issue-edit-title"
                      value={editTitle}
                      onInput={(e) => setEditTitle((e.target as HTMLInputElement).value)}
                      disabled={pending}
                    />
                  </div>;
                } @else {
                  <h1
                    data-testid="issue-title"
                    className="text-[24px] font-semibold text-foreground"
                  >
                    {issue!.title as string}
                    <span className="ml-2 font-light text-muted-foreground">
                      {`#${issue!.number}` as string}
                    </span>
                  </h1>;
                }
                <div className="mt-2 flex flex-wrap items-center gap-2">
                  @if (issue!.state === "open") {
                    <Badge variant="open">
                      <CircleDot aria-hidden size={12} />
                      Open
                    </Badge>;
                  } @else {
                    <Badge variant="merged">
                      <CircleCheck aria-hidden size={12} />
                      Closed
                    </Badge>;
                  }
                  <p className="text-[14px] text-muted-foreground">
                    {
                      `@${issue!.author_username} opened this issue ${formatRelative(issue!.created_at)}` as string
                    }
                    {
                      ` · ${issue!.comment_count ?? 0} comment${(issue!.comment_count ?? 0) === 1 ? "" : "s"}` as string
                    }
                  </p>
                </div>
              </div>
              <div className="flex flex-wrap gap-2">
                @if (canEdit && !editing) {
                  <Button type="button" variant="outline" onClick={startEdit}>
                    Edit
                  </Button>;
                }
                @if (showClose) {
                  <Button
                    type="button"
                    variant="outline"
                    disabled={pending}
                    onClick={closeIssue}
                  >
                    Close issue
                  </Button>;
                }
                @if (showReopen) {
                  <Button
                    type="button"
                    variant="outline"
                    disabled={pending}
                    onClick={reopenIssue}
                  >
                    Reopen
                  </Button>;
                }
                @if (canAdmin && notArchived) {
                  <Button
                    type="button"
                    variant="destructive"
                    onClick={() => {
                      setDeleteError("");
                      setDeleteOpen(true);
                    }}
                  >
                    Delete issue
                  </Button>;
                }
              </div>
            </div>

            @if (editing) {
              <div className="flex flex-col gap-2">
                <MarkdownWritePreview
                  id="issue-edit-body"
                  value={editBody}
                  onValueChange={setEditBody}
                  rows={10}
                  disabled={pending}
                  owner={owner}
                  repo={repoName}
                />
                <div className="flex gap-2">
                  <Button type="button" disabled={pending} onClick={saveEdit}>
                    Save
                  </Button>
                  <Button
                    type="button"
                    variant="outline"
                    disabled={pending}
                    onClick={() => {
                      setEditing(false);
                      setActionError("");
                    }}
                  >
                    Cancel
                  </Button>
                </div>
              </div>;
            } @else {
              <MarkdownHtml
                html={bodyHtml as string}
                className="rounded-md border border-border px-4 py-3"
                data-testid="issue-body"
              />;
            }

            <IssueReactionsBar
              owner={owner}
              repoName={repoName}
              number={number}
              target="issue"
              reactions={issue?.reactions ?? []}
              canInteract={canInteract}
            />

            <IssueHistoryPanel owner={owner} repoName={repoName} number={number} />

            <IssueComments
              owner={owner}
              repoName={repoName}
              number={number}
              canWrite={canWrite}
              canInteract={canInteract}
              currentUserId={userId}
            />
          </div>
          <aside className="flex flex-col gap-6">
            <IssueLabelsPanel
              owner={owner}
              repoName={repoName}
              number={number}
              issue={issue!}
              canWrite={canWrite}
            />
            <IssueAssigneesPanel
              owner={owner}
              repoName={repoName}
              number={number}
              issue={issue!}
              canWrite={canWrite}
            />
            <IssueLinkedPrsPanel
              owner={owner}
              repoName={repoName}
              number={number}
              canWrite={canWrite}
            />
          </aside>
        </div>

        <IssueDeleteDialog
          open={deleteOpen}
          onOpenChange={setDeleteOpen}
          issueNumber={issue!.number}
          pending={deletePending}
          error={deleteError}
          onConfirm={confirmDelete}
        />
      </>;
    }
  }
}

export default IssueDetailPage;

/** Page root — RepoLayout (chrome + repo store) around the leaf content. */
export function IssueDetailPage() @{
  <AppPage>
    <RepoLayout>
      <IssueDetailPageContent />
    </RepoLayout>
  </AppPage>;
}
