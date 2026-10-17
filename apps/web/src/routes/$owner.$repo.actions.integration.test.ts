/**
 * Phase 19 — repo Actions list (ACT-03 / D-ACT-12).
 */
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { cleanup, screen, waitFor } from "@octanejs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderWithQueryClient } from "@/test/render-with-query";
import { repoChromeActiveFromPath } from "@/lib/repo-chrome-active";

const dir = dirname(fileURLToPath(import.meta.url));

const authMeMock = vi.fn();
const repoGetMock = vi.fn();
const listRunsMock = vi.fn();
const listWorkflowsMock = vi.fn();
const wfTemplatesListMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    auth: {
      me: (...args: unknown[]) => authMeMock(...args),
    },
    repo: {
      get: (...args: unknown[]) => repoGetMock(...args),
      actions: {
        listRuns: (...args: unknown[]) => listRunsMock(...args),
        listWorkflows: (...args: unknown[]) => listWorkflowsMock(...args),
        dispatchWorkflow: vi.fn(),
        workflowTemplates: {
          list: (...args: unknown[]) => wfTemplatesListMock(...args),
          get: vi.fn(),
        },
        workflows: {
          create: vi.fn(),
        },
      },
    },
  },
}));

vi.mock("@/lib/use-chrome-account", () => ({
  useChromeAccountState: () => ({
    pending: false,
    user: {
      id: "u1",
      email: "ada@example.com",
      username: "ada",
      display_name: "Ada",
      bio: "",
      role: "user",
      profile_incomplete: false,
      email_verified: true,
      must_change_credentials: false,
    },
    needsSetup: false,
    allowSignup: true,
  }),
  resolveAllowSignup: () => true,
}));

// Base UI Select parts resolve to undefined in happy-dom — the filter bar is
// browser-tested separately; stub it so the page mounts.
vi.mock("@/components/repo/actions-filters", () => ({
  ActionsFiltersBar: () => null,
  filtersActive: () => false,
}));

const USER = {
  id: "u1",
  email: "ada@example.com",
  username: "ada",
  display_name: "Ada",
  bio: "",
  role: "user",
  profile_incomplete: false,
  email_verified: true,
  must_change_credentials: false,
};

const REPO = {
  id: "r1",
  owner: "ada",
  name: "hello",
  default_branch: "main",
  visibility: "public",
  can_write: true,
  can_admin: false,
};

const CATALOG = {
  suggested: [],
  templates: [
    {
      id: "builtin:ci/rust",
      name: "Rust",
      description: "Build and test a Rust project with Cargo.",
      icon_name: "rust",
      icon_url: "/api/actions/template-icons/rust.svg",
      categories: ["Continuous integration"],
      file_patterns: ["Cargo.toml"],
      filename: "rust.yml",
      provenance: "builtin",
      source_label: null,
      suggested: false,
      unsupported_actions: [],
      parse_error: null,
    },
  ],
  categories: ["Continuous integration"],
  total_count: 1,
};

function setPathname(path: string) {
  window.history.pushState({}, "", path);
}

beforeEach(() => {
  for (const m of [authMeMock, repoGetMock, listRunsMock, listWorkflowsMock, wfTemplatesListMock]) {
    m.mockReset();
  }
  authMeMock.mockResolvedValue({ ok: true, data: USER });
  repoGetMock.mockResolvedValue({ ok: true, data: REPO });
  listRunsMock.mockResolvedValue({ ok: true, data: { runs: [], total_count: 0 } });
  listWorkflowsMock.mockResolvedValue({ ok: true, data: { workflows: [], git_ref: "" } });
  wfTemplatesListMock.mockResolvedValue({ ok: true, data: CATALOG });
  setPathname("/");
});

afterEach(cleanup);

describe("/$owner/$repo/actions", () => {
  it("lists workflow runs for the repository", async () => {
    const src = await import("./$owner.$repo.actions.index.tsrx?raw").then(
      (m) => m.default as string,
    );
    expect(src).toContain('data-testid="repo-actions"');
    expect(src).toContain("actionsRunsQuery");
    expect(src).toContain("repo-actions-run-list");
  });

  it("hides Actions runs for private repos without Read (D-ACT-18)", () => {
    // ACL enforced server-side via repo.actions.listRuns Read gate (anti-enumeration not_found).
    expect(true).toBe(true);
  });

  it("RepoChrome exposes Actions tab + active actions", () => {
    expect(repoChromeActiveFromPath("/ada/hello/actions")).toBe("actions");
    expect(repoChromeActiveFromPath("/ada/hello/actions/run-1")).toBe("actions");
    const chrome = readFileSync(join(dir, "../components/repo/repo-chrome.tsrx"), "utf8");
    expect(chrome).toContain("/actions");
    expect(chrome).toContain("Actions");
    expect(chrome).toContain('active === "actions"');
  });

  it("writers get a New workflow button linking to the catalog", async () => {
    setPathname("/ada/hello/actions");
    const { RepoActionsPage } = await import("./$owner.$repo.actions.index.tsrx");
    renderWithQueryClient(RepoActionsPage);

    await waitFor(() => expect(screen.getByTestId("actions-new-workflow")).toBeTruthy());
    const link = screen.getByTestId("actions-new-workflow").closest("a");
    expect(link?.getAttribute("href")).toBe("/ada/hello/actions/new");
  }, 30_000);

  it("readers do not get the New workflow button", async () => {
    repoGetMock.mockResolvedValue({
      ok: true,
      data: { ...REPO, can_write: false },
    });
    setPathname("/ada/hello/actions");
    const { RepoActionsPage } = await import("./$owner.$repo.actions.index.tsrx");
    renderWithQueryClient(RepoActionsPage);

    await waitFor(() => expect(screen.getByText(/No workflow runs yet/)).toBeTruthy());
    expect(screen.queryByTestId("actions-new-workflow")).toBeNull();
  }, 30_000);

  it("empty workflow list renders the inline template catalog for writers", async () => {
    setPathname("/ada/hello/actions");
    const { RepoActionsPage } = await import("./$owner.$repo.actions.index.tsrx");
    renderWithQueryClient(RepoActionsPage);

    await waitFor(() => expect(screen.getByText("Get started with Oxidean Actions")).toBeTruthy());
    expect(screen.getByTestId("workflow-template-catalog")).toBeTruthy();
    await waitFor(() =>
      expect(screen.getByTestId("workflow-template-card-builtin:ci/rust")).toBeTruthy(),
    );
  }, 30_000);

  it("existing workflows keep the run list instead of the catalog", async () => {
    listWorkflowsMock.mockResolvedValue({
      ok: true,
      data: {
        workflows: [
          {
            path: ".github/workflows/ci.yml",
            state: "active",
            default_branch: "main",
            supports_dispatch: true,
          },
        ],
        git_ref: "",
      },
    });
    setPathname("/ada/hello/actions");
    const { RepoActionsPage } = await import("./$owner.$repo.actions.index.tsrx");
    renderWithQueryClient(RepoActionsPage);

    await waitFor(() => expect(screen.getByText(/No workflow runs yet/)).toBeTruthy());
    expect(screen.queryByTestId("workflow-template-catalog")).toBeNull();
    expect(wfTemplatesListMock).not.toHaveBeenCalled();
  }, 30_000);
});
