import type { PullPublic, RepoPublic } from "@oxidean/api-client";
import { PullConversation } from "@/components/repo/pull-conversation";
import { PullV2MergeCard } from "./pull-v2-merge-card";
import { PullV2Rail } from "./pull-v2-rail";

type PullV2OverviewProps = {
  owner: string;
  repoName: string;
  number: number;
  repo: RepoPublic;
  pull: PullPublic;
  canWrite: boolean;
  canInteract: boolean;
  isAuthor: boolean;
  currentUserId: string;
};

/**
 * Overview tab of the new PR experience (WI-0001 S8): the merge box becomes
 * a compact "Recommended action" card, and a "Status and details" rail
 * (reviewers / assignees / labels / activity) sits at lg:w-72 on the right.
 * The timeline below stays on the shared PullConversation.
 */
export function PullV2Overview(props: PullV2OverviewProps) @{
  <div className="flex items-start gap-6" data-testid="pull-v2-overview">
    <div className="min-w-0 flex-1 space-y-6">
      <PullV2MergeCard
        owner={props.owner}
        repoName={props.repoName}
        number={props.number}
        canWrite={props.canWrite}
        pullState={(props.pull.state as string) ?? "open"}
        draft={props.pull.draft}
        headSha={props.pull.head_sha}
      />
      <PullConversation
        owner={props.owner}
        repoName={props.repoName}
        number={props.number}
        canWrite={props.canWrite}
        canInteract={props.canInteract}
        currentUserId={props.currentUserId}
        body={(props.pull.body || "") as string}
      />
    </div>
    <aside className="hidden w-72 shrink-0 lg:block" aria-label="Status and details">
      <PullV2Rail
        owner={props.owner}
        repoName={props.repoName}
        number={props.number}
        pull={props.pull}
        canWrite={props.canWrite}
      />
    </aside>
  </div>;
}
