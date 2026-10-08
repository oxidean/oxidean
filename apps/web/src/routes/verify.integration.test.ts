/**
 * RESEARCH P1 / D-QH-03 — verify route export/render contracts (happy-dom).
 */
import { cleanup, render, screen, waitFor } from "@octanejs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const meMock = vi.fn();
const verifyMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    auth: {
      me: (...args: unknown[]) => meMock(...args),
      verify: (...args: unknown[]) => verifyMock(...args),
      requestVerify: vi.fn(),
      resendVerify: vi.fn(),
    },
  },
}));

import { VerifyPage } from "./verify";

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
  window.history.replaceState({}, "", "/verify");
});

beforeEach(() => {
  window.history.replaceState({}, "", "/verify");
  meMock.mockResolvedValue({
    ok: false,
    error: { code: "auth.unauthenticated", message: "n" },
  });
  verifyMock.mockReset();
});

describe("/verify Wave 0 contracts (RESEARCH P1)", () => {
  it("exports VerifyPage", () => {
    expect(typeof VerifyPage).toBe("function");
  });

  it("prompts sign-in when anonymous", async () => {
    render(VerifyPage);

    await waitFor(() => {
      expect(screen.getByText(/Sign in to finish verifying this email/i)).toBeInTheDocument();
    });
    expect(screen.getByRole("button", { name: "Sign in" })).toBeInTheDocument();
  });

  it("redeems magic token when primary already verified", async () => {
    window.history.replaceState({}, "", "/verify?token=secondary-magic");
    meMock.mockResolvedValue({
      ok: true,
      data: {
        id: "u1",
        email: "prim@ex.com",
        username: "user",
        display_name: "U",
        bio: "",
        avatar_url: null,
        role: "user",
        profile_incomplete: false,
        email_verified: true,
        must_change_credentials: false,
      },
    });
    verifyMock.mockResolvedValue({
      ok: true,
      data: {
        id: "u1",
        email: "prim@ex.com",
        username: "user",
        display_name: "U",
        bio: "",
        avatar_url: null,
        role: "user",
        profile_incomplete: false,
        email_verified: true,
        must_change_credentials: false,
      },
    });
    const assign = vi.fn();
    const originalLocation = window.location;
    Object.defineProperty(window, "location", {
      configurable: true,
      value: {
        href: "http://localhost/verify?token=secondary-magic",
        search: "?token=secondary-magic",
        pathname: "/verify",
        assign,
        replace: vi.fn(),
        reload: vi.fn(),
      },
    });

    render(VerifyPage);

    await waitFor(() => {
      expect(verifyMock).toHaveBeenCalledWith({ token: "secondary-magic" });
    });
    expect(screen.queryByText(/Your email is already verified/i)).not.toBeInTheDocument();
    await waitFor(() => {
      expect(assign).toHaveBeenCalledWith("/settings/profile");
    });

    Object.defineProperty(window, "location", {
      configurable: true,
      value: originalLocation,
    });
  });

  it("shows already verified when primary verified and no token", async () => {
    meMock.mockResolvedValue({
      ok: true,
      data: {
        id: "u1",
        email: "prim@ex.com",
        username: "user",
        display_name: "U",
        bio: "",
        avatar_url: null,
        role: "user",
        profile_incomplete: false,
        email_verified: true,
        must_change_credentials: false,
      },
    });

    render(VerifyPage);

    await waitFor(() => {
      expect(screen.getByText(/Your email is already verified/i)).toBeInTheDocument();
    });
    expect(verifyMock).not.toHaveBeenCalled();
  });
});
