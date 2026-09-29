/**
 * Chromium gate for mirror HTTPS↔SSH + sync-mode radios (happy-dom cannot catch).
 * Complements stack-browser `expectMirrorAuthToggleFlow`.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MirrorSettingsPanel } from "@/components/repo/mirror-settings-panel";
import {
  cleanupBrowserMount,
  clickTestId,
  debugBody,
  expectNoOctaneOverlayInDocument,
  mountWithQueryClient,
} from "@/test/browser-mount";
import { trackDomErrors } from "@/test/dom-errors";

const mirrorGetMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    repo: {
      mirror: {
        get: (...args: unknown[]) => mirrorGetMock(...args),
        upsert: vi.fn(),
        delete: vi.fn(),
        syncNow: vi.fn(),
        generateSshKey: vi.fn(),
        fetchHostKey: vi.fn(),
        rotateWebhookSecret: vi.fn(),
      },
    },
  },
}));

vi.mock("@/lib/toast", () => ({
  toastSuccess: vi.fn(),
}));

beforeEach(() => {
  mirrorGetMock.mockReset();
  mirrorGetMock.mockResolvedValue({ ok: true, data: { mirror: null } });
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

describe("MirrorSettingsPanel browser DOM races", () => {
  it("sync-mode + auth-kind radio toggles do not throw insertBefore", async () => {
    const tracker = trackDomErrors();
    try {
      await mountWithQueryClient(MirrorSettingsPanel, {
        owner: "ada",
        name: "hello",
        can_admin: true,
      });

      await waitForTestId("repo-mirror-settings");
      await waitForTestId("mirror-auth-kind-ssh");

      await clickTestId("mirror-sync-mode-exact");
      await waitForTestId("mirror-exact-warning");
      await clickTestId("mirror-sync-mode-merge");
      await clickTestId("mirror-auth-kind-ssh");
      await clickTestId("mirror-auth-kind-https");
      await clickTestId("mirror-auth-kind-ssh");

      expect(document.querySelector('[data-testid="mirror-auth-ssh"]')).toBeTruthy();
      tracker.expectNoDomRaces();
      expectNoOctaneOverlayInDocument();
    } finally {
      tracker.dispose();
    }
  }, 30_000);
});
