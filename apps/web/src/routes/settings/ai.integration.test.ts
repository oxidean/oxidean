/**
 * Account /settings/ai — agent skill installs + MCP server wiring.
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

  it("happy: renders the MCP section — endpoint, CLI commands, client snippets", async () => {
    window.history.pushState({}, "", "/settings/ai");
    meMock.mockResolvedValue({ ok: true, data: user });
    renderWithQueryClient(AiSettingsPage);

    await waitFor(() => {
      expect(screen.getByRole("heading", { name: "MCP server" })).toBeInTheDocument();
    });

    const endpoint = screen.getByRole("textbox", { name: "Copy MCP endpoint URL" });
    expect(endpoint).toHaveValue("https://forge.example/api/mcp");
    const install = screen.getByRole("textbox", { name: "Copy MCP install command" });
    expect(install).toHaveValue("ox mcp install");
    const detected = screen.getByRole("textbox", { name: "Copy detected MCP install command" });
    expect(detected).toHaveValue("ox mcp install --detected --project");
    const status = screen.getByRole("textbox", { name: "Copy MCP status command" });
    expect(status).toHaveValue("ox mcp status");

    const claude = screen.getByRole("textbox", { name: "Copy Claude Code MCP add command" });
    expect(claude).toHaveValue(
      'claude mcp add --transport http oxidean https://forge.example/api/mcp --header "Authorization: Bearer <pat>"',
    );
    const cursor = screen.getByRole("textbox", { name: "Copy Cursor MCP config JSON" });
    expect(cursor).toHaveValue(
      '{"mcpServers":{"oxidean":{"url":"https://forge.example/api/mcp","headers":{"Authorization":"Bearer <pat>"}}}}',
    );
    const vscode = screen.getByRole("textbox", { name: "Copy VS Code MCP config JSON" });
    expect(vscode).toHaveValue(
      '{"servers":{"oxidean":{"type":"http","url":"https://forge.example/api/mcp","headers":{"Authorization":"Bearer <pat>"}}}}',
    );
    const remote = screen.getByRole("textbox", { name: "Copy mcp-remote bridge command" });
    expect(remote).toHaveValue(
      'npx mcp-remote https://forge.example/api/mcp --header "Authorization: Bearer <pat>"',
    );

    // Guidance links — PAT minting and the admin gate. ("Personal access
    // tokens" also appears in the settings nav, so match by href.)
    expect(
      screen.getAllByRole("link").some((l) => l.getAttribute("href") === "/settings/tokens"),
    ).toBe(true);
    expect(screen.getAllByRole("link").some((l) => l.getAttribute("href") === "/admin/mcp")).toBe(
      true,
    );
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
