/**
 * Chromium gate for /verify — InputOtp + form.Subscribe + resend across the
 * loading → form / need-sign-in phase switches, mounted without DOM races.
 * Companion to the happy-dom contracts in verify.integration.test.ts.
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
const verifyMock = vi.fn();
const resendVerifyMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    auth: {
      me: (...args: unknown[]) => meMock(...args),
      verify: (...args: unknown[]) => verifyMock(...args),
      requestVerify: vi.fn(),
      resendVerify: (...args: unknown[]) => resendVerifyMock(...args),
    },
  },
}));

import { VerifyPage } from "./verify";

beforeEach(() => {
  meMock.mockReset();
  verifyMock.mockReset();
  resendVerifyMock.mockReset();
  window.history.replaceState({}, "", "/verify");
});

afterEach(async () => {
  await cleanupBrowserMount();
});

async function waitForSelector(selector: string, ms = 10_000): Promise<Element> {
  const deadline = Date.now() + ms;
  while (Date.now() < deadline) {
    const el = document.querySelector(selector);
    if (el) return el;
    await new Promise((r) => setTimeout(r, 50));
  }
  throw new Error(`${selector} never appeared. ${debugBody()}`);
}

async function waitForText(text: string, ms = 10_000): Promise<void> {
  const deadline = Date.now() + ms;
  while (Date.now() < deadline) {
    if (document.body.textContent?.includes(text)) return;
    await new Promise((r) => setTimeout(r, 50));
  }
  throw new Error(`${text} never appeared. ${debugBody()}`);
}

const unverifiedUser = {
  id: "u-1",
  username: "verifyuser",
  email: "verify@oxidean.local",
  email_verified: false,
};

describe("VerifyPage browser DOM races", () => {
  it("form phase mounts InputOtp and resend round-trips without races", async () => {
    meMock.mockResolvedValue({ ok: true, data: unverifiedUser });
    verifyMock.mockResolvedValue({
      ok: false,
      error: { code: "auth.invalid_token", message: "bad" },
    });
    resendVerifyMock.mockResolvedValue({ ok: true, data: null });

    const tracker = trackDomErrors();
    try {
      await mountWithQueryClient(VerifyPage, {});
      const otp = (await waitForSelector("#verify-code")) as HTMLInputElement;
      expect(otp).toBeTruthy();

      await act(async () => {
        otp.focus();
      });

      const resend = Array.from(document.querySelectorAll("button")).find(
        (b) => b.textContent?.trim() === "Resend email",
      ) as HTMLButtonElement;
      expect(resend).toBeTruthy();
      await act(async () => {
        resend.click();
      });
      await waitForText("Verification email sent");
      expect(resendVerifyMock).toHaveBeenCalled();

      expectNoOctaneOverlayInDocument();
      tracker.expectNoDomRaces();
    } finally {
      tracker.dispose();
    }
  });

  it("anonymous mount falls back to a plain anchor for the sign-in link", async () => {
    meMock.mockResolvedValue({
      ok: false,
      error: { code: "auth.unauthenticated", message: "" },
    });

    const tracker = trackDomErrors();
    try {
      await mountWithQueryClient(VerifyPage, {});
      await waitForText("Sign in to finish verifying this email.");

      const signIn = Array.from(document.querySelectorAll("a[href]")).find((a) =>
        a.getAttribute("href")?.startsWith("/login"),
      );
      expect(signIn).toBeTruthy();
      expect(signIn?.getAttribute("href")).toContain("returnTo=");

      expectNoOctaneOverlayInDocument();
      tracker.expectNoDomRaces();
    } finally {
      tracker.dispose();
    }
  });
});
