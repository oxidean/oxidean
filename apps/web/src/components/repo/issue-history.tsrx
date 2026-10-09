import { useQuery } from "@octanejs/tanstack-query";
import { History } from "@octanejs/lucide";
import type { IssueRevisionPublic } from "@oxidean/api-client";
import { EmptyState } from "@/components/ui/empty-state";
import { List, ListRow } from "@/components/ui/list";
import { Skeleton } from "@/components/ui/skeleton";
import { apiClient } from "@/lib/api-client";
import { formatRelative } from "@/lib/relative-time";

type IssueHistoryPanelProps = {
  owner: string;
  repoName: string;
  number: number;
};

/** Prior title/body revision trail (D-ISS-04). */
export function IssueHistoryPanel(props: IssueHistoryPanelProps) @{
  const historyQ = useQuery({
    queryKey: ["issue", "history", props.owner, props.repoName, props.number] as const,
    queryFn: async () => {
      const res = await apiClient.issue.history({
        owner: props.owner,
        name: props.repoName,
        number: props.number,
      });
      if (!res.ok) throw new Error(res.error.message || "Could not load history.");
      return res.data;
    },
    enabled: !!props.owner && !!props.repoName && props.number > 0,
  });

  const revisions: IssueRevisionPublic[] = historyQ.data?.revisions ?? [];
  const showList = !historyQ.isPending && !historyQ.isError && revisions.length > 0;
  const showEmpty = !historyQ.isPending && !historyQ.isError && revisions.length === 0;

  <section aria-labelledby="issue-history-heading" className="flex flex-col gap-3">
    <h2 id="issue-history-heading" className="text-[16px] font-semibold text-foreground">
      Edit history
    </h2>
    @if (historyQ.isPending) {
      <Skeleton className="h-20 w-full" />;
    } @else {
      @if (historyQ.isError) {
        <p role="alert" className="text-[14px] text-destructive">
          Could not load edit history.
        </p>;
      } @else {
        <>
          @if (showEmpty) {
            <EmptyState
              compact
              icon={<History aria-hidden size={20} />}
              title="No prior edits"
              description="Edits to the title or body will be tracked here."
            />;
          }
          @if (showList) {
            <List>
              @for (const rev of revisions; key rev.id) {
                <ListRow className="flex-col items-start gap-1 text-[13px]">
                  <p
                    className="text-muted-foreground"
                    title={rev.created_at as string}
                  >
                    {
                      `@${rev.editor_username} edited ${formatRelative(rev.created_at)}` as string
                    }
                  </p>
                  <p className="font-medium text-foreground">
                    {rev.title as string}
                  </p>
                  <pre className="whitespace-pre-wrap font-sans text-muted-foreground">
                    {rev.body as string}
                  </pre>
                </ListRow>;
              }
            </List>;
          }
        </>;
      }
    }
  </section>;
}
