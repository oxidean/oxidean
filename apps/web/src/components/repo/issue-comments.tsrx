import { useQuery, useQueryClient } from "@octanejs/tanstack-query";
import type {
  CommentRevisionPublic,
  IssueCommentPublic,
  IssueCommentsListResponse,
} from "@oxidean/api-client";
import { useForm } from "@octanejs/tanstack-form";
import { useState } from "octane";
import { AuthErrorBanner } from "@/components/auth-shell";
import { IssueReactionsBar } from "@/components/repo/issue-reactions";
import { MarkdownBody } from "@/components/repo/markdown-body";
import { MarkdownWritePreview } from "@/components/repo/markdown-write-preview";
import { Button } from "@/components/ui/button";
import { apiClient } from "@/lib/api-client";
import { formatRelative } from "@/lib/relative-time";
import { AppLink } from "@/components/ui/app-link";

type IssueCommentsProps = {
  owner: string;
  repoName: string;
  number: number;
  canWrite: boolean;
  canInteract: boolean;
  currentUserId: string;
};

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";

function CommentBody(props: { body: string; owner: string; repoName: string }) @{
  <MarkdownBody
    markdown={props.body}
    owner={props.owner}
    repo={props.repoName}
    data-testid="issue-comment-body"
  />;
}

function CommentHistoryDisclosure(props: {
  owner: string;
  repoName: string;
  number: number;
  commentId: string;
}) @{
  const [open, setOpen] = useState(false);
  const historyQ = useQuery({
    queryKey: [
      "issue",
      "comments",
      "history",
      props.owner,
      props.repoName,
      props.number,
      props.commentId,
    ] as const,
    queryFn: async () => {
      const res = await apiClient.issue.comments.history({
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

function CommentItem(props: {
  comment: IssueCommentPublic;
  owner: string;
  repoName: string;
  number: number;
  canWrite: boolean;
  canInteract: boolean;
  currentUserId: string;
  onUpdated: (comment: IssueCommentPublic) => void;
  onDeleted: (id: string) => void;
}) @{
  const comment = props.comment;
  const isAuthor = !!props.currentUserId && comment.author_id === props.currentUserId;
  const canEdit = isAuthor;
  const canDelete = isAuthor || props.canWrite;
  const [editing, setEditing] = useState(false);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState("");

  const editForm = useForm({
    defaultValues: { body: comment.body },
    onSubmit: async ({ value }) => {
      setError("");
      setPending(true);
      try {
        const res = await apiClient.issue.comments.update({
          owner: props.owner,
          name: props.repoName,
          number: props.number,
          commentId: comment.id,
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
      const res = await apiClient.issue.comments.delete({
        owner: props.owner,
        name: props.repoName,
        number: props.number,
        commentId: comment.id,
      });
      if (!res.ok) {
        setError(res.error.message || "Could not delete comment.");
        setPending(false);
        return;
      }
      props.onDeleted(comment.id);
      setPending(false);
    } catch {
      setError(NETWORK_ERROR);
      setPending(false);
    }
  }

  <li className="rounded-md border border-border px-4 py-3" data-testid="issue-comment">
    <div className="flex flex-wrap items-start justify-between gap-2">
      <p className="text-[13px] text-muted-foreground">
        <AppLink
          href={`/${comment.author_username}` as string}
          className="font-semibold text-foreground no-underline hover:text-primary"
        >
          {`@${comment.author_username}` as string}
        </AppLink>
        {` commented ` as string}
        <span title={comment.created_at as string}>
          {formatRelative(comment.created_at) as string}
        </span>
      </p>
      <div className="flex gap-2">
        @if (canEdit && !editing) {
          <Button
            type="button"
            variant="ghost"
            disabled={pending}
            onClick={() => {
              editForm.setFieldValue("body", comment.body);
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
      </div>
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
              id={`comment-edit-${comment.id}`}
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
      <div className="mt-2 flex flex-col gap-2">
        <CommentBody body={comment.body} owner={props.owner} repoName={props.repoName} />
        <IssueReactionsBar
          owner={props.owner}
          repoName={props.repoName}
          number={props.number}
          target="comment"
          commentId={comment.id}
          reactions={comment.reactions ?? []}
          canInteract={props.canInteract}
        />
        <CommentHistoryDisclosure
          owner={props.owner}
          repoName={props.repoName}
          number={props.number}
          commentId={comment.id}
        />
      </div>;
    }
  </li>;
}

/** Issue comment thread with create/edit/delete + history (ISS-02 / D-ISS-09 / D-ISS-12). */
export function IssueComments(props: IssueCommentsProps) @{
  const queryClient = useQueryClient();
  const [pending, setPending] = useState(false);
  const [error, setError] = useState("");

  const listQ = useQuery({
    queryKey: ["issue", "comments", "list", props.owner, props.repoName, props.number] as const,
    queryFn: async () => {
      const res = await apiClient.issue.comments.list({
        owner: props.owner,
        name: props.repoName,
        number: props.number,
      });
      if (!res.ok) throw new Error(res.error.message || "Could not load comments.");
      return res.data;
    },
    enabled: !!props.owner && !!props.repoName && props.number > 0,
  });

  const comments: IssueCommentPublic[] = listQ.data?.comments ?? [];

  const listKey = ["issue", "comments", "list", props.owner, props.repoName, props.number] as const;

  // Patch the cached thread with the mutation response instead of refetching
  // the whole list — the API returns the full comment row.
  function patchComments(fn: (cs: IssueCommentPublic[]) => IssueCommentPublic[]) {
    queryClient.setQueryData<IssueCommentsListResponse>(listKey, (old) =>
      old ? { comments: fn(old.comments) } : old,
    );
  }

  async function invalidate() {
    await queryClient.invalidateQueries({
      queryKey: listKey,
    });
  }

  const composeForm = useForm({
    defaultValues: { body: "" },
    onSubmit: async ({ value }) => {
      setError("");
      if (!value.body.trim()) {
        setError("Comment body is required.");
        return;
      }
      setPending(true);
      try {
        const res = await apiClient.issue.comments.create({
          owner: props.owner,
          name: props.repoName,
          number: props.number,
          body: value.body,
        });
        if (!res.ok) {
          setError(res.error.message || "Could not post comment.");
          setPending(false);
          return;
        }
        composeForm.reset();
        if (listQ.data) {
          patchComments((cs) => [...cs, res.data]);
        } else {
          await invalidate();
        }
        setPending(false);
      } catch {
        setError(NETWORK_ERROR);
        setPending(false);
      }
    },
  });

  <section
    aria-labelledby="issue-comments-heading"
    data-testid="issue-comments"
    className="flex flex-col gap-4"
  >
    <h2 id="issue-comments-heading" className="text-[16px] font-semibold text-foreground">
      Comments
    </h2>
    @if (listQ.isPending) {
      <p className="text-[14px] text-muted-foreground">Loading comments…</p>;
    } @else {
      @if (listQ.isError) {
        <p role="alert" className="text-[14px] text-destructive">
          Could not load comments.
        </p>;
      } @else {
        @if (comments.length === 0) {
          <p className="text-[14px] text-muted-foreground">No comments yet.</p>;
        } @else {
          <ul className="flex flex-col gap-3">
            @for (const c of comments; key c.id) {
              <CommentItem
                comment={c}
                owner={props.owner}
                repoName={props.repoName}
                number={props.number}
                canWrite={props.canWrite}
                canInteract={props.canInteract}
                currentUserId={props.currentUserId}
                onUpdated={(c) =>
                  patchComments((cs) => cs.map((x) => (x.id === c.id ? c : x)))
                }
                onDeleted={(id) => patchComments((cs) => cs.filter((x) => x.id !== id))}
              />;
            }
          </ul>;
        }
      }
    }

    @if (props.canInteract) {
      <form
        method="post"
        action="#"
        className="flex flex-col gap-3"
        data-testid="issue-comment-compose"
        onSubmit={(e) => {
          e.preventDefault();
          void composeForm.handleSubmit();
        }}
      >
        @if (error) {
          <AuthErrorBanner message={error} />;
        }
        <composeForm.Field name="body">
          {(field) => (
            <MarkdownWritePreview
              id="issue-comment-body"
              value={field.state.value}
              onValueChange={field.handleChange}
              rows={6}
              disabled={pending}
              placeholder="Leave a comment (markdown supported)"
              owner={props.owner}
              repo={props.repoName}
            />
          )}
        </composeForm.Field>
        <div>
          <Button type="submit" disabled={pending}>
            {(pending ? "Commenting…" : "Comment") as string}
          </Button>
        </div>
      </form>;
    }
    @if (!props.currentUserId && !props.canInteract) {
      <p className="rounded-md border border-border bg-muted/30 px-4 py-3 text-[13px] text-muted-foreground">
        <AppLink
          href="/login"
          className="font-medium text-primary no-underline hover:underline"
        >
          Sign in
        </AppLink>
        {" to join this conversation." as string}
      </p>;
    }
    @if (props.currentUserId && !props.canInteract) {
      <p className="rounded-md border border-border bg-muted/30 px-4 py-3 text-[13px] text-muted-foreground">
        {"Verify your email to join this conversation." as string}
      </p>;
    }
  </section>;
}
