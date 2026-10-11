/**
 * GIT-19 web file-editing routes — happy-dom mount coverage (route-coverage
 * manifest). Each page renders its affordance + the shared commit form.
 *
 * Ported to the Astro/query model: params come from `usePathname()` +
 * `matchPath` (set via `history.pushState`), and loader data is driven by
 * mocking the `@/lib/ssr-repo` fetch helpers.
 */
import { cleanup, screen, waitFor } from "@octanejs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderWithQueryClient } from "@/test/render-with-query";

const commitPolicyMock = vi.fn();
const fileCreateMock = vi.fn();
const fileUpdateMock = vi.fn();
const fileDeleteMock = vi.fn();
const fileRenameMock = vi.fn();
const fileUploadMock = vi.fn();
const fileMkdirMock = vi.fn();
const authMeMock = vi.fn();
const bootstrapStatusMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    auth: {
      me: (...args: unknown[]) => authMeMock(...args),
      bootstrapStatus: (...args: unknown[]) => bootstrapStatusMock(...args),
    },
    repo: {
      file: {
        commitPolicy: (...args: unknown[]) => commitPolicyMock(...args),
        create: (...args: unknown[]) => fileCreateMock(...args),
        update: (...args: unknown[]) => fileUpdateMock(...args),
        delete: (...args: unknown[]) => fileDeleteMock(...args),
        rename: (...args: unknown[]) => fileRenameMock(...args),
        upload: (...args: unknown[]) => fileUploadMock(...args),
        mkdir: (...args: unknown[]) => fileMkdirMock(...args),
      },
    },
  },
}));

const repoGetMock = vi.fn();
const repoRefsMock = vi.fn();
const repoBlobMock = vi.fn();
const repoTreeMock = vi.fn();

vi.mock("@/lib/ssr-repo", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/lib/ssr-repo")>();
  return {
    ...actual,
    fetchRepoGet: (...args: unknown[]) => repoGetMock(...args),
    fetchRepoRefs: (...args: unknown[]) => repoRefsMock(...args),
    fetchRepoBlob: (...args: unknown[]) => repoBlobMock(...args),
    fetchRepoTree: (...args: unknown[]) => repoTreeMock(...args),
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

const POLICY = {
  branch: "main",
  direct_commit_allowed: true,
  requires_pr: false,
};

const REFS = { refs: [{ name: "refs/heads/main" }] };

function setPathname(path: string) {
  window.history.pushState({}, "", path);
}

function mockBlob(overrides: Record<string, unknown> = {}) {
  repoBlobMock.mockResolvedValue({
    ok: true,
    data: {
      ref: "refs/heads/main",
      path: "src/main.rs",
      content: "fn main() {}\n",
      is_binary: false,
      truncated: false,
      encoding: "utf-8",
      size: 12,
      ...overrides,
    },
  });
}

beforeEach(() => {
  for (const m of [
    commitPolicyMock,
    fileCreateMock,
    fileUpdateMock,
    fileDeleteMock,
    fileRenameMock,
    fileUploadMock,
    fileMkdirMock,
    authMeMock,
    bootstrapStatusMock,
    repoGetMock,
    repoRefsMock,
    repoBlobMock,
    repoTreeMock,
  ]) {
    m.mockReset();
  }
  commitPolicyMock.mockResolvedValue({ ok: true, data: POLICY });
  authMeMock.mockResolvedValue({ ok: true, data: USER });
  bootstrapStatusMock.mockResolvedValue({ ok: true, data: { needs_setup: false } });
  repoGetMock.mockResolvedValue({ ok: true, data: REPO });
  repoRefsMock.mockResolvedValue({ ok: true, data: REFS });
  repoBlobMock.mockResolvedValue({ ok: false, error: { code: "git.not_found", message: "nf" } });
  repoTreeMock.mockResolvedValue({ ok: false, error: { code: "git.not_found", message: "nf" } });
  setPathname("/");
});

afterEach(cleanup);

describe("GIT-19 web file-editing routes", () => {
  it("edit route mounts editor + commit form", async () => {
    mockBlob();
    setPathname("/ada/hello/edit/main/src/main.rs");
    const { RepoFileEditPage } = await import("./$owner.$repo.edit.$.tsrx");
    renderWithQueryClient(RepoFileEditPage);
    await waitFor(() => expect(screen.getByTestId("file-edit-content")).toBeTruthy());
    expect(screen.getByTestId("file-edit-path")).toBeTruthy();
    await waitFor(() => expect(screen.getByTestId("file-commit-form")).toBeTruthy());
  }, 30_000);

  it("edit route refuses binary blobs", async () => {
    mockBlob({ path: "img/logo.png", content: "", is_binary: true });
    setPathname("/ada/hello/edit/main/img/logo.png");
    const { RepoFileEditPage } = await import("./$owner.$repo.edit.$.tsrx");
    renderWithQueryClient(RepoFileEditPage);
    await waitFor(() => expect(screen.getByText(/can't be edited in the browser/)).toBeTruthy());
  }, 30_000);

  it("new route mounts filename input + commit form", async () => {
    setPathname("/ada/hello/new/main/src");
    const { RepoFileNewPage } = await import("./$owner.$repo.new.$.tsrx");
    renderWithQueryClient(RepoFileNewPage);
    await waitFor(() => expect(screen.getByTestId("file-new-name")).toBeTruthy());
    expect(screen.getByTestId("file-new-content")).toBeTruthy();
    await waitFor(() => expect(screen.getByTestId("file-commit-form")).toBeTruthy());
  }, 30_000);

  it("mkdir route mounts dirname input + gitkeep note", async () => {
    setPathname("/ada/hello/mkdir/main/src");
    const { RepoFileMkdirPage } = await import("./$owner.$repo.mkdir.$.tsrx");
    renderWithQueryClient(RepoFileMkdirPage);
    await waitFor(() => expect(screen.getByTestId("file-mkdir-name")).toBeTruthy());
    expect(screen.getByText(/\.gitkeep/)).toBeTruthy();
    await waitFor(() => expect(screen.getByTestId("file-commit-form")).toBeTruthy());
  }, 30_000);

  it("delete route mounts warning + commit form", async () => {
    mockBlob();
    setPathname("/ada/hello/delete/main/src/main.rs");
    const { RepoFileDeletePage } = await import("./$owner.$repo.delete.$.tsrx");
    renderWithQueryClient(RepoFileDeletePage);
    await waitFor(() => expect(screen.getByText(/Delete src\/main\.rs\?/)).toBeTruthy());
    await waitFor(() => expect(screen.getByTestId("file-commit-form")).toBeTruthy());
  }, 30_000);

  it("upload route mounts dropzone + commit form", async () => {
    setPathname("/ada/hello/upload/main/src");
    const { RepoFileUploadPage } = await import("./$owner.$repo.upload.$.tsrx");
    renderWithQueryClient(RepoFileUploadPage);
    await waitFor(() =>
      expect(screen.getByText(/Drop files here, or click to browse/)).toBeTruthy(),
    );
    await waitFor(() => expect(screen.getByTestId("file-commit-form")).toBeTruthy());
  }, 30_000);

  it("routes deny non-writers", async () => {
    repoGetMock.mockResolvedValue({ ok: true, data: { ...REPO, can_write: false } });
    setPathname("/ada/hello/new/main/src");
    const { RepoFileNewPage } = await import("./$owner.$repo.new.$.tsrx");
    renderWithQueryClient(RepoFileNewPage);
    await waitFor(() => expect(screen.getByText(/You need write access/)).toBeTruthy());
  }, 30_000);
});
