/**
 * Chromium gate for the teams list + create form mount (inputs, no races).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanupBrowserMount,
  clickTestId,
  debugBody,
  expectNoOctaneOverlayInDocument,
  mountWithQueryClient,
} from "@/test/browser-mount";
import { trackDomErrors } from "@/test/dom-errors";

const meMock = vi.fn();
const orgGetMock = vi.fn();
const listMineMock = vi.fn();
const teamsListMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    auth: { me: (...args: unknown[]) => meMock(...args) },
    org: {
      get: (...args: unknown[]) => orgGetMock(...args),
      listMine: (...args: unknown[]) => listMineMock(...args),
      teams: {
        list: (...args: unknown[]) => teamsListMock(...args),
        create: vi.fn(),
      },
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

import { OrgTeamsPage } from "./$owner.settings.teams";

const org = {
  id: "o1",
  slug: "acme",
  display_name: "Acme",
  member_base_permission: "none" as const,
};

beforeEach(() => {
  window.history.pushState({}, "", "/acme/settings/teams");
  meMock.mockReset();
  orgGetMock.mockReset();
  listMineMock.mockReset();
  teamsListMock.mockReset();
  meMock.mockResolvedValue({ ok: true, data: { id: "u1", username: "ada" } });
  orgGetMock.mockResolvedValue({ ok: true, data: org });
  listMineMock.mockResolvedValue({
    ok: true,
    data: { orgs: [{ slug: "acme", role: "admin" }] },
  });
  teamsListMock.mockResolvedValue({
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
          repo_count: 0,
          created_at: "x",
          updated_at: "x",
        },
      ],
    },
  });
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

describe("OrgTeamsPage browser mount", () => {
  it("list + create form render without DOM races", async () => {
    const tracker = trackDomErrors();
    try {
      await mountWithQueryClient(OrgTeamsPage, {});
      await waitForTestId("org-teams-page");
      await waitForTestId("team-create-open");
      await clickTestId("team-create-open");
      await waitForTestId("team-create-form");
      await waitForTestId("team-create-name");

      tracker.expectNoDomRaces();
      expectNoOctaneOverlayInDocument();
      expect(teamsListMock).toHaveBeenCalled();
    } finally {
      tracker.dispose();
    }
  }, 30_000);
});
