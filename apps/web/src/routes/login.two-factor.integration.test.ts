/**
 * #130 — /login/two-factor: pending-challenge UI renders verify vs enroll
 * challenge kinds and the expired/empty state.
 */
import { cleanup, render, screen, waitFor } from "@octanejs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const meMock = vi.fn();
const statusMock = vi.fn();
const completeMock = vi.fn();
const emailSendMock = vi.fn();
const passkeyBeginMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    auth: {
      me: (...args: unknown[]) => meMock(...args),
    },
    twoFactor: {
      status: (...args: unknown[]) => statusMock(...args),
      complete: (...args: unknown[]) => completeMock(...args),
      emailSend: (...args: unknown[]) => emailSendMock(...args),
      passkeyBegin: (...args: unknown[]) => passkeyBeginMock(...args),
      passkeyComplete: vi.fn(),
      passkeyRegisterBegin: vi.fn(),
      passkeyRegisterComplete: vi.fn(),
      passkeyUpdate: vi.fn(),
      passkeyDelete: vi.fn(),
      totpBegin: vi.fn(),
      totpConfirm: vi.fn(),
      totpDisable: vi.fn(),
      recoveryRegenerate: vi.fn(),
    },
  },
}));

import { TwoFactorLoginPage } from "./login.two-factor";

function challenge(over: Record<string, unknown> = {}) {
  return {
    ok: true,
    data: {
      totp_enabled: false,
      recovery_codes_remaining: 0,
      passkeys: [],
      challenge_pending: true,
      challenge_kind: "verify",
      challenge_methods: ["totp", "recovery", "email"],
      enroll_required: false,
      ...over,
    },
  };
}

beforeEach(() => {
  window.history.pushState({}, "", "/login/two-factor?return=%2Fsettings");
  meMock.mockReset();
  statusMock.mockReset();
  completeMock.mockReset();
  emailSendMock.mockReset();
  meMock.mockResolvedValue({
    ok: false,
    error: { code: "auth.unauthenticated", message: "n" },
  });
  statusMock.mockResolvedValue(challenge());
});

afterEach(cleanup);

describe("/login/two-factor (#130)", () => {
  it("verify challenge: renders the code input + email option", async () => {
    render(TwoFactorLoginPage);

    await waitFor(
      () => {
        expect(screen.getByLabelText("Verification code")).toBeInTheDocument();
      },
      { timeout: 20_000 },
    );
    expect(screen.getByRole("button", { name: "Verify" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Email me a sign-in code/i })).toBeInTheDocument();
    expect(document.body.textContent).toContain("recovery code");
  }, 30_000);

  it("enroll challenge: renders the method picker", async () => {
    statusMock.mockResolvedValue(
      challenge({
        challenge_kind: "enroll",
        challenge_methods: ["totp", "passkey"],
        enroll_required: true,
      }),
    );
    render(TwoFactorLoginPage);

    await waitFor(
      () => {
        expect(screen.getByRole("button", { name: /Authenticator app/i })).toBeInTheDocument();
      },
      { timeout: 20_000 },
    );
    expect(document.body.textContent).toContain("requires two-factor authentication");
  }, 30_000);

  it("no pending challenge: expired state links back to sign in", async () => {
    statusMock.mockResolvedValue(challenge({ challenge_pending: false, challenge_kind: null }));
    render(TwoFactorLoginPage);

    await waitFor(
      () => {
        expect(document.body.textContent).toContain("no longer active");
      },
      { timeout: 20_000 },
    );
    expect(screen.getByText("Back to sign in")).toBeInTheDocument();
  }, 30_000);
});
