import { CircleDot, GitPullRequest } from "@octanejs/lucide";
import { EmptyState } from "@/components/ui/empty-state";

/**
 * Shown on /issues* and /pulls* when a repo admin disabled the unit in
 * Settings → Features (COL-13). Data is never deleted — re-enabling restores
 * the surfaces immediately.
 */
export function RepoUnitDisabled(props: {
  unit: "issues" | "pulls";
  /** Repo admin sees a hint pointing at Settings → Features. */
  canAdmin?: boolean;
}) @{
  const isIssues = props.unit === "issues";
  const title = isIssues ? "Issues are disabled" : "Pull requests are disabled";
  const description = props.canAdmin
    ? `${isIssues ? "Issues" : "Pull requests"} are turned off for this repository. Re-enable them under Settings → Features — no data was removed.`
    : `This repository does not accept ${isIssues ? "issues" : "pull requests"} right now.`;

  <div className="mx-auto max-w-lg py-10">
    <EmptyState
      icon={
        isIssues ? <CircleDot aria-hidden size={22} /> : <GitPullRequest aria-hidden size={22} />
      }
      title={title as string}
      description={description as string}
    />
  </div>;
}
