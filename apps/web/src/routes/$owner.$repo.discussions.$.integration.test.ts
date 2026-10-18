/**
 * Repo discussions view — route coverage + the Discussions unit toggle.
 */
import { cleanup, waitFor } from "@octanejs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderWithQueryClient } from "@/test/render-with-query";
import { RepoDiscussionsPage } from "./$owner.$repo.discussions.$";

describe("/$owner/$repo/discussions/$", () => {
  it("exports RepoDiscussionsPage", () => {
    expect(typeof RepoDiscussionsPage).toBe("function");
  });

  it("uses AppLink for internal navigation (client-side routing)", async () => {
    const src = await import("./$owner.$repo.discussions.$.tsrx?raw").then((m) =>
      String((m as { default: string }).default),
    );
    expect(src).toMatch(/AppLink/);
    expect(src).not.toMatch(/<a\s+href=\{?["'`]\//);
  });

  it("drives thread/comment CRUD through the discussions.* RPC surface", async () => {
    const src = await import("./$owner.$repo.discussions.$.tsrx?raw").then((m) =>
      String((m as { default: string }).default),
    );
    expect(src).toMatch(/discussions\.list/);
    expect(src).toMatch(/discussions\.get/);
    expect(src).toMatch(/discussions\.create/);
    expect(src).toMatch(/discussions\.pin/);
    expect(src).toMatch(/discussions\.lock/);
    expect(src).toMatch(/discussions\.delete/);
    expect(src).toMatch(/discussions\.categories\.list/);
    expect(src).toMatch(/discussions\.comments\.list/);
    expect(src).toMatch(/discussions\.comments\.create/);
    expect(src).toMatch(/discussions\.comments\.delete/);
    expect(src).toMatch(/discussions\.answer\.set/);
  });
});

const repoGetMock = vi.fn();
const discussionsListMock = vi.fn();
const discussionsGetMock = vi.fn();
const categoriesListMock = vi.fn();
const commentsListMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    repo: {
      get: (...args: unknown[]) => repoGetMock(...args),
    },
    discussions: {
      list: (...args: unknown[]) => discussionsListMock(...args),
      get: (...args: unknown[]) => discussionsGetMock(...args),
      create: vi.fn(),
      update: vi.fn(),
      delete: vi.fn(),
      pin: vi.fn(),
      lock: vi.fn(),
      transfer: vi.fn(),
      labels: { set: vi.fn() },
      answer: { set: vi.fn(), clear: vi.fn() },
      categories: {
        list: (...args: unknown[]) => categoriesListMock(...args),
        create: vi.fn(),
        update: vi.fn(),
        delete: vi.fn(),
      },
      comments: {
        list: (...args: unknown[]) => commentsListMock(...args),
        create: vi.fn(),
        update: vi.fn(),
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
  discussions_enabled: true,
};

const qaCategory = {
  id: "cat1",
  repo_id: "r1",
  name: "Q&A",
  slug: "q-a",
  emoji: "",
  description: "",
  is_answerable: true,
  position: 0,
  created_at: "2026-10-02T00:00:00Z",
  updated_at: "2026-10-02T00:00:00Z",
};

const thread = {
  id: "d1",
  repo_id: "r1",
  number: 1,
  title: "How do I X?",
  body: "Stuck on step 2.",
  category: qaCategory,
  author_id: "u1",
  author_username: "ada",
  locked: false,
  pinned: false,
  label_ids: [],
  html_url: "/ada/hello/discussions/1",
  created_at: "2026-10-02T00:00:00Z",
  updated_at: "2026-10-02T00:00:00Z",
};

beforeEach(() => {
  window.history.pushState({}, "", "/ada/hello/discussions");
  repoGetMock.mockReset();
  discussionsListMock.mockReset();
  discussionsGetMock.mockReset();
  categoriesListMock.mockReset();
  commentsListMock.mockReset();
});

afterEach(() => {
  cleanup();
});

describe("RepoDiscussionsPage (unit toggle)", () => {
  it("renders the unit-disabled state when discussions are off", async () => {
    repoGetMock.mockResolvedValue({
      ok: true,
      data: { ...baseRepo, discussions_enabled: false },
    });
    renderWithQueryClient(RepoDiscussionsPage);
    await waitFor(() => {
      expect(document.body.textContent).toContain("Discussions are disabled");
    });
    expect(discussionsListMock).not.toHaveBeenCalled();
  });

  it("renders the empty CTA when no discussions exist", async () => {
    repoGetMock.mockResolvedValue({ ok: true, data: baseRepo });
    categoriesListMock.mockResolvedValue({ ok: true, data: { categories: [] } });
    discussionsListMock.mockResolvedValue({ ok: true, data: { discussions: [] } });
    renderWithQueryClient(RepoDiscussionsPage);
    await waitFor(() => {
      expect(document.body.textContent).toContain("No discussions yet");
    });
  });

  it("lists threads on the index view", async () => {
    repoGetMock.mockResolvedValue({ ok: true, data: baseRepo });
    categoriesListMock.mockResolvedValue({ ok: true, data: { categories: [qaCategory] } });
    discussionsListMock.mockResolvedValue({ ok: true, data: { discussions: [thread] } });
    renderWithQueryClient(RepoDiscussionsPage);
    await waitFor(() => {
      expect(document.body.textContent).toContain("How do I X?");
    });
    expect(document.body.textContent).toContain("Q&A");
  });

  it("filters by ?category=<slug>", async () => {
    window.history.pushState({}, "", "/ada/hello/discussions?category=q-a");
    repoGetMock.mockResolvedValue({ ok: true, data: baseRepo });
    categoriesListMock.mockResolvedValue({ ok: true, data: { categories: [qaCategory] } });
    discussionsListMock.mockResolvedValue({ ok: true, data: { discussions: [thread] } });
    renderWithQueryClient(RepoDiscussionsPage);
    await waitFor(() => {
      expect(discussionsListMock).toHaveBeenCalledWith(
        expect.objectContaining({ category: "q-a" }),
      );
    });
  });

  it("renders the thread view for /discussions/{n}", async () => {
    window.history.pushState({}, "", "/ada/hello/discussions/1");
    repoGetMock.mockResolvedValue({ ok: true, data: baseRepo });
    categoriesListMock.mockResolvedValue({ ok: true, data: { categories: [qaCategory] } });
    discussionsGetMock.mockResolvedValue({ ok: true, data: thread });
    commentsListMock.mockResolvedValue({
      ok: true,
      data: {
        comments: [
          {
            id: "c1",
            discussion_id: "d1",
            author_id: "u2",
            author_username: "bob",
            body: "Try turning it off and on.",
            html_url: "/ada/hello/discussions/1#comment-c1",
            created_at: "2026-10-02T00:00:00Z",
            updated_at: "2026-10-02T00:00:00Z",
          },
        ],
      },
    });
    renderWithQueryClient(RepoDiscussionsPage);
    await waitFor(() => {
      expect(document.body.textContent).toContain("Stuck on step 2.");
    });
    expect(document.body.textContent).toContain("Try turning it off and on.");
  });
});
