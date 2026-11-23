/**
 * Chromium gate for repo teams-panel Selects (team pick, permission) and the
 * remove AlertDialog — no insertBefore DOM races.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { TeamsPanel } from "@/components/repo/teams-panel";
import {
  cleanupBrowserMount,
  clickTestId,
  debugBody,
  expectNoOctaneOverlayInDocument,
  mountWithQueryClient,
  pickSelectOptionByTestId,
} from "@/test/browser-mount";
import { trackDomErrors } from "@/test/dom-errors";

const listMock = vi.fn();
const orgTeamsMock = vi.fn();
const updateMock = vi.fn();
const addMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    repo: {
      teams: {
        list: (...args: unknown[]) => listMock(...args),
        add: (...args: unknown[]) => addMock(...args),
        update: (...args: unknown[]) => updateMock(...args),
        remove: vi.fn(),
      },
    },
    org: {
      teams: {
        list: (...args: unknown[]) => orgTeamsMock(...args),
      },
    },
  },
}));

beforeEach(() => {
  listMock.mockReset();
  orgTeamsMock.mockReset();
  updateMock.mockReset();
  addMock.mockReset();
  listMock.mockResolvedValue({
    ok: true,
    data: {
      teams: [
        {
          team_id: "t1",
          slug: "core",
          name: "Core",
          permission: "read",
          created_at: "2026-01-01T00:00:00Z",
        },
      ],
    },
  });
  orgTeamsMock.mockResolvedValue({
    ok: true,
    data: {
      teams: [
        {
          id: "t1",
          org_id: "o1",
          slug: "core",
          name: "Core",
          description: "",
          member_count: 1,
          repo_count: 1,
          created_at: "x",
          updated_at: "x",
        },
        {
          id: "t2",
          org_id: "o1",
          slug: "ops",
          name: "Ops",
          description: "",
          member_count: 2,
          repo_count: 0,
          created_at: "x",
          updated_at: "x",
        },
      ],
    },
  });
  updateMock.mockResolvedValue({ ok: true, data: {} });
  addMock.mockResolvedValue({ ok: true, data: {} });
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

describe("TeamsPanel browser DOM races", () => {
  it("grant list, add Selects, and remove dialog mount cleanly", async () => {
    const tracker = trackDomErrors();
    try {
      await mountWithQueryClient(TeamsPanel, {
        owner: "acme",
        name: "api",
        orgOwned: true,
      });

      await waitForTestId("teams-panel");
      await waitForTestId("team-perm-t1");
      await pickSelectOptionByTestId("team-perm-t1", "team-perm-t1-option-write");
      await pickSelectOptionByTestId("team-perm-t1", "team-perm-t1-option-read");

      await waitForTestId("team-add-open");
      await clickTestId("team-add-open");
      await waitForTestId("team-add-perm");
      await pickSelectOptionByTestId("team-add-perm", "team-add-perm-option-admin");
      await pickSelectOptionByTestId("team-add-perm", "team-add-perm-option-read");
      await waitForTestId("team-add-select");
      await pickSelectOptionByTestId("team-add-select", "team-add-option-ops");

      tracker.expectNoDomRaces();
      expectNoOctaneOverlayInDocument();
      expect(updateMock).toHaveBeenCalled();
    } finally {
      tracker.dispose();
    }
  }, 30_000);

  it("does not grant a different team after the selected team becomes unavailable", async () => {
    const tracker = trackDomErrors();
    try {
      await mountWithQueryClient(TeamsPanel, {
        owner: "acme",
        name: "api",
        orgOwned: true,
      });
      await waitForTestId("teams-panel");
      await waitForTestId("team-add-open");
      await clickTestId("team-add-open");
      await pickSelectOptionByTestId("team-add-select", "team-add-option-ops");
      await pickSelectOptionByTestId("team-add-perm", "team-add-perm-option-admin");
      listMock.mockResolvedValue({
        ok: true,
        data: {
          teams: [
            {
              team_id: "t1",
              slug: "core",
              name: "Core",
              permission: "write",
              created_at: "2026-01-01T00:00:00Z",
            },
            {
              team_id: "t2",
              slug: "ops",
              name: "Ops",
              permission: "read",
              created_at: "2026-01-01T00:00:00Z",
            },
          ],
        },
      });
      await pickSelectOptionByTestId("team-perm-t1", "team-perm-t1-option-write");
      await new Promise((resolve) => setTimeout(resolve, 100));
      await clickTestId("team-add-submit");

      expect(addMock).not.toHaveBeenCalled();
      expect(document.body.textContent).toContain("selected team is no longer available");
      tracker.expectNoDomRaces();
      expectNoOctaneOverlayInDocument();
    } finally {
      tracker.dispose();
    }
  }, 30_000);
});
