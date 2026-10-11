/**
 * Account /settings/general — theme, default branch, logout controls.
 */
import { cleanup, screen, waitFor } from "@octanejs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderWithQueryClient } from "@/test/render-with-query";

const getProfileMock = vi.fn();
const updateProfileMock = vi.fn();
const logoutMock = vi.fn();
const logoutAllMock = vi.fn();
const meMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    auth: {
      me: (...args: unknown[]) => meMock(...args),
      logout: (...args: unknown[]) => logoutMock(...args),
      logoutAll: (...args: unknown[]) => logoutAllMock(...args),
    },
    user: {
      getProfile: (...args: unknown[]) => getProfileMock(...args),
      updateProfile: (...args: unknown[]) => updateProfileMock(...args),
    },
  },
}));

import { GeneralPage } from "./general";

const readyUser = {
  id: "u1",
  email: "general@oxidean.local",
  username: "generaluser",
  display_name: "General User",
  bio: "",
  avatar_url: null as null,
  role: "user",
  profile_incomplete: false,
  email_verified: true,
  must_change_credentials: false,
  default_branch: "main",
};

afterEach(cleanup);

beforeEach(() => {
  window.history.pushState({}, "", "/settings/general");
  meMock.mockReset();
  logoutMock.mockReset();
  logoutAllMock.mockReset();
  updateProfileMock.mockReset();
  getProfileMock.mockReset();
  meMock.mockResolvedValue({ ok: true, data: readyUser });
  getProfileMock.mockResolvedValue({ ok: true, data: readyUser });
});

describe("/settings/general", () => {
  it("keys the page query on settingsGeneral", async () => {
    const src = await import("./general.tsrx?raw").then((m) => String(m.default));
    expect(src).toContain('"settingsGeneral"');
  });

  it("redirects anonymous sessions to login", async () => {
    getProfileMock.mockResolvedValue({
      ok: false,
      error: { code: "auth.unauthenticated", message: "sign in required" },
    });
    const assign = vi.spyOn(window.location, "assign").mockImplementation(() => {});
    renderWithQueryClient(GeneralPage);
    await waitFor(() => {
      expect(assign).toHaveBeenCalledWith("/login?returnTo=/settings/general");
    });
    assign.mockRestore();
  });

  it("happy: renders theme, default branch, and logout controls", async () => {
    renderWithQueryClient(GeneralPage);

    await waitFor(() => {
      expect(screen.getByRole("heading", { name: "General" })).toBeInTheDocument();
    });
    expect(screen.getByTestId("settings-general-page")).toBeInTheDocument();
    expect(screen.getByRole("listbox", { name: "Theme" })).toBeInTheDocument();
    expect(screen.getByLabelText("Default branch name")).toHaveValue("main");
    expect(screen.getByRole("button", { name: "Log out" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Log out all devices" })).toBeInTheDocument();

    const nav = screen.getByRole("navigation", { name: "Account settings" });
    expect(nav.querySelector("p")?.textContent).toBe("Settings");
    expect(nav.querySelector('a[href="/settings/general"]')).toBeTruthy();
    const account = nav.querySelector('a[href="/settings/profile"]');
    expect(account?.textContent).toBe("Account");
    expect(nav.querySelector('a[href="/settings/emails"]')).toBeNull();
  });

  it("unhappy: shows loader error message", async () => {
    getProfileMock.mockResolvedValue({
      ok: false,
      error: { code: "internal", message: "Could not load settings." },
    });
    renderWithQueryClient(GeneralPage);

    await waitFor(() => {
      expect(screen.getByText("Could not load settings.")).toBeInTheDocument();
    });
    expect(screen.queryByTestId("settings-general-page")).not.toBeInTheDocument();
  });

  it("edge: opens logout-all confirm dialog and can dismiss", async () => {
    renderWithQueryClient(GeneralPage);

    await waitFor(() => {
      expect(screen.getByRole("button", { name: "Log out all devices" })).toBeInTheDocument();
    });
    screen.getByRole("button", { name: "Log out all devices" }).click();
    await waitFor(() => {
      expect(screen.getByRole("dialog")).toBeInTheDocument();
    });
    screen.getByRole("button", { name: "Stay signed in" }).click();
    await waitFor(() => {
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    });
  });
});
