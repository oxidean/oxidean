/**
 * Account /settings/ai — agent skill installs + MCP server wiring.
 */
import { cleanup, screen, waitFor } from "@octanejs/testing-library";
import { afterEach, describe, expect, it, vi } from "vitest";
import { renderWithQueryClient } from "@/test/render-with-query";
import { mcpHarnessSnippets } from "@/lib/mcp-harness-snippets";
import { getHighlighter } from "@/lib/highlight";

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
    const upgrade = screen.getByRole("textbox", { name: "Copy MCP upgrade command" });
    expect(upgrade).toHaveValue("ox mcp install --upgrade");
    const status = screen.getByRole("textbox", { name: "Copy MCP status command" });
    expect(status).toHaveValue("ox mcp status");

    // Per-harness picker — defaults to Claude Code, renders a real JSON block
    // (never a live token) and names the file it belongs in.
    expect(screen.getByTestId("mcp-harness-select")).toBeTruthy();
    const block = screen.getByTestId("mcp-snippet-block");
    expect(block.getAttribute("data-harness")).toBe("claude");
    expect(block.textContent).toContain('"mcpServers"');
    expect(block.textContent).toContain('"oxidean"');
    expect(block.textContent).toContain('"https://forge.example/api/mcp"');
    expect(block.textContent).toContain('"Bearer <pat>"');
    expect(screen.getByTestId("mcp-snippet-file").textContent).toContain(".mcp.json");
    expect(screen.getByRole("button", { name: "Copy Claude Code config" })).toBeTruthy();

    // The Claude add command is folded into the claude/token variant block —
    // same aria label, still no real token.
    expect(screen.getByTestId("mcp-snippet-command-label").textContent).toContain(
      "Claude Code also has its own add command",
    );
    const claude = screen.getByRole("textbox", { name: "Copy Claude Code MCP add command" });
    expect(claude).toHaveValue(
      'claude mcp add --transport http oxidean https://forge.example/api/mcp --header "Authorization: Bearer <pat>"',
    );

    // Token/OAuth mode toggle — OAuth variant drops the static header and
    // shows the per-harness note. Warm the highlighter first so the swap
    // exercises the synchronous (no unstyled frame) path.
    await getHighlighter();
    const oauth = screen.getByTestId("mcp-auth-oauth");
    oauth.click();
    await waitFor(() => {
      expect(screen.getByTestId("mcp-snippet-block").getAttribute("data-auth")).toBe("oauth");
    });
    expect(screen.getByTestId("mcp-snippet-block").textContent).not.toContain("Bearer");
    // The swap renders styled markup in the same commit — plainHtml has no
    // data-language marker, so this would catch an unstyled flash frame.
    expect(
      screen.getByTestId("mcp-snippet-block").querySelector("pre[data-language]"),
    ).toBeTruthy();
    expect(screen.getByTestId("mcp-snippet-note").textContent).toContain("OAuth");
    expect(screen.getByRole("textbox", { name: "Copy Claude Code MCP add command" })).toHaveValue(
      "claude mcp add --transport http oxidean https://forge.example/api/mcp",
    );

    // mcp-remote bridging lives under the generic tab (both auth modes).
    const generic = mcpHarnessSnippets("https://forge.example/api/mcp").find(
      (s) => s.id === "generic",
    );
    expect(generic?.token.command?.cmd).toBe(
      'npx mcp-remote https://forge.example/api/mcp --header "Authorization: Bearer <pat>"',
    );
    expect(generic?.oauth.command?.cmd).toBe("npx mcp-remote https://forge.example/api/mcp");

    // Guidance links — PAT minting and the admin gate. ("Personal access
    // tokens" also appears in the settings nav, so match by href.)
    expect(
      screen.getAllByRole("link").some((l) => l.getAttribute("href") === "/settings/tokens"),
    ).toBe(true);
    expect(screen.getAllByRole("link").some((l) => l.getAttribute("href") === "/admin/mcp")).toBe(
      true,
    );
  });

  it("happy: SSR-highlighted default snippet renders styled on first paint", async () => {
    window.history.pushState({}, "", "/settings/ai");
    meMock.mockResolvedValue({ ok: true, data: user });
    renderWithQueryClient(AiSettingsPage, {
      props: {
        mcpInitialHtml:
          '<pre data-language="json" class="shiki"><code><span class="line">ssr-markup</span></code></pre>',
      },
    });

    await waitFor(() => {
      expect(screen.getByRole("heading", { name: "MCP server" })).toBeInTheDocument();
    });
    const block = screen.getByTestId("mcp-snippet-block");
    expect(block.getAttribute("data-harness")).toBe("claude");
    expect(block.querySelector("pre[data-language]")).toBeTruthy();
    expect(block.textContent).toContain("ssr-markup");
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
