/**
 * Repo wiki view — route coverage + COL-13 Wiki unit toggle.
 */
import { cleanup, waitFor } from "@octanejs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderWithQueryClient } from "@/test/render-with-query";
import { RepoWikiPage } from "./$owner.$repo.wiki.$";

describe("/$owner/$repo/wiki/$", () => {
  it("exports RepoWikiPage", () => {
    expect(typeof RepoWikiPage).toBe("function");
  });

  it("uses AppLink for internal navigation (client-side routing)", async () => {
    const src = await import("./$owner.$repo.wiki.$.tsrx?raw").then((m) =>
      String((m as { default: string }).default),
    );
    expect(src).toMatch(/AppLink/);
    expect(src).not.toMatch(/<a\s+href=\{?["'`]\//);
  });

  it("drives page CRUD through the wiki.* RPC surface", async () => {
    const src = await import("./$owner.$repo.wiki.$.tsrx?raw").then((m) =>
      String((m as { default: string }).default),
    );
    expect(src).toMatch(/wiki\.pages\.list/);
    expect(src).toMatch(/wiki\.pages\.get/);
    expect(src).toMatch(/wiki\.pages\.create/);
    expect(src).toMatch(/wiki\.pages\.update/);
    expect(src).toMatch(/wiki\.pages\.delete/);
    expect(src).toMatch(/wiki\.revisions\.list/);
  });
});

const repoGetMock = vi.fn();
const pagesListMock = vi.fn();
const pagesGetMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    repo: {
      get: (...args: unknown[]) => repoGetMock(...args),
    },
    wiki: {
      pages: {
        list: (...args: unknown[]) => pagesListMock(...args),
        get: (...args: unknown[]) => pagesGetMock(...args),
        create: vi.fn(),
        update: vi.fn(),
        delete: vi.fn(),
      },
      revisions: {
        list: vi.fn(),
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
  wiki_enabled: true,
};

beforeEach(() => {
  window.history.pushState({}, "", "/ada/hello/wiki");
  repoGetMock.mockReset();
  pagesListMock.mockReset();
  pagesGetMock.mockReset();
});

afterEach(() => {
  cleanup();
});

describe("RepoWikiPage (COL-13 unit toggle)", () => {
  it("renders the unit-disabled state when the wiki unit is off", async () => {
    repoGetMock.mockResolvedValue({
      ok: true,
      data: { ...baseRepo, wiki_enabled: false },
    });
    renderWithQueryClient(RepoWikiPage);
    await waitFor(() => {
      expect(document.body.textContent).toContain("Wiki is disabled");
    });
    expect(pagesListMock).not.toHaveBeenCalled();
  });

  it("renders the first-page CTA on an empty wiki", async () => {
    repoGetMock.mockResolvedValue({ ok: true, data: baseRepo });
    pagesListMock.mockResolvedValue({ ok: true, data: { pages: [] } });
    pagesGetMock.mockResolvedValue({
      ok: false,
      error: { code: "wiki.not_found", message: "wiki page not found" },
    });
    renderWithQueryClient(RepoWikiPage);
    await waitFor(() => {
      expect(document.body.textContent).toContain("Welcome to the wiki");
    });
    expect(document.body.textContent).toContain("Create the first page");
  });
});
