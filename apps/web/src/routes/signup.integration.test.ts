import { cleanup, render, screen, waitFor } from "@octanejs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@/lib/bootstrap", () => ({
  redirectIfNeedsSetup: vi.fn(async () => false),
}));

const providerConfigMock = vi.fn();
const meMock = vi.fn();
const signupMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    auth: {
      me: (...args: unknown[]) => meMock(...args),
      providerConfig: (...args: unknown[]) => providerConfigMock(...args),
      signup: (...args: unknown[]) => signupMock(...args),
    },
  },
}));

import { SignupPage } from "./signup";

afterEach(cleanup);

beforeEach(() => {
  window.history.pushState({}, "", "/signup");
  providerConfigMock.mockReset();
  meMock.mockReset();
  signupMock.mockReset();
  meMock.mockResolvedValue({
    ok: false,
    error: { code: "auth.unauthenticated", message: "n" },
  });
  providerConfigMock.mockResolvedValue({
    ok: true,
    data: { mode: "local", allow_signup: true },
  });
});

describe("/signup closed-signup gate (D-06)", () => {
  it("renders a closed-registration state instead of the form when allow_signup is false", async () => {
    providerConfigMock.mockResolvedValue({
      ok: true,
      data: { mode: "local", allow_signup: false },
    });
    render(SignupPage);
    await waitFor(() => {
      expect(screen.queryByLabelText("Email")).not.toBeInTheDocument();
      expect(document.body.textContent).toMatch(/closed/i);
    });
  });

  it("renders the form when allow_signup is true", async () => {
    render(SignupPage);
    await waitFor(() => {
      expect(screen.getByLabelText("Email")).toBeInTheDocument();
    });
  });
});

describe("SignupPage AUTH-05 (no invite)", () => {
  it("local signup form has email/username/password only — no invite fields", async () => {
    render(SignupPage);

    await waitFor(() => {
      expect(screen.getByLabelText("Email")).toBeInTheDocument();
    });
    expect(screen.getByLabelText("Username")).toBeInTheDocument();
    expect(screen.getByLabelText("Password")).toBeInTheDocument();
    expect(screen.getByLabelText("Confirm password")).toBeInTheDocument();

    expect(screen.queryByLabelText(/invite/i)).not.toBeInTheDocument();
    expect(screen.queryByPlaceholderText(/invite/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/invite/i)).not.toBeInTheDocument();
    expect(screen.queryByRole("textbox", { name: /invite/i })).not.toBeInTheDocument();
  });
});
