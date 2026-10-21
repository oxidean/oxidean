import type {
  ProjectCardPublic,
  ProjectColumnPublic,
  ProjectPublic,
  RepoPublic,
} from "@oxidean/api-client";
import { queryOptions, useQuery, useQueryClient } from "@octanejs/tanstack-query";

import { matchPath } from "@/lib/route-params";
import { usePathname } from "@/lib/pathname.tsrx";
import { RepoLayout } from "@/routes/$owner.$repo.tsrx";
import { AppPage } from "@/components/app-page";
import { useState } from "octane";
import { KanbanSquare, Plus, Trash2 } from "@octanejs/lucide";
import { RepoNotFound } from "@/components/repo/repo-not-found";
import { RepoUnitDisabled } from "@/components/repo/repo-unit-disabled";
import { AppLink } from "@/components/ui/app-link";
import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/empty-state";
import { Input } from "@/components/ui/input";
import { apiClient } from "@/lib/api-client";
import { fetchRepoGet } from "@/lib/ssr-repo";
import { formatRelative } from "@/lib/relative-time";
import { toastSuccess } from "@/lib/toast";

/** View parsed out of the `/projects/*` splat — list or a single board. */
type ProjectsView = "list" | "board";

type BoardData = {
  project: ProjectPublic;
  columns: ProjectColumnPublic[];
  /** `column_id` → cards (position-ordered by the API). */
  cards: Record<string, ProjectCardPublic[]>;
};

type ProjectsLoaderData =
  | { kind: "not_found" }
  | { kind: "error"; message: string }
  | { kind: "disabled"; repo: RepoPublic }
  | {
      kind: "ready";
      repo: RepoPublic;
      view: ProjectsView;
      projects: ProjectPublic[];
      board: BoardData | null;
      /** Board number parsed from the splat but the API didn't find it. */
      boardMissing: boolean;
    };

type Phase = { kind: "loading" } | ProjectsLoaderData;

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";

function parseProjectsSplat(splat: string | undefined): { view: ProjectsView; board: number } {
  const segs = (splat ?? "").split("/").filter(Boolean);
  if (segs.length === 1) {
    const n = Number(segs[0]);
    if (Number.isInteger(n) && n >= 1) return { view: "board", board: n };
  }
  return { view: "list", board: 0 };
}

async function loadProjects(
  owner: string,
  repoName: string,
  splat: string | undefined,
): Promise<ProjectsLoaderData> {
  const got = await fetchRepoGet({ owner, name: repoName });
  if (!got.ok) {
    return got.error.status === 404
      ? { kind: "not_found" }
      : { kind: "error", message: got.error.message || NETWORK_ERROR };
  }
  const repo = got.data;
  if (repo.projects_enabled === false) {
    return { kind: "disabled", repo };
  }
  const { view, board } = parseProjectsSplat(splat);

  const listRes = await apiClient.projects.list({ owner, name: repoName, state: "all" });
  if (!listRes.ok) {
    return listRes.error.status === 404
      ? { kind: "not_found" }
      : { kind: "error", message: listRes.error.message || NETWORK_ERROR };
  }
  const projects = listRes.data.projects;

  let boardData: BoardData | null = null;
  let boardMissing = false;
  if (view === "board") {
    const projectRes = await apiClient.projects.get({ owner, name: repoName, project: board });
    if (!projectRes.ok) {
      boardMissing = true;
    } else {
      const colRes = await apiClient.projects.columns.list({
        owner,
        name: repoName,
        project: board,
      });
      const columns = colRes.ok ? colRes.data.columns : [];
      const cards: Record<string, ProjectCardPublic[]> = {};
      for (const col of columns) {
        const cardRes = await apiClient.projects.cards.list({
          owner,
          name: repoName,
          column: col.id,
          archived: true,
        });
        cards[col.id] = cardRes.ok ? cardRes.data.cards : [];
      }
      boardData = { project: projectRes.data, columns, cards };
    }
  }
  return { kind: "ready", repo, view, projects, board: boardData, boardMissing };
}

function projectsQueryOptions(params: Record<string, string | undefined>) {
  return queryOptions({
    queryKey: [
      "repo-page",
      "/$owner/$repo/projects/$",
      params.owner ?? "",
      params.repo ?? "",
      params._splat ?? "",
    ],
    queryFn: async (): Promise<ProjectsLoaderData> => {
      const owner = String((params as { owner?: string }).owner ?? "");
      const repoName = String((params as { repo?: string }).repo ?? "");
      const splat = (params as { _splat?: string })._splat;
      try {
        return await loadProjects(owner, repoName, splat);
      } catch {
        return { kind: "error", message: NETWORK_ERROR };
      }
    },
    enabled: Boolean(params.owner && params.repo),
    retry: false,
    staleTime: 15_000,
  });
}

function boardHref(owner: string, repoName: string, number: number): string {
  return `/${owner}/${repoName}/projects/${number}`;
}

/** A single board row on the index. */
function ProjectRow(props: { owner: string; repoName: string; project: ProjectPublic }) @{
  const p = props.project;
  <AppLink
    href={boardHref(props.owner, props.repoName, p.number)}
    className="flex items-center justify-between gap-3 rounded-md border border-border px-4 py-3 hover:bg-accent/40"
    data-testid={`project-row-${p.number}`}
  >
    <span className="min-w-0">
      <span className="block truncate text-[15px] font-medium text-foreground">{p.title}</span>
      <span className="text-[13px] text-muted-foreground">#{p.number}</span>
    </span>
    <span className="shrink-0 text-[13px] text-muted-foreground">
      {p.state === "closed" ? "Closed" : `Updated ${formatRelative(p.updated_at)}`}
    </span>
  </AppLink>;
}

/** Board list — GitHub's `/projects` index. */
function ProjectList(props: {
  owner: string;
  repoName: string;
  projects: ProjectPublic[];
  canWrite: boolean;
  onChanged: () => void;
}) @{
  const [title, setTitle] = useState("");
  const [creating, setCreating] = useState(false);
  const [error, setError] = useState("");
  const open = props.projects.filter((p) => p.state !== "closed");
  const closed = props.projects.filter((p) => p.state === "closed");

  async function createProject() {
    const t = title.trim();
    if (!t || creating) return;
    setError("");
    setCreating(true);
    try {
      const res = await apiClient.projects.create({
        owner: props.owner,
        name: props.repoName,
        title: t,
      });
      if (!res.ok) {
        setError(res.error.message || "Could not create the project.");
        return;
      }
      setTitle("");
      toastSuccess("Project created.");
      props.onChanged();
    } catch {
      setError(NETWORK_ERROR);
    } finally {
      setCreating(false);
    }
  }

  <div className="flex flex-col gap-6">
    @if (props.canWrite) {
      <form
        className="flex max-w-md items-end gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          void createProject();
        }}
      >
        <div className="flex-1">
          <Input
            value={title}
            onInput={(e) => setTitle((e.target as HTMLInputElement).value)}
            placeholder="New project name"
            aria-label="New project name"
            data-testid="project-new-title"
          />
        </div>
        <Button type="submit" disabled={!title.trim() || creating}>
          New project
        </Button>
      </form>;
    }
    @if (error) {
      <p role="alert" className="text-[14px] text-destructive">
        {error as string}
      </p>;
    }
    @if (props.projects.length === 0) {
      <EmptyState
        icon={<KanbanSquare aria-hidden size={22} />}
        title="No projects yet"
        description={
          props.canWrite
            ? "Create a project board to organize issues and pull requests."
            : "This repository has no project boards."
        }
      />;
    } @else {
      <div className="flex flex-col gap-2" data-testid="project-list">
        @for (p of open; key p.id) {
          <ProjectRow owner={props.owner} repoName={props.repoName} project={p} />;
        }
        @if (closed.length > 0) {
          <>
            <h2 className="mt-4 text-[14px] font-semibold text-muted-foreground">
              Closed
            </h2>
            @for (p of closed; key p.id) {
              <ProjectRow
                owner={props.owner}
                repoName={props.repoName}
                project={p}
              />;
            }
          </>;
        }
      </div>;
    }
  </div>;
}

/** A single card in a board column. */
function BoardCard(props: {
  card: ProjectCardPublic;
  canWrite: boolean;
  onDelete: (card: ProjectCardPublic) => void;
  onConvert: (card: ProjectCardPublic) => void;
}) @{
  const linked = !!props.card.content_url;
  const [confirming, setConfirming] = useState(false);
  <div
    className="group flex items-start justify-between gap-2 rounded-md border border-border bg-card px-3 py-2"
    data-testid={`project-card-${props.card.id}`}
  >
    <div className="min-w-0">
      @if (linked) {
        <>
          <AppLink
            href={(props.card.content_url ?? "#") as string}
            className="block truncate text-[14px] font-medium text-foreground hover:underline"
          >
            {props.card.content_title ?? props.card.note ?? "Linked item"}
          </AppLink>
          <span className="text-[12px] text-muted-foreground">
            {props.card.content_type === "pull_request" ? "Pull" : "Issue"} #
            {String(props.card.content_number ?? "")}
            {props.card.content_state ? ` · ${props.card.content_state}` : ""}
          </span>
        </>;
      } @else {
        <p className="whitespace-pre-wrap text-[14px] text-foreground">
          {props.card.note ?? ""}
        </p>;
      }
      @if (props.card.archived) {
        <span className="text-[11px] font-medium uppercase tracking-wide text-muted-foreground">
          Archived
        </span>;
      }
    </div>
    @if (props.canWrite) {
      <div className="flex shrink-0 gap-1">
        @if (!linked && !props.card.archived) {
          @if (confirming) {
            <Button
              variant="outline"
              size="sm"
              onClick={() => props.onConvert(props.card)}
              data-testid={`card-convert-confirm-${props.card.id}`}
            >
              Convert?
            </Button>;
          } @else {
            <Button
              variant="ghost"
              size="sm"
              aria-label="Convert note to issue"
              title="Convert to issue"
              onClick={() => setConfirming(true)}
              data-testid={`card-convert-${props.card.id}`}
            >
              →Issue
            </Button>;
          }
        }
        <Button
          variant="ghost"
          size="sm"
          aria-label="Delete card"
          onClick={() => props.onDelete(props.card)}
          data-testid={`card-delete-${props.card.id}`}
        >
          <Trash2 aria-hidden size={14} />
        </Button>
      </div>;
    }
  </div>;
}

/** Board view — GitHub's column layout. */
function ProjectBoard(props: {
  owner: string;
  repoName: string;
  board: BoardData;
  canWrite: boolean;
  canAdmin: boolean;
  onChanged: () => void;
}) @{
  const [newColumn, setNewColumn] = useState("");
  const [newCardCol, setNewCardCol] = useState("");
  const [newCardNote, setNewCardNote] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const project = props.board.project;
  const closed = project.state === "closed";

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
        setError(res.error?.message || "The project operation failed.");
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

  async function addColumn() {
    const column_name = newColumn.trim();
    if (!column_name) return;
    await run(
      () =>
        apiClient.projects.columns.create({
          owner: props.owner,
          name: props.repoName,
          project: project.number,
          column_name,
        }),
      "Column added.",
    ).then(() => setNewColumn(""));
  }

  async function addCard() {
    const note = newCardNote.trim();
    if (!note || !newCardCol) return;
    await run(
      () =>
        apiClient.projects.cards.create({
          owner: props.owner,
          name: props.repoName,
          column: newCardCol,
          note,
        }),
      "Card added.",
    ).then(() => {
      setNewCardNote("");
    });
  }

  async function setState(state: "open" | "closed") {
    await run(
      () =>
        apiClient.projects.update({
          owner: props.owner,
          name: props.repoName,
          project: project.number,
          state,
        }),
      state === "closed" ? "Project closed." : "Project reopened.",
    );
  }

  async function deleteBoard() {
    await run(
      () =>
        apiClient.projects.delete({
          owner: props.owner,
          name: props.repoName,
          project: project.number,
        }),
      "Project deleted.",
    );
  }

  <div className="flex flex-col gap-5">
    <div className="flex flex-wrap items-center justify-between gap-3">
      <div className="min-w-0">
        <h1 className="truncate text-[20px] font-semibold text-foreground">
          {project.title}
          <span className="ml-2 text-[15px] font-normal text-muted-foreground">
            #{project.number}
          </span>
        </h1>
        @if (project.body) {
          <p className="mt-1 whitespace-pre-wrap text-[14px] text-muted-foreground">
            {project.body}
          </p>;
        }
      </div>
      @if (props.canWrite) {
        <div className="flex shrink-0 gap-2">
          @if (closed) {
            <Button variant="outline" size="sm" onClick={() => void setState("open")}>
              Reopen
            </Button>;
          } @else {
            <Button variant="outline" size="sm" onClick={() => void setState("closed")}>
              Close project
            </Button>;
          }
          @if (props.canAdmin) {
            <Button
              variant="destructive"
              size="sm"
              onClick={() => void deleteBoard()}
              data-testid="project-delete"
            >
              Delete
            </Button>;
          }
        </div>;
      }
    </div>
    @if (error) {
      <p role="alert" className="text-[14px] text-destructive">
        {error as string}
      </p>;
    }
    @if (closed) {
      <p className="rounded-md border border-border bg-muted/40 px-3 py-2 text-[13px] text-muted-foreground">
        This project is closed — columns and cards are read-only.
      </p>;
    }

    <div className="flex items-start gap-4 overflow-x-auto pb-4" data-testid="project-board">
      @for (col of props.board.columns) {
        <section
          className="flex w-72 shrink-0 flex-col gap-2 rounded-lg border border-border bg-muted/30 p-3"
          aria-label={col.name}
          data-testid={`project-column-${col.id}`}
        >
          <div className="flex items-center justify-between gap-2">
            <h2 className="truncate text-[14px] font-semibold text-foreground">{col.name}</h2>
            <span className="text-[12px] text-muted-foreground">
              {String((props.board.cards[col.id] ?? []).filter((c) => !c.archived).length)}
            </span>
          </div>
          @for (card of props.board.cards[col.id] ?? []) {
            <BoardCard
              card={card}
              canWrite={props.canWrite && !closed}
              onDelete={(c) =>
                void run(
                  () =>
                    apiClient.projects.cards.delete({
                      owner: props.owner,
                      name: props.repoName,
                      project: project.number,
                      card: c.id,
                    }),
                  "Card deleted.",
                )
              }
              onConvert={(c) =>
                void run(
                  () =>
                    apiClient.projects.cards.convert({
                      owner: props.owner,
                      name: props.repoName,
                      project: project.number,
                      card: c.id,
                    }),
                  "Card converted to an issue.",
                )
              }
            />;
          }
          @if (props.canWrite && !closed) {
            <Button
              variant="ghost"
              size="sm"
              className="justify-start text-muted-foreground"
              onClick={() => setNewCardCol(col.id)}
              data-testid={`column-add-card-${col.id}`}
            >
              <Plus aria-hidden size={14} className="mr-1" /> Add card
            </Button>;
          }
        </section>;
      }
      @if (props.canWrite && !closed) {
        <form
          className="flex w-64 shrink-0 items-center gap-2"
          onSubmit={(e) => {
            e.preventDefault();
            void addColumn();
          }}
        >
          <Input
            value={newColumn}
            onInput={(e) => setNewColumn((e.target as HTMLInputElement).value)}
            placeholder="New column"
            aria-label="New column name"
            data-testid="column-new-name"
          />
          <Button type="submit" variant="outline" size="sm" disabled={!newColumn.trim()}>
            Add
          </Button>
        </form>;
      }
    </div>

    @if (newCardCol) {
      <form
        className="flex max-w-lg items-center gap-2 rounded-md border border-border p-3"
        onSubmit={(e) => {
          e.preventDefault();
          void addCard();
        }}
        data-testid="card-new-form"
      >
        <Input
          value={newCardNote}
          onInput={(e) => setNewCardNote((e.target as HTMLInputElement).value)}
          placeholder="Card note"
          aria-label="Card note"
          data-testid="card-new-note"
        />
        <Button type="submit" size="sm" disabled={!newCardNote.trim()}>
          Add card
        </Button>
        <Button type="button" variant="ghost" size="sm" onClick={() => setNewCardCol("")}>
          Cancel
        </Button>
      </form>;
    }
  </div>;
}

function ProjectsPageContent() @{
  const pathname = usePathname();
  const queryClient = useQueryClient();
  const params = (matchPath("/$owner/$repo/projects/$", pathname) ?? {}) as {
    owner?: string;
    repo?: string;
    _splat?: string;
  };
  const owner = params.owner ?? "";
  const repoName = params.repo ?? "";
  const loaderData = useQuery(projectsQueryOptions(params)).data;
  const phase: Phase = loaderData ?? { kind: "loading" };

  function refresh() {
    void queryClient.invalidateQueries({ queryKey: ["repo-page"] });
  }

  @if (phase.kind === "not_found") {
    <RepoNotFound />;
  } @else if (phase.kind === "loading") {
    <p className="text-[16px] text-muted-foreground">Loading projects…</p>;
  } @else if (phase.kind === "error") {
    <p role="alert" className="text-[16px] text-destructive">
      {phase.message as string}
    </p>;
  } @else if (phase.kind === "disabled") {
    <RepoUnitDisabled unit="projects" canAdmin={phase.repo.can_admin} />;
  } @else if (phase.view === "board" && phase.board) {
    <ProjectBoard
      owner={owner}
      repoName={repoName}
      board={phase.board}
      canWrite={!!phase.repo.can_write}
      canAdmin={!!phase.repo.can_admin}
      onChanged={refresh}
    />;
  } @else if (phase.view === "board" && phase.boardMissing) {
    <EmptyState
      icon={<KanbanSquare aria-hidden size={22} />}
      title="Project not found"
      description="This project board does not exist or was deleted."
    />;
  } @else {
    <ProjectList
      owner={owner}
      repoName={repoName}
      projects={phase.projects}
      canWrite={!!phase.repo.can_write}
      onChanged={refresh}
    />;
  }
}

/** Projects — repo-scoped classic boards (COL-13 Projects unit). */
export function RepoProjectsPage() @{
  <AppPage>
    <RepoLayout>
      <ProjectsPageContent />
    </RepoLayout>
  </AppPage>;
}
