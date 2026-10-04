import { createElement } from "octane";
import { cleanup, fireEvent, screen, waitFor } from "@octanejs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderWithQueryClient } from "@/test/render-with-query";
import type { RepoLayoutLoaderData } from "@/lib/repo-store";

const collaboratorsListMock = vi.fn();
const lfsGetUsageMock = vi.fn();
const lfsListObjectsMock = vi.fn();
const setArchivedMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    repo: {
      collaborators: {
        list: (...args: unknown[]) => collaboratorsListMock(...args),
      },
      lfs: {
        getUsage: (...args: unknown[]) => lfsGetUsageMock(...args),
        listObjects: (...args: unknown[]) => lfsListObjectsMock(...args),
      },
      setArchived: (...args: unknown[]) => setArchivedMock(...args),
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
  updated_at: "2026-09-14T00:00:00Z",
  can_admin: true,
  can_write: true,
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

vi.mock("@octanejs/tanstack-router", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@octanejs/tanstack-router")>();
  return {
    ...actual,
    useParams: () => ({ owner: "ada", repo: "hello" }),
    useLoaderData: () => layoutData,
    useNavigate: () => vi.fn(),
    Link: (props: { to?: string; href?: string; children?: unknown; className?: string }) =>
      createElement(
        "a",
        {
          href: (props.href ?? props.to ?? "#") as string,
          className: props.className,
        } as never,
        props.children as never,
      ),
  };
});

import { RepoSettingsPage } from "./$owner.$repo.settings";

afterEach(cleanup);

describe("/$owner/$repo/settings render mount (G-11.1-15)", () => {
  beforeEach(() => {
    collaboratorsListMock.mockReset();
    lfsGetUsageMock.mockReset();
    lfsListObjectsMock.mockReset();
    collaboratorsListMock.mockResolvedValue({
      ok: true,
      data: { collaborators: [] },
    });
    lfsGetUsageMock.mockResolvedValue({
      ok: true,
      data: {
        enabled: false,
        object_count: 0,
        logical_bytes: 0,
        quota_repo_bytes: 0,
        objects: [],
      },
    });
    lfsListObjectsMock.mockResolvedValue({
      ok: true,
      data: { objects: [] },
    });
  });

  it("renders settings shell with Visibility section for can_admin repo", async () => {
    renderWithQueryClient(RepoSettingsPage);

    await waitFor(
      () => {
        expect(screen.getByRole("heading", { name: "Visibility" })).toBeTruthy();
        expect(screen.getByText("Choose who can see this repository.")).toBeTruthy();
        expect(screen.getByRole("button", { name: "Public" })).toBeTruthy();
        expect(screen.getByRole("button", { name: "Private" })).toBeTruthy();
        expect(screen.getByText("Collaborators")).toBeTruthy();
      },
      { timeout: 10_000 },
    );
  });

  it("shows the archive toggle in the danger zone for admins (GIT-20)", async () => {
    layoutData.repo = { ...adminRepo };
    renderWithQueryClient(RepoSettingsPage);

    await waitFor(
      () => {
        expect(screen.getByText("Danger zone")).toBeTruthy();
        expect(screen.getByRole("button", { name: "Archive repository" })).toBeTruthy();
      },
      { timeout: 10_000 },
    );
  });

  it("archives after typed confirm and calls repo.setArchived (GIT-20)", async () => {
    layoutData.repo = { ...adminRepo };
    setArchivedMock.mockReset();
    setArchivedMock.mockResolvedValue({
      ok: true,
      data: { ...adminRepo, archived: true },
    });
    renderWithQueryClient(RepoSettingsPage);

    fireEvent.click(await screen.findByRole("button", { name: "Archive repository" }));
    const dialog = await screen.findByTestId("repo-archive-confirm");
    const confirmInput = document.getElementById("archive-confirm");
    expect(confirmInput).toBeTruthy();
    expect(dialog).toBeTruthy();

    // Confirm button stays disabled until the typed name matches.
    fireEvent.input(confirmInput!, { target: { value: "hello" } });
    await waitFor(() => {
      const btn = dialog.querySelector("button:last-child") as HTMLButtonElement;
      expect(btn.disabled).toBe(false);
    });
    fireEvent.click(dialog.querySelector("button:last-child") as HTMLButtonElement);

    await waitFor(() => {
      expect(setArchivedMock).toHaveBeenCalledWith({
        owner: "ada",
        name: "hello",
        archived: true,
      });
    });
  });

  it("offers Unarchive for an archived repository (GIT-20)", async () => {
    layoutData.repo = { ...adminRepo, archived: true };
    renderWithQueryClient(RepoSettingsPage);

    await waitFor(
      () => {
        expect(
          screen.getAllByRole("button", { name: "Unarchive repository" }).length,
        ).toBeGreaterThan(0);
      },
      { timeout: 10_000 },
    );
    layoutData.repo = { ...adminRepo };
  });
});
