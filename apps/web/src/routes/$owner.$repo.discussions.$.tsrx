import type {
  DiscussionCategoryPublic,
  DiscussionCommentPublic,
  DiscussionPublic,
  RepoPublic,
} from "@oxidean/api-client";
import { queryOptions, useQuery, useQueryClient } from "@octanejs/tanstack-query";

import { matchPath } from "@/lib/route-params";
import { usePathname } from "@/lib/pathname.tsrx";
import { RepoLayout } from "@/routes/$owner.$repo.tsrx";
import { AppPage } from "@/components/app-page";
import { useState } from "octane";
import { CheckCircle2, Lock, MessagesSquare, Pin, PinOff, Trash2, Unlock } from "@octanejs/lucide";
import { RepoNotFound } from "@/components/repo/repo-not-found";
import { RepoUnitDisabled } from "@/components/repo/repo-unit-disabled";
import { AppLink } from "@/components/ui/app-link";
import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/empty-state";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { MarkdownBody } from "@/components/repo/markdown-body";
import { apiClient } from "@/lib/api-client";
import { fetchRepoGet } from "@/lib/ssr-repo";
import { formatRelative } from "@/lib/relative-time";
import { toastSuccess } from "@/lib/toast";

/** View parsed out of the `/discussions/*` splat — index or one thread. */
type DiscussionsView = "list" | "thread";

type DiscussionsLoaderData =
  | { kind: "not_found" }
  | { kind: "error"; message: string }
  | { kind: "disabled"; repo: RepoPublic }
  | {
      kind: "ready";
      repo: RepoPublic;
      view: DiscussionsView;
      categories: DiscussionCategoryPublic[];
      /** Category filter carried by `/discussions?category=<slug>`. */
      filterCategory: string;
      discussions: DiscussionPublic[];
      thread: DiscussionPublic | null;
      comments: DiscussionCommentPublic[];
      /** Splat parsed as a number but the API didn't find it. */
      threadMissing: boolean;
    };

type Phase = { kind: "loading" } | DiscussionsLoaderData;

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";

function parseSplat(splat: string | undefined): { view: DiscussionsView; thread: number } {
  const segs = (splat ?? "").split("/").filter(Boolean);
  if (segs.length === 1) {
    const n = Number(segs[0]);
    if (Number.isInteger(n) && n >= 1) return { view: "thread", thread: n };
  }
  return { view: "list", thread: 0 };
}

async function loadDiscussions(
  owner: string,
  repoName: string,
  splat: string | undefined,
  categoryFilter: string,
): Promise<DiscussionsLoaderData> {
  const got = await fetchRepoGet({ owner, name: repoName });
  if (!got.ok) {
    return got.error.status === 404
      ? { kind: "not_found" }
      : { kind: "error", message: got.error.message || NETWORK_ERROR };
  }
  const repo = got.data;
  if (repo.discussions_enabled === false) {
    return { kind: "disabled", repo };
  }
  const { view, thread } = parseSplat(splat);

  const catRes = await apiClient.discussions.categories.list({ owner, name: repoName });
  const categories = catRes.ok ? catRes.data.categories : [];

  if (view === "thread") {
    const threadRes = await apiClient.discussions.get({
      owner,
      name: repoName,
      discussion: thread,
    });
    if (!threadRes.ok) {
      return threadRes.error.status === 404
        ? {
            kind: "ready",
            repo,
            view,
            categories,
            filterCategory: "",
            discussions: [],
            thread: null,
            comments: [],
            threadMissing: true,
          }
        : { kind: "error", message: threadRes.error.message || NETWORK_ERROR };
    }
    const commentRes = await apiClient.discussions.comments.list({
      owner,
      name: repoName,
      discussion: thread,
    });
    return {
      kind: "ready",
      repo,
      view,
      categories,
      filterCategory: "",
      discussions: [],
      thread: threadRes.data,
      comments: commentRes.ok ? commentRes.data.comments : [],
      threadMissing: false,
    };
  }

  const listRes = await apiClient.discussions.list({
    owner,
    name: repoName,
    category: categoryFilter || undefined,
  });
  if (!listRes.ok) {
    // A stale `?category=<slug>` filter (renamed/deleted) just falls back to
    // the unfiltered index.
    const retry = await apiClient.discussions.list({ owner, name: repoName });
    if (!retry.ok) {
      return { kind: "error", message: listRes.error.message || NETWORK_ERROR };
    }
    return {
      kind: "ready",
      repo,
      view,
      categories,
      filterCategory: "",
      discussions: retry.data.discussions,
      thread: null,
      comments: [],
      threadMissing: false,
    };
  }
  return {
    kind: "ready",
    repo,
    view,
    categories,
    filterCategory: categoryFilter,
    discussions: listRes.data.discussions,
    thread: null,
    comments: [],
    threadMissing: false,
  };
}

function discussionsQueryOptions(params: Record<string, string | undefined>) {
  return queryOptions({
    queryKey: [
      "repo-page",
      "/$owner/$repo/discussions/$",
      params.owner ?? "",
      params.repo ?? "",
      params._splat ?? "",
      params.category ?? "",
    ],
    queryFn: async (): Promise<DiscussionsLoaderData> => {
      const owner = String((params as { owner?: string }).owner ?? "");
      const repoName = String((params as { repo?: string }).repo ?? "");
      try {
        return await loadDiscussions(owner, repoName, params._splat, String(params.category ?? ""));
      } catch {
        return { kind: "error", message: NETWORK_ERROR };
      }
    },
    enabled: Boolean(params.owner && params.repo),
    retry: false,
    staleTime: 15_000,
  });
}

function threadHref(owner: string, repoName: string, number: number): string {
  return `/${owner}/${repoName}/discussions/${number}`;
}

function categoryBadge(cat: DiscussionCategoryPublic) {
  return `${cat.emoji ? `${cat.emoji} ` : ""}${cat.name}`;
}

/** A single discussion row on the index. */
function DiscussionRow(props: {
  owner: string;
  repoName: string;
  discussion: DiscussionPublic;
}) @{
  const d = props.discussion;
  <AppLink
    href={threadHref(props.owner, props.repoName, d.number)}
    className="flex items-start justify-between gap-3 rounded-md border border-border px-4 py-3 hover:bg-accent/40"
    data-testid={`discussion-row-${d.number}`}
  >
    <span className="min-w-0">
      <span className="flex flex-wrap items-center gap-2">
        <span className="truncate text-[15px] font-medium text-foreground">{d.title}</span>
        @if (d.pinned) {
          <Pin aria-hidden size={13} className="shrink-0 text-muted-foreground" />;
        }
        @if (d.category) {
          <span className="rounded-full border border-border px-2 py-0.5 text-[12px] text-muted-foreground">
            {categoryBadge(d.category as DiscussionCategoryPublic)}
          </span>;
        }
        @if (d.answer_comment_id) {
          <span className="rounded-full border border-success/40 bg-success/10 px-2 py-0.5 text-[12px] text-success">
            Answered
          </span>;
        }
      </span>
      <span className="mt-1 block text-[13px] text-muted-foreground">
        #{d.number}
        {d.author_username ? ` · ${d.author_username}` : ""}
        {` · ${formatRelative(d.created_at)}`}
      </span>
    </span>
    <span className="flex shrink-0 items-center gap-1 text-[13px] text-muted-foreground">
      @if (d.locked) {
        <Lock aria-hidden size={13} />;
      }
    </span>
  </AppLink>;
}

/** New-discussion composer (write+). */
function NewDiscussionForm(props: {
  owner: string;
  repoName: string;
  categories: DiscussionCategoryPublic[];
  onChanged: () => void;
}) @{
  const [title, setTitle] = useState("");
  const [body, setBody] = useState("");
  const [category, setCategory] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  async function create() {
    const t = title.trim();
    if (!t || busy) return;
    setError("");
    setBusy(true);
    try {
      const res = await apiClient.discussions.create({
        owner: props.owner,
        name: props.repoName,
        title: t,
        body,
        category: category || undefined,
      });
      if (!res.ok) {
        setError(res.error.message || "Could not start the discussion.");
        return;
      }
      setTitle("");
      setBody("");
      toastSuccess("Discussion started.");
      props.onChanged();
    } catch {
      setError(NETWORK_ERROR);
    } finally {
      setBusy(false);
    }
  }

  <form
    className="flex flex-col gap-2 rounded-md border border-border p-4"
    onSubmit={(e) => {
      e.preventDefault();
      void create();
    }}
    data-testid="discussion-new-form"
  >
    <div className="flex items-center gap-2">
      <div className="flex-1">
        <Input
          value={title}
          onInput={(e) => setTitle((e.target as HTMLInputElement).value)}
          placeholder="Discussion title"
          aria-label="Discussion title"
          data-testid="discussion-new-title"
        />
      </div>
      @if (props.categories.length > 0) {
        <select
          className="h-9 rounded-md border border-input bg-background px-2 text-[14px] text-foreground"
          value={category}
          onChange={(e) => setCategory((e.target as HTMLSelectElement).value)}
          aria-label="Category"
          data-testid="discussion-new-category"
        >
          <option value="">Uncategorized</option>
          @for (c of props.categories) {
            <option value={c.id}>{categoryBadge(c)}</option>;
          }
        </select>;
      }
      <Button type="submit" disabled={!title.trim() || busy}>
        New discussion
      </Button>
    </div>
    <Textarea
      value={body}
      onInput={(e) => setBody((e.target as HTMLTextAreaElement).value)}
      placeholder="Write the first post…"
      aria-label="Discussion body"
      data-testid="discussion-new-body"
    />
    @if (error) {
      <p role="alert" className="text-[14px] text-destructive">
        {error as string}
      </p>;
    }
  </form>;
}

/** Index — category rail + thread list (GitHub's `/discussions`). */
function DiscussionList(props: {
  owner: string;
  repoName: string;
  categories: DiscussionCategoryPublic[];
  filterCategory: string;
  discussions: DiscussionPublic[];
  canWrite: boolean;
  onChanged: () => void;
}) @{
  const base = `/${props.owner}/${props.repoName}/discussions`;
  <div className="flex flex-col gap-6">
    @if (props.categories.length > 0) {
      <div
        className="flex flex-wrap items-center gap-2"
        data-testid="discussion-category-rail"
      >
        <AppLink
          href={base}
          className={
            props.filterCategory === ""
              ? "rounded-full border border-border bg-accent px-3 py-1 text-[13px] font-medium text-foreground"
              : "rounded-full border border-border px-3 py-1 text-[13px] text-muted-foreground hover:bg-accent/40"
          }
        >
          All
        </AppLink>
        @for (c of props.categories) {
          <AppLink
            href={`${base}?category=${encodeURIComponent(c.slug)}`}
            className={
              props.filterCategory === c.slug
                ? "rounded-full border border-border bg-accent px-3 py-1 text-[13px] font-medium text-foreground"
                : "rounded-full border border-border px-3 py-1 text-[13px] text-muted-foreground hover:bg-accent/40"
            }
            data-testid={`discussion-category-${c.slug}`}
          >
            {categoryBadge(c)}
          </AppLink>;
        }
      </div>;
    }
    @if (props.canWrite) {
      <NewDiscussionForm
        owner={props.owner}
        repoName={props.repoName}
        categories={props.categories}
        onChanged={props.onChanged}
      />;
    }
    @if (props.discussions.length === 0) {
      <EmptyState
        icon={<MessagesSquare aria-hidden size={22} />}
        title="No discussions yet"
        description={
          props.canWrite
            ? "Start a discussion to ask a question, share an idea, or talk with the community."
            : "This repository has no discussions."
        }
      />;
    } @else {
      <div className="flex flex-col gap-2" data-testid="discussion-list">
        @for (d of props.discussions; key d.id) {
          <DiscussionRow
            owner={props.owner}
            repoName={props.repoName}
            discussion={d}
          />;
        }
      </div>;
    }
  </div>;
}

/** One comment (or nested reply) on the thread. */
function CommentView(props: {
  comment: DiscussionCommentPublic;
  answerCommentId?: string | null;
  canModerate: boolean;
  canAnswer: boolean;
  isAnswerable: boolean;
  locked: boolean;
  onAnswer: (comment: DiscussionCommentPublic) => void;
  onDelete: (comment: DiscussionCommentPublic) => void;
}) @{
  const c = props.comment;
  const isAnswer = props.answerCommentId === c.id;
  <div
    className={
      isAnswer
        ? "rounded-md border border-success/50 bg-success/5 p-4"
        : "rounded-md border border-border p-4"
    }
    data-testid={`discussion-comment-${c.id}`}
  >
    <div className="flex items-center justify-between gap-2">
      <span className="text-[13px] text-muted-foreground">
        {c.author_username ?? "unknown"}
        {` · ${formatRelative(c.created_at)}`}
      </span>
      <span className="flex items-center gap-1">
        @if (isAnswer) {
          <span className="flex items-center gap-1 text-[12px] font-medium text-success">
            <CheckCircle2 aria-hidden size={13} /> Answer
          </span>;
        } @else {
          @if (props.canAnswer && props.isAnswerable && !props.locked) {
            <Button
              variant="ghost"
              size="sm"
              onClick={() => props.onAnswer(c)}
              data-testid={`comment-answer-${c.id}`}
            >
              Mark as answer
            </Button>;
          }
        }
        @if (props.canModerate || false) {
          <Button
            variant="ghost"
            size="sm"
            aria-label="Delete comment"
            onClick={() => props.onDelete(c)}
            data-testid={`comment-delete-${c.id}`}
          >
            <Trash2 aria-hidden size={14} />
          </Button>;
        }
      </span>
    </div>
    <MarkdownBody markdown={c.body} className="mt-2 text-[14px]" />
  </div>;
}

/** Thread view — the original post plus the comment stream. */
function ThreadView(props: {
  owner: string;
  repoName: string;
  thread: DiscussionPublic;
  comments: DiscussionCommentPublic[];
  canWrite: boolean;
  onChanged: () => void;
}) @{
  const d = props.thread;
  const [body, setBody] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const topLevel = props.comments.filter((c) => !c.parent_id);
  const repliesOf = (parentId: string) => props.comments.filter((c) => c.parent_id === parentId);
  const isAnswerable = !!d.category?.is_answerable;
  // GitHub: moderators (write+) and the thread author can mark answers /
  // lock-pin-delete; comment deletion is author-or-moderator.
  const canModerate = props.canWrite;

  async function run(
    fn: () => Promise<{ ok: boolean; error?: { message?: string } }>,
    ok?: string,
  ) {
    if (busy) return;
    setError("");
    setBusy(true);
    try {
      const res = await fn();
      if (!res.ok) {
        setError(res.error?.message || "The discussion operation failed.");
        return;
      }
      if (ok) toastSuccess(ok);
      props.onChanged();
    } catch {
      setError(NETWORK_ERROR);
    } finally {
      setBusy(false);
    }
  }

  async function postComment() {
    const b = body.trim();
    if (!b || busy) return;
    await run(
      () =>
        apiClient.discussions.comments.create({
          owner: props.owner,
          name: props.repoName,
          discussion: d.number,
          body: b,
        }),
      "Comment posted.",
    ).then(() => setBody(""));
  }

  <div className="flex flex-col gap-5">
    <div className="flex flex-wrap items-start justify-between gap-3">
      <div className="min-w-0">
        <h1 className="flex flex-wrap items-center gap-2 text-[20px] font-semibold text-foreground">
          {d.title}
          <span className="text-[15px] font-normal text-muted-foreground">#{d.number}</span>
        </h1>
        <p className="mt-1 flex flex-wrap items-center gap-2 text-[13px] text-muted-foreground">
          {d.author_username ? `started by ${d.author_username}` : ""}
          {` · ${formatRelative(d.created_at)}`}
          @if (d.category) {
            <span className="rounded-full border border-border px-2 py-0.5 text-[12px]">
              {categoryBadge(d.category as DiscussionCategoryPublic)}
            </span>;
          }
          @if (d.pinned) {
            <span className="flex items-center gap-1">
              <Pin aria-hidden size={12} /> Pinned
            </span>;
          }
          @if (d.locked) {
            <span className="flex items-center gap-1">
              <Lock aria-hidden size={12} /> Locked
            </span>;
          }
        </p>
      </div>
      @if (canModerate) {
        <div className="flex shrink-0 gap-2">
          <Button
            variant="outline"
            size="sm"
            onClick={() =>
              void run(
                () =>
                  apiClient.discussions.pin({
                    owner: props.owner,
                    name: props.repoName,
                    discussion: d.number,
                    pinned: !d.pinned,
                  }),
                d.pinned ? "Unpinned." : "Pinned.",
              )
            }
            data-testid="discussion-pin"
          >
            {d.pinned ? <PinOff aria-hidden size={14} /> : <Pin aria-hidden size={14} />}
          </Button>
          <Button
            variant="outline"
            size="sm"
            onClick={() =>
              void run(
                () =>
                  apiClient.discussions.lock({
                    owner: props.owner,
                    name: props.repoName,
                    discussion: d.number,
                    locked: !d.locked,
                  }),
                d.locked ? "Unlocked." : "Locked.",
              )
            }
            data-testid="discussion-lock"
          >
            {d.locked ? <Unlock aria-hidden size={14} /> : <Lock aria-hidden size={14} />}
          </Button>
          <Button
            variant="destructive"
            size="sm"
            onClick={() =>
              void run(
                () =>
                  apiClient.discussions.delete({
                    owner: props.owner,
                    name: props.repoName,
                    discussion: d.number,
                  }),
                "Discussion deleted.",
              )
            }
            data-testid="discussion-delete"
          >
            Delete
          </Button>
        </div>;
      }
    </div>
    @if (error) {
      <p role="alert" className="text-[14px] text-destructive">
        {error as string}
      </p>;
    }

    <div className="rounded-md border border-border p-4" data-testid="discussion-body">
      @if (d.body) {
        <MarkdownBody
          markdown={d.body}
          owner={props.owner}
          repo={props.repoName}
          data-testid="discussion-body-md"
        />;
      } @else {
        <p className="text-[14px] text-muted-foreground">No description.</p>;
      }
    </div>

    @for (c of topLevel; key c.id) {
      <div className="flex flex-col gap-2">
        <CommentView
          comment={c}
          answerCommentId={d.answer_comment_id}
          canModerate={canModerate}
          canAnswer={canModerate}
          isAnswerable={isAnswerable}
          locked={d.locked}
          onAnswer={(target) =>
            void run(
              () =>
                apiClient.discussions.answer.set({
                  owner: props.owner,
                  name: props.repoName,
                  discussion: d.number,
                  comment: target.id,
                }),
              "Marked as the answer.",
            )
          }
          onDelete={(target) =>
            void run(
              () =>
                apiClient.discussions.comments.delete({
                  owner: props.owner,
                  name: props.repoName,
                  discussion: d.number,
                  comment: target.id,
                }),
              "Comment deleted.",
            )
          }
        />
        @for (r of repliesOf(c.id); key r.id) {
          <div className="ml-8">
            <CommentView
              comment={r}
              answerCommentId={d.answer_comment_id}
              canModerate={canModerate}
              canAnswer={canModerate}
              isAnswerable={isAnswerable}
              locked={d.locked}
              onAnswer={(target) =>
                void run(
                  () =>
                    apiClient.discussions.answer.set({
                      owner: props.owner,
                      name: props.repoName,
                      discussion: d.number,
                      comment: target.id,
                    }),
                  "Marked as the answer.",
                )
              }
              onDelete={(target) =>
                void run(
                  () =>
                    apiClient.discussions.comments.delete({
                      owner: props.owner,
                      name: props.repoName,
                      discussion: d.number,
                      comment: target.id,
                    }),
                  "Comment deleted.",
                )
              }
            />
          </div>;
        }
      </div>;
    }

    @if (d.locked && !canModerate) {
      <p className="rounded-md border border-border bg-muted/40 px-3 py-2 text-[13px] text-muted-foreground">
        This discussion is locked — only collaborators can comment.
      </p>;
    } @else {
      <form
        className="flex flex-col gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          void postComment();
        }}
        data-testid="comment-new-form"
      >
        <Textarea
          value={body}
          onInput={(e) => setBody((e.target as HTMLTextAreaElement).value)}
          placeholder="Write a reply…"
          aria-label="Reply"
          data-testid="comment-new-body"
        />
        <div>
          <Button type="submit" size="sm" disabled={!body.trim() || busy}>
            Comment
          </Button>
        </div>
      </form>;
    }
  </div>;
}

function DiscussionsPageContent() @{
  const pathname = usePathname();
  const queryClient = useQueryClient();
  const params = (matchPath("/$owner/$repo/discussions/$", pathname) ?? {}) as {
    owner?: string;
    repo?: string;
    _splat?: string;
    category?: string;
  };
  const search = new URLSearchParams(typeof window !== "undefined" ? window.location.search : "");
  params.category = search.get("category") ?? "";
  const owner = params.owner ?? "";
  const repoName = params.repo ?? "";
  const loaderData = useQuery(discussionsQueryOptions(params)).data;
  const phase: Phase = loaderData ?? { kind: "loading" };

  function refresh() {
    void queryClient.invalidateQueries({ queryKey: ["repo-page"] });
  }

  @if (phase.kind === "not_found") {
    <RepoNotFound />;
  } @else if (phase.kind === "loading") {
    <p className="text-[16px] text-muted-foreground">Loading discussions…</p>;
  } @else if (phase.kind === "error") {
    <p role="alert" className="text-[16px] text-destructive">
      {phase.message as string}
    </p>;
  } @else if (phase.kind === "disabled") {
    <RepoUnitDisabled unit="discussions" canAdmin={phase.repo.can_admin} />;
  } @else if (phase.view === "thread" && phase.thread) {
    <ThreadView
      owner={owner}
      repoName={repoName}
      thread={phase.thread}
      comments={phase.comments}
      canWrite={!!phase.repo.can_write}
      onChanged={refresh}
    />;
  } @else if (phase.view === "thread" && phase.threadMissing) {
    <EmptyState
      icon={<MessagesSquare aria-hidden size={22} />}
      title="Discussion not found"
      description="This discussion does not exist or was deleted."
    />;
  } @else {
    <DiscussionList
      owner={owner}
      repoName={repoName}
      categories={phase.categories}
      filterCategory={phase.filterCategory}
      discussions={phase.discussions}
      canWrite={!!phase.repo.can_write}
      onChanged={refresh}
    />;
  }
}

/** Discussions — repo-scoped categories, threads, and comments. */
export function RepoDiscussionsPage() @{
  <AppPage>
    <RepoLayout>
      <DiscussionsPageContent />
    </RepoLayout>
  </AppPage>;
}
