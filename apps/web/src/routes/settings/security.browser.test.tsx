/**
 * Chromium gate (#130): /settings/security AlertDialogs (TOTP disable,
 * recovery-code regenerate) must not throw insertBefore / Octane overlay.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanupBrowserMount,
  debugBody,
  expectNoOctaneOverlayInDocument,
  mountWithQueryClient,
} from "@/test/browser-mount";
import { trackDomErrors } from "@/test/dom-errors";
import { act } from "octane";

const meMock = vi.fn();
const statusMock = vi.fn();
const totpDisableMock = vi.fn();
const regenMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    auth: {
      me: (...args: unknown[]) => meMock(...args),
      providerConfig: vi.fn(async () => ({
        ok: true,
        data: { mode: "local", allow_signup: true },
      })),
      bootstrapStatus: vi.fn(async () => ({ ok: true, data: { needs_setup: false } })),
    },
    twoFactor: {
      status: (...args: unknown[]) => statusMock(...args),
      totpBegin: vi.fn(),
      totpConfirm: vi.fn(),
      totpDisable: (...args: unknown[]) => totpDisableMock(...args),
      recoveryRegenerate: (...args: unknown[]) => regenMock(...args),
      passkeyDelete: vi.fn(),
      passkeyUpdate: vi.fn(),
      passkeyRegisterBegin: vi.fn(),
      passkeyRegisterComplete: vi.fn(),
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

import { SecuritySettingsPage } from "./security";

const user = {
  id: "u1",
  email: "ada@example.com",
  username: "ada",
  display_name: "Ada",
  bio: "",
  avatar_url: null,
  role: "user",
  profile_incomplete: false,
  email_verified: true,
  must_change_credentials: false,
  two_factor_enabled: true,
  two_factor_enroll_required: false,
};

beforeEach(() => {
  window.history.pushState({}, "", "/settings/security");
  meMock.mockReset();
  statusMock.mockReset();
  totpDisableMock.mockReset();
  regenMock.mockReset();
  meMock.mockResolvedValue({ ok: true, data: user });
  statusMock.mockResolvedValue({
    ok: true,
    data: {
      totp_enabled: true,
      recovery_codes_remaining: 8,
      passkeys: [
        {
          id: "pk1",
          nickname: "MacBook Touch ID",
          created_at: "2026-01-01T00:00:00Z",
          last_used_at: null,
        },
      ],
      challenge_pending: false,
      challenge_kind: null,
      challenge_methods: [],
      enroll_required: false,
    },
  });
  totpDisableMock.mockResolvedValue({ ok: true, data: { ok: true } });
  regenMock.mockResolvedValue({ ok: true, data: { recovery_codes: ["aaaaa-bbbbb"] } });
});

afterEach(async () => {
  await cleanupBrowserMount();
});

async function waitForSelector(selector: string, ms = 15_000): Promise<Element> {
  const deadline = Date.now() + ms;
  while (Date.now() < deadline) {
    const el = document.querySelector(selector);
    if (el) return el;
    await new Promise((r) => setTimeout(r, 50));
  }
  throw new Error(`${selector} never appeared. ${debugBody()}`);
}

async function clickButtonText(label: string): Promise<void> {
  const deadline = Date.now() + 15_000;
  while (Date.now() < deadline) {
    const btn = Array.from(document.querySelectorAll("button")).find(
      (b) => b.textContent?.trim() === label,
    );
    if (btn) {
      await act(async () => {
        btn.dispatchEvent(new MouseEvent("click", { bubbles: true }));
      });
      return;
    }
    await new Promise((r) => setTimeout(r, 50));
  }
  throw new Error(`button "${label}" never appeared. ${debugBody()}`);
}

describe("/settings/security browser DOM races", () => {
  it("disable + regenerate AlertDialogs open/cancel/commit cleanly", async () => {
    const tracker = trackDomErrors();
    try {
      await mountWithQueryClient(SecuritySettingsPage);
      await waitForSelector('[data-testid="settings-security-page"]');

      // Disable-app dialog: open → cancel → reopen → commit.
      await clickButtonText("Disable");
      await waitForSelector('[data-slot="alert-dialog-content"]');
      await clickButtonText("Keep app");
      await new Promise((r) => setTimeout(r, 120));

      await clickButtonText("Disable");
      await waitForSelector('[data-slot="alert-dialog-content"]');
      await clickButtonText("Disable app");
      await act(async () => {
        await new Promise((r) => setTimeout(r, 120));
      });
      expect(totpDisableMock).toHaveBeenCalled();

      // Regenerate dialog: open → cancel.
      await clickButtonText("Generate new codes");
      await waitForSelector('[data-slot="alert-dialog-content"]');
      await clickButtonText("Keep current codes");
      await new Promise((r) => setTimeout(r, 120));

      tracker.expectNoDomRaces();
      expectNoOctaneOverlayInDocument();
    } finally {
      tracker.dispose();
    }
  }, 40_000);
});
