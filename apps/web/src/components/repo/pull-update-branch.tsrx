import { useMutation, useQuery, useQueryClient } from "@octanejs/tanstack-query";
import { useState } from "octane";
import { pullBranchStatusQueryOptions, pullUpdateBranchMutationOptions } from "@oxidean/api-client";
import { Button } from "@/components/ui/button";
import { apiClient } from "@/lib/api-client";

type PullUpdateBranchProps = {
  owner: string;
  repoName: string;
  number: number;
  pullState: string;
  baseRef: string;
};

function commitsLabel(n: number): string {
  return n === 1 ? "1 commit" : `${n} commits`;
}

/** GitHub-style "Update branch" row — shown when the head is behind base (GIT-24). */
export function PullUpdateBranch(props: PullUpdateBranchProps) @{
  const queryClient = useQueryClient();
  const [error, setError] = useState<string | null>(null);

  const statusQ = useQuery({
    ...pullBranchStatusQueryOptions(apiClient, {
      owner: props.owner,
      name: props.repoName,
      number: props.number,
    }),
    enabled: props.pullState === "open" && !!props.owner && !!props.repoName && props.number > 0,
  });
  const status = statusQ.data;

  const update = useMutation({
    ...pullUpdateBranchMutationOptions(apiClient),
    onSuccess: async () => {
      setError(null);
      // Head sha + diff inputs changed — refresh all pull-scoped queries.
      const keys = ["get", "branchStatus", "commits", "files", "comments", "reviews"];
      for (const key of keys) {
        await queryClient.invalidateQueries({
          queryKey: ["pull", key, props.owner, props.repoName, props.number],
        });
      }
    },
    onError: (err: Error) => {
      setError(err.message || "Couldn't update the head branch.");
    },
  });

  @if (!status || status.status !== "behind") {
    <div />;
  } @else {
    <div
      className="flex flex-wrap items-center justify-between gap-3 rounded-md border border-border p-3"
      data-testid="pull-update-branch-panel"
    >
      <p className="text-sm text-muted-foreground">
        This branch is {commitsLabel(status.behind_count ?? 0) as string} behind{" "}
        <span className="font-mono">{props.baseRef as string}</span>.
      </p>
      @if (status.can_update) {
        <Button
          type="button"
          variant="outline"
          size="sm"
          disabled={update.isPending}
          onClick={() =>
            update.mutate({
              owner: props.owner,
              name: props.repoName,
              number: props.number,
            })
          }
        >
          {(update.isPending ? "Updating…" : "Update branch") as string}
        </Button>;
      } @else {
        <span className="text-xs text-muted-foreground">
          Requires write access to the head branch
        </span>;
      }
      @if (error) {
        <p role="alert" className="w-full text-sm text-destructive">
          {error as string}
        </p>;
      }
    </div>;
  }
}
