/**
 * RESEARCH P1 / D-QH-03 — login route export/render contracts (happy-dom).
 */
import { cleanup, render, screen, waitFor } from "@octanejs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    auth: {
      me: vi.fn(async () => ({
        ok: false,
        error: { code: "auth.unauthenticated", message: "n" },
      })),
      login: vi.fn(),
      loginBegin: vi.fn(),
      providerConfig: vi.fn(async () => ({
        ok: true,
        data: { mode: "local", allow_signup: true },
      })),
    },
    twoFactor: {
      status: vi.fn(async () => ({
        ok: true,
        data: {
          totp_enabled: false,
          recovery_codes_remaining: 0,
          passkeys: [],
          challenge_pending: false,
          challenge_kind: null,
          challenge_methods: [],
          challenge_next: null,
          enroll_required: false,
        },
      })),
    },
  },
}));

import { LoginPage } from "./login";

afterEach(cleanup);

beforeEach(() => {
  window.history.pushState({}, "", "/login");
});

describe("/login Wave 0 contracts (RESEARCH P1)", () => {
  it("exports LoginPage", () => {
    expect(typeof LoginPage).toBe("function");
  });

  it("renders the identifier-first sign-in step (#130)", async () => {
    render(LoginPage);

    await waitFor(() => {
      expect(screen.getByLabelText("Email or username")).toBeInTheDocument();
      expect(screen.getByText("Create an account")).toBeInTheDocument();
    });
    // Password is a later step — the identifier step has none.
    expect(screen.queryByLabelText("Password")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Continue" })).toBeInTheDocument();
    expect(screen.getByRole("checkbox", { name: "Remember me" })).toBeInTheDocument();
  });
});
