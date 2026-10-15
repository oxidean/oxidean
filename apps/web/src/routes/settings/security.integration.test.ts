/**
 * #130 — /settings/security: two-factor section renders factor state,
 * enrollment entry points, and the policy banner.
 */
import { cleanup, render, screen, waitFor } from "@octanejs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const meMock = vi.fn();
const statusMock = vi.fn();
const totpBeginMock = vi.fn();
const totpConfirmMock = vi.fn();
const totpDisableMock = vi.fn();
const regenMock = vi.fn();
const passkeyDeleteMock = vi.fn();
const passkeyUpdateMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    auth: {
      me: (...args: unknown[]) => meMock(...args),
    },
    twoFactor: {
      status: (...args: unknown[]) => statusMock(...args),
      totpBegin: (...args: unknown[]) => totpBeginMock(...args),
      totpConfirm: (...args: unknown[]) => totpConfirmMock(...args),
      totpDisable: (...args: unknown[]) => totpDisableMock(...args),
      recoveryRegenerate: (...args: unknown[]) => regenMock(...args),
      passkeyDelete: (...args: unknown[]) => passkeyDeleteMock(...args),
      passkeyUpdate: (...args: unknown[]) => passkeyUpdateMock(...args),
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
  avatar_url: null as null,
  role: "user",
  profile_incomplete: false,
  email_verified: true,
  must_change_credentials: false,
  two_factor_enabled: false,
  two_factor_enroll_required: false,
};

function status(over: Record<string, unknown> = {}) {
  return {
    ok: true,
    data: {
      totp_enabled: false,
      recovery_codes_remaining: 0,
      passkeys: [],
      challenge_pending: false,
      challenge_kind: null,
      challenge_methods: [],
      enroll_required: false,
      ...over,
    },
  };
}

beforeEach(() => {
  window.history.pushState({}, "", "/settings/security");
  meMock.mockReset();
  statusMock.mockReset();
  totpBeginMock.mockReset();
  totpConfirmMock.mockReset();
  totpDisableMock.mockReset();
  regenMock.mockReset();
  passkeyDeleteMock.mockReset();
  passkeyUpdateMock.mockReset();
  meMock.mockResolvedValue({ ok: true, data: user });
  statusMock.mockResolvedValue(status());
});

afterEach(cleanup);

describe("/settings/security (#130)", () => {
  it("renders the two-factor section with a Set up action when no factor", async () => {
    render(SecuritySettingsPage);

    await waitFor(
      () => {
        expect(document.querySelector("h1")?.textContent).toBe("Security");
      },
      { timeout: 20_000 },
    );
    expect(screen.getByText("Authenticator app")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Set up" })).toBeInTheDocument();
    expect(screen.getByText("Passkeys")).toBeInTheDocument();
    expect(screen.getByText("Recovery codes")).toBeInTheDocument();
  }, 30_000);

  it("shows Enabled + Disable when TOTP is configured", async () => {
    statusMock.mockResolvedValue(status({ totp_enabled: true, recovery_codes_remaining: 8 }));
    render(SecuritySettingsPage);

    await waitFor(
      () => {
        expect(screen.getByText("Enabled")).toBeInTheDocument();
      },
      { timeout: 20_000 },
    );
    expect(screen.getByRole("button", { name: "Disable" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Generate new codes" })).toBeInTheDocument();
    expect(document.body.textContent).toContain("8 unused codes remaining");
  }, 30_000);

  it("shows the enrollment-required banner when policy forces a factor", async () => {
    statusMock.mockResolvedValue(status({ enroll_required: true }));
    render(SecuritySettingsPage);

    await waitFor(
      () => {
        expect(document.body.textContent).toContain("requires two-factor authentication");
      },
      { timeout: 20_000 },
    );
  }, 30_000);

  it("lists passkeys with rename/remove actions", async () => {
    statusMock.mockResolvedValue(
      status({
        passkeys: [
          {
            id: "pk1",
            nickname: "MacBook Touch ID",
            created_at: "2026-01-01T00:00:00Z",
            last_used_at: null,
          },
        ],
      }),
    );
    render(SecuritySettingsPage);

    await waitFor(
      () => {
        expect(screen.getByText("MacBook Touch ID")).toBeInTheDocument();
      },
      { timeout: 20_000 },
    );
    expect(screen.getByRole("button", { name: "Rename MacBook Touch ID" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Remove MacBook Touch ID" })).toBeInTheDocument();
  }, 30_000);
});
