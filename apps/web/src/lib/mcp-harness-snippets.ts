/**
 * Per-harness MCP config blocks for the `/settings/ai` page. Shapes mirror
 * `entry_json` / `codex_section` in `crates/oxidean-cli/src/mcp.rs` — keep the
 * two in sync when the registry's wire format changes. The bearer is always
 * the `<pat>` placeholder; real tokens never render on this page.
 *
 * Each harness carries two variants: `token` (static `Authorization: Bearer`
 * header — PAT or OAuth access token) and `oauth` (no credential in the file —
 * the client discovers the instance's OAuth endpoints via the RFC 9728
 * `resource_metadata` pointer on the 401 challenge and runs the browser flow
 * itself).
 */

export type McpAuthVariant = "token" | "oauth";

type McpHarnessVariantBlock = {
  /** Full file content to paste. */
  code: string;
  /** Extra copyable command rendered under the block (e.g. `claude mcp add`). */
  command?: { label: string; cmd: string; aria: string };
  /** Short note rendered under the block. */
  note?: string;
};

export type McpHarnessSnippet = {
  /** Harness id — same as `ox mcp --harness`. */
  id: string;
  label: string;
  /** File the block belongs in (project-scoped path where the harness has one). */
  file: string;
  token: McpHarnessVariantBlock;
  oauth: McpHarnessVariantBlock;
};

const json = (v: unknown) => `${JSON.stringify(v, null, 2)}\n`;

export function mcpHarnessSnippets(endpoint: string): McpHarnessSnippet[] {
  const headers = { Authorization: "Bearer <pat>" };
  const pat = "Bearer <pat>";
  return [
    {
      id: "claude",
      label: "Claude Code",
      file: ".mcp.json",
      token: {
        code: json({
          mcpServers: {
            oxidean: { type: "http", url: endpoint, headers },
          },
        }),
        command: {
          label: "Claude Code also has its own add command:",
          cmd: `claude mcp add --transport http oxidean ${endpoint} --header "Authorization: ${pat}"`,
          aria: "Copy Claude Code MCP add command",
        },
      },
      oauth: {
        code: json({
          mcpServers: { oxidean: { type: "http", url: endpoint } },
        }),
        command: {
          label: "Claude Code also has its own add command:",
          cmd: `claude mcp add --transport http oxidean ${endpoint}`,
          aria: "Copy Claude Code MCP add command",
        },
        note: "No credential in the file — Claude Code discovers the OAuth endpoints and runs the browser flow on first connect (/mcp to check status).",
      },
    },
    {
      id: "cursor",
      label: "Cursor",
      file: ".cursor/mcp.json",
      token: {
        code: json({
          mcpServers: { oxidean: { url: endpoint, headers } },
        }),
      },
      oauth: {
        code: json({
          mcpServers: { oxidean: { url: endpoint } },
        }),
        note: "Cursor runs the OAuth flow in a browser the first time the server connects.",
      },
    },
    {
      id: "vscode",
      label: "VS Code",
      file: ".vscode/mcp.json",
      token: {
        code: json({
          servers: {
            oxidean: { type: "http", url: endpoint, headers },
          },
        }),
      },
      oauth: {
        code: json({
          servers: { oxidean: { type: "http", url: endpoint } },
        }),
        note: "VS Code prompts to authenticate the server on first use.",
      },
    },
    {
      id: "windsurf",
      label: "Windsurf",
      file: "~/.codeium/windsurf/mcp_config.json",
      token: {
        code: json({
          mcpServers: { oxidean: { serverUrl: endpoint, headers } },
        }),
      },
      oauth: {
        code: json({
          mcpServers: { oxidean: { serverUrl: endpoint } },
        }),
        note: "Windsurf negotiates OAuth on first connect — use the Token config if your version predates MCP OAuth support.",
      },
    },
    {
      id: "gemini",
      label: "Gemini CLI",
      file: ".gemini/settings.json",
      token: {
        code: json({
          mcpServers: { oxidean: { httpUrl: endpoint, headers } },
        }),
      },
      oauth: {
        code: json({
          mcpServers: {
            oxidean: { httpUrl: endpoint, oauth: { enabled: true } },
          },
        }),
        note: "Run /mcp auth inside Gemini CLI to complete the browser flow.",
      },
    },
    {
      id: "codex",
      label: "Codex",
      file: ".codex/config.toml",
      token: {
        code: `[mcp_servers.oxidean]\nurl = "${endpoint}"\nbearer_token_env_var = "OXIDEAN_TOKEN"\n`,
        note: "Codex reads the token from the OXIDEAN_TOKEN environment variable — export it, don't paste it into the file.",
      },
      oauth: {
        code: `[mcp_servers.oxidean]\nurl = "${endpoint}"\n`,
        note: "Complete the browser login with `codex mcp login oxidean` after saving the config.",
      },
    },
    {
      id: "kiro",
      label: "Kiro",
      file: ".kiro/settings/mcp.json",
      token: {
        code: json({
          mcpServers: { oxidean: { url: endpoint, headers } },
        }),
      },
      oauth: {
        code: json({
          mcpServers: { oxidean: { url: endpoint } },
        }),
        note: "Kiro negotiates OAuth on first connect — use the Token config if your version predates MCP OAuth support.",
      },
    },
    {
      id: "opencode",
      label: "OpenCode",
      file: "opencode.json",
      token: {
        code: json({
          mcp: {
            oxidean: { type: "remote", url: endpoint, enabled: true, headers },
          },
        }),
      },
      oauth: {
        code: json({
          mcp: {
            oxidean: { type: "remote", url: endpoint, enabled: true, oauth: {} },
          },
        }),
        note: "OpenCode resolves the OAuth endpoints automatically and opens the browser flow on first use.",
      },
    },
    {
      id: "zed",
      label: "Zed",
      file: ".zed/settings.json",
      token: {
        code: json({
          context_servers: {
            oxidean: {
              command: {
                path: "npx",
                args: ["mcp-remote", endpoint, "--header", `Authorization: ${pat}`],
              },
            },
          },
        }),
        note: "Stdio-only — bridges to the HTTP endpoint through mcp-remote.",
      },
      oauth: {
        code: json({
          context_servers: {
            oxidean: {
              command: { path: "npx", args: ["mcp-remote", endpoint] },
            },
          },
        }),
        note: "Stdio-only — mcp-remote runs the OAuth flow itself and caches the token.",
      },
    },
    {
      id: "generic",
      label: "Any `mcpServers` client",
      file: "mcp.json",
      token: {
        code: json({
          mcpServers: { oxidean: { url: endpoint, headers } },
        }),
        command: {
          label: "Stdio-only clients — bridge through mcp-remote:",
          cmd: `npx mcp-remote ${endpoint} --header "Authorization: ${pat}"`,
          aria: "Copy mcp-remote bridge command",
        },
      },
      oauth: {
        code: json({
          mcpServers: { oxidean: { url: endpoint } },
        }),
        command: {
          label: "Stdio-only clients — bridge through mcp-remote:",
          cmd: `npx mcp-remote ${endpoint}`,
          aria: "Copy mcp-remote bridge command",
        },
        note: "Without a static header the client (or mcp-remote) discovers OAuth from the endpoint's 401 challenge.",
      },
    },
  ];
}
