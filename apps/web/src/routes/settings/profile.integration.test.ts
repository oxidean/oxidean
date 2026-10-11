/**
 * Account page (`/settings/profile`): profile fields + email addresses section.
 */
import { cleanup, screen, waitFor } from "@octanejs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderWithQueryClient } from "@/test/render-with-query";

const getProfileMock = vi.fn();
const listMock = vi.fn();
const meMock = vi.fn();
const verifyMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    auth: {
      me: (...args: unknown[]) => meMock(...args),
      verify: (...args: unknown[]) => verifyMock(...args),
      logout: vi.fn(),
      logoutAll: vi.fn(),
    },
    user: {
      getProfile: (...args: unknown[]) => getProfileMock(...args),
      updateProfile: vi.fn(),
    },
    email: {
      list: (...args: unknown[]) => listMock(...args),
      add: vi.fn(),
      remove: vi.fn(),
      setPrimary: vi.fn(),
      resendVerify: vi.fn(),
    },
  },
}));

type LoaderShape =
  | { kind: "unauthenticated" }
  | { kind: "error"; message: string }
  | {
      kind: "ready";
      user: {
        id: string;
        email: string;
        username: string;
        display_name: string;
        bio: string;
        avatar_url: null;
        role: string;
        profile_incomplete: boolean;
        email_verified: boolean;
        must_change_credentials: boolean;
        default_branch: string;
      };
      emails: unknown[];
    };

function applyLoader(d: LoaderShape | undefined) {
  if (d === undefined) {
    getProfileMock.mockReturnValue(new Promise(() => {}));
    return;
  }
  if (d.kind === "unauthenticated") {
    getProfileMock.mockResolvedValue({
      ok: false,
      error: { code: "auth.unauthenticated", message: "n" },
    });
    return;
  }
  if (d.kind === "error") {
    getProfileMock.mockResolvedValue({ ok: false, error: { code: "x", message: d.message } });
    return;
  }
  getProfileMock.mockResolvedValue({ ok: true, data: d.user });
  listMock.mockResolvedValue({ ok: true, data: d.emails });
}

import { ProfilePage } from "./profile";

const readyUser = {
  id: "u1",
  email: "ada@example.com",
  username: "profileuser",
  display_name: "Profile User",
  bio: "",
  avatar_url: null as null,
  role: "user",
  profile_incomplete: false,
  email_verified: true,
  must_change_credentials: false,
  default_branch: "main",
};

const readyEmails = [
  {
    id: "e1",
    email: "ada@example.com",
    is_primary: true,
    verified: true,
    verified_at: "2026-01-01T00:00:00Z",
    created_at: "2026-01-01T00:00:00Z",
  },
  {
    id: "e2",
    email: "work@example.com",
    is_primary: false,
    verified: false,
    verified_at: null,
    created_at: "2026-01-02T00:00:00Z",
  },
];

beforeEach(() => {
  window.history.pushState({}, "", "/settings/profile");
  getProfileMock.mockReset();
  listMock.mockReset();
  meMock.mockReset();
  verifyMock.mockReset();
  applyLoader({ kind: "ready", user: readyUser, emails: readyEmails });
  getProfileMock.mockResolvedValue({ ok: true, data: readyUser });
  listMock.mockResolvedValue({ ok: true, data: readyEmails });
  meMock.mockResolvedValue({ ok: true, data: readyUser });
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("/settings/profile (Account)", () => {
  it("exports ProfilePage", () => {
    expect(typeof ProfilePage).toBe("function");
  });

  it("renders Account heading, profile form, and emails section", async () => {
    renderWithQueryClient(ProfilePage);

    await waitFor(() => {
      expect(screen.getByRole("heading", { name: "Account" })).toBeInTheDocument();
    });
    expect(screen.getByDisplayValue("profileuser")).toBeInTheDocument();
    expect(screen.getByTestId("settings-account-emails")).toBeInTheDocument();
    expect(screen.getAllByText("ada@example.com").length).toBeGreaterThanOrEqual(1);
    expect(screen.getByText("Primary")).toBeInTheDocument();
    expect(screen.getByText("Primary address")).toBeInTheDocument();
    expect(screen.getByTestId("email-primary-badge")).toBeInTheDocument();
    expect(screen.getByText(/Primary email/)).toBeInTheDocument();
    expect(screen.getByText("Verified")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Make primary" })).not.toBeInTheDocument();
    const add = screen.getAllByRole("button", { name: /Add email address/i })[0]!;
    expect(add).toBeInTheDocument();
    expect(add).not.toBeDisabled();
    // Theme / default branch / logout live on General.
    expect(screen.queryByLabelText("Default branch name")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Log out" })).not.toBeInTheDocument();
    expect(screen.queryByRole("listbox", { name: "Theme" })).not.toBeInTheDocument();
  });

  it("settings nav labels Account under Settings header", async () => {
    const { container } = renderWithQueryClient(ProfilePage);

    await waitFor(() => {
      expect(container.querySelector('nav[aria-label="Account settings"]')).toBeTruthy();
    });
    const nav = container.querySelector('nav[aria-label="Account settings"]')!;
    expect(nav.querySelector("p")?.textContent).toBe("Settings");
    const account = nav.querySelector('a[href="/settings/profile"]');
    expect(account?.textContent).toBe("Account");
    expect(account?.getAttribute("aria-current")).toBe("page");
    expect(nav.querySelector('a[href="/settings/emails"]')).toBeNull();
  });

  it("Enter code opens OTP panel for unverified secondary", async () => {
    renderWithQueryClient(ProfilePage);

    await waitFor(() => {
      expect(screen.getByText("work@example.com")).toBeInTheDocument();
    });
    expect(screen.getByText("Unverified")).toBeInTheDocument();
    screen.getByRole("button", { name: "Enter code" }).click();

    await waitFor(() => {
      expect(screen.getByTestId("settings-email-verify-code")).toBeInTheDocument();
    });
    const panel = screen.getByTestId("settings-email-verify-code");
    expect(panel.textContent).toMatch(/We sent an 8-digit code to/);
    expect(panel.querySelector("#account-email-verify-code")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Verify email" })).toBeDisabled();
  });
});
