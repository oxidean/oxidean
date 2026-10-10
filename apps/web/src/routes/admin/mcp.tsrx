import { queryOptions, useQuery, useQueryClient } from "@octanejs/tanstack-query";
import { AppPage } from "@/components/app-page";
import type { AdminMcpSettingsPublic, UserPublic } from "@oxidean/api-client";
import { useEffect, useState } from "octane";
import { AdminSettingsLayout, SettingsShell } from "@/components/settings/settings-shell";
import { AppLink } from "@/components/ui/app-link";
import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { apiClient } from "@/lib/api-client";
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
              The instance Model Context Protocol endpoint (<code>POST /api/mcp</code>) lets MCP
              clients — agentic editors, CLIs, orchestrators — work with repositories, issues, pull
              requests, Actions runs, and packages. Callers authenticate with a personal access
              token (<code>oxidean_pat_…</code> / <code>oxidean_fg_…</code>) or the signed-in
              session cookie; anonymous callers see public data only.
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

            <section className="mt-10 flex flex-col gap-3" aria-labelledby="mcp-connect-heading">
              <h2 id="mcp-connect-heading" className="text-[18px] font-semibold">
                Connecting clients
              </h2>
              <p className="text-[14px] leading-[1.5] text-muted-foreground">
                Point an MCP-capable client at <code>POST /api/mcp</code> on this instance with an{" "}
                <code>Authorization: Bearer</code> personal access token — mint one under Settings →
                Personal access tokens. Classic tokens need the
                <code> repo</code> scope for repository tools (<code>package:read</code>/
                <code>package:write</code> for packages); fine-grained tokens need
                <code> contents</code> permission plus repository selection. Full client setup
                (Claude Code, Cursor, stdio bridges) is documented in <code>docs/MCP.md</code>.
              </p>
            </section>
          </AdminSettingsLayout>;
        }
      }
    }
  }
}
