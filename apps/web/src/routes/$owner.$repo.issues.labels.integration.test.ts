import { createElement } from "octane";
import { cleanup, waitFor } from "@octanejs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderWithQueryClient } from "@/test/render-with-query";
import type { RepoLayoutLoaderData } from "@/lib/repo-store";

const repoGetMock = vi.fn();
const labelListMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    repo: {
      get: (...args: unknown[]) => repoGetMock(...args),
    },
    label: {
      listForRepo: (...args: unknown[]) => labelListMock(...args),
      create: vi.fn(),
      update: vi.fn(),
      delete: vi.fn(),
    },
  },
}));

const adminRepo = {
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
  issues_enabled: false,
  pulls_enabled: false,
};

const layoutData: RepoLayoutLoaderData = {
  owner: "ada",
  repoName: "hello",
  status: "ok",
  repo: adminRepo,
  me: null,
  message: "",
  publicOrigin: "http://127.0.0.1:8080",
  sshHost: "127.0.0.1",
  sshPort: 2222,
};

const disabledRouteLoader = { kind: "disabled" as const, repo: adminRepo };

vi.mock("@octanejs/tanstack-router", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@octanejs/tanstack-router")>();
  return {
    ...actual,
    useParams: () => ({ owner: "ada", repo: "hello" }),
    useLoaderData: (opts: { from?: string }) =>
      opts?.from === "/$owner/$repo" ? layoutData : disabledRouteLoader,
    useNavigate: () => vi.fn(),
    Link: (props: { to?: string; href?: string; children?: unknown; className?: string }) =>
      createElement(
        "a",
        {
          href: props.to ?? props.href ?? "#",
          className: props.className,
        },
        props.children,
      ),
  };
});

const { RepoIssueLabelsPage } = await import("./$owner.$repo.issues.labels.tsrx");

beforeEach(() => {
  repoGetMock.mockReset();
  labelListMock.mockReset();
  repoGetMock.mockResolvedValue({ ok: true, data: adminRepo });
  labelListMock.mockResolvedValue({ ok: true, data: { labels: [] } });
});

afterEach(() => {
  cleanup();
});

describe("RepoIssueLabelsPage (COL-13 unit toggles)", () => {
  it("renders the unit-disabled state when issues and pulls are off", async () => {
    renderWithQueryClient(RepoIssueLabelsPage);
    await waitFor(() => {
      expect(document.body.textContent).toMatch(/disabled|disabled/i);
    });
    // The label form must not render while the unit surface is off.
    expect(document.querySelector("#repo-label-name")).toBeNull();
  });
});
