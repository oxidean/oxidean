/**
 * Chromium gate for the danger-zone TypeToConfirmDialog open → type → confirm
 * cycles — real-DOM proof for the fixed overlays; happy-dom misses the
 * insertBefore races these dialogs were built to avoid.
 */
import { act } from "octane";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { RepoPublic } from "@oxidean/api-client";
import { DangerZonePanel } from "@/components/repo/danger-zone-panel";
import {
  cleanupBrowserMount,
  debugBody,
  expectNoOctaneOverlayInDocument,
  mountWithQueryClient,
} from "@/test/browser-mount";
import { trackDomErrors } from "@/test/dom-errors";

const transferMock = vi.fn();
const setArchivedMock = vi.fn();
const softDeleteMock = vi.fn();
const userLookupMock = vi.fn();
const orgListMineMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    repo: {
      rename: vi.fn(),
      transfer: (...args: unknown[]) => transferMock(...args),
      setArchived: (...args: unknown[]) => setArchivedMock(...args),
      softDelete: (...args: unknown[]) => softDeleteMock(...args),
    },
    user: { lookup: (...args: unknown[]) => userLookupMock(...args) },
    org: { listMine: (...args: unknown[]) => orgListMineMock(...args) },
  },
}));

vi.mock("@/lib/toast", () => ({
  toastSuccess: vi.fn(),
  toastError: vi.fn(),
}));

const repo = {
  id: "r1",
  owner_id: "u1",
  owner_type: "user",
  owner_username: "ada",
  name: "hello",
  description: "",
  visibility: "public",
  default_branch: "main",
  updated_at: "2026-09-14T00:00:00Z",
  can_admin: true,
  can_write: true,
} as RepoPublic;

beforeEach(() => {
  transferMock.mockReset();
  setArchivedMock.mockReset();
  softDeleteMock.mockReset();
  userLookupMock.mockReset();
  orgListMineMock.mockReset();
  transferMock.mockResolvedValue({
    ok: true,
    data: { repo: { ...repo, owner_username: "bob" } },
  });
  setArchivedMock.mockResolvedValue({
    ok: true,
    data: { ...repo, archived: true },
  });
  softDeleteMock.mockResolvedValue({ ok: true, data: { name: "hello" } });
  userLookupMock.mockResolvedValue({ ok: true, data: { users: [] } });
  orgListMineMock.mockResolvedValue({ ok: true, data: { orgs: [] } });
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

async function waitForGone(testId: string, ms = 10_000): Promise<void> {
  const deadline = Date.now() + ms;
  while (Date.now() < deadline) {
    if (!document.querySelector(`[data-testid="${testId}"]`)) return;
    await new Promise((r) => setTimeout(r, 50));
  }
  throw new Error(`[data-testid="${testId}"] never detached. ${debugBody()}`);
}

function findButton(scope: ParentNode, text: string): HTMLButtonElement | undefined {
  return [...scope.querySelectorAll("button")].find((b) => b.textContent?.trim() === text) as
    | HTMLButtonElement
    | undefined;
}

async function waitForEnabledButton(scope: ParentNode, text: string, ms = 10_000) {
  const deadline = Date.now() + ms;
  while (Date.now() < deadline) {
    const btn = findButton(scope, text);
    if (btn && !btn.disabled) return btn;
    await new Promise((r) => setTimeout(r, 50));
  }
  throw new Error(`button "${text}" never became enabled. ${debugBody()}`);
}

async function clickButton(scope: ParentNode, text: string): Promise<void> {
  const btn = await waitForEnabledButton(scope, text);
  await act(async () => {
    btn.click();
  });
}

async function typeIntoSelector(selector: string, value: string): Promise<void> {
  const el = document.querySelector(selector);
  if (!el) throw new Error(`no element ${selector}. ${debugBody()}`);
  await act(async () => {
    const input = el as HTMLInputElement;
    input.focus();
    input.value = value;
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

describe("DangerZonePanel browser DOM races", () => {
  it("transfer, archive, and delete confirm dialogs cycle without DOM races", async () => {
    const tracker = trackDomErrors();
    try {
      await mountWithQueryClient(DangerZonePanel, {
        owner: "ada",
        name: "hello",
        repo,
      });
      await waitForTestId("owner-lookup");

      // Transfer: destination enables the trigger; typed name arms confirm.
      await typeIntoSelector("#repo-transfer-dest", "bob");
      await clickButton(document, "Transfer repository");
      await waitForTestId("repo-transfer-confirm");
      await typeIntoSelector("#transfer-confirm", "hello");
      const transferDialog = await waitForTestId("repo-transfer-confirm");
      await clickButton(transferDialog, "Transfer repository");
      await waitForGone("repo-transfer-confirm");
      expect(transferMock).toHaveBeenCalledWith({
        owner: "ada",
        name: "hello",
        destOwner: "bob",
        destOwnerType: "user",
        confirmName: "hello",
      });

      // Archive: same overlay cycle, different panel state.
      await clickButton(document, "Archive repository");
      await waitForTestId("repo-archive-confirm");
      await typeIntoSelector("#archive-confirm", "hello");
      const archiveDialog = await waitForTestId("repo-archive-confirm");
      await clickButton(archiveDialog, "Archive repository");
      await waitForGone("repo-archive-confirm");
      expect(setArchivedMock).toHaveBeenCalledWith({
        owner: "ada",
        name: "hello",
        archived: true,
      });

      // Delete: third overlay mount against post-mutation tree.
      await clickButton(document, "Delete repository");
      await waitForTestId("repo-delete-confirm");
      await typeIntoSelector("#soft-delete-confirm", "hello");
      const deleteDialog = await waitForTestId("repo-delete-confirm");
      await clickButton(deleteDialog, "Delete repository");
      await waitForGone("repo-delete-confirm");
      expect(softDeleteMock).toHaveBeenCalledWith({
        owner: "ada",
        name: "hello",
        confirmName: "hello",
      });

      tracker.expectNoDomRaces();
      expectNoOctaneOverlayInDocument();
    } finally {
      tracker.dispose();
    }
  }, 30_000);
});
