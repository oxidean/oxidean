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
      providerConfig: vi.fn(async () => ({
        ok: true,
        data: { mode: "local", allow_signup: true },
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

  it("renders local sign-in form fields", async () => {
    render(LoginPage);

    await waitFor(() => {
      expect(screen.getByLabelText("Email or username")).toBeInTheDocument();
      expect(screen.getByText("Create an account")).toBeInTheDocument();
    });
    expect(screen.getByLabelText("Password")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Sign in" })).toBeInTheDocument();
  });
});
