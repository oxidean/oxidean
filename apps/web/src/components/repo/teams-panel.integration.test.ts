import { cleanup, fireEvent, screen, waitFor } from "@octanejs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { TeamsPanel } from "@/components/repo/teams-panel";
import { renderWithQueryClient } from "@/test/render-with-query";

const listMock = vi.fn();
const addMock = vi.fn();
const updateMock = vi.fn();
const removeMock = vi.fn();
const orgTeamsMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    repo: {
      teams: {
        list: (...args: unknown[]) => listMock(...args),
        add: (...args: unknown[]) => addMock(...args),
        update: (...args: unknown[]) => updateMock(...args),
        remove: (...args: unknown[]) => removeMock(...args),
      },
    },
    org: {
      teams: {
        list: (...args: unknown[]) => orgTeamsMock(...args),
      },
    },
  },
}));

const GRANTED = {
  team_id: "t1",
  slug: "core",
  name: "Core",
  permission: "read" as const,
  created_at: "2026-01-01T00:00:00Z",
};

const ORG_TEAM = (id: string, slug: string, name: string) => ({
  id,
  org_id: "o1",
  slug,
  name,
  description: "",
  member_count: 1,
  repo_count: 0,
  created_at: "x",
  updated_at: "x",
});

describe("repo settings TeamsPanel (COL-05)", () => {
  afterEach(() => {
    cleanup();
  });

  beforeEach(() => {
    for (const m of [listMock, addMock, updateMock, removeMock, orgTeamsMock]) {
      m.mockReset();
    }
    listMock.mockResolvedValue({ ok: true, data: { teams: [GRANTED] } });
    orgTeamsMock.mockResolvedValue({
      ok: true,
      data: { teams: [ORG_TEAM("t1", "core", "Core"), ORG_TEAM("t2", "ops", "Ops")] },
    });
    addMock.mockResolvedValue({ ok: true, data: {} });
    updateMock.mockResolvedValue({ ok: true, data: {} });
    removeMock.mockResolvedValue({ ok: true, data: { ok: true } });
  });

  it("renders nothing on user-owned repos", async () => {
    renderWithQueryClient(TeamsPanel, {
      props: { owner: "ada", name: "hello", orgOwned: false },
    });
    expect(screen.queryByTestId("teams-panel")).toBeNull();
    expect(listMock).not.toHaveBeenCalled();
  });

  it("lists granted teams; add flow only offers ungranted teams", async () => {
    renderWithQueryClient(TeamsPanel, {
      props: { owner: "acme", name: "api", orgOwned: true },
    });

    await waitFor(() => {
      expect(screen.getByTestId("teams-panel")).toBeTruthy();
      expect(screen.getByTestId("repo-team-core")).toBeTruthy();
      expect(screen.getByTestId("team-add-open")).toBeTruthy();
    });

    fireEvent.click(screen.getByTestId("team-add-open"));
    await waitFor(() => {
      expect(screen.getByTestId("team-add-form")).toBeTruthy();
    });

    // `core` is already granted — the only ungranted org team is `ops`, so
    // the default selection resolves to it on submit.
    fireEvent.click(screen.getByTestId("team-add-submit"));
    await waitFor(() => {
      expect(addMock).toHaveBeenCalledWith({
        owner: "acme",
        name: "api",
        team_slug: "ops",
        permission: "read",
      });
    });
  });

  it("remove flow hits repo.teams.remove with the team slug", async () => {
    renderWithQueryClient(TeamsPanel, {
      props: { owner: "acme", name: "api", orgOwned: true },
    });

    await waitFor(() => {
      expect(screen.getByTestId("repo-team-core")).toBeTruthy();
    });
    const row = screen.getByTestId("repo-team-core");
    const removeBtn = Array.from(row.querySelectorAll("button")).find(
      (b) => b.textContent === "Remove",
    );
    expect(removeBtn).toBeTruthy();
    fireEvent.click(removeBtn as Element);
    await waitFor(() => {
      expect(screen.getByTestId("team-remove-confirm")).toBeTruthy();
    });
    fireEvent.click(screen.getByTestId("team-remove-confirm"));
    await waitFor(() => {
      expect(removeMock).toHaveBeenCalledWith({
        owner: "acme",
        name: "api",
        team_slug: "core",
      });
    });
  });

  it("surfaces API errors from the list call", async () => {
    listMock.mockResolvedValue({
      ok: false,
      error: { code: "repo.not_found", message: "Repository not found" },
    });
    renderWithQueryClient(TeamsPanel, {
      props: { owner: "acme", name: "api", orgOwned: true },
    });
    await waitFor(() => {
      expect(screen.getByRole("alert")).toBeTruthy();
    });
  });
});
