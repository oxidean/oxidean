/**
 * Chromium gate for collaborators permission Select + add form open (DOM races).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { CollaboratorsPanel } from "@/components/repo/collaborators-panel";
import {
  cleanupBrowserMount,
  clickTestId,
  debugBody,
  expectNoOctaneOverlayInDocument,
  mountWithQueryClient,
  pickSelectOptionByTestId,
} from "@/test/browser-mount";
import { trackDomErrors } from "@/test/dom-errors";

const listCollabsMock = vi.fn();
const listInvitesMock = vi.fn();
const lookupMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    repo: {
      collaborators: {
        list: (...args: unknown[]) => listCollabsMock(...args),
        add: vi.fn(),
        update: vi.fn(),
        remove: vi.fn(),
      },
      invites: {
        list: (...args: unknown[]) => listInvitesMock(...args),
        create: vi.fn(),
        revoke: vi.fn(),
      },
    },
    user: {
      lookup: (...args: unknown[]) => lookupMock(...args),
    },
  },
}));

beforeEach(() => {
  listCollabsMock.mockReset();
  listInvitesMock.mockReset();
  lookupMock.mockReset();
  listCollabsMock.mockResolvedValue({ ok: true, data: { collaborators: [] } });
  listInvitesMock.mockResolvedValue({ ok: true, data: { invites: [] } });
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

describe("CollaboratorsPanel browser DOM races", () => {
  it("add form permission Select does not throw insertBefore", async () => {
    const tracker = trackDomErrors();
    try {
      await mountWithQueryClient(CollaboratorsPanel, {
        owner: "ada",
        name: "hello",
      });

      await waitForTestId("collaborators-panel");
      await waitForTestId("collab-add-open");
      await clickTestId("collab-add-open");
      await waitForTestId("collab-add-perm");

      await pickSelectOptionByTestId("collab-add-perm", "collab-add-perm-option-admin");
      await pickSelectOptionByTestId("collab-add-perm", "collab-add-perm-option-read");

      expect(document.querySelector('[data-testid="collab-add-perm"]')).toBeTruthy();

      await waitForTestId("repo-invite-perm");
      await pickSelectOptionByTestId("repo-invite-perm", "repo-invite-perm-option-admin");
      await pickSelectOptionByTestId("repo-invite-perm", "repo-invite-perm-option-write");

      tracker.expectNoDomRaces();
      expectNoOctaneOverlayInDocument();
    } finally {
      tracker.dispose();
    }
  }, 30_000);
});
