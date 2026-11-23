import { cleanup, screen, waitFor } from "@octanejs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderWithQueryClient } from "@/test/render-with-query";

const meMock = vi.fn();
const bootstrapStatusMock = vi.fn();
const orgGetMock = vi.fn();
const listMineMock = vi.fn();
const teamsListMock = vi.fn();
const teamGetMock = vi.fn();
const teamMembersMock = vi.fn();
const teamReposMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    auth: {
      me: (...args: unknown[]) => meMock(...args),
      bootstrapStatus: (...args: unknown[]) => bootstrapStatusMock(...args),
    },
    org: {
      get: (...args: unknown[]) => orgGetMock(...args),
      listMine: (...args: unknown[]) => listMineMock(...args),
      teams: {
        list: (...args: unknown[]) => teamsListMock(...args),
        get: (...args: unknown[]) => teamGetMock(...args),
        create: vi.fn(),
        update: vi.fn(),
        delete: vi.fn(),
        members: {
          list: (...args: unknown[]) => teamMembersMock(...args),
          add: vi.fn(),
          setRole: vi.fn(),
          remove: vi.fn(),
        },
        repos: {
          list: (...args: unknown[]) => teamReposMock(...args),
          add: vi.fn(),
          update: vi.fn(),
          remove: vi.fn(),
        },
      },
    },
    user: {
      lookup: vi.fn().mockResolvedValue({ ok: true, data: { users: [] } }),
    },
  },
}));

import { OrgTeamDetailPage } from "./$owner.settings.teams.$team";
import { OrgTeamsPage } from "./$owner.settings.teams";

const org = {
  id: "o1",
  slug: "acme",
  display_name: "Acme",
  member_base_permission: "none" as const,
};

const team = {
  id: "t1",
  org_id: "o1",
  slug: "core",
  name: "Core",
  description: "core team",
  member_count: 1,
  repo_count: 1,
  created_at: "2026-01-01T00:00:00Z",
  updated_at: "2026-01-01T00:00:00Z",
};

beforeEach(() => {
  meMock.mockReset().mockResolvedValue({ ok: true, data: { id: "u1", username: "ada" } });
  bootstrapStatusMock.mockReset().mockResolvedValue({ ok: true, data: { needs_setup: false } });
  orgGetMock.mockReset().mockResolvedValue({ ok: true, data: org });
  listMineMock
    .mockReset()
    .mockResolvedValue({ ok: true, data: { orgs: [{ slug: "acme", role: "admin" }] } });
  teamsListMock.mockReset().mockResolvedValue({ ok: true, data: { teams: [team] } });
  teamGetMock.mockReset().mockResolvedValue({ ok: true, data: team });
  teamMembersMock.mockReset().mockResolvedValue({
    ok: true,
    data: {
      members: [
        { user_id: "u2", username: "bob", role: "member", created_at: "2026-01-01T00:00:00Z" },
      ],
    },
  });
  teamReposMock.mockReset().mockResolvedValue({
    ok: true,
    data: {
      repos: [
        {
          team_id: "t1",
          repo_id: "r1",
          repo_name: "api",
          permission: "read",
          created_at: "2026-01-01T00:00:00Z",
        },
      ],
    },
  });
});

afterEach(cleanup);

describe("organization team routes", () => {
  it("renders the team list", async () => {
    window.history.pushState({}, "", "/acme/settings/teams");
    renderWithQueryClient(OrgTeamsPage);
    await waitFor(() => expect(screen.getByTestId("org-teams-page")).toBeInTheDocument());
    await waitFor(() => {
      expect(screen.getByTestId("team-row-core")).toBeInTheDocument();
    });
  });

  it("lets an organization member find teams without admin controls", async () => {
    listMineMock.mockResolvedValue({
      ok: true,
      data: { orgs: [{ slug: "acme", role: "member" }] },
    });
    window.history.pushState({}, "", "/acme/settings/teams");
    renderWithQueryClient(OrgTeamsPage);

    await waitFor(() => expect(screen.getByTestId("team-row-core")).toBeInTheDocument());
    expect(screen.queryByTestId("team-create-open")).not.toBeInTheDocument();
  });

  it("renders the team detail", async () => {
    window.history.pushState({}, "", "/acme/settings/teams/core");
    renderWithQueryClient(OrgTeamDetailPage);
    await waitFor(() => expect(screen.getByTestId("org-team-detail")).toBeInTheDocument());
    expect(screen.getByText("Core")).toBeInTheDocument();
  });

  it("lets a team maintainer manage that team without org-admin access", async () => {
    listMineMock.mockResolvedValue({
      ok: true,
      data: { orgs: [{ slug: "acme", role: "member" }] },
    });
    teamMembersMock.mockResolvedValue({
      ok: true,
      data: {
        members: [
          {
            user_id: "u1",
            username: "ada",
            role: "maintainer",
            created_at: "2026-01-01T00:00:00Z",
          },
          { user_id: "u2", username: "bob", role: "member", created_at: "2026-01-01T00:00:00Z" },
        ],
      },
    });
    window.history.pushState({}, "", "/acme/settings/teams/core");
    renderWithQueryClient(OrgTeamDetailPage);

    await waitFor(() => expect(screen.getByTestId("team-edit-open")).toBeInTheDocument());
    expect(screen.getByTestId("member-add-open")).toBeInTheDocument();
    expect(screen.getByTestId("repo-add-open")).toBeInTheDocument();
    expect(screen.queryByTestId("team-delete-open")).not.toBeInTheDocument();
  });
});
