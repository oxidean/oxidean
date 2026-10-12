/**
 * Account /settings/ai — agent skills install + upgrade commands.
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

import { AiSettingsPage } from "./ai";

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

describe("/settings/ai", () => {
  it("happy: renders skill install/upgrade commands and nav link", async () => {
    window.history.pushState({}, "", "/settings/ai");
    meMock.mockResolvedValue({ ok: true, data: user });
    renderWithQueryClient(AiSettingsPage);

    await waitFor(() => {
      expect(screen.getByRole("heading", { name: "AI agents" })).toBeInTheDocument();
    });
    expect(screen.getByTestId("settings-ai-page")).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Skills" })).toBeInTheDocument();

    const interactive = screen.getByRole("textbox", {
      name: "Copy interactive skill install command",
    });
    expect(interactive).toHaveValue("ox skill install");
    const detected = screen.getByRole("textbox", {
      name: "Copy detected skill install command",
    });
    expect(detected).toHaveValue("ox skill install --detected --project");
    const global = screen.getByRole("textbox", {
      name: "Copy global skill install command",
    });
    expect(global).toHaveValue("ox skill install --detected --global");
    const upgrade = screen.getByRole("textbox", { name: "Copy skill upgrade command" });
    expect(upgrade).toHaveValue("ox skill install --upgrade");
    const fetchCmd = screen.getByRole("textbox", { name: "Copy skill download command" });
    expect(fetchCmd).toHaveValue("curl -fsSL https://forge.example/cli/skill.md");
    expect(
      screen.getByRole("button", { name: "Copy detected skill install command" }),
    ).toBeInTheDocument();

    const nav = screen.getByRole("navigation", { name: "Account settings" });
    expect(nav.querySelector('a[href="/settings/ai"]')).toBeTruthy();
  });

  it("unhappy: unauthenticated sessions redirect to login", async () => {
    window.history.pushState({}, "", "/settings/ai");
    meMock.mockResolvedValue({
      ok: false,
      error: { code: "auth.unauthenticated", message: "n" },
    });
    renderWithQueryClient(AiSettingsPage);

    await waitFor(() => {
      expect(window.location.pathname).toBe("/login");
      expect(window.location.search).toBe("?returnTo=/settings/ai");
    });
  });
});
