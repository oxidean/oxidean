import { queryOptions, useQuery, useQueryClient } from "@octanejs/tanstack-query";
import { useRepoLayoutData } from "@/lib/repo-store.tsrx";
import { matchPath } from "@/lib/route-params";
import { usePathname, useSearchString } from "@/lib/pathname.tsrx";
import { RepoLayout } from "@/routes/$owner.$repo.tsrx";
import { AppPage } from "@/components/app-page";
import type { MilestonePublic, RepoPublic } from "@oxidean/api-client";
import { useForm } from "@octanejs/tanstack-form";
import { useState } from "octane";
import { AuthErrorBanner } from "@/components/auth-shell";
import { RepoNotFound } from "@/components/repo/repo-not-found";
import { RepoUnitDisabled } from "@/components/repo/repo-unit-disabled";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { apiClient } from "@/lib/api-client";

import { fetchRepoGet } from "@/lib/ssr-repo";
import { AppLink } from "@/components/ui/app-link";

export type RepoMilestonesLoaderData =
  | { kind: "not_found" }
  | { kind: "error"; message: string }
  // Issues unit disabled for this repo (COL-13).
  | { kind: "disabled"; repo: RepoPublic }
  | { kind: "ready"; repo: RepoPublic; milestones: MilestonePublic[] };

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";

function milestonesQueryKey(owner: string, name: string) {
  return ["milestone", "list", owner, name] as const;
}

type MilestoneStateFilter = "open" | "closed" | "all";

function visibleMilestones(
  milestones: MilestonePublic[],
  state: MilestoneStateFilter,
): MilestonePublic[] {
  if (state === "all") return milestones;
  return milestones.filter((m) => m.state === state);
}

function milestoneProgressPct(m: MilestonePublic): number {
  const total = (m.open_issues ?? 0) + (m.closed_issues ?? 0);
  if (total === 0) return 0;
  return Math.round(((m.closed_issues ?? 0) / total) * 100);
}

function dueLabel(m: MilestonePublic): string {
  if (!m.due_on) return "No due date";
  const day = m.due_on.slice(0, 10);
  const due = new Date(`${day}T00:00:00Z`);
  const today = new Date();
  const todayUtc = new Date(
    Date.UTC(today.getUTCFullYear(), today.getUTCMonth(), today.getUTCDate()),
  );
  const past = due < todayUtc && m.state === "open";
  return `${past ? "Past due " : "Due by "}${day}`;
}

async function loadRepoMilestones(
  owner: string,
  repoName: string,
): Promise<RepoMilestonesLoaderData> {
  const got = await fetchRepoGet({ owner, name: repoName });
  if (!got.ok) {
    if (got.error.code === "repo.not_found") return { kind: "not_found" };
    return {
      kind: "error",
      message: got.error.message || "Could not load repository.",
    };
  }
  if (got.data.issues_enabled === false) {
    return { kind: "disabled", repo: got.data };
  }
  const list = await apiClient.milestone.list({
    owner,
    name: repoName,
    state: "all",
  });
  if (!list.ok) {
    if (list.error.code === "repo.not_found") return { kind: "not_found" };
    if (list.error.code === "repo.issues.disabled") {
      return { kind: "disabled", repo: got.data };
    }
    return {
      kind: "error",
      message: list.error.message || "Could not load milestones.",
    };
  }
  return { kind: "ready", repo: got.data, milestones: list.data.milestones };
}

function repoIssuesMilestonesQueryOptions(
  params: Record<string, string | undefined>,
  searchStr = "",
) {
  return queryOptions({
    queryKey: [
      "repo-page",
      "/$owner/$repo/issues/milestones",
      params.owner ?? "",
      params.repo ?? "",
      params._splat ?? "",
      params.n ?? "",
      params.sha ?? "",
      params.tag ?? "",
      params.run ?? "",
      searchStr,
    ],
    queryFn: async (): Promise<RepoMilestonesLoaderData> => {
      const owner = String((params as { owner?: string }).owner ?? "");
      const repoName = String((params as { repo?: string }).repo ?? "");
      try {
        return await loadRepoMilestones(owner, repoName);
      } catch {
        return { kind: "error", message: NETWORK_ERROR };
      }
    },
    enabled: Boolean(params.owner && params.repo),
    retry: false,
    staleTime: 30_000,
  });
}

/** Repo milestones — list + create/edit/close/delete for Write+ (D-ISS milestones). */
function RepoIssueMilestonesPageContent() @{
  const pathname = usePathname();
  const searchStr = useSearchString();
  const params = (matchPath("/$owner/$repo/issues/milestones", pathname) ?? {}) as {
    owner?: string;
    repo?: string;
  };
  const owner = params.owner ?? "";
  const repoName = params.repo ?? "";
  const layout = useRepoLayoutData();
  const routeLoader = useQuery(repoIssuesMilestonesQueryOptions(params, searchStr)).data;
  const queryClient = useQueryClient();

  const repoQ = useQuery({
    queryKey: ["repo", "get", owner, repoName] as const,
    queryFn: async () => {
      const res = await apiClient.repo.get({ owner, name: repoName });
      if (!res.ok) throw new Error(res.error.message || NETWORK_ERROR);
      return res.data;
    },
    enabled: !!owner && !!repoName,
    initialData:
      routeLoader?.kind === "ready" || routeLoader?.kind === "disabled"
        ? routeLoader.repo
        : layout?.status === "ok"
          ? (layout.repo ?? undefined)
          : undefined,
  });

  const milestonesQ = useQuery({
    queryKey: milestonesQueryKey(owner, repoName),
    queryFn: async () => {
      const res = await apiClient.milestone.list({
        owner,
        name: repoName,
        state: "all",
      });
      if (!res.ok) throw new Error(res.error.message || NETWORK_ERROR);
      return res.data.milestones;
    },
    enabled: !!owner && !!repoName && routeLoader?.kind !== "disabled",
    initialData: routeLoader?.kind === "ready" ? routeLoader.milestones : undefined,
  });

  const [stateFilter, setStateFilter] = useState<MilestoneStateFilter>("open");
  const [formError, setFormError] = useState("");
  const [pending, setPending] = useState(false);
  const [rowError, setRowError] = useState("");
  const [editing, setEditing] = useState<MilestonePublic | null>(null);
  const [formOpen, setFormOpen] = useState(false);

  const form = useForm({
    defaultValues: {
      title: "",
      description: "",
      due_on: "",
      state: "open",
    },
    onSubmit: async ({ value }) => {
      setFormError("");
      const trimmed = value.title.trim();
      if (!trimmed) {
        setFormError("Title is required.");
        return;
      }
      setPending(true);
      try {
        const due = value.due_on.trim() ? value.due_on.trim() : null;
        const res = editing
          ? await apiClient.milestone.update({
              owner,
              name: repoName,
              number: editing.number,
              title: trimmed,
              description: value.description.trim() || null,
              due_on: due,
              state: value.state === "closed" ? "closed" : "open",
            })
          : await apiClient.milestone.create({
              owner,
              name: repoName,
              title: trimmed,
              description: value.description.trim() || null,
              due_on: due,
            });
        if (!res.ok) {
          setFormError(res.error.message || NETWORK_ERROR);
          setPending(false);
          return;
        }
        form.reset();
        setEditing(null);
        setFormOpen(false);
        setPending(false);
        await refresh();
      } catch {
        setFormError(NETWORK_ERROR);
        setPending(false);
      }
    },
  });

  const repo = repoQ.data ?? null;
  const unitDisabled = routeLoader?.kind === "disabled" || repo?.issues_enabled === false;
  // GIT-20: archived repos hide write affordances (server still rejects mutations).
  const canWrite = !!repo?.can_write && !repo?.archived;
  const milestones: MilestonePublic[] = milestonesQ.data ?? [];
  const openCount = milestones.filter((m) => m.state === "open").length;
  const closedCount = milestones.filter((m) => m.state === "closed").length;
  const shown = visibleMilestones(milestones, stateFilter);
  const notFound =
    layout?.status === "not_found" ||
    routeLoader?.kind === "not_found" ||
    (!repoQ.isPending && !repo && routeLoader?.kind !== "ready");

  async function refresh() {
    await queryClient.invalidateQueries({
      queryKey: milestonesQueryKey(owner, repoName),
    });
    await queryClient.invalidateQueries({
      queryKey: ["issue", "list", owner, repoName],
    });
  }

  function startCreate() {
    setFormError("");
    setEditing(null);
    form.reset();
    setFormOpen(true);
  }

  function startEdit(m: MilestonePublic) {
    setFormError("");
    setEditing(m);
    form.reset();
    form.setFieldValue("title", m.title);
    form.setFieldValue("description", m.description);
    form.setFieldValue("due_on", m.due_on ? m.due_on.slice(0, 10) : "");
    form.setFieldValue("state", m.state === "closed" ? "closed" : "open");
    setFormOpen(true);
  }

  function cancelForm() {
    setFormError("");
    setEditing(null);
    form.reset();
    setFormOpen(false);
  }

  async function toggleState(m: MilestonePublic) {
    setRowError("");
    try {
      const res = await apiClient.milestone.update({
        owner,
        name: repoName,
        number: m.number,
        state: m.state === "open" ? "closed" : "open",
      });
      if (!res.ok) {
        setRowError(res.error.message || NETWORK_ERROR);
        return;
      }
      await refresh();
    } catch {
      setRowError(NETWORK_ERROR);
    }
  }

  async function deleteMilestone(m: MilestonePublic) {
    setRowError("");
    try {
      const res = await apiClient.milestone.delete({
        owner,
        name: repoName,
        number: m.number,
      });
      if (!res.ok) {
        setRowError(res.error.message || NETWORK_ERROR);
        return;
      }
      await refresh();
    } catch {
      setRowError(NETWORK_ERROR);
    }
  }

  @if (unitDisabled) {
    <RepoUnitDisabled unit="issues" canAdmin={!!repo?.can_admin} />;
  } @else if (notFound) {
    <RepoNotFound />;
  } @else {
    @if (!repo) {
      <p className="text-[16px] text-muted-foreground">Loading milestones…</p>;
    } @else {
      <>
        <div className="mb-4 flex flex-wrap items-center justify-between gap-3">
          <div>
            <h2 className="text-[18px] font-semibold text-foreground">Milestones</h2>
            <p className="text-[14px] text-muted-foreground">
              Group issues by release or project phase.
            </p>
          </div>
          <div className="flex flex-wrap items-center gap-3">
            <AppLink
              href={`/${owner}/${repoName}/issues` as string}
              className="text-[14px] text-primary no-underline hover:underline"
            >
              Back to issues
            </AppLink>
            @if (canWrite && !formOpen) {
              <Button type="button" size="sm" onClick={startCreate}>
                New milestone
              </Button>;
            }
          </div>
        </div>

        @if (formError) {
          <div className="mb-4">
            <AuthErrorBanner message={formError} />
          </div>;
        }
        @if (rowError) {
          <div className="mb-4">
            <AuthErrorBanner message={rowError} />
          </div>;
        }

        @if (formOpen && canWrite) {
          <form
            method="post"
            action="#"
            className="mb-8 flex flex-col gap-3 rounded-md border border-border p-4"
            onSubmit={(e) => {
              e.preventDefault();
              void form.handleSubmit();
            }}
          >
            <p className="text-[14px] font-semibold text-foreground">
              {(editing ? `Edit milestone #${editing.number}` : "New milestone") as string}
            </p>
            <div className="grid gap-3 sm:grid-cols-2">
              <form.Field name="title">
                {(field) => (
                  <div className="flex flex-col gap-1.5">
                    <Label htmlFor="repo-milestone-title">Title</Label>
                    <Input
                      id="repo-milestone-title"
                      value={field.state.value}
                      onBlur={field.handleBlur}
                      onInput={(e) =>
                        field.handleChange((e.target as HTMLInputElement).value)
                      }
                      disabled={pending}
                    />
                  </div>
                )}
              </form.Field>
              <form.Field name="due_on">
                {(field) => (
                  <div className="flex flex-col gap-1.5">
                    <Label htmlFor="repo-milestone-due">Due date (optional)</Label>
                    <Input
                      id="repo-milestone-due"
                      type="date"
                      value={field.state.value}
                      onBlur={field.handleBlur}
                      onInput={(e) =>
                        field.handleChange((e.target as HTMLInputElement).value)
                      }
                      disabled={pending}
                    />
                  </div>
                )}
              </form.Field>
            </div>
            <form.Field name="description">
              {(field) => (
                <div className="flex flex-col gap-1.5">
                  <Label htmlFor="repo-milestone-desc">Description (optional)</Label>
                  <Input
                    id="repo-milestone-desc"
                    value={field.state.value}
                    onBlur={field.handleBlur}
                    onInput={(e) => field.handleChange((e.target as HTMLInputElement).value)}
                    disabled={pending}
                  />
                </div>
              )}
            </form.Field>
            @if (editing) {
              <form.Field name="state">
                {(field) => (
                  <div className="flex flex-col gap-1.5">
                    <Label htmlFor="repo-milestone-state">State</Label>
                    <select
                      id="repo-milestone-state"
                      className="h-9 rounded-md border border-border bg-background px-3 text-[14px] text-foreground"
                      value={field.state.value as string}
                      onChange={(e) =>
                        field.handleChange((e.target as HTMLSelectElement).value)
                      }
                      disabled={pending}
                    >
                      <option value="open">Open</option>
                      <option value="closed">Closed</option>
                    </select>
                  </div>
                )}
              </form.Field>;
            }
            <div className="flex gap-2">
              <Button type="submit" disabled={pending}>
                {(editing ? "Save changes" : "Create milestone") as string}
              </Button>
              <Button type="button" variant="outline" disabled={pending} onClick={cancelForm}>
                Cancel
              </Button>
            </div>
          </form>;
        }

        <div
          role="tablist"
          aria-label="Milestone state"
          className="mb-4 inline-flex items-center gap-1 rounded-md border border-border p-1"
        >
          <button
            type="button"
            role="tab"
            aria-selected={stateFilter === "open"}
            className={
              `rounded px-2.5 py-1 text-[13px] font-medium ${stateFilter === "open" ? "bg-muted text-foreground" : "text-muted-foreground"}` as string
            }
            onClick={() => setStateFilter("open")}
          >
            {`${openCount} Open` as string}
          </button>
          <button
            type="button"
            role="tab"
            aria-selected={stateFilter === "closed"}
            className={
              `rounded px-2.5 py-1 text-[13px] font-medium ${stateFilter === "closed" ? "bg-muted text-foreground" : "text-muted-foreground"}` as string
            }
            onClick={() => setStateFilter("closed")}
          >
            {`${closedCount} Closed` as string}
          </button>
          <button
            type="button"
            role="tab"
            aria-selected={stateFilter === "all"}
            className={
              `rounded px-2.5 py-1 text-[13px] font-medium ${stateFilter === "all" ? "bg-muted text-foreground" : "text-muted-foreground"}` as string
            }
            onClick={() => setStateFilter("all")}
          >
            All
          </button>
        </div>

        @if (milestonesQ.isPending) {
          <p className="text-[16px] text-muted-foreground">Loading milestones…</p>;
        } @else {
          @if (shown.length === 0) {
            <p className="text-[16px] text-muted-foreground">
              {
                (stateFilter === "open"
                  ? "No open milestones."
                  : stateFilter === "closed"
                    ? "No closed milestones."
                    : "No milestones yet.") as string
              }
            </p>;
          } @else {
            <ul className="divide-y divide-border border-y border-border">
              @for (const m of shown; key m.id) {
                <li className="py-4">
                  <div className="flex flex-wrap items-start justify-between gap-3">
                    <div className="min-w-0">
                      <AppLink
                        href={
                          `/${owner}/${repoName}/issues?milestone=${String(m.number)}` as string
                        }
                        className="text-[16px] font-semibold text-foreground no-underline hover:text-primary"
                      >
                        {m.title as string}
                      </AppLink>
                      <p className="mt-1 text-[13px] text-muted-foreground">
                        {
                          `${dueLabel(m)} · ${m.open_issues ?? 0} open · ${m.closed_issues ?? 0} closed` as string
                        }
                      </p>
                      @if (m.description) {
                        <p className="mt-1 text-[13px] text-muted-foreground">
                          {m.description as string}
                        </p>;
                      }
                    </div>
                    @if (canWrite) {
                      <div className="flex flex-wrap gap-2">
                        <Button
                          type="button"
                          size="sm"
                          variant="outline"
                          onClick={() => startEdit(m)}
                        >
                          Edit
                        </Button>
                        <Button
                          type="button"
                          size="sm"
                          variant="outline"
                          onClick={() => toggleState(m)}
                        >
                          {(m.state === "open" ? "Close" : "Reopen") as string}
                        </Button>
                        <Button
                          type="button"
                          size="sm"
                          variant="outline"
                          onClick={() => deleteMilestone(m)}
                        >
                          Delete
                        </Button>
                      </div>;
                    }
                  </div>
                  @if (
                    (m.open_issues ?? 0) + (m.closed_issues ?? 0) >
                    0
                  ) {
                    <div
                      className="mt-3 h-2 w-full max-w-md overflow-hidden rounded-full bg-muted"
                      role="progressbar"
                      aria-valuemin={0}
                      aria-valuemax={100}
                      aria-valuenow={milestoneProgressPct(m) as number}
                    >
                      <div
                        className="h-full bg-success"
                        style={
                          { width: `${milestoneProgressPct(m)}%` } as {
                            width: string;
                          }
                        }
                      />
                    </div>;
                  }
                </li>;
              }
            </ul>;
          }
        }
      </>;
    }
  }
}

export default RepoIssueMilestonesPage;

/** Page root — RepoLayout (chrome + repo store) around the leaf content. */
export function RepoIssueMilestonesPage() @{
  <AppPage>
    <RepoLayout>
      <RepoIssueMilestonesPageContent />
    </RepoLayout>
  </AppPage>;
}
