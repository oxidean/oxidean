import { useQuery } from "@octanejs/tanstack-query";
import { AppPage } from "@/components/app-page";
import { AccountSettingsLayout } from "@/components/settings/settings-shell";
import { CopyCommand } from "@/components/settings/copy-command";
import { PageHeader } from "@/components/ui/page-header";
import { usePageRedirect } from "@/lib/nav.tsrx";
import { resolvePublicOriginClient } from "@/lib/public-origin";
import { authSessionQueryOptions } from "@/lib/session-queries";

/** `/settings/ai` — agent-facing integrations (skills today, MCP later). */
export function AiSettingsPage() @{
  <AppPage>
    <AiSettingsContent />
  </AppPage>;
}

function AiSettingsContent() @{
  const meQ = useQuery(authSessionQueryOptions());
  // Auth gate (formerly loader): anonymous → login.
  usePageRedirect(meQ.data === null || meQ.isError ? "/login?returnTo=/settings/ai" : null);

  const origin = resolvePublicOriginClient();
  const skillInstallCmd = "ox skill install";
  const skillDetectedCmd = "ox skill install --detected --project";
  const skillGlobalCmd = "ox skill install --detected --global";
  const skillUpgradeCmd = "ox skill install --upgrade";
  const skillFetchCmd = `curl -fsSL ${origin}/cli/skill.md`;

  <AccountSettingsLayout active="ai" data-testid="settings-ai-page">
    <PageHeader
      title="AI agents"
      description="Give coding agents the context to work with this Oxidean instance — install the ox skill into your harness today, wire MCP servers here later."
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
  </AccountSettingsLayout>;
}
