import type { PullPublic } from "@oxidean/api-client";
import { MarkdownBody } from "@/components/repo/markdown-body";

type PullV2DescriptionPanelProps = {
  owner: string;
  repoName: string;
  pull: PullPublic;
};

/**
 * Docked "Overview" pane for the pull-v2 chrome — GitHub's `?panel=overview`
 * surface. Read-only reference of the PR body; editing stays in the main
 * Summary block so there is a single write path.
 */
export function PullV2DescriptionPanel(props: PullV2DescriptionPanelProps) @{
  const body = (props.pull.body || "") as string;

  <div
    className="overflow-hidden rounded-md border border-border bg-card"
    data-testid="pull-v2-description-panel"
  >
    <div className="border-b border-border px-3 py-2">
      <p className="text-[13px] font-semibold text-foreground">Overview</p>
    </div>
    <div className="px-3 py-3">
      @if (body.trim()) {
        <MarkdownBody
          markdown={body}
          owner={props.owner}
          repo={props.repoName}
          data-testid="pull-v2-description-body"
        />;
      } @else {
        <p className="text-[13px] text-muted-foreground">No description provided.</p>;
      }
    </div>
  </div>;
}
