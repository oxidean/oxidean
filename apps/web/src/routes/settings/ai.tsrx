import { useQuery } from "@octanejs/tanstack-query";
import { AppPage } from "@/components/app-page";
import { AccountSettingsLayout } from "@/components/settings/settings-shell";
import { CopyCommand } from "@/components/settings/copy-command";
import { McpHarnessBlocks } from "@/components/settings/mcp-harness-blocks";
import { AppLink } from "@/components/ui/app-link";
import { PageHeader } from "@/components/ui/page-header";
import { usePageRedirect } from "@/lib/nav.tsrx";
import { resolvePublicOriginClient } from "@/lib/public-origin";
import { authSessionQueryOptions } from "@/lib/session-queries";
import type { HighlightTheme } from "@/lib/highlight";

/** `/settings/ai` — agent-facing integrations: ox skill installs + MCP wiring. */
export function AiSettingsPage(props: {
  /** SSR-highlighted default MCP snippet (astro frontmatter) — first paint styled. */
  mcpInitialHtml?: string | null;
  mcpInitialTheme?: HighlightTheme | null;
}) @{
  <AppPage>
    <AiSettingsContent
      mcpInitialHtml={props.mcpInitialHtml ?? null}
      mcpInitialTheme={props.mcpInitialTheme ?? null}
    />
  </AppPage>;
}

function AiSettingsContent(props: {
  mcpInitialHtml: string | null;
  mcpInitialTheme: HighlightTheme | null;
}) @{
  const meQ = useQuery(authSessionQueryOptions());
  // Auth gate (formerly loader): anonymous → login.
  usePageRedirect(meQ.data === null || meQ.isError ? "/login?returnTo=/settings/ai" : null);

  const origin = resolvePublicOriginClient();
  const skillInstallCmd = "ox skill install";
  const skillDetectedCmd = "ox skill install --detected --project";
  const skillGlobalCmd = "ox skill install --detected --global";
  const skillUpgradeCmd = "ox skill install --upgrade";
  const skillFetchCmd = `curl -fsSL ${origin}/cli/skill.md`;

  const mcpEndpoint = `${origin}/api/mcp`;
  const mcpInstallCmd = "ox mcp install";
  const mcpDetectedCmd = "ox mcp install --detected --project";
  const mcpUpgradeCmd = "ox mcp install --upgrade";
  const mcpStatusCmd = "ox mcp status";

  <AccountSettingsLayout active="ai" data-testid="settings-ai-page">
    <PageHeader
      title="AI agents"
      description="Give coding agents the context to work with this Oxidean instance — install the ox skill into your harness, or point an MCP-capable client at the forge endpoint."
    />

    <h2 className="mt-8 font-[family-name:var(--font-display)] text-[18px] font-semibold text-foreground">
      Skills
    </h2>
    <p className="mt-1 text-[14px] text-muted-foreground">
      The <code className="font-mono">ox</code> skill teaches agent harnesses — Claude Code, Cursor,
      Codex, Copilot, Devin, and friends — how to drive the <code className="font-mono">ox</code>{" "}
      CLI against this instance. It ships inside the binary and is also served at{" "}
      <code className="font-mono">/cli/skill.md</code>, versioned with the server.
    </p>
    <p className="mt-3 text-[13px] font-medium text-foreground">
      Pick harnesses interactively (installed ones are marked)
    </p>
    <div className="mt-2">
      <CopyCommand command={skillInstallCmd} ariaLabel="Copy interactive skill install command" />
    </div>
    <p className="mt-3 text-[13px] font-medium text-foreground">
      Every detected harness — repo-local skill dirs
    </p>
    <div className="mt-2">
      <CopyCommand command={skillDetectedCmd} ariaLabel="Copy detected skill install command" />
    </div>
    <p className="mt-3 text-[13px] font-medium text-foreground">
      Every detected harness — user-level dirs instead
    </p>
    <div className="mt-2">
      <CopyCommand command={skillGlobalCmd} ariaLabel="Copy global skill install command" />
    </div>
    <p className="mt-3 text-[13px] font-medium text-foreground">
      Refresh existing installs after <code className="font-mono">ox self-update</code>
    </p>
    <div className="mt-2">
      <CopyCommand command={skillUpgradeCmd} ariaLabel="Copy skill upgrade command" />
    </div>
    <p className="mt-2 text-[13px] text-muted-foreground">
      <code className="font-mono">ox skill print</code> emits the raw SKILL.md; fetch it without the
      CLI from this instance:
    </p>
    <div className="mt-2">
      <CopyCommand command={skillFetchCmd} ariaLabel="Copy skill download command" />
    </div>

    <h2 className="mt-10 font-[family-name:var(--font-display)] text-[18px] font-semibold text-foreground">
      MCP server
    </h2>
    <p className="mt-1 text-[14px] text-muted-foreground">
      This instance serves a Model Context Protocol endpoint at{" "}
      <code className="font-mono">{mcpEndpoint}</code> — streamable HTTP, JSON-RPC 2.0. It exposes
      repository, issue, pull request, Actions, and package tools so MCP-capable agents can work the
      forge directly. Private data needs a credential — either OAuth, where the client opens this
      instance's browser flow itself, or a bearer token minted under{" "}
      <AppLink href="/settings/tokens" className="text-foreground underline underline-offset-2">
        Personal access tokens
      </AppLink>{" "}
      (<code className="font-mono">repo</code>, plus <code className="font-mono">package:*</code>{" "}
      for the registry tools); the token lives in your harness config, never in this page. Anonymous
      access sees public repositories only. Instance admins can disable the endpoint under{" "}
      <AppLink href="/admin/mcp" className="text-foreground underline underline-offset-2">
        Site administration
      </AppLink>
      .
    </p>

    <p className="mt-4 text-[13px] font-medium text-foreground">
      Endpoint — <code className="font-mono">POST</code> accepts one JSON-RPC message per request
    </p>
    <div className="mt-2">
      <CopyCommand command={mcpEndpoint} ariaLabel="Copy MCP endpoint URL" />
    </div>

    <p className="mt-4 text-[13px] font-medium text-foreground">
      Configure harnesses with the CLI — picks them interactively, merges an{" "}
      <code className="font-mono">oxidean</code> entry (and reuses your{" "}
      <code className="font-mono">ox</code> login as the bearer)
    </p>
    <div className="mt-2">
      <CopyCommand command={mcpInstallCmd} ariaLabel="Copy MCP install command" />
    </div>
    <div className="mt-2">
      <CopyCommand command={mcpDetectedCmd} ariaLabel="Copy detected MCP install command" />
    </div>
    <p className="mt-3 text-[13px] font-medium text-foreground">
      Check what's configured — files plus a live probe of the endpoint
    </p>
    <div className="mt-2">
      <CopyCommand command={mcpStatusCmd} ariaLabel="Copy MCP status command" />
    </div>
    <p className="mt-3 text-[13px] font-medium text-foreground">
      Refresh existing entries after <code className="font-mono">ox self-update</code> or a token
      rotation — rewrites <code className="font-mono">oxidean</code> blocks in place, keeping each
      file's credential
    </p>
    <div className="mt-2">
      <CopyCommand command={mcpUpgradeCmd} ariaLabel="Copy MCP upgrade command" />
    </div>
    <p className="mt-2 text-[13px] text-muted-foreground">
      <code className="font-mono">ox mcp print</code> emits the config block to paste by hand,{" "}
      <code className="font-mono">ox mcp remove</code> strips it again, and{" "}
      <code className="font-mono">--token</code>/<code className="font-mono">--token-stdin</code>/
      <code className="font-mono">--web</code>/<code className="font-mono">--anonymous</code>{" "}
      override how install resolves the credential.
    </p>

    <p className="mt-4 text-[13px] font-medium text-foreground">
      By hand — pick your client and auth mode, paste the block into its config file (replace{" "}
      <code className="font-mono">&lt;pat&gt;</code> when using a token)
    </p>
    <div className="mt-2">
      <McpHarnessBlocks
        endpoint={mcpEndpoint}
        initialHtml={props.mcpInitialHtml}
        initialTheme={props.mcpInitialTheme}
      />
    </div>
  </AccountSettingsLayout>;
}
