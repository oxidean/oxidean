import { createFileRoute, useLoaderData } from "@octanejs/tanstack-router";
import { useQuery, useQueryClient } from "@octanejs/tanstack-query";
import type { UserPublic } from "@oxidean/api-client";
import { useEffect, useState } from "octane";
import { Cpu } from "@octanejs/lucide";
import { ActionsRunnersDocs } from "@/components/admin/actions-runners-docs";
import { AdminSettingsLayout, SettingsShell } from "@/components/settings/settings-shell";
import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/empty-state";
import { List, ListRow } from "@/components/ui/list";
import { PageHeader } from "@/components/ui/page-header";
import { apiClient } from "@/lib/api-client";
import { authSessionQueryOptions, setAuthMeCache } from "@/lib/session-queries";
import { fetchSessionMe } from "@/lib/ssr-auth";
import { toastError, toastSuccess } from "@/lib/toast";
import { AppLink } from "@/components/ui/app-link";

type AdminGateLoader =
  | { kind: "unauthenticated" }
  | { kind: "forbidden" }
  | { kind: "error"; message: string }
  | { kind: "ready"; me: UserPublic };

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";
const FORBIDDEN = "You need admin access to manage Actions runners.";

export const Route = createFileRoute("/admin/runners")({
  component: AdminRunnersPage,
  head: () => ({ meta: [{ title: "Actions runners · Oxidean" }] }),
  loader: async (): Promise<AdminGateLoader> => {
    try {
      const meRes = await fetchSessionMe();
      if (!meRes.ok) {
        if (meRes.error.code === "auth.unauthenticated") {
          return { kind: "unauthenticated" };
        }
        return { kind: "error", message: meRes.error.message || "Could not load session." };
      }
      if (meRes.data.role !== "sys-admin") {
        return { kind: "forbidden" };
      }
      return { kind: "ready", me: meRes.data };
    } catch {
      return { kind: "error", message: NETWORK_ERROR };
    }
  },
});

/** Instance registration tokens for official / third-party runners (D-ACT-08). */
export function AdminRunnersPage() @{
  const queryClient = useQueryClient();
  const loaderData = useLoaderData({ from: "/admin/runners" }) as AdminGateLoader | undefined;
  const [tokenOnce, setTokenOnce] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (loaderData?.kind === "unauthenticated") {
      window.location.assign("/login?returnTo=/admin/runners");
      return;
    }
    if (loaderData?.kind === "ready") {
      setAuthMeCache(queryClient, loaderData.me);
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

  const runnersQ = useQuery({
    queryKey: ["admin", "actions", "listRunners"] as const,
    enabled: isSysAdmin || loaderData?.kind === "ready",
    queryFn: async () => {
      const res = await apiClient.admin.actions.listRunners();
      if (!res.ok) {
        if (res.error.code === "admin.forbidden") throw new Error(FORBIDDEN);
        throw new Error(res.error.message || NETWORK_ERROR);
      }
      return res.data.runners;
    },
  });

  async function createToken() {
    if (busy || !isSysAdmin) return;
    setBusy(true);
    const res = await apiClient.admin.actions.createRegistrationToken();
    setBusy(false);
    if (!res.ok) {
      toastError(res.error.message ?? "Failed to create registration token");
      return;
    }
    setTokenOnce(res.data.token);
    toastSuccess("Copy this token now — it will not be shown again");
  }

  const runners = runnersQ.data ?? [];
  const signedOut = loaderData?.kind === "unauthenticated" || (me.isSuccess && me.data === null);
  const forbidden =
    loaderData?.kind === "forbidden" ||
    (me.isSuccess && me.data !== null && me.data.role !== "sys-admin");
  const loaderError = loaderData?.kind === "error" ? loaderData.message : "";

  @if (signedOut) {
    <SettingsShell>
      <p className="text-[15px] text-muted-foreground">
        Sign in as a system administrator to manage Actions runners.
      </p>
      <AppLink
        href="/login?returnTo=/admin/runners"
        className="mt-4 inline-block text-[14px] font-medium text-primary hover:underline"
      >
        Sign in
      </AppLink>
    </SettingsShell>;
  } @else {
    @if (forbidden) {
      <SettingsShell>
        <p className="text-[15px] text-destructive" role="alert">
          {FORBIDDEN as string}
        </p>
      </SettingsShell>;
    } @else {
      @if (loaderError) {
        <SettingsShell>
          <p className="text-[15px] text-destructive" role="alert">
            {loaderError as string}
          </p>
        </SettingsShell>;
      } @else {
        <AdminSettingsLayout active="runners" data-testid="admin-runners">
          <PageHeader
            icon={<Cpu aria-hidden size={22} />}
            title="Actions runners"
            description="Register self-hosted runners that execute workflow jobs for this instance."
          />

          <div className="mt-6">
            <ActionsRunnersDocs />
          </div>

          <section className="mt-10" aria-labelledby="reg-token-heading">
            <h2 id="reg-token-heading" className="text-[18px] font-semibold text-foreground">
              Registration token
            </h2>
            <p className="mt-1 text-[14px] text-muted-foreground">
              Create a one-time token, then pass it to the runner as{" "}
              <code className="rounded bg-muted px-1 py-0.5 font-mono text-[13px]">
                OXIDEAN_RUNNER_REGISTRATION_TOKEN
              </code>
              . Never commit real tokens.
            </p>
            <div className="mt-4 space-y-4">
              <Button type="button" disabled={busy} onClick={() => void createToken()}>
                Create registration token
              </Button>
              @if (tokenOnce) {
                <pre
                  className="overflow-x-auto rounded-md border border-border bg-muted/50 p-4 font-mono text-[13px] text-foreground"
                  data-testid="admin-runner-token-once"
                >
                  {tokenOnce as string}
                </pre>;
              } @else {
                <p className="text-[14px] text-muted-foreground">
                  Newly created tokens are shown once. Rotate compromised tokens immediately.
                </p>;
              }
            </div>
          </section>

          <section className="mt-10" aria-labelledby="runners-list-heading">
            <h2 id="runners-list-heading" className="text-[18px] font-semibold text-foreground">
              Registered runners
            </h2>
            <p className="mt-1 text-[14px] text-muted-foreground">
              Names and labels reported by runners after successful registration. Labels must match{" "}
              <code className="rounded bg-muted px-1 py-0.5 font-mono text-[13px]">runs-on</code> in
              workflows for jobs to be claimed.
            </p>
            @if (runnersQ.isError) {
              <p className="mt-4 text-[14px] text-destructive" role="alert">
                {(runnersQ.error as Error)?.message || "Could not load runners."}
              </p>;
            } @else if (runners.length === 0) {
              <EmptyState
                className="mt-4"
                compact
                icon={<Cpu aria-hidden size={20} />}
                title="No runners registered yet"
                description="Create a token and start the runner image to see entries here."
              />;
            } @else {
              <List className="mt-4">
                @for (const r of runners; key r.id) {
                  <ListRow className="gap-2 text-[14px]">
                    <Cpu
                      aria-hidden
                      size={16}
                      className="shrink-0 text-muted-foreground"
                    />
                    <span className="font-medium text-foreground">
                      {r.name as string}
                    </span>
                    <span className="text-muted-foreground">
                      {" — " + (r.labels.join(", ") as string)}
                    </span>
                  </ListRow>;
                }
              </List>;
            }
          </section>
        </AdminSettingsLayout>;
      }
    }
  }
}
