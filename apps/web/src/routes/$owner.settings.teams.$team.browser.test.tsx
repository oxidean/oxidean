/**
 * Chromium gate for team-detail Selects (member role, repo permission),
 * form.Subscribe add forms, and AlertDialogs — no insertBefore DOM races.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanupBrowserMount,
  clickTestId,
  debugBody,
  expectNoOctaneOverlayInDocument,
  mountWithQueryClient,
  pickSelectOptionByTestId,
} from "@/test/browser-mount";
import { trackDomErrors } from "@/test/dom-errors";

const meMock = vi.fn();
const orgGetMock = vi.fn();
const listMineMock = vi.fn();
const teamGetMock = vi.fn();
const teamMembersMock = vi.fn();
const teamReposMock = vi.fn();
const teamsListMock = vi.fn();
const setRoleMock = vi.fn();
const repoUpdateMock = vi.fn();
const lookupMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    auth: { me: (...args: unknown[]) => meMock(...args) },
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
          remove: vi.fn(),
          setRole: (...args: unknown[]) => setRoleMock(...args),
        },
        repos: {
          list: (...args: unknown[]) => teamReposMock(...args),
          add: vi.fn(),
          update: (...args: unknown[]) => repoUpdateMock(...args),
          remove: vi.fn(),
        },
      },
    },
    user: {
      lookup: (...args: unknown[]) => lookupMock(...args),
    },
  },
}));

vi.mock("@/lib/toast", () => ({
  appToastManager: {
    add: vi.fn(),
    remove: vi.fn(),
    update: vi.fn(),
    close: vi.fn(),
    promise: vi.fn(),
  },
  toastSuccess: vi.fn(),
  toastError: vi.fn(),
}));

import { OrgTeamDetailPage } from "./$owner.settings.teams.$team";

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
  window.history.pushState({}, "", "/acme/settings/teams/core");
  for (const m of [
    meMock,
    orgGetMock,
    listMineMock,
    teamGetMock,
    teamMembersMock,
    teamReposMock,
    teamsListMock,
    setRoleMock,
    repoUpdateMock,
    lookupMock,
  ]) {
    m.mockReset();
  }
  meMock.mockResolvedValue({ ok: true, data: { id: "u1", username: "ada" } });
  orgGetMock.mockResolvedValue({ ok: true, data: org });
  listMineMock.mockResolvedValue({
    ok: true,
    data: { orgs: [{ slug: "acme", role: "admin" }] },
  });
  teamGetMock.mockResolvedValue({ ok: true, data: team });
  teamMembersMock.mockResolvedValue({
    ok: true,
    data: {
      members: [
        {
          user_id: "u2",
          username: "bob",
          role: "member",
          created_at: "2026-01-01T00:00:00Z",
        },
      ],
    },
  });
  teamReposMock.mockResolvedValue({
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
  teamsListMock.mockResolvedValue({ ok: true, data: { teams: [team] } });
  setRoleMock.mockResolvedValue({
    ok: true,
    data: { user_id: "u2", username: "bob", role: "maintainer", created_at: "x" },
  });
  repoUpdateMock.mockResolvedValue({ ok: true, data: {} });
  lookupMock.mockResolvedValue({ ok: true, data: { users: [] } });
});

afterEach(async () => {
  await cleanupBrowserMount();
});

async function waitForTestId(testId: string, ms = 10_000): Promise<Element> {
  const deadline = Date.now() + ms;
  while (Date.now() < deadline) {
    const el = document.querySelector(`[data-testid="${testId}"]`);
    if (el) return el;
    await new Promise((r) => setTimeout(r, 50));
  }
  throw new Error(`[data-testid="${testId}"] never appeared. ${debugBody()}`);
}

describe("OrgTeamDetailPage browser DOM races", () => {
  it("role/permission Selects and dialogs mount cleanly", async () => {
    const tracker = trackDomErrors();
    try {
      await mountWithQueryClient(OrgTeamDetailPage, {});

      await waitForTestId("org-team-detail");
      await waitForTestId("member-role-u2");
      await pickSelectOptionByTestId("member-role-u2", "member-role-u2-option-maintainer");
      await pickSelectOptionByTestId("member-role-u2", "member-role-u2-option-member");

      await waitForTestId("repo-perm-r1");
      await pickSelectOptionByTestId("repo-perm-r1", "repo-perm-r1-option-write");
      await pickSelectOptionByTestId("repo-perm-r1", "repo-perm-r1-option-read");

      await clickTestId("member-add-open");
      await waitForTestId("team-member-role");
      await pickSelectOptionByTestId("team-member-role", "team-member-role-option-maintainer");

      await clickTestId("repo-add-open");
      await waitForTestId("team-repo-perm");
      await pickSelectOptionByTestId("team-repo-perm", "team-repo-perm-option-admin");
      await pickSelectOptionByTestId("team-repo-perm", "team-repo-perm-option-read");

      await clickTestId("team-delete-open");
      await waitForTestId("team-delete-confirm");

      tracker.expectNoDomRaces();
      expectNoOctaneOverlayInDocument();
      expect(setRoleMock).toHaveBeenCalled();
      expect(repoUpdateMock).toHaveBeenCalled();
    } finally {
      tracker.dispose();
    }
  }, 30_000);
});
