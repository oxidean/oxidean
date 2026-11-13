import { useQuery, useQueryClient } from "@octanejs/tanstack-query";
import type {
  CommentRevisionPublic,
  PullCommentPublic,
  PullCommentsListResponse,
  PullPublic,
  RepoPublic,
} from "@oxidean/api-client";
import { useForm } from "@octanejs/tanstack-form";
import { Bold, Code, Italic, Link, List, Pencil, Quote, Strikethrough } from "@octanejs/lucide";
import { useRef, useState } from "octane";
import { AuthErrorBanner } from "@/components/auth-shell";
import { MarkdownBody } from "@/components/repo/markdown-body";
import { MarkdownWritePreview } from "@/components/repo/markdown-write-preview";
import { ComposerExtras } from "./pull-v2-composer-extras";
import { PullV2MergeCard } from "./pull-v2-merge-card";
import { AppLink } from "@/components/ui/app-link";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import { apiClient } from "@/lib/api-client";
import { formatRelative } from "@/lib/relative-time";
import { cn } from "@/lib/utils";

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

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";

type CommentFilter = "all" | "unresolved" | "mine";

const FILTERS: { id: CommentFilter; label: string }[] = [
  { id: "all", label: "All" },
  { id: "unresolved", label: "Unresolved" },
  { id: "mine", label: "Mine" },
];

function CommentBody(props: { body: string; owner: string; repoName: string }) @{
  <MarkdownBody markdown={props.body} owner={props.owner} repo={props.repoName} />;
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

function OverviewCommentItem(props: {
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
          {`${c.path}${c.line != null ? `:${c.line}` : ""}` as string}
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
      <CommentHistoryDisclosure
        owner={props.owner}
        repoName={props.repoName}
        number={props.number}
        commentId={c.id}
      />;
    }
  </li>;
}

/**
 * New-comment composer with a GitHub-style markdown toolbar — every button
 * wraps the textarea selection (or prefixes its lines) at the cursor. The
 * field stays a native textarea per Octane conventions.
 */
function OverviewComposer(props: {
  busy: boolean;
  onPost: (body: string) => Promise<boolean>;
}) @{
  const [draft, setDraft] = useState("");
  const taRef = useRef<HTMLTextAreaElement | null>(null);

  function commit(next: string, caret: number, caretEnd?: number) {
    setDraft(next);
    const ta = taRef.current;
    if (ta && typeof requestAnimationFrame === "function") {
      requestAnimationFrame(() => {
        ta.focus();
        try {
          ta.setSelectionRange(caret, caretEnd ?? caret);
        } catch {
          /* selection restore is best-effort */
        }
      });
    }
  }

  function wrap(before: string, after: string, placeholder: string) {
    const ta = taRef.current;
    const v = ta ? ta.value : draft;
    const s = ta && typeof ta.selectionStart === "number" ? ta.selectionStart : v.length;
    const e = ta && typeof ta.selectionEnd === "number" ? ta.selectionEnd : v.length;
    const sel = v.slice(s, e) || placeholder;
    commit(
      v.slice(0, s) + before + sel + after + v.slice(e),
      s + before.length,
      s + before.length + sel.length,
    );
  }

  function prefixLines(prefix: string) {
    const ta = taRef.current;
    const v = ta ? ta.value : draft;
    const s = ta && typeof ta.selectionStart === "number" ? ta.selectionStart : v.length;
    const e = ta && typeof ta.selectionEnd === "number" ? ta.selectionEnd : v.length;
    const ls = v.lastIndexOf("\n", Math.max(0, s - 1)) + 1;
    const nl = v.indexOf("\n", e);
    const le = nl < 0 ? v.length : nl;
    const block = v
      .slice(ls, le)
      .split("\n")
      .map((l) => (l ? prefix + l : prefix.trimEnd()))
      .join("\n");
    commit(v.slice(0, ls) + block + v.slice(le), ls, ls + block.length);
  }

  async function post() {
    const body = draft.trim();
    if (!body) return;
    if (await props.onPost(body)) setDraft("");
  }

  const toolCls =
    "inline-flex size-7 items-center justify-center rounded-sm text-muted-foreground hover:bg-muted hover:text-foreground disabled:opacity-50";

  <div className="space-y-2" data-testid="pull-v2-composer">
    <div
      className="flex flex-wrap items-center gap-0.5 rounded-t-md border border-b-0 border-border bg-muted/40 px-1.5 py-1"
      role="toolbar"
      aria-label="Markdown formatting"
      data-testid="pull-v2-md-toolbar"
    >
      <button
        type="button"
        className={toolCls}
        aria-label="Bold"
        title="Bold"
        data-testid="pull-v2-md-bold"
        disabled={props.busy}
        onClick={() => wrap("**", "**", "bold text")}
      >
        <Bold aria-hidden size={14} />
      </button>
      <button
        type="button"
        className={toolCls}
        aria-label="Italic"
        title="Italic"
        data-testid="pull-v2-md-italic"
        disabled={props.busy}
        onClick={() => wrap("_", "_", "italic text")}
      >
        <Italic aria-hidden size={14} />
      </button>
      <button
        type="button"
        className={toolCls}
        aria-label="Strikethrough"
        title="Strikethrough"
        data-testid="pull-v2-md-strike"
        disabled={props.busy}
        onClick={() => wrap("~~", "~~", "strikethrough text")}
      >
        <Strikethrough aria-hidden size={14} />
      </button>
      <button
        type="button"
        className={toolCls}
        aria-label="Code"
        title="Code"
        data-testid="pull-v2-md-code"
        disabled={props.busy}
        onClick={() => wrap("`", "`", "code")}
      >
        <Code aria-hidden size={14} />
      </button>
      <button
        type="button"
        className={toolCls}
        aria-label="Quote"
        title="Quote"
        data-testid="pull-v2-md-quote"
        disabled={props.busy}
        onClick={() => prefixLines("> ")}
      >
        <Quote aria-hidden size={14} />
      </button>
      <button
        type="button"
        className={toolCls}
        aria-label="Insert link"
        title="Insert link"
        data-testid="pull-v2-md-link"
        disabled={props.busy}
        onClick={() => wrap("[", "](url)", "link text")}
      >
        <Link aria-hidden size={14} />
      </button>
      <button
        type="button"
        className={toolCls}
        aria-label="Bulleted list"
        title="Bulleted list"
        data-testid="pull-v2-md-list"
        disabled={props.busy}
        onClick={() => prefixLines("- ")}
      >
        <List aria-hidden size={14} />
      </button>
      <ComposerExtras value={draft} onValueChange={setDraft} disabled={props.busy} />
    </div>
    <Textarea
      ref={taRef}
      rows={3}
      className="rounded-t-none text-[13px]"
      placeholder="Leave a comment"
      value={draft}
      disabled={props.busy}
      data-testid="pull-v2-comment-draft"
      onInput={(e) => setDraft((e.currentTarget as HTMLTextAreaElement).value)}
    />
    <Button
      type="button"
      disabled={props.busy || !draft.trim()}
      data-testid="pull-v2-comment-submit"
      onClick={() => void post()}
    >
      Comment
    </Button>
  </div>;
}

/**
 * Conversation body for the v2 overview — same comment model + rendering as
 * the shared PullConversation, plus GitHub's segmentation (All / Unresolved /
 * Mine), oldest↔newest sort, a comment count, and a pending-drafts banner.
 */
function PullV2Conversation(props: {
  owner: string;
  repoName: string;
  number: number;
  canWrite: boolean;
  canInteract: boolean;
  currentUserId: string;
  body: string;
}) @{
  const queryClient = useQueryClient();
  const [filter, setFilter] = useState<CommentFilter>("all");
  const [sort, setSort] = useState<"oldest" | "newest">("oldest");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [editingBody, setEditingBody] = useState(false);
  const [bodyDraft, setBodyDraft] = useState("");
  const [bodyBusy, setBodyBusy] = useState(false);
  const [bodyError, setBodyError] = useState("");

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

  const listKey = ["pull", "comments", props.owner, props.repoName, props.number] as const;
  const all: PullCommentPublic[] = commentsQ.data?.comments ?? [];
  // Review drafts (pending === true) aren't conversation — they surface as a
  // banner pointing at Files changed, where the submit-review bar lives.
  const pending = all.filter((c) => c.pending);
  const posted = all.filter((c) => !c.pending);
  const filtered = posted.filter((c) => {
    if (filter === "unresolved") return !!c.path && !c.resolved;
    if (filter === "mine") return c.author_id === props.currentUserId;
    return true;
  });
  const ordered = [...filtered].sort((a, b) =>
    sort === "newest"
      ? b.created_at.localeCompare(a.created_at)
      : a.created_at.localeCompare(b.created_at),
  );
  const changesHref = `/${props.owner}/${props.repoName}/pull/${props.number}/changes`;

  function patchComments(fn: (cs: PullCommentPublic[]) => PullCommentPublic[]) {
    queryClient.setQueryData<PullCommentsListResponse>(listKey, (old) =>
      old ? { comments: fn(old.comments) } : old,
    );
  }

  async function postComment(body: string): Promise<boolean> {
    setBusy(true);
    setError(null);
    try {
      const res = await apiClient.pull.comments.create({
        owner: props.owner,
        name: props.repoName,
        number: props.number,
        body,
      });
      setBusy(false);
      if (!res.ok) {
        setError(res.error.message || "Could not post comment");
        return false;
      }
      if (queryClient.getQueryData<PullCommentsListResponse>(listKey)) {
        patchComments((cs) => [...cs, res.data]);
      } else {
        await queryClient.invalidateQueries({ queryKey: listKey });
      }
      return true;
    } catch {
      setBusy(false);
      setError(NETWORK_ERROR);
      return false;
    }
  }

  async function toggleResolve(c: PullCommentPublic) {
    setBusy(true);
    setError(null);
    try {
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
    } catch {
      setBusy(false);
      setError(NETWORK_ERROR);
    }
  }

  async function saveBody() {
    setBodyBusy(true);
    setBodyError("");
    try {
      const res = await apiClient.pull.update({
        owner: props.owner,
        name: props.repoName,
        number: props.number,
        body: bodyDraft,
      });
      setBodyBusy(false);
      if (!res.ok) {
        setBodyError(res.error.message || "Could not update the description.");
        return;
      }
      setEditingBody(false);
      await queryClient.invalidateQueries({
        queryKey: ["pull", "get", props.owner, props.repoName, props.number],
      });
    } catch {
      setBodyBusy(false);
      setBodyError(NETWORK_ERROR);
    }
  }

  <div className="space-y-4" data-testid="pull-v2-conversation">
    <div className="rounded-md border border-border p-3">
      <div className="mb-2 flex items-center gap-2">
        <p className="text-sm font-medium text-muted-foreground">Summary</p>
        @if (props.canWrite && !editingBody) {
          <button
            type="button"
            className="ml-auto inline-flex size-6 items-center justify-center rounded-sm text-muted-foreground hover:bg-muted hover:text-foreground"
            aria-label="Edit description"
            data-testid="pull-v2-body-edit"
            onClick={() => {
              setBodyDraft(props.body);
              setBodyError("");
              setEditingBody(true);
            }}
          >
            <Pencil aria-hidden size={13} />
          </button>;
        }
      </div>
      @if (editingBody) {
        <div className="space-y-2" data-testid="pull-v2-body-editor">
          <MarkdownWritePreview
            id="pull-v2-body-editor"
            value={bodyDraft}
            onValueChange={setBodyDraft}
            rows={8}
            disabled={bodyBusy}
            owner={props.owner}
            repo={props.repoName}
          />
          <div className="flex gap-2">
            <Button
              type="button"
              size="sm"
              disabled={bodyBusy}
              data-testid="pull-v2-body-save"
              onClick={() => void saveBody()}
            >
              Save
            </Button>
            <Button
              type="button"
              size="sm"
              variant="ghost"
              disabled={bodyBusy}
              data-testid="pull-v2-body-cancel"
              onClick={() => {
                setEditingBody(false);
                setBodyError("");
              }}
            >
              Cancel
            </Button>
          </div>
          @if (bodyError) {
            <p className="text-[12px] text-destructive" role="alert">
              {bodyError as string}
            </p>;
          }
        </div>;
      } @else {
        <CommentBody body={props.body} owner={props.owner} repoName={props.repoName} />;
      }
    </div>

    @if (pending.length > 0) {
      <div
        className="flex flex-wrap items-center gap-2 rounded-md border border-primary/30 bg-primary/5 px-3 py-2 text-[13px] text-foreground"
        data-testid="pull-v2-pending-banner"
      >
        <span>
          {
            `${pending.length} pending review comment${pending.length === 1 ? "" : "s"} — finish your review to post them.` as string
          }
        </span>
        <AppLink
          href={changesHref as string}
          className="font-medium text-primary no-underline hover:underline"
        >
          Go to Files changed
        </AppLink>
      </div>;
    }

    <div className="flex flex-wrap items-center gap-2">
      <div className="flex gap-1" role="group" aria-label="Filter comments">
        @for (const f of FILTERS; key f.id) {
          <button
            type="button"
            className={
              cn(
                "rounded-full px-2 py-0.5 text-[12px]",
                filter === f.id
                  ? "bg-primary/10 font-semibold text-primary"
                  : "text-muted-foreground hover:bg-muted",
              ) as string
            }
            data-testid={`pull-v2-overview-filter-${f.id}`}
            onClick={() => setFilter(f.id)}
          >
            {f.label as string}
          </button>;
        }
      </div>
      <span className="text-[12px] text-muted-foreground" data-testid="pull-v2-comments-count">
        {`${filtered.length} comment${filtered.length === 1 ? "" : "s"}` as string}
      </span>
      <select
        className="ml-auto rounded-md border border-border bg-background px-1.5 py-0.5 text-[12px] text-foreground"
        aria-label="Sort comments"
        value={sort}
        data-testid="pull-v2-comments-sort"
        onChange={(e) => {
          const v = (e.currentTarget as HTMLSelectElement).value;
          if (v === "oldest" || v === "newest") setSort(v);
        }}
      >
        <option value="oldest">Oldest first</option>
        <option value="newest">Newest first</option>
      </select>
    </div>

    @if (commentsQ.isPending) {
      <p className="text-sm text-muted-foreground">Loading comments…</p>;
    } @else {
      @if (commentsQ.isError) {
        <p className="text-sm text-destructive">Could not load comments.</p>;
      } @else {
        @if (ordered.length === 0) {
          <p className="text-sm text-muted-foreground" data-testid="pull-v2-comments-empty">
            No comments match this filter.
          </p>;
        } @else {
          <ul className="space-y-3">
            @for (const c of ordered; key c.id) {
              <OverviewCommentItem
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
    }

    @if (error) {
      <p className="text-sm text-destructive" role="alert">
        {error as string}
      </p>;
    }

    @if (props.canInteract) {
      <OverviewComposer busy={busy} onPost={postComment} />;
    }
  </div>;
}

/**
 * Overview tab of the new PR experience (WI-0001 S8): the merge card sits on
 * top of the v2 conversation (segmentation + toolbar composer). The right
 * pane — GitHub's Description/Activity/Comments/Merge-status selector — is
 * rendered one level up in PullDetailV2 so it persists across tabs.
 */
export function PullV2Overview(props: PullV2OverviewProps) @{
  <div className="space-y-6" data-testid="pull-v2-overview">
    <PullV2MergeCard
      owner={props.owner}
      repoName={props.repoName}
      number={props.number}
      canWrite={props.canWrite}
      pullState={(props.pull.state as string) ?? "open"}
      draft={props.pull.draft}
      headSha={props.pull.head_sha}
      mergedAt={props.pull.merged_at ?? null}
      mergedBy={props.pull.merged_by ?? null}
      closedAt={props.pull.closed_at ?? null}
      headOwner={props.pull.head_owner}
    />
    <PullV2Conversation
      owner={props.owner}
      repoName={props.repoName}
      number={props.number}
      canWrite={props.canWrite}
      canInteract={props.canInteract}
      currentUserId={props.currentUserId}
      body={(props.pull.body || "") as string}
    />
  </div>;
}
