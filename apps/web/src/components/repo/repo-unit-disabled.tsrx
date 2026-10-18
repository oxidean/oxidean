import { BookOpen, CircleDot, GitPullRequest } from "@octanejs/lucide";
import { EmptyState } from "@/components/ui/empty-state";

/**
 * Shown on /issues*, /pulls*, and /wiki* when a repo admin disabled the unit
 * in Settings → Features (COL-13). Data is never deleted — re-enabling
 * restores the surfaces immediately.
 */
export function RepoUnitDisabled(props: {
  unit: "issues" | "pulls" | "wiki";
  /** Repo admin sees a hint pointing at Settings → Features. */
  canAdmin?: boolean;
}) @{
  const unitLabel =
    props.unit === "issues" ? "Issues" : props.unit === "pulls" ? "Pull requests" : "Wiki";
  const title = `${unitLabel} ${props.unit === "wiki" ? "is" : "are"} disabled`;
  const description = props.canAdmin
    ? `${unitLabel} ${props.unit === "wiki" ? "is" : "are"} turned off for this repository. Re-enable under Settings → Features — no data was removed.`
    : props.unit === "wiki"
      ? "This repository's wiki is turned off right now."
      : `This repository does not accept ${props.unit === "issues" ? "issues" : "pull requests"} right now.`;

  <div className="mx-auto max-w-lg py-10">
    <EmptyState
      icon={
        props.unit === "issues" ? (
          <CircleDot aria-hidden size={22} />
        ) : props.unit === "pulls" ? (
          <GitPullRequest aria-hidden size={22} />
        ) : (
          <BookOpen aria-hidden size={22} />
        )
      }
      title={title as string}
      description={description as string}
    />
  </div>;
}
