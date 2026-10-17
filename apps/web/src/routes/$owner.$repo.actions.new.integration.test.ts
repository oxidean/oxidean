/**
 * /$owner/$repo/actions/new — workflow template catalog page (happy-dom).
 * Mocks repo.get + workflowTemplates.list and asserts section rendering,
 * search filtering, Configure link encoding, and the writer gate.
 */
import { cleanup, screen, waitFor } from "@octanejs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderWithQueryClient } from "@/test/render-with-query";

const wfListMock = vi.fn();
const authMeMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    auth: {
      me: (...args: unknown[]) => authMeMock(...args),
    },
    repo: {
      actions: {
        workflowTemplates: {
          list: (...args: unknown[]) => wfListMock(...args),
          get: vi.fn(),
        },
      },
    },
  },
}));

const repoGetMock = vi.fn();

vi.mock("@/lib/ssr-repo", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/lib/ssr-repo")>();
  return {
    ...actual,
    fetchRepoGet: (...args: unknown[]) => repoGetMock(...args),
  };
});

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

const RUST_TEMPLATE = {
  id: "builtin:ci/rust",
  name: "Rust",
  description: "Build and test a Rust project with Cargo.",
  icon_name: "rust",
  icon_url: "/api/actions/template-icons/rust.svg",
  categories: ["Rust", "Continuous integration"],
  file_patterns: ["Cargo.toml"],
  filename: "rust.yml",
  provenance: "builtin",
  source_label: null,
  suggested: true,
  unsupported_actions: [],
  parse_error: null,
};

const DEPLOY_TEMPLATE = {
  id: "builtin:deployments/aws",
  name: "Deploy to AWS",
  description: "Ship the app to AWS.",
  icon_name: null,
  icon_url: null,
  categories: ["Deployment"],
  file_patterns: [],
  filename: "aws.yml",
  provenance: "builtin",
  source_label: null,
  suggested: false,
  unsupported_actions: ["aws-actions/configure-aws-credentials"],
  parse_error: null,
};

const CATALOG = {
  suggested: [RUST_TEMPLATE],
  templates: [RUST_TEMPLATE, DEPLOY_TEMPLATE],
  categories: ["Deployment", "Continuous integration"],
  total_count: 2,
};

function setPathname(path: string) {
  window.history.pushState({}, "", path);
}

beforeEach(() => {
  wfListMock.mockReset();
  authMeMock.mockReset();
  repoGetMock.mockReset();
  authMeMock.mockResolvedValue({ ok: true, data: USER });
  repoGetMock.mockResolvedValue({ ok: true, data: REPO });
  wfListMock.mockResolvedValue({ ok: true, data: CATALOG });
  setPathname("/");
});

afterEach(cleanup);

describe("/$owner/$repo/actions/new", () => {
  it("renders suggested + headline category sections", async () => {
    setPathname("/ada/hello/actions/new");
    const { RepoActionsNewPage } = await import("./$owner.$repo.actions.new.tsrx");
    renderWithQueryClient(RepoActionsNewPage);

    await waitFor(() => expect(screen.getByText("Suggested for this repository")).toBeTruthy());
    expect(screen.getByText("Choose a workflow")).toBeTruthy();
    expect(screen.getByText("Continuous integration")).toBeTruthy();
    expect(screen.getByText("Deployment")).toBeTruthy();
    // Suggested templates render in the Suggested section AND their category
    // section — GitHub parity.
    expect(screen.getAllByTestId("workflow-template-card-builtin:ci/rust").length).toBe(2);
  }, 30_000);

  it("Configure links carry the encoded template id", async () => {
    setPathname("/ada/hello/actions/new");
    const { RepoActionsNewPage } = await import("./$owner.$repo.actions.new.tsrx");
    renderWithQueryClient(RepoActionsNewPage);

    await waitFor(() =>
      expect(
        screen.getAllByTestId("workflow-template-configure-builtin:ci/rust").length,
      ).toBeGreaterThan(0),
    );
    const link = screen
      .getAllByTestId("workflow-template-configure-builtin:ci/rust")[0]
      .closest("a");
    expect(link?.getAttribute("href")).toContain(
      "/ada/hello/new/main/.github/workflows?template=builtin%3Aci%2Frust",
    );
    const blank = screen.getByTestId("workflow-template-blank").closest("a");
    expect(blank?.getAttribute("href")).toContain(
      "/ada/hello/new/main/.github/workflows?filename=main.yml",
    );
  }, 30_000);

  it("client-side search filters cards", async () => {
    setPathname("/ada/hello/actions/new");
    const { RepoActionsNewPage } = await import("./$owner.$repo.actions.new.tsrx");
    renderWithQueryClient(RepoActionsNewPage);

    const search = (await waitFor(() =>
      screen.getByTestId("workflow-template-search"),
    )) as HTMLInputElement;

    await waitFor(() =>
      expect(
        screen.getAllByTestId("workflow-template-card-builtin:ci/rust").length,
      ).toBeGreaterThan(0),
    );
    search.value = "aws";
    search.dispatchEvent(new Event("input", { bubbles: true }));

    await waitFor(() => {
      expect(screen.queryAllByTestId("workflow-template-card-builtin:ci/rust").length).toBe(0);
      expect(screen.getByTestId("workflow-template-card-builtin:deployments/aws")).toBeTruthy();
    });
  }, 30_000);

  it("readers see the write-access gate instead of the catalog", async () => {
    repoGetMock.mockResolvedValue({
      ok: true,
      data: { ...REPO, can_write: false },
    });
    setPathname("/ada/hello/actions/new");
    const { RepoActionsNewPage } = await import("./$owner.$repo.actions.new.tsrx");
    renderWithQueryClient(RepoActionsNewPage);

    await waitFor(() => expect(screen.getByText(/You need write access/)).toBeTruthy());
    expect(screen.queryByTestId("workflow-template-catalog")).toBeNull();
  }, 30_000);
});
