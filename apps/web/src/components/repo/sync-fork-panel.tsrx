import { useMutation, useQuery, useQueryClient } from "@octanejs/tanstack-query";
import { useState } from "octane";
import { repoForkStatusQueryOptions, repoSyncForkMutationOptions } from "@oxidean/api-client";
import { Button } from "@/components/ui/button";
import { apiClient } from "@/lib/api-client";

type SyncForkPanelProps = {
  owner: string;
  repoName: string;
  /** Write+ on the fork — the sync button only shows for writers. */
  canWrite: boolean;
};

function commitsLabel(n: number): string {
  return n === 1 ? "1 commit" : `${n} commits`;
}

/** "Sync fork" card — live upstream divergence + ff/merge action (GIT-24). */
export function SyncForkPanel(props: SyncForkPanelProps) @{
  const queryClient = useQueryClient();
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const statusQ = useQuery(
    repoForkStatusQueryOptions(apiClient, {
      owner: props.owner,
      name: props.repoName,
    }),
  );
  const status = statusQ.data;

  const sync = useMutation({
    ...repoSyncForkMutationOptions(apiClient),
    onSuccess: async (data) => {
      setError(null);
      setNotice(
        data.status === "fast_forwarded"
          ? `Fast-forwarded ${data.branch} to ${data.upstream_owner}/${data.upstream_name}.`
          : data.status === "merged"
            ? `Merged ${data.upstream_owner}/${data.upstream_name}:${data.upstream_branch} into ${data.branch}.`
            : `${data.branch} is already up to date.`,
      );
      // Refreshes repo-scoped queries (fork status, tree, commits) to the new tip.
      await queryClient.invalidateQueries({
        queryKey: ["repo", props.owner, props.repoName],
      });
    },
    onError: (err: Error) => {
      setNotice(null);
      setError(err.message || "Couldn't sync fork.");
    },
  });

  const upstream = status ? `${status.upstream_owner}/${status.upstream_name}` : "";
  const summaryText = !status
    ? ""
    : status.status === "diverged"
      ? `This branch is ${commitsLabel(status.behind_count)} behind and ${commitsLabel(status.ahead_count)} ahead of ${upstream}.`
      : status.status === "behind"
        ? `This branch is ${commitsLabel(status.behind_count)} behind ${upstream}.`
        : "";
  const showCard = (!!status && status.status !== "up_to_date") || !!notice || !!error;

  @if (showCard) {
    <div
      className="mt-2 flex flex-wrap items-center gap-3 rounded-md border border-border bg-muted/30 px-3 py-2"
      data-testid="sync-fork-panel"
    >
      @if (summaryText) {
        <p className="text-[13px] text-muted-foreground">{summaryText as string}</p>;
      }
      @if (notice) {
        <p className="text-[13px] text-foreground">{notice as string}</p>;
      }
      @if (error) {
        <p role="alert" className="text-[13px] text-destructive">
          {error as string}
        </p>;
      }
      @if (
        props.canWrite &&
        !!status &&
        status.status !== "up_to_date"
      ) {
        <Button
          type="button"
          variant="outline"
          size="sm"
          className="ml-auto"
          disabled={sync.isPending}
          onClick={() => sync.mutate({ owner: props.owner, name: props.repoName })}
        >
          {(sync.isPending ? "Syncing…" : "Sync fork") as string}
        </Button>;
      }
    </div>;
  }
}
