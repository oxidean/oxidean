import { useQuery, useQueryClient } from "@octanejs/tanstack-query";
import type { PullCommentPublic } from "@oxidean/api-client";
import { useState } from "octane";
import { MarkdownBody } from "@/components/repo/markdown-body";
import { MarkdownWritePreview } from "@/components/repo/markdown-write-preview";
import { Button } from "@/components/ui/button";
import { apiClient } from "@/lib/api-client";
import { formatRelative } from "@/lib/relative-time";
import { AppLink } from "@/components/ui/app-link";

export type PullConversationProps = {
  owner: string;
  repoName: string;
  number: number;
  canWrite: boolean;
  body: string;
};

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";

function CommentBody(props: { body: string; owner: string; repoName: string }) @{
  <MarkdownBody markdown={props.body} owner={props.owner} repo={props.repoName} />;
}

export function PullConversation(props: PullConversationProps) @{
  const queryClient = useQueryClient();
  const [draft, setDraft] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const commentsQ = useQuery({
    queryKey: ["pull", "comments", props.owner, props.repoName, props.number] as const,
    queryFn: async () => {
      const res = await apiClient.pull.comments.list({
        owner: props.owner,
        name: props.repoName,
        number: props.number,
      });
      if (!res.ok) throw new Error(res.error.message || NETWORK_ERROR);
      return res.data;
    },
  });

  const comments: PullCommentPublic[] = commentsQ.data?.comments ?? [];

  async function submitComment() {
    if (!draft.trim()) return;
    setBusy(true);
    setError(null);
    const res = await apiClient.pull.comments.create({
      owner: props.owner,
      name: props.repoName,
      number: props.number,
      body: draft,
    });
    setBusy(false);
    if (!res.ok) {
      setError(res.error.message || "Could not post comment");
      return;
    }
    setDraft("");
    await queryClient.invalidateQueries({
      queryKey: ["pull", "comments", props.owner, props.repoName, props.number],
    });
  }

  async function toggleResolve(c: PullCommentPublic) {
    setBusy(true);
    setError(null);
    const res = await apiClient.pull.comments.resolve({
      owner: props.owner,
      name: props.repoName,
      number: props.number,
      comment_id: c.id,
      resolved: !c.resolved,
    });
    setBusy(false);
    if (!res.ok) {
      setError(res.error.message || "Could not update thread");
      return;
    }
    await queryClient.invalidateQueries({
      queryKey: ["pull", "comments", props.owner, props.repoName, props.number],
    });
  }

  <div className="space-y-4" data-testid="pull-conversation">
    <div className="rounded-md border border-border p-3">
      <p className="mb-2 text-sm font-medium text-muted-foreground">Description</p>
      <CommentBody body={props.body} owner={props.owner} repoName={props.repoName} />
    </div>

    @if (commentsQ.isPending) {
      <p className="text-sm text-muted-foreground">Loading comments…</p>;
    } @else {
      @if (commentsQ.isError) {
        <p className="text-sm text-destructive">Could not load comments.</p>;
      } @else {
        <ul className="space-y-3">
          @for (const c of comments; key c.id) {
            <li className="rounded-md border border-border p-3">
              <div className="mb-1 flex flex-wrap items-center gap-2 text-sm">
                <AppLink
                  href={`/${c.author_username}` as string}
                  className="font-semibold text-foreground no-underline hover:text-primary"
                >
                  {("@" + c.author_username) as string}
                </AppLink>
                <span
                  className="text-[12px] text-muted-foreground"
                  title={c.created_at as string}
                >
                  {`commented ${formatRelative(c.created_at)}` as string}
                </span>
                @if (c.path) {
                  <span className="font-mono text-[12px] text-muted-foreground">
                    {`${c.path}:${c.line ?? "?"}` as string}
                  </span>;
                }
                @if (c.outdated) {
                  <span className="rounded bg-muted px-1.5 py-0.5 text-[11px] uppercase tracking-wide text-muted-foreground">
                    Outdated
                  </span>;
                }
                @if (c.resolved) {
                  <span className="text-[12px] text-muted-foreground">
                    Resolved
                  </span>;
                }
              </div>
              <CommentBody
                body={c.body}
                owner={props.owner}
                repoName={props.repoName}
              />
              @if (props.canWrite && c.path) {
                <div className="mt-2">
                  <button
                    type="button"
                    className="text-[13px] text-muted-foreground underline-offset-2 hover:underline"
                    disabled={busy}
                    onClick={() => void toggleResolve(c)}
                  >
                    {(c.resolved ? "Unresolve" : "Resolve") as string}
                  </button>
                </div>;
              }
            </li>;
          }
        </ul>;
      }
    }

    @if (props.canWrite) {
      <div className="space-y-2">
        <MarkdownWritePreview
          value={draft}
          onValueChange={setDraft}
          owner={props.owner}
          repo={props.repoName}
          placeholder="Leave a comment"
        />
        @if (error) {
          <p className="text-sm text-destructive">{error as string}</p>;
        }
        <Button
          type="button"
          disabled={busy || !draft.trim()}
          onClick={() => void submitComment()}
        >
          Comment
        </Button>
      </div>;
    }
  </div>;
}
