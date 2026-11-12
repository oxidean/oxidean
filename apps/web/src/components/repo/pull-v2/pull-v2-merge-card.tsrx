import { useQuery, useQueryClient } from "@octanejs/tanstack-query";
import type { MergeMethod, MergeQueueGetResponse, RepoMergeSettings } from "@oxidean/api-client";
import { CheckCircle2, CircleAlert, Loader2, MinusCircle, TriangleAlert } from "@octanejs/lucide";
import { useState } from "octane";
import { Button } from "@/components/ui/button";
import { apiClient } from "@/lib/api-client";
import { cn } from "@/lib/utils";

type PullV2MergeCardProps = {
  owner: string;
  repoName: string;
  number: number;
  canWrite: boolean;
  pullState: string;
  draft: boolean;
  headSha: string;
};

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";

const REASON_LABELS: Record<string, string> = {
  reviews: "Required approving reviews",
  checks: "Required status checks",
  conversations: "Unresolved review conversations",
  up_to_date: "Branch must be up to date with the base",
  locked: "Branch is locked",
  draft: "Draft pull requests cannot be merged",
  force_push: "Force pushes are not allowed",
  deletions: "Deletions are not allowed",
  last_push_approval: "Approval required from someone other than the last pusher",
  linear_history: "Merge commits are not allowed (linear history required)",
  merge_queue: "Merging must go through the merge queue",
};

function extractProtectionReasons(err: {
  code: string;
  message: string;
  data?: unknown;
}): string[] {
  if (err.code !== "pull.merge_blocked" && err.code !== "repo.branch_protection") return [];
  const data = err.data as { reasons?: unknown } | undefined;
  if (!data || !Array.isArray(data.reasons)) return [];
  return data.reasons.filter((r): r is string => typeof r === "string");
}

/**
 * "Recommended action" card for the v2 overview (WI-0001 S8): a compact
 * merge affordance replacing the classic merge box — checks rollup, branch
 * status, merge-queue awareness, and method selection per repo settings.
 * Same RPC surface as pull-merge-panel (which stays for the legacy UI).
 */
export function PullV2MergeCard(props: PullV2MergeCardProps) @{
  const queryClient = useQueryClient();
  const [method, setMethod] = useState<MergeMethod>("merge");
  const [deleteBranch, setDeleteBranch] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [conflict, setConflict] = useState(false);
  const [blockers, setBlockers] = useState<string[]>([]);

  const settingsQ = useQuery({
    queryKey: ["mergeSettings", props.owner, props.repoName] as const,
    queryFn: async () => {
      const res = await apiClient.mergeSettings.get({
        owner: props.owner,
        name: props.repoName,
      });
      if (!res.ok) throw new Error(res.error.message || NETWORK_ERROR);
      return res.data;
    },
    enabled: props.canWrite && props.pullState === "open",
  });

  const queueQ = useQuery({
    queryKey: ["mergeQueue", "get", props.owner, props.repoName, props.number] as const,
    queryFn: async (): Promise<MergeQueueGetResponse> => {
      const res = await apiClient.mergeQueue.get({
        owner: props.owner,
        name: props.repoName,
        number: props.number,
      });
      if (!res.ok) throw new Error(res.error.message || NETWORK_ERROR);
      return res.data;
    },
    enabled: props.canWrite && props.pullState === "open",
  });

  const checksQ = useQuery({
    queryKey: ["check", "runs", "listForRef", props.owner, props.repoName, props.headSha] as const,
    queryFn: async () => {
      const res = await apiClient.check.runs.listForRef({
        owner: props.owner,
        name: props.repoName,
        ref: props.headSha,
      });
      if (!res.ok) throw new Error(res.error.message || NETWORK_ERROR);
      return res.data;
    },
    enabled: !!props.headSha && props.pullState === "open",
    refetchInterval: 15_000,
  });

  const branchQ = useQuery({
    queryKey: ["pull", "branchStatus", props.owner, props.repoName, props.number] as const,
    queryFn: async () => {
      const res = await apiClient.pull.branchStatus({
        owner: props.owner,
        name: props.repoName,
        number: props.number,
      });
      if (!res.ok) throw new Error(res.error.message || NETWORK_ERROR);
      return res.data;
    },
    enabled: props.pullState === "open",
  });

  const settings: RepoMergeSettings | undefined = settingsQ.data;
  const queue = queueQ.data;

  const checkRuns = checksQ.data?.check_runs ?? [];
  const failingChecks = checkRuns.filter(
    (r) =>
      r.status === "completed" &&
      !!r.conclusion &&
      !["success", "neutral", "skipped"].includes(r.conclusion),
  ).length;
  const pendingChecks = checkRuns.filter((r) => r.status !== "completed").length;

  const behindBy = branchQ.data?.status === "behind" ? (branchQ.data.behind_count ?? 0) : 0;

  const enabledMethods: { id: MergeMethod; label: string }[] = [];
  if (!settings || settings.allow_merge_commit)
    enabledMethods.push({ id: "merge", label: "Merge" });
  if (!settings || settings.allow_squash_merge)
    enabledMethods.push({ id: "squash", label: "Squash" });
  if (!settings || settings.allow_rebase_merge)
    enabledMethods.push({ id: "rebase", label: "Rebase" });
  const activeMethod = enabledMethods.some((m) => m.id === method)
    ? method
    : (enabledMethods[0]?.id ?? "merge");

  async function invalidatePull() {
    await Promise.all([
      queryClient.invalidateQueries({
        queryKey: ["pull", "get", props.owner, props.repoName, props.number],
      }),
      queryClient.invalidateQueries({
        queryKey: ["mergeQueue", "get", props.owner, props.repoName, props.number],
      }),
    ]);
  }

  async function merge() {
    setBusy(true);
    setError(null);
    setConflict(false);
    setBlockers([]);
    const res = await apiClient.pull.merge({
      owner: props.owner,
      name: props.repoName,
      number: props.number,
      method: activeMethod,
      delete_branch: deleteBranch,
    });
    setBusy(false);
    if (!res.ok) {
      if (/conflict/i.test(`${res.error.code} ${res.error.message}`)) {
        setConflict(true);
      }
      if (res.error.code === "pull.merge_queue_required") {
        setBlockers(["merge_queue"]);
      } else {
        const reasons = extractProtectionReasons(res.error);
        if (reasons.length > 0) setBlockers(reasons);
      }
      setError(res.error.message || "Merge failed");
      return;
    }
    await invalidatePull();
  }

  async function enqueue() {
    setBusy(true);
    setError(null);
    const res = await apiClient.mergeQueue.enqueue({
      owner: props.owner,
      name: props.repoName,
      number: props.number,
    });
    setBusy(false);
    if (!res.ok) {
      const reasons = extractProtectionReasons(res.error);
      if (reasons.length > 0) setBlockers(reasons);
      setError(res.error.message || "Could not add to the merge queue");
      return;
    }
    await invalidatePull();
  }

  async function dequeue() {
    setBusy(true);
    setError(null);
    const res = await apiClient.mergeQueue.dequeue({
      owner: props.owner,
      name: props.repoName,
      number: props.number,
    });
    setBusy(false);
    if (!res.ok) {
      setError(res.error.message || "Could not remove from the merge queue");
      return;
    }
    await invalidatePull();
  }

  @if (props.pullState !== "open") {
    <div />;
  } @else {
    <div className="rounded-md border border-border bg-card" data-testid="pull-v2-merge-card">
      <div className="flex items-center gap-2 border-b border-border px-3 py-2">
        @if (conflict) {
          <CircleAlert aria-hidden size={16} className="shrink-0 text-destructive" />;
        } @else {
          @if (failingChecks > 0 || blockers.length > 0) {
            <CircleAlert aria-hidden size={16} className="shrink-0 text-destructive" />;
          } @else {
            @if (pendingChecks > 0) {
              <Loader2
                aria-hidden
                size={16}
                className="shrink-0 animate-spin text-amber-500"
              />;
            } @else {
              <CheckCircle2 aria-hidden size={16} className="shrink-0 text-success" />;
            }
          }
        }
        <h3 className="text-[14px] font-semibold text-foreground">Recommended action</h3>
      </div>
      <div className="space-y-3 px-3 py-3 text-[13px]">
        @if (conflict) {
          <div data-testid="pull-v2-merge-conflict">
            <p className="font-medium text-destructive">This branch has conflicts</p>
            <p className="mt-0.5 text-muted-foreground">
              Resolve the conflicts on the head branch before merging.
            </p>
            <Button type="button" size="sm" className="mt-2" disabled>
              Resolve conflicts
            </Button>
          </div>;
        }
        <ul className="space-y-1 text-muted-foreground">
          @if (failingChecks > 0) {
            <li
              className="flex items-center gap-1.5 text-destructive"
              data-testid="pull-v2-checks-failing"
            >
              <MinusCircle aria-hidden size={13} />
              {`${failingChecks} failing check${failingChecks === 1 ? "" : "s"}` as string}
            </li>;
          }
          @if (pendingChecks > 0) {
            <li className="flex items-center gap-1.5" data-testid="pull-v2-checks-pending">
              <Loader2 aria-hidden size={13} className="animate-spin text-amber-500" />
              {
                `${pendingChecks} check${pendingChecks === 1 ? "" : "s"} still pending` as string
              }
            </li>;
          }
          @if (
            checkRuns.length > 0 &&
            failingChecks === 0 &&
            pendingChecks === 0
          ) {
            <li
              className="flex items-center gap-1.5 text-success"
              data-testid="pull-v2-checks-ok"
            >
              <CheckCircle2 aria-hidden size={13} />
              {
                `All ${checkRuns.length} check${checkRuns.length === 1 ? "" : "s"} passed` as string
              }
            </li>;
          }
          @if (behindBy > 0) {
            <li
              className="flex items-center gap-1.5 text-amber-600"
              data-testid="pull-v2-branch-behind"
            >
              <TriangleAlert aria-hidden size={13} />
              {
                `Branch is ${behindBy} commit${behindBy === 1 ? "" : "s"} behind the base` as string
              }
            </li>;
          }
        </ul>
        @if (blockers.length > 0) {
          <ul
            className="list-disc space-y-0.5 pl-5 text-destructive"
            data-testid="pull-v2-merge-blockers"
          >
            @for (const code of blockers; key code) {
              <li>{(REASON_LABELS[code] ?? code) as string}</li>;
            }
          </ul>;
        }
        @if (!props.canWrite) {
          <p className="text-muted-foreground">
            You don't have permission to merge this pull request.
          </p>;
        } @else {
          <>
            @if (queueQ.isPending || settingsQ.isPending) {
              <p className="text-muted-foreground">Loading merge options…</p>;
            } @else {
              <>
                @if (queue?.required) {
                  <>
                    @if (
                      queue?.entry &&
                      queue.entry.state === "queued"
                    ) {
                      <>
                        <p className="text-muted-foreground">
                          This pull request is in the merge queue
                          {queue.entry.group_id
                            ? " — waiting for required checks on the merge group"
                            : ` at position ${queue.entry.position}`}
                          .
                        </p>
                        <Button
                          type="button"
                          variant="ghost"
                          size="sm"
                          disabled={busy}
                          onClick={() => void dequeue()}
                        >
                          Remove from queue
                        </Button>
                      </>;
                    } @else {
                      <>
                        <p className="text-muted-foreground">
                          Merging is handled through the merge queue for
                          this branch.
                        </p>
                        <Button
                          type="button"
                          size="sm"
                          disabled={busy || props.draft}
                          title={
                            props.draft
                              ? "Mark the pull request ready first"
                              : undefined
                          }
                          data-testid="pull-v2-merge-button"
                          onClick={() => void enqueue()}
                        >
                          Merge when ready
                        </Button>
                      </>;
                    }
                  </>;
                } @else {
                  <>
                    <div
                      className="flex flex-wrap gap-1"
                      role="group"
                      aria-label="Merge method"
                    >
                      @for (const m of enabledMethods; key m.id) {
                        <button
                          type="button"
                          className={
                            cn(
                              "rounded-md px-2 py-1 text-[12px]",
                              activeMethod === m.id
                                ? "bg-muted font-semibold text-foreground"
                                : "text-muted-foreground hover:bg-muted/60",
                            ) as string
                          }
                          data-testid={`pull-v2-method-${m.id}`}
                          onClick={() => setMethod(m.id)}
                        >
                          {m.label as string}
                        </button>;
                      }
                    </div>
                    <label className="flex items-center gap-2 text-[12px] text-muted-foreground">
                      <input
                        type="checkbox"
                        checked={deleteBranch}
                        onChange={(e) =>
                          setDeleteBranch((e.target as HTMLInputElement).checked)
                        }
                      />
                      Delete head branch after merge
                    </label>
                    <Button
                      type="button"
                      size="sm"
                      disabled={busy || props.draft}
                      title={
                        props.draft
                          ? "Mark the pull request ready first"
                          : undefined
                      }
                      data-testid="pull-v2-merge-button"
                      onClick={() => void merge()}
                    >
                      {
                        (activeMethod === "merge"
                          ? "Merge pull request"
                          : activeMethod === "squash"
                            ? "Squash and merge"
                            : "Rebase and merge") as string
                      }
                    </Button>
                  </>;
                }
              </>;
            }
          </>;
        }
        <p
          className={cn("text-destructive", !error && "hidden") as string}
          role="alert"
          data-testid="pull-v2-merge-error"
        >
          {(error ?? "") as string}
        </p>
      </div>
    </div>;
  }
}
