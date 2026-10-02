/**
 * GIT-19 web file-editing routes — happy-dom mount coverage (route-coverage
 * manifest). Each page renders its affordance + the shared commit form.
 */
import { cleanup, screen, waitFor } from "@octanejs/testing-library";
import { createElement } from "octane";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderWithQueryClient } from "@/test/render-with-query";

const commitPolicyMock = vi.fn();
const fileCreateMock = vi.fn();
const fileUpdateMock = vi.fn();
const fileDeleteMock = vi.fn();
const fileRenameMock = vi.fn();
const fileUploadMock = vi.fn();
const fileMkdirMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
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

let currentParams: Record<string, string> = {};
let currentLoaderData: unknown = undefined;

vi.mock("@octanejs/tanstack-router", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@octanejs/tanstack-router")>();
  function MockLink(props: {
    to?: string;
    href?: string;
    children?: unknown;
    className?: string;
    preload?: string;
  }) {
    return createElement(
      "a",
      {
        href: (props.href ?? props.to ?? "#") as string,
        className: props.className,
      } as never,
      props.children as never,
    );
  }
  return {
    ...actual,
    useParams: () => currentParams,
    useLoaderData: () => currentLoaderData,
    useNavigate: () => vi.fn(),
    Link: MockLink,
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

const SPLAT_PARAMS = { owner: "ada", repo: "hello", _splat: "main/src" };

beforeEach(() => {
  commitPolicyMock.mockReset();
  commitPolicyMock.mockResolvedValue({ ok: true, data: POLICY });
  currentParams = { ...SPLAT_PARAMS };
  currentLoaderData = undefined;
});

afterEach(cleanup);

describe("GIT-19 web file-editing routes", () => {
  it("edit route mounts editor + commit form", async () => {
    currentLoaderData = {
      kind: "ready",
      repo: REPO,
      refName: "main",
      path: "src/main.rs",
      content: "fn main() {}\n",
      editable: true,
    };
    const { RepoFileEditPage } = await import("./$owner.$repo.edit.$.tsrx");
    renderWithQueryClient(RepoFileEditPage);
    await waitFor(() => expect(screen.getByTestId("file-edit-content")).toBeTruthy());
    expect(screen.getByTestId("file-edit-path")).toBeTruthy();
    await waitFor(() => expect(screen.getByTestId("file-commit-form")).toBeTruthy());
  }, 30_000);

  it("edit route refuses binary blobs", async () => {
    currentLoaderData = {
      kind: "ready",
      repo: REPO,
      refName: "main",
      path: "img/logo.png",
      content: "",
      editable: false,
    };
    const { RepoFileEditPage } = await import("./$owner.$repo.edit.$.tsrx");
    renderWithQueryClient(RepoFileEditPage);
    await waitFor(() => expect(screen.getByText(/can't be edited in the browser/)).toBeTruthy());
  }, 30_000);

  it("new route mounts filename input + commit form", async () => {
    currentLoaderData = { kind: "ready", repo: REPO, refName: "main", dir: "src" };
    const { RepoFileNewPage } = await import("./$owner.$repo.new.$.tsrx");
    renderWithQueryClient(RepoFileNewPage);
    await waitFor(() => expect(screen.getByTestId("file-new-name")).toBeTruthy());
    expect(screen.getByTestId("file-new-content")).toBeTruthy();
    await waitFor(() => expect(screen.getByTestId("file-commit-form")).toBeTruthy());
  }, 30_000);

  it("mkdir route mounts dirname input + gitkeep note", async () => {
    currentLoaderData = { kind: "ready", repo: REPO, refName: "main", dir: "src" };
    const { RepoFileMkdirPage } = await import("./$owner.$repo.mkdir.$.tsrx");
    renderWithQueryClient(RepoFileMkdirPage);
    await waitFor(() => expect(screen.getByTestId("file-mkdir-name")).toBeTruthy());
    expect(screen.getByText(/\.gitkeep/)).toBeTruthy();
    await waitFor(() => expect(screen.getByTestId("file-commit-form")).toBeTruthy());
  }, 30_000);

  it("delete route mounts warning + commit form", async () => {
    currentLoaderData = {
      kind: "ready",
      repo: REPO,
      refName: "main",
      path: "src/main.rs",
      isDir: false,
      sizeLabel: "12 B",
    };
    const { RepoFileDeletePage } = await import("./$owner.$repo.delete.$.tsrx");
    renderWithQueryClient(RepoFileDeletePage);
    await waitFor(() => expect(screen.getByText(/Delete src\/main\.rs\?/)).toBeTruthy());
    await waitFor(() => expect(screen.getByTestId("file-commit-form")).toBeTruthy());
  }, 30_000);

  it("upload route mounts dropzone + commit form", async () => {
    currentLoaderData = { kind: "ready", repo: REPO, refName: "main", dir: "src" };
    const { RepoFileUploadPage } = await import("./$owner.$repo.upload.$.tsrx");
    renderWithQueryClient(RepoFileUploadPage);
    await waitFor(() =>
      expect(screen.getByText(/Drop files here, or click to browse/)).toBeTruthy(),
    );
    await waitFor(() => expect(screen.getByTestId("file-commit-form")).toBeTruthy());
  }, 30_000);

  it("routes deny non-writers", async () => {
    currentLoaderData = {
      kind: "ready",
      repo: { ...REPO, can_write: false },
      refName: "main",
      dir: "src",
    };
    const { RepoFileNewPage } = await import("./$owner.$repo.new.$.tsrx");
    renderWithQueryClient(RepoFileNewPage);
    await waitFor(() => expect(screen.getByText(/You need write access/)).toBeTruthy());
  }, 30_000);
});
