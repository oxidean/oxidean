/**
 * Chromium gate (#130): passkey remove AlertDialog + rename inline form must
 * not throw insertBefore / Octane overlay DOM races.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { PasskeySection } from "@/components/settings/passkey-section";
import {
  cleanupBrowserMount,
  clickAriaLabel,
  debugBody,
  expectNoOctaneOverlayInDocument,
  mountWithQueryClient,
} from "@/test/browser-mount";
import { trackDomErrors } from "@/test/dom-errors";
import { act } from "octane";

const passkeyDeleteMock = vi.fn();
const passkeyUpdateMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    twoFactor: {
      passkeyDelete: (...args: unknown[]) => passkeyDeleteMock(...args),
      passkeyUpdate: (...args: unknown[]) => passkeyUpdateMock(...args),
      passkeyRegisterBegin: vi.fn(),
      passkeyRegisterComplete: vi.fn(),
      status: vi.fn(),
      totpBegin: vi.fn(),
      totpConfirm: vi.fn(),
      totpDisable: vi.fn(),
      recoveryRegenerate: vi.fn(),
      complete: vi.fn(),
      passkeyBegin: vi.fn(),
      passkeyComplete: vi.fn(),
      emailSend: vi.fn(),
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

const passkeys = [
  {
    id: "pk1",
    nickname: "MacBook Touch ID",
    created_at: "2026-01-01T00:00:00Z",
    last_used_at: null,
  },
  {
    id: "pk2",
    nickname: "YubiKey",
    created_at: "2026-01-02T00:00:00Z",
    last_used_at: "2026-02-01T00:00:00Z",
  },
];

beforeEach(() => {
  passkeyDeleteMock.mockReset();
  passkeyUpdateMock.mockReset();
  passkeyDeleteMock.mockResolvedValue({ ok: true, data: { ok: true } });
  passkeyUpdateMock.mockResolvedValue({ ok: true, data: { ok: true } });
});

afterEach(async () => {
  await cleanupBrowserMount();
});

async function clickButtonText(label: string, ms = 10_000): Promise<void> {
  const deadline = Date.now() + ms;
  while (Date.now() < deadline) {
    const btn = Array.from(document.querySelectorAll("button")).find(
      (b) => (b.textContent ?? "").trim() === label,
    );
    if (btn) {
      await act(async () => {
        (btn as HTMLElement).click();
      });
      return;
    }
    await new Promise((r) => setTimeout(r, 50));
  }
  throw new Error(`button "${label}" never appeared. ${debugBody()}`);
}

async function waitForSelector(selector: string, ms = 10_000): Promise<Element> {
  const deadline = Date.now() + ms;
  while (Date.now() < deadline) {
    const el = document.querySelector(selector);
    if (el) return el;
    await new Promise((r) => setTimeout(r, 50));
  }
  throw new Error(`${selector} never appeared. ${debugBody()}`);
}

describe("PasskeySection browser DOM races", () => {
  it("remove AlertDialog opens/cancels/deletes without insertBefore", async () => {
    const tracker = trackDomErrors();
    try {
      await mountWithQueryClient(PasskeySection, { passkeys });

      await clickAriaLabel("Remove MacBook Touch ID");
      await waitForSelector('[data-slot="alert-dialog-content"]');
      expect(document.body.textContent).toContain("Remove passkey?");

      // Cancel path — dialog closes, list stays.
      await clickButtonText("Keep passkey");
      await new Promise((r) => setTimeout(r, 100));

      // Re-open and commit the delete.
      await clickAriaLabel("Remove YubiKey");
      await waitForSelector('[data-slot="alert-dialog-content"]');
      await clickButtonText("Remove passkey");
      await act(async () => {
        await new Promise((r) => setTimeout(r, 100));
      });
      expect(passkeyDeleteMock).toHaveBeenCalledWith({ id: "pk2" });

      tracker.expectNoDomRaces();
      expectNoOctaneOverlayInDocument();
    } finally {
      tracker.dispose();
    }
  }, 30_000);

  it("rename inline form swaps row content without insertBefore", async () => {
    const tracker = trackDomErrors();
    try {
      await mountWithQueryClient(PasskeySection, { passkeys });

      await clickAriaLabel("Rename MacBook Touch ID");
      const input = await waitForSelector('[aria-label="Passkey nickname"]');

      const cancel = Array.from(document.querySelectorAll("button")).find(
        (b) => b.textContent?.trim() === "Cancel",
      );
      if (!cancel || !input) throw new Error(`rename form missing. ${debugBody()}`);
      cancel.dispatchEvent(new MouseEvent("click", { bubbles: true }));
      await new Promise((r) => setTimeout(r, 80));

      await clickAriaLabel("Rename YubiKey");
      await waitForSelector('[aria-label="Passkey nickname"]');

      tracker.expectNoDomRaces();
      expectNoOctaneOverlayInDocument();
    } finally {
      tracker.dispose();
    }
  }, 30_000);
});
