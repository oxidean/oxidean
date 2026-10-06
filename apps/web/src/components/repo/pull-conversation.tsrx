import { useQuery, useQueryClient } from "@octanejs/tanstack-query";
import type {
  CommentRevisionPublic,
  PullCommentPublic,
  PullCommentsListResponse,
} from "@oxidean/api-client";
import { useForm } from "@octanejs/tanstack-form";
import { useState } from "octane";
import { AuthErrorBanner } from "@/components/auth-shell";
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
  canInteract: boolean;
  currentUserId: string;
  body: string;
};

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";

function CommentBody(props: { body: string; owner: string; repoName: string }) @{
  <MarkdownBody markdown={props.body} owner={props.owner} repo={props.repoName} />;
}

function PullCommentHistoryDisclosure(props: {
  owner: string;
  repoName: string;
  number: number;
  commentId: string;
}) @{
  const [open, setOpen] = useState(false);
  const historyQ = useQuery({
    queryKey: [
      "pull",
      "comments",
      "history",
      props.owner,
      props.repoName,
      props.number,
      props.commentId,
    ] as const,
    queryFn: async () => {
      const res = await apiClient.pull.comments.history({
        owner: props.owner,
        name: props.repoName,
        number: props.number,
        commentId: props.commentId,
      });
      if (!res.ok) throw new Error(res.error.message || "Could not load history.");
      return res.data;
    },
    enabled: open && !!props.commentId,
  });

  const revisions: CommentRevisionPublic[] = historyQ.data?.revisions ?? [];

  <div className="mt-2">
    <button
      type="button"
      className="text-[13px] text-muted-foreground underline-offset-2 hover:underline"
      onClick={() => setOpen(!open)}
    >
      {(open ? "Hide edit history" : "Show edit history") as string}
    </button>
    @if (open) {
      <div className="mt-2" data-testid="comment-history">
        @if (historyQ.isPending) {
          <p className="text-[13px] text-muted-foreground">Loading…</p>;
        } @else {
          @if (historyQ.isError) {
            <p role="alert" className="text-[13px] text-destructive">
              Could not load comment history.
            </p>;
          } @else {
            @if (revisions.length === 0) {
              <p className="text-[13px] text-muted-foreground">No prior edits.</p>;
            } @else {
              <ul className="flex flex-col gap-2">
                @for (const rev of revisions; key rev.id) {
                  <li className="rounded-md border border-border px-2 py-1 text-[12px]">
                    <p
                      className="text-muted-foreground"
                      title={rev.created_at as string}
                    >
                      {
                        `@${rev.editor_username} edited ${formatRelative(rev.created_at)}` as string
                      }
                    </p>
                    <pre className="mt-1 whitespace-pre-wrap font-sans text-foreground">
                      {rev.body as string}
                    </pre>
                  </li>;
                }
              </ul>;
            }
          }
        }
      </div>;
    }
  </div>;
}

function PullCommentItem(props: {
  comment: PullCommentPublic;
  owner: string;
  repoName: string;
  number: number;
  canWrite: boolean;
  currentUserId: string;
  busy: boolean;
  onToggleResolve: (c: PullCommentPublic) => void;
  onUpdated: (comment: PullCommentPublic) => void;
  onDeleted: (id: string) => void;
}) @{
  const c = props.comment;
  const isAuthor = !!props.currentUserId && c.author_id === props.currentUserId;
  const canEdit = isAuthor;
  const canDelete = isAuthor || props.canWrite;
  const [editing, setEditing] = useState(false);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState("");

  const editForm = useForm({
    defaultValues: { body: c.body },
    onSubmit: async ({ value }) => {
      setError("");
      setPending(true);
      try {
        const res = await apiClient.pull.comments.update({
          owner: props.owner,
          name: props.repoName,
          number: props.number,
          commentId: c.id,
          body: value.body,
        });
        if (!res.ok) {
          setError(res.error.message || "Could not update comment.");
          setPending(false);
          return;
        }
        props.onUpdated(res.data);
        setEditing(false);
        setPending(false);
      } catch {
        setError(NETWORK_ERROR);
        setPending(false);
      }
    },
  });

  async function remove() {
    setError("");
    setPending(true);
    try {
      const res = await apiClient.pull.comments.delete({
        owner: props.owner,
        name: props.repoName,
        number: props.number,
        commentId: c.id,
      });
      if (!res.ok) {
        setError(res.error.message || "Could not delete comment.");
        setPending(false);
        return;
      }
      props.onDeleted(c.id);
      setPending(false);
    } catch {
      setError(NETWORK_ERROR);
      setPending(false);
    }
  }

  <li className="rounded-md border border-border p-3" data-testid="pull-comment">
    <div className="mb-1 flex flex-wrap items-center gap-2 text-sm">
      <AppLink
        href={`/${c.author_username}` as string}
        className="font-semibold text-foreground no-underline hover:text-primary"
      >
        {("@" + c.author_username) as string}
      </AppLink>
      <span className="text-[12px] text-muted-foreground" title={c.created_at as string}>
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
        <span className="text-[12px] text-muted-foreground">Resolved</span>;
      }
      <span className="ml-auto flex gap-2">
        @if (canEdit && !editing) {
          <Button
            type="button"
            variant="ghost"
            disabled={pending}
            onClick={() => {
              editForm.setFieldValue("body", c.body);
              setEditing(true);
              setError("");
            }}
          >
            Edit
          </Button>;
        }
        @if (canDelete && !editing) {
          <Button
            type="button"
            variant="ghost"
            disabled={pending}
            onClick={() => void remove()}
          >
            Delete
          </Button>;
        }
      </span>
    </div>
    @if (error) {
      <div className="mt-2">
        <AuthErrorBanner message={error} />
      </div>;
    }
    @if (editing) {
      <form
        method="post"
        action="#"
        className="mt-3 flex flex-col gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          void editForm.handleSubmit();
        }}
      >
        <editForm.Field name="body">
          {(field) => (
            <MarkdownWritePreview
              id={`pull-comment-edit-${c.id}`}
              value={field.state.value}
              onValueChange={field.handleChange}
              rows={6}
              disabled={pending}
              owner={props.owner}
              repo={props.repoName}
            />
          )}
        </editForm.Field>
        <div className="flex gap-2">
          <Button type="submit" disabled={pending}>
            Save
          </Button>
          <Button
            type="button"
            variant="secondary"
            disabled={pending}
            onClick={() => {
              setEditing(false);
              setError("");
            }}
          >
            Cancel
          </Button>
        </div>
      </form>;
    } @else {
      <CommentBody body={c.body} owner={props.owner} repoName={props.repoName} />;
    }
    @if (props.canWrite && c.path && !editing) {
      <div className="mt-2">
        <button
          type="button"
          className="text-[13px] text-muted-foreground underline-offset-2 hover:underline"
          disabled={props.busy}
          onClick={() => props.onToggleResolve(c)}
        >
          {(c.resolved ? "Unresolve" : "Resolve") as string}
        </button>
      </div>;
    }
    @if (!editing) {
      <PullCommentHistoryDisclosure
        owner={props.owner}
        repoName={props.repoName}
        number={props.number}
        commentId={c.id}
      />;
    }
  </li>;
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

  const listKey = ["pull", "comments", props.owner, props.repoName, props.number] as const;

  // Patch the cached thread with the mutation response instead of refetching
  // the whole list — the API returns the full comment row.
  function patchComments(fn: (cs: PullCommentPublic[]) => PullCommentPublic[]) {
    queryClient.setQueryData<PullCommentsListResponse>(listKey, (old) =>
      old ? { comments: fn(old.comments) } : old,
    );
  }

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
    if (commentsQ.data) {
      patchComments((cs) => [...cs, res.data]);
    } else {
      await queryClient.invalidateQueries({ queryKey: listKey });
    }
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
    patchComments((cs) => cs.map((x) => (x.id === res.data.id ? res.data : x)));
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
            <PullCommentItem
              comment={c}
              owner={props.owner}
              repoName={props.repoName}
              number={props.number}
              canWrite={props.canWrite}
              currentUserId={props.currentUserId}
              busy={busy}
              onToggleResolve={(x) => void toggleResolve(x)}
              onUpdated={(x) =>
                patchComments((cs) => cs.map((y) => (y.id === x.id ? x : y)))
              }
              onDeleted={(id) => patchComments((cs) => cs.filter((x) => x.id !== id))}
            />;
          }
        </ul>;
      }
    }

    @if (props.canInteract) {
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
