import { useQuery } from "@octanejs/tanstack-query";
import { useState } from "octane";
import { AppPage } from "@/components/app-page";
import { AccountSettingsLayout } from "@/components/settings/settings-shell";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { PageHeader } from "@/components/ui/page-header";
import { usePageRedirect } from "@/lib/nav.tsrx";
import { resolvePublicOriginClient } from "@/lib/public-origin";
import { authSessionQueryOptions } from "@/lib/session-queries";

function CopyCommand(props: { command: string; ariaLabel: string }) @{
  const [copied, setCopied] = useState(false);

  async function copy() {
    try {
      await navigator.clipboard.writeText(props.command);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 2000);
    } catch {
      /* manual copy via focus-select */
    }
  }

  <div className="flex flex-col gap-3 sm:flex-row">
    <Input
      readOnly
      value={props.command as string}
      aria-label={props.ariaLabel as string}
      className="min-w-0 flex-1 font-mono text-[13px]"
      onFocus={(e) => {
        (e.currentTarget as HTMLInputElement).select();
      }}
    />
    <Button
      type="button"
      variant="outline"
      className="shrink-0"
      aria-label={props.ariaLabel as string}
      onClick={() => void copy()}
    >
      {(copied ? "Copied" : "Copy") as string}
    </Button>
  </div>;
}

/** `/settings/cli` — ox CLI install + sign-in (title/meta via shell). */
export function CliSettingsPage() @{
  <AppPage>
    <CliSettingsContent />
  </AppPage>;
}

function CliSettingsContent() @{
  const meQ = useQuery(authSessionQueryOptions());
  // Auth gate (formerly loader): anonymous → login.
  usePageRedirect(meQ.data === null || meQ.isError ? "/login?returnTo=/settings/cli" : null);

  const origin = resolvePublicOriginClient();
  const installCmd = `curl -fsSL ${origin}/cli/install.sh | sh`;
  const installPs1Cmd = `irm ${origin}/cli/install.ps1 | iex`;
  const loginCmd = `ox auth login --instance ${origin} --token <pat>`;
  const loginWebCmd = `ox auth login --instance ${origin} --web`;

  <AccountSettingsLayout active="cli" data-testid="settings-cli-page">
    <PageHeader
      title="Command line"
      description="ox is the companion CLI for this Oxidean instance — repos, issues, pull requests, actions, and packages from your terminal."
    />

    <h2 className="mt-8 font-[family-name:var(--font-display)] text-[18px] font-semibold text-foreground">
      Install
    </h2>
    <p className="mt-1 text-[14px] text-muted-foreground">
      The installer downloads the prebuilt binary for your platform from this instance and verifies
      its checksum.
    </p>
    <p className="mt-3 text-[13px] font-medium text-foreground">Linux · macOS · WSL</p>
    <div className="mt-2">
      <CopyCommand command={installCmd} ariaLabel="Copy install command" />
    </div>
    <p className="mt-3 text-[13px] font-medium text-foreground">Windows (PowerShell)</p>
    <div className="mt-2">
      <CopyCommand command={installPs1Cmd} ariaLabel="Copy PowerShell install command" />
    </div>
    <p className="mt-2 text-[13px] text-muted-foreground">
      Installs to <code className="font-mono">~/.local/bin/ox</code> by default — set{" "}
      <code className="font-mono">OXIDEAN_INSTALL_DIR</code> to change it. Native{" "}
      <code className="font-mono">.deb</code>/<code className="font-mono">.rpm</code>/
      <code className="font-mono">.apk</code>/Arch packages ship on the project&apos;s release page.
      No prebuilt binary for your platform? Build from source with{" "}
      <code className="font-mono">cargo install --path crates/oxidean-cli</code>.
    </p>

    <h2 className="mt-8 font-[family-name:var(--font-display)] text-[18px] font-semibold text-foreground">
      Sign in
    </h2>
    <p className="mt-1 text-[14px] text-muted-foreground">
      Approve in the browser — the CLI prints a code and polls until you confirm:
    </p>
    <div className="mt-3">
      <CopyCommand command={loginWebCmd} ariaLabel="Copy browser login command" />
    </div>
    <p className="mt-2 text-[13px] text-muted-foreground">
      Or create a personal access token under Personal access tokens and paste it:
    </p>
    <div className="mt-2">
      <CopyCommand command={loginCmd} ariaLabel="Copy login command" />
    </div>

    <h2 className="mt-8 font-[family-name:var(--font-display)] text-[18px] font-semibold text-foreground">
      Staying up to date
    </h2>
    <p className="mt-1 text-[14px] text-muted-foreground">
      <code className="font-mono">ox self-update</code> fetches the latest binary this instance
      serves and replaces the executable in place — the CLI stays in lockstep with the server it
      talks to.
    </p>
  </AccountSettingsLayout>;
}
