/**
 * Account /settings/cli — ox install, login, and self-update commands.
 */
import { cleanup, screen, waitFor } from "@octanejs/testing-library";
import { afterEach, describe, expect, it, vi } from "vitest";
import { renderWithQueryClient } from "@/test/render-with-query";

const meMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    auth: {
      me: (...args: unknown[]) => meMock(...args),
    },
  },
}));

vi.mock("@/lib/public-origin", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/lib/public-origin")>();
  return {
    ...actual,
    resolvePublicOriginClient: () => "https://forge.example",
  };
});

import { CliSettingsPage } from "./cli";

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
};

afterEach(cleanup);

describe("/settings/cli", () => {
  it("happy: renders origin-baked install and login commands", async () => {
    window.history.pushState({}, "", "/settings/cli");
    meMock.mockResolvedValue({ ok: true, data: user });
    renderWithQueryClient(CliSettingsPage);

    await waitFor(() => {
      expect(screen.getByRole("heading", { name: "Command line" })).toBeInTheDocument();
    });
    expect(screen.getByTestId("settings-cli-page")).toBeInTheDocument();

    const install = screen.getByRole("textbox", {
      name: "Copy install command",
    });
    expect(install).toHaveValue("curl -fsSL https://forge.example/cli/install.sh | sh");
    const login = screen.getByRole("textbox", { name: "Copy login command" });
    expect(login).toHaveValue("ox auth login --instance https://forge.example --token <pat>");
    expect(screen.getByRole("button", { name: "Copy install command" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Copy login command" })).toBeInTheDocument();

    const nav = screen.getByRole("navigation", { name: "Account settings" });
    expect(nav.querySelector('a[href="/settings/cli"]')).toBeTruthy();
  });

  it("unhappy: unauthenticated sessions redirect to login", async () => {
    window.history.pushState({}, "", "/settings/cli");
    meMock.mockResolvedValue({
      ok: false,
      error: { code: "auth.unauthenticated", message: "n" },
    });
    renderWithQueryClient(CliSettingsPage);

    await waitFor(() => {
      expect(window.location.pathname).toBe("/login");
      expect(window.location.search).toBe("?returnTo=/settings/cli");
    });
  });
});
