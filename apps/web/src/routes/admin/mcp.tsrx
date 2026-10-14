import { queryOptions, useQuery, useQueryClient } from "@octanejs/tanstack-query";
import { AppPage } from "@/components/app-page";
import {
  MCP_PROTOCOL_VERSIONS,
  type AdminMcpSettingsPublic,
  type UserPublic,
} from "@oxidean/api-client";
import { useEffect, useState } from "octane";
import { CopyCommand } from "@/components/settings/copy-command";
import { AdminSettingsLayout, SettingsShell } from "@/components/settings/settings-shell";
import { AppLink } from "@/components/ui/app-link";
import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { apiClient } from "@/lib/api-client";
import { resolvePublicOriginClient } from "@/lib/public-origin";
import { authSessionQueryOptions, setAuthMeCache } from "@/lib/session-queries";
import { fetchAdminMcpSettings, fetchSessionMe } from "@/lib/ssr-auth";
import { toastError, toastSuccess } from "@/lib/toast";

export type AdminMcpLoaderData =
  | { kind: "unauthenticated" }
  | { kind: "forbidden" }
  | { kind: "error"; message: string }
  | {
      kind: "ready";
      me: UserPublic;
      settings: AdminMcpSettingsPublic;
    };

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";
const FORBIDDEN = "You need admin access to manage MCP settings.";

const settingsQueryKey = ["admin", "mcp", "settings"] as const;

function adminMcpQueryOptions() {
  return queryOptions({
    queryKey: ["admin", "adminMcp"],
    queryFn: async (): Promise<AdminMcpLoaderData> => {
      try {
        const meRes = await fetchSessionMe();
        if (!meRes.ok) {
          if (meRes.error.code === "auth.unauthenticated") {
            return { kind: "unauthenticated" };
          }
          return {
            kind: "error",
            message: meRes.error.message || "Could not load session.",
          };
        }
        if (meRes.data.role !== "sys-admin") {
          return { kind: "forbidden" };
        }
        const settingsRes = await fetchAdminMcpSettings();
        if (!settingsRes.ok) {
          if (settingsRes.error.code === "admin.forbidden") {
            return { kind: "forbidden" };
          }
          return {
            kind: "error",
            message: settingsRes.error.message || FORBIDDEN,
          };
        }
        return {
          kind: "ready",
          me: meRes.data,
          settings: settingsRes.data,
        };
      } catch {
        return { kind: "error", message: NETWORK_ERROR };
      }
    },
    retry: false,
    staleTime: 30_000,
  });
}

/** Exported for admin/mcp integration + browser tests (AGT-03). */
export function AdminMcpPage() @{
  <AppPage>
    <AdminMcpContent />
  </AppPage>;
}

export function AdminMcpContent() @{
  const queryClient = useQueryClient();
  const loaderData = useQuery(adminMcpQueryOptions()).data;

  useEffect(() => {
    if (loaderData?.kind === "unauthenticated") {
      window.location.assign("/login?returnTo=/admin/mcp");
      return;
    }
    if (loaderData?.kind === "ready") {
      setAuthMeCache(queryClient, loaderData.me);
      queryClient.setQueryData(settingsQueryKey, loaderData.settings);
    }
  }, [loaderData, queryClient]);

  const me = useQuery({
    ...authSessionQueryOptions(),
    ...(loaderData?.kind === "ready"
      ? { initialData: loaderData.me }
      : loaderData?.kind === "unauthenticated"
        ? { initialData: null as UserPublic | null }
        : {}),
  });
  const isSysAdmin = me.isSuccess && me.data?.role === "sys-admin";

  const settingsQ = useQuery({
    queryKey: settingsQueryKey,
    enabled: isSysAdmin || loaderData?.kind === "ready",
    initialData: loaderData?.kind === "ready" ? loaderData.settings : undefined,
    queryFn: async (): Promise<AdminMcpSettingsPublic> => {
      const res = await apiClient.admin.mcp.getSettings();
      if (!res.ok) throw new Error(res.error.message || FORBIDDEN);
      return res.data;
    },
  });

  const [pending, setPending] = useState(false);

  async function setEnabled(next: boolean) {
    setPending(true);
    try {
      const res = await apiClient.admin.mcp.updateSettings({ enabled: next });
      if (!res.ok) {
        toastError(res.error.message || "Could not save MCP settings.");
        setPending(false);
        return;
      }
      await queryClient.invalidateQueries({ queryKey: ["admin", "mcp"] });
      toastSuccess(next ? "MCP endpoint enabled." : "MCP endpoint disabled.");
      setPending(false);
    } catch {
      toastError(NETWORK_ERROR);
      setPending(false);
    }
  }

  async function clearOverride() {
    setPending(true);
    try {
      const res = await apiClient.admin.mcp.updateSettings({ clear_overrides: true });
      if (!res.ok) {
        toastError(res.error.message || "Could not clear the override.");
        setPending(false);
        return;
      }
      await queryClient.invalidateQueries({ queryKey: ["admin", "mcp"] });
      toastSuccess("Reverted to the OXIDEAN_MCP_ENABLED default.");
      setPending(false);
    } catch {
      toastError(NETWORK_ERROR);
      setPending(false);
    }
  }

  const [probe, setProbe] = useState<{ kind: "running" | "up" | "down"; detail: string } | null>(
    null,
  );

  /** POST a JSON-RPC `initialize` same-origin — the same handshake MCP clients send. */
  async function probeEndpoint() {
    const started = performance.now();
    setProbe({ kind: "running", detail: "Sending initialize…" });
    try {
      const res = await fetch("/api/mcp", {
        method: "POST",
        headers: { "content-type": "application/json", accept: "application/json" },
        body: JSON.stringify({
          jsonrpc: "2.0",
          id: "admin-mcp-probe",
          method: "initialize",
          params: {
            protocolVersion: MCP_PROTOCOL_VERSIONS.at(-1),
            capabilities: {},
            clientInfo: { name: "oxidean-admin", version: "0" },
          },
        }),
      });
      const ms = Math.round(performance.now() - started);
      const body = (await res.json().catch(() => null)) as {
        result?: {
          protocolVersion?: string;
          serverInfo?: { name?: string; version?: string };
        };
        error?: { code?: number; message?: string };
      } | null;
      const rpcError = body?.error;
      if (res.ok && body?.result) {
        const info = body.result.serverInfo ?? {};
        const version = info.version ? ` ${info.version}` : "";
        const proto = body.result.protocolVersion
          ? `, protocol ${body.result.protocolVersion}`
          : "";
        setProbe({
          kind: "up",
          detail: `Up — ${info.name ?? "oxidean"}${version}${proto} (${ms} ms)`,
        });
      } else if (rpcError?.message?.includes("mcp.disabled")) {
        setProbe({ kind: "down", detail: `Disabled — ${rpcError.message}` });
      } else if (res.status === 401 || res.status === 403) {
        setProbe({
          kind: "down",
          detail: `Unauthorized — HTTP ${res.status}; the admin session cookie was rejected`,
        });
      } else if (rpcError) {
        setProbe({
          kind: "down",
          detail: `JSON-RPC error ${rpcError.code}: ${rpcError.message}`,
        });
      } else {
        setProbe({ kind: "down", detail: `Unexpected response — HTTP ${res.status}` });
      }
    } catch {
      setProbe({ kind: "down", detail: "Unreachable — the request failed before a response" });
    }
  }

  useEffect(() => {
    if (loaderData) return;
    if (me.isPending) return;
    if (me.isError) return;
    if (me.data === null) {
      window.location.assign("/login?returnTo=/admin/mcp");
    }
  }, [loaderData, me.isPending, me.isError, me.data]);

  const readyFromLoader = loaderData?.kind === "ready";
  const sessionKnown =
    readyFromLoader ||
    loaderData?.kind === "unauthenticated" ||
    loaderData?.kind === "forbidden" ||
    loaderData?.kind === "error" ||
    !me.isPending;
  const signedOut =
    loaderData?.kind === "unauthenticated" ||
    (sessionKnown && !me.isPending && (!me.isSuccess || !me.data));
  const forbidden =
    loaderData?.kind === "forbidden" || (sessionKnown && me.isSuccess && !!me.data && !isSysAdmin);
  const loaderError = loaderData?.kind === "error" ? loaderData.message : "";
  const settings = settingsQ.data ?? (readyFromLoader ? loaderData.settings : undefined);
  const enabled = settings?.enabled === true;
  const overridden = settings?.enabled_overridden === true;

  @if (!sessionKnown) {
    <></>;
  } @else {
    @if (signedOut) {
      <SettingsShell bare>
        <div className="text-center">
          <p className="text-[16px] text-muted-foreground">
            Sign in as a system administrator to manage MCP settings.
          </p>
          <AppLink
            href="/login?returnTo=/admin/mcp"
            className="mt-4 inline-block text-primary underline-offset-2 hover:underline"
          >
            Sign in
          </AppLink>
        </div>
      </SettingsShell>;
    } @else {
      @if (forbidden) {
        <SettingsShell bare>
          <p role="alert" className="text-center text-[16px] text-destructive">
            {FORBIDDEN as string}
          </p>
        </SettingsShell>;
      } @else {
        @if ((loaderError || settingsQ.isError) && !settings) {
          <SettingsShell bare>
            <p role="alert" className="text-center text-[16px] text-destructive">
              {(loaderError || NETWORK_ERROR) as string}
            </p>
          </SettingsShell>;
        } @else {
          <AdminSettingsLayout active="mcp" data-testid="admin-mcp-page">
            <h1 className="font-[family-name:var(--font-display)] text-[24px] font-semibold leading-[1.2] text-foreground">
              MCP endpoint
            </h1>
            <p className="mt-2 text-[16px] text-muted-foreground">
              The instance Model Context Protocol endpoint lets MCP clients — agentic editors, CLIs,
              orchestrators — work with repositories, issues, pull requests, Actions runs, and
              packages. This page gates the endpoint instance-wide; per-user client setup lives
              under{" "}
              <AppLink href="/settings/ai" className="text-foreground underline underline-offset-2">
                Settings → AI agents
              </AppLink>
              .
            </p>

            <section className="mt-8 flex flex-col gap-4" aria-labelledby="mcp-toggle-heading">
              <h2 id="mcp-toggle-heading" className="text-[18px] font-semibold">
                Endpoint status
              </h2>
              <p className="text-[14px] text-foreground" data-testid="admin-mcp-status">
                Status: <strong>{(enabled ? "Enabled" : "Disabled") as string}</strong>
                {(overridden ? " (admin override)" : " (environment default)") as string}
              </p>
              <div className="flex min-h-11 items-center gap-2">
                <Switch
                  id="admin-mcp-enabled"
                  data-testid="admin-mcp-enabled"
                  aria-label="Enable the MCP endpoint"
                  checked={enabled}
                  disabled={pending}
                  onCheckedChange={(v) => void setEnabled(v === true)}
                />
                <Label htmlFor="admin-mcp-enabled">Enable the MCP endpoint</Label>
              </div>
              <p className="text-[14px] font-normal leading-[1.4] text-muted-foreground">
                When off, <code>/api/mcp</code> answers <code>404 mcp.disabled</code>. Without an
                override the <code>OXIDEAN_MCP_ENABLED</code> environment default applies (on when
                unset).
              </p>
              @if (overridden) {
                <div>
                  <Button
                    type="button"
                    variant="secondary"
                    disabled={pending}
                    data-testid="admin-mcp-clear-override"
                    onClick={() => void clearOverride()}
                  >
                    Use environment default
                  </Button>
                </div>;
              }
            </section>

            <section
              className="mt-10 flex flex-col gap-3"
              aria-labelledby="mcp-endpoint-url-heading"
            >
              <h2 id="mcp-endpoint-url-heading" className="text-[18px] font-semibold">
                Endpoint URL
              </h2>
              <p className="text-[14px] leading-[1.5] text-muted-foreground">
                Streamable HTTP in JSON mode — one JSON-RPC 2.0 message per <code>POST</code>. This
                is the address users paste into MCP clients (or hand to <code>ox mcp install</code>
                ).
              </p>
              <CopyCommand
                command={`${resolvePublicOriginClient()}/api/mcp`}
                ariaLabel="Copy MCP endpoint URL"
              />
              <div className="flex items-center gap-3">
                <Button
                  type="button"
                  variant="outline"
                  disabled={probe?.kind === "running"}
                  data-testid="admin-mcp-probe"
                  onClick={() => void probeEndpoint()}
                >
                  {(probe?.kind === "running" ? "Testing…" : "Test endpoint") as string}
                </Button>
                @if (probe !== null && probe.kind !== "running") {
                  <p
                    role="status"
                    aria-live="polite"
                    data-testid="admin-mcp-probe-result"
                    data-probe-kind={probe.kind}
                    className={
                      probe.kind === "up"
                        ? "text-[13px] text-success"
                        : "text-[13px] text-destructive"
                    }
                  >
                    {probe.detail as string}
                  </p>;
                }
              </div>
            </section>

            <section className="mt-10 flex flex-col gap-3" aria-labelledby="mcp-user-setup-heading">
              <h2 id="mcp-user-setup-heading" className="text-[18px] font-semibold">
                User-facing client setup
              </h2>
              <p className="text-[14px] leading-[1.5] text-muted-foreground">
                Per-user MCP configuration — the endpoint URL, <code>ox mcp</code> commands, and
                ready-to-paste config blocks for Claude Code, Cursor, VS Code, and other clients —
                lives under{" "}
                <AppLink
                  href="/settings/ai"
                  className="text-foreground underline underline-offset-2"
                >
                  Settings → AI agents
                </AppLink>
                . Point users there instead of writing config by hand.
              </p>
              <p className="text-[14px] leading-[1.5] text-muted-foreground">
                Callers authenticate with a personal access token (<code>oxidean_pat_…</code> /{" "}
                <code>oxidean_fg_…</code>) minted under Settings → Personal access tokens — classic
                tokens need
                <code> repo</code> (<code>package:read</code>/<code>package:write</code> for
                packages); fine-grained tokens need <code> contents</code> plus repository
                selection. The signed-in session cookie works too; anonymous callers see public data
                only.
              </p>
            </section>
          </AdminSettingsLayout>;
        }
      }
    }
  }
}
