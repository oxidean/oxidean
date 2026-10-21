/**
 * Chromium gate for the repo Actions settings panel — enable Switch plus the
 * default-permissions and fork-PR Base UI Selects (DOM races on mount + pick).
 */
import { afterEach, beforeEach, describe, it, vi } from "vitest";
import { ActionsSettingsPanel } from "@/components/repo/actions-settings-panel";
import {
  cleanupBrowserMount,
  clickTestId,
  debugBody,
  expectNoOctaneOverlayInDocument,
  mountWithQueryClient,
  pickSelectOptionByTestId,
} from "@/test/browser-mount";
import { trackDomErrors } from "@/test/dom-errors";

const getEnabledMock = vi.fn();
const secretsListMock = vi.fn();
const forkPrMock = vi.fn();
const prTargetMock = vi.fn();
const defaultPermsMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    repo: {
      actions: {
        getEnabled: (...args: unknown[]) => getEnabledMock(...args),
        setEnabled: vi.fn(async () => ({ ok: true, data: { enabled: true } })),
        getForkPrApproval: (...args: unknown[]) => forkPrMock(...args),
        setForkPrApproval: vi.fn(async () => ({ ok: true, data: {} })),
        getPullRequestTarget: (...args: unknown[]) => prTargetMock(...args),
        setPullRequestTarget: vi.fn(async () => ({ ok: true, data: {} })),
        getDefaultPermissions: (...args: unknown[]) => defaultPermsMock(...args),
        setDefaultPermissions: vi.fn(async () => ({ ok: true, data: {} })),
        secrets: {
          list: (...args: unknown[]) => secretsListMock(...args),
          put: vi.fn(async () => ({ ok: true, data: {} })),
          delete: vi.fn(async () => ({ ok: true, data: {} })),
        },
      },
    },
  },
}));

beforeEach(() => {
  getEnabledMock.mockReset().mockResolvedValue({ ok: true, data: { enabled: true } });
  secretsListMock
    .mockReset()
    .mockResolvedValue({ ok: true, data: { secrets: [{ name: "REG_TOKEN" }] } });
  forkPrMock.mockReset().mockResolvedValue({ ok: true, data: { policy: "never" } });
  prTargetMock.mockReset().mockResolvedValue({ ok: true, data: { enabled: false } });
  defaultPermsMock
    .mockReset()
    .mockResolvedValue({ ok: true, data: { default_permissions: "read" } });
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

describe("ActionsSettingsPanel browser DOM races", () => {
  it("enable switch + workflow-permission Selects interact cleanly", async () => {
    const tracker = trackDomErrors();
    try {
      await mountWithQueryClient(ActionsSettingsPanel, {
        owner: "ada",
        name: "hello",
        can_admin: true,
      });

      await waitForTestId("repo-actions-settings");
      await waitForTestId("repo-actions-enabled");
      await clickTestId("repo-actions-enabled");

      await waitForTestId("actions-default-perms");
      await pickSelectOptionByTestId("actions-default-perms", "actions-default-perms-option-write");

      await waitForTestId("actions-fork-pr");
      await pickSelectOptionByTestId("actions-fork-pr", "actions-fork-pr-option-always");

      await waitForTestId("actions-pr-target");
      await clickTestId("actions-pr-target");

      await waitForTestId("repo-actions-secret-delete-REG_TOKEN");
      await clickTestId("repo-actions-secret-delete-REG_TOKEN");

      tracker.expectNoDomRaces();
      expectNoOctaneOverlayInDocument();
    } finally {
      tracker.dispose();
    }
  }, 30_000);
});
