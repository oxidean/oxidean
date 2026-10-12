/**
 * Per-harness MCP config blocks for the `/settings/ai` page. Shapes mirror
 * `entry_json` / `codex_section` in `crates/oxidean-cli/src/mcp.rs` — keep the
 * two in sync when the registry's wire format changes. The bearer is always
 * the `<pat>` placeholder; real tokens never render on this page.
 */

export type McpHarnessSnippet = {
  /** Harness id — same as `ox mcp --harness`. */
  id: string;
  label: string;
  /** File the block belongs in (project-scoped path where the harness has one). */
  file: string;
  /** Full file content to paste. */
  code: string;
};

const json = (v: unknown) => `${JSON.stringify(v, null, 2)}\n`;

export function mcpHarnessSnippets(endpoint: string): McpHarnessSnippet[] {
  const headers = { Authorization: "Bearer <pat>" };
  return [
    {
      id: "claude",
      label: "Claude Code",
      file: ".mcp.json",
      code: json({
        mcpServers: {
          oxidean: { type: "http", url: endpoint, headers },
        },
      }),
    },
    {
      id: "cursor",
      label: "Cursor",
      file: ".cursor/mcp.json",
      code: json({
        mcpServers: { oxidean: { url: endpoint, headers } },
      }),
    },
    {
      id: "vscode",
      label: "VS Code",
      file: ".vscode/mcp.json",
      code: json({
        servers: {
          oxidean: { type: "http", url: endpoint, headers },
        },
      }),
    },
    {
      id: "windsurf",
      label: "Windsurf",
      file: "~/.codeium/windsurf/mcp_config.json",
      code: json({
        mcpServers: { oxidean: { serverUrl: endpoint, headers } },
      }),
    },
    {
      id: "gemini",
      label: "Gemini CLI",
      file: ".gemini/settings.json",
      code: json({
        mcpServers: { oxidean: { httpUrl: endpoint, headers } },
      }),
    },
    {
      id: "codex",
      label: "Codex",
      file: ".codex/config.toml",
      code: `[mcp_servers.oxidean]\nurl = "${endpoint}"\nbearer_token_env_var = "OXIDEAN_TOKEN"\n`,
    },
    {
      id: "kiro",
      label: "Kiro",
      file: ".kiro/settings/mcp.json",
      code: json({
        mcpServers: { oxidean: { url: endpoint, headers } },
      }),
    },
    {
      id: "opencode",
      label: "OpenCode",
      file: "opencode.json",
      code: json({
        mcp: {
          oxidean: { type: "remote", url: endpoint, enabled: true, headers },
        },
      }),
    },
    {
      id: "zed",
      label: "Zed",
      file: ".zed/settings.json",
      code: json({
        context_servers: {
          oxidean: {
            command: {
              path: "npx",
              args: ["mcp-remote", endpoint, "--header", "Authorization: Bearer <pat>"],
            },
          },
        },
      }),
    },
    {
      id: "generic",
      label: "Any `mcpServers` client",
      file: "mcp.json",
      code: json({
        mcpServers: { oxidean: { url: endpoint, headers } },
      }),
    },
  ];
}
