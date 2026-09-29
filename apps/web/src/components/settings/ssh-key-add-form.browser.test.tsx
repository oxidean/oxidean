/**
 * Chromium gate: SSH key usage checkboxes must not throw insertBefore.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { SshKeyAddForm } from "@/components/settings/ssh-key-add-form";
import {
  cleanupBrowserMount,
  clickAriaLabel,
  debugBody,
  expectNoOctaneOverlayInDocument,
  mountWithQueryClient,
} from "@/test/browser-mount";
import { trackDomErrors } from "@/test/dom-errors";

const addMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    sshKey: {
      add: (...args: unknown[]) => addMock(...args),
      list: vi.fn(),
      delete: vi.fn(),
    },
  },
}));

beforeEach(() => {
  addMock.mockReset();
  addMock.mockResolvedValue({ ok: true, data: { item: {} } });
});

afterEach(async () => {
  await cleanupBrowserMount();
});

describe("SshKeyAddForm browser DOM races", () => {
  it("toggling usage checkboxes does not throw insertBefore / overlay", async () => {
    const tracker = trackDomErrors();
    try {
      await mountWithQueryClient(SshKeyAddForm);

      const auth = document.querySelector('[aria-label="Authentication"]');
      if (!auth) {
        throw new Error(`Authentication checkbox not mounted. ${debugBody()}`);
      }

      await clickAriaLabel("Authentication");
      await clickAriaLabel("Commit signing");
      await clickAriaLabel("Authentication");
      await clickAriaLabel("Commit signing");

      expect(document.querySelector('[aria-label="Commit signing"]')).toBeTruthy();
      tracker.expectNoDomRaces();
      expectNoOctaneOverlayInDocument();
    } finally {
      tracker.dispose();
    }
  }, 30_000);
});
