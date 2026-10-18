/**
 * Repo projects view — route coverage + COL-13 Projects unit toggle.
 */
import { cleanup, waitFor } from "@octanejs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderWithQueryClient } from "@/test/render-with-query";
import { RepoProjectsPage } from "./$owner.$repo.projects.$";

describe("/$owner/$repo/projects/$", () => {
  it("exports RepoProjectsPage", () => {
    expect(typeof RepoProjectsPage).toBe("function");
  });

  it("uses AppLink for internal navigation (client-side routing)", async () => {
    const src = await import("./$owner.$repo.projects.$.tsrx?raw").then((m) =>
      String((m as { default: string }).default),
    );
    expect(src).toMatch(/AppLink/);
    expect(src).not.toMatch(/<a\s+href=\{?["'`]\//);
  });

  it("drives board/card CRUD through the projects.* RPC surface", async () => {
    const src = await import("./$owner.$repo.projects.$.tsrx?raw").then((m) =>
      String((m as { default: string }).default),
    );
    expect(src).toMatch(/projects\.list/);
    expect(src).toMatch(/projects\.get/);
    expect(src).toMatch(/projects\.create/);
    expect(src).toMatch(/projects\.update/);
    expect(src).toMatch(/projects\.delete/);
    expect(src).toMatch(/projects\.columns\.list/);
    expect(src).toMatch(/projects\.columns\.create/);
    expect(src).toMatch(/projects\.cards\.list/);
    expect(src).toMatch(/projects\.cards\.create/);
    expect(src).toMatch(/projects\.cards\.convert/);
    expect(src).toMatch(/projects\.cards\.delete/);
  });
});

const repoGetMock = vi.fn();
const projectsListMock = vi.fn();
const projectsGetMock = vi.fn();
const columnsListMock = vi.fn();
const cardsListMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    repo: {
      get: (...args: unknown[]) => repoGetMock(...args),
    },
    projects: {
      list: (...args: unknown[]) => projectsListMock(...args),
      get: (...args: unknown[]) => projectsGetMock(...args),
      create: vi.fn(),
      update: vi.fn(),
      delete: vi.fn(),
      columns: {
        list: (...args: unknown[]) => columnsListMock(...args),
        create: vi.fn(),
        update: vi.fn(),
        move: vi.fn(),
        delete: vi.fn(),
      },
      cards: {
        list: (...args: unknown[]) => cardsListMock(...args),
        create: vi.fn(),
        update: vi.fn(),
        move: vi.fn(),
        convert: vi.fn(),
        delete: vi.fn(),
      },
    },
  },
}));

const baseRepo = {
  id: "r1",
  owner_id: "u1",
  owner_type: "user" as const,
  owner_username: "ada",
  name: "hello",
  description: "",
  visibility: "public" as const,
  default_branch: "main",
  updated_at: "2026-10-02T00:00:00Z",
  can_admin: true,
  can_write: true,
  projects_enabled: true,
};

const board = {
  id: "p1",
  repo_id: "r1",
  number: 1,
  title: "Roadmap",
  body: "",
  state: "open",
  html_url: "/ada/hello/projects/1",
  created_at: "2026-10-02T00:00:00Z",
  updated_at: "2026-10-02T00:00:00Z",
};

beforeEach(() => {
  window.history.pushState({}, "", "/ada/hello/projects");
  repoGetMock.mockReset();
  projectsListMock.mockReset();
  projectsGetMock.mockReset();
  columnsListMock.mockReset();
  cardsListMock.mockReset();
});

afterEach(() => {
  cleanup();
});

describe("RepoProjectsPage (COL-13 unit toggle)", () => {
  it("renders the unit-disabled state when the projects unit is off", async () => {
    repoGetMock.mockResolvedValue({
      ok: true,
      data: { ...baseRepo, projects_enabled: false },
    });
    renderWithQueryClient(RepoProjectsPage);
    await waitFor(() => {
      expect(document.body.textContent).toContain("Projects is disabled");
    });
    expect(projectsListMock).not.toHaveBeenCalled();
  });

  it("renders the empty-board CTA when no projects exist", async () => {
    repoGetMock.mockResolvedValue({ ok: true, data: baseRepo });
    projectsListMock.mockResolvedValue({ ok: true, data: { projects: [] } });
    renderWithQueryClient(RepoProjectsPage);
    await waitFor(() => {
      expect(document.body.textContent).toContain("No projects yet");
    });
  });

  it("lists boards on the index view", async () => {
    repoGetMock.mockResolvedValue({ ok: true, data: baseRepo });
    projectsListMock.mockResolvedValue({
      ok: true,
      data: { projects: [board] },
    });
    renderWithQueryClient(RepoProjectsPage);
    await waitFor(() => {
      expect(document.body.textContent).toContain("Roadmap");
    });
  });

  it("renders the board view for /projects/{n}", async () => {
    window.history.pushState({}, "", "/ada/hello/projects/1");
    repoGetMock.mockResolvedValue({ ok: true, data: baseRepo });
    projectsListMock.mockResolvedValue({ ok: true, data: { projects: [board] } });
    projectsGetMock.mockResolvedValue({ ok: true, data: board });
    columnsListMock.mockResolvedValue({
      ok: true,
      data: {
        columns: [
          {
            id: "c1",
            project_id: "p1",
            name: "Todo",
            position: 0,
            created_at: "2026-10-02T00:00:00Z",
            updated_at: "2026-10-02T00:00:00Z",
          },
        ],
      },
    });
    cardsListMock.mockResolvedValue({
      ok: true,
      data: {
        cards: [
          {
            id: "k1",
            column_id: "c1",
            note: "Write the spec",
            position: 0,
            archived: false,
            created_at: "2026-10-02T00:00:00Z",
            updated_at: "2026-10-02T00:00:00Z",
          },
        ],
      },
    });
    renderWithQueryClient(RepoProjectsPage);
    await waitFor(() => {
      expect(document.body.textContent).toContain("Write the spec");
    });
    expect(document.body.textContent).toContain("Todo");
  });
});
