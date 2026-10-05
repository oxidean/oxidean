import { createFileRoute, useLoaderData } from "@octanejs/tanstack-router";
import { useEffect, useState } from "octane";
import { AccountSettingsLayout } from "@/components/settings/settings-shell";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { PageHeader } from "@/components/ui/page-header";
import { resolvePublicOriginClient } from "@/lib/public-origin";
import { fetchSessionMe } from "@/lib/ssr-auth";
import { fetchPublicOrigin } from "@/lib/ssr-repo";

export type CliLoaderData = { kind: "unauthenticated" } | { kind: "ready"; origin: string };

export const Route = createFileRoute("/settings/cli")({
  component: CliSettingsPage,
  head: () => ({ meta: [{ title: "Command line · Oxidean" }] }),
  loader: async (): Promise<CliLoaderData> => {
    try {
      const res = await fetchSessionMe();
      if (!res.ok) {
        return { kind: "unauthenticated" };
      }
      const origin = (await fetchPublicOrigin()) ?? "";
      return { kind: "ready", origin };
    } catch {
      return { kind: "unauthenticated" };
    }
  },
});

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

export function CliSettingsPage() @{
  const loaderData = useLoaderData({ from: "/settings/cli" }) as CliLoaderData | undefined;

  useEffect(() => {
    if (loaderData?.kind === "unauthenticated") {
      window.location.assign("/login?returnTo=/settings/cli");
    }
  }, [loaderData]);

  const origin =
    loaderData?.kind === "ready" && loaderData.origin
      ? loaderData.origin
      : resolvePublicOriginClient();
  const installCmd = `curl -fsSL ${origin}/cli/install.sh | sh`;
  const loginCmd = `ox auth login --instance ${origin} --token <pat>`;

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
    <div className="mt-3">
      <CopyCommand command={installCmd} ariaLabel="Copy install command" />
    </div>
    <p className="mt-2 text-[13px] text-muted-foreground">
      Installs to <code className="font-mono">~/.local/bin/ox</code> by default — set{" "}
      <code className="font-mono">OXIDEAN_INSTALL_DIR</code> to change it. No prebuilt binary for
      your platform? Build from source with{" "}
      <code className="font-mono">cargo install --path crates/oxidean-cli</code>.
    </p>

    <h2 className="mt-8 font-[family-name:var(--font-display)] text-[18px] font-semibold text-foreground">
      Sign in
    </h2>
    <p className="mt-1 text-[14px] text-muted-foreground">
      Create a personal access token under Personal access tokens, then:
    </p>
    <div className="mt-3">
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
