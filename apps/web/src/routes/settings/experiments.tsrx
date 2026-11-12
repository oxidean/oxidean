import { useQuery, useQueryClient } from "@octanejs/tanstack-query";
import { AppPage } from "@/components/app-page";
import type { ExperimentPublic } from "@oxidean/api-client";
import { useState } from "octane";
import { AccountSettingsLayout, SettingsShell } from "@/components/settings/settings-shell";
import { Badge } from "@/components/ui/badge";
import { Label } from "@/components/ui/label";
import { PageHeader } from "@/components/ui/page-header";
import { Switch } from "@/components/ui/switch";
import { apiClient } from "@/lib/api-client";
import { usePageRedirect } from "@/lib/nav.tsrx";
import {
  authSessionQueryOptions,
  experimentsListQueryKey,
  experimentsListQueryOptions,
  setAuthMeCache,
} from "@/lib/session-queries";
import { toastError } from "@/lib/toast";

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";
const LOGIN_RETURN = "/login?returnTo=/settings/experiments";

/** `/settings/experiments` — per-user feature-flag enrollment (WI-0001). */
export function ExperimentsSettingsPage() @{
  <AppPage>
    <ExperimentsContent />
  </AppPage>;
}

function ExperimentsContent() @{
  const meQ = useQuery(authSessionQueryOptions());
  usePageRedirect(meQ.data === null || meQ.isError ? LOGIN_RETURN : null);

  const listQ = useQuery(experimentsListQueryOptions());
  const queryClient = useQueryClient();
  const [pendingKey, setPendingKey] = useState<string | null>(null);

  async function toggle(exp: ExperimentPublic, enabled: boolean) {
    if (pendingKey !== null) return;
    setPendingKey(exp.key);
    try {
      const res = await apiClient.experiments.set({ key: exp.key, enabled });
      if (!res.ok) {
        toastError(res.error.message || "Could not update the experiment.");
        return;
      }
      // set echoes the full UserPublic — write auth.me and the list cache
      // from it so gates and this page agree without a refetch.
      setAuthMeCache(queryClient, res.data);
      queryClient.setQueryData(
        experimentsListQueryKey,
        (old: { experiments: ExperimentPublic[] } | undefined) =>
          old && {
            experiments: old.experiments.map((e) => (e.key === exp.key ? { ...e, enabled } : e)),
          },
      );
    } catch {
      toastError(NETWORK_ERROR);
    } finally {
      setPendingKey(null);
    }
  }

  @if (listQ.isPending || meQ.data == null) {
    <SettingsShell bare data-testid="settings-experiments-loading">
      <p className="text-[14px] text-muted-foreground">Loading experiments…</p>
    </SettingsShell>;
  } @else if (listQ.isError) {
    <SettingsShell bare data-testid="settings-experiments-error">
      <p className="text-[14px] text-destructive">
        Couldn't load experiments. Refresh to try again.
      </p>
    </SettingsShell>;
  } @else {
    <AccountSettingsLayout active="experiments" data-testid="settings-experiments-page">
      <PageHeader
        title="Experiments"
        description="Opt in to early features before they're on for everyone. Experiments are per account — turning one on only changes what you see."
      />

      @if (listQ.data.experiments.length === 0) {
        <p className="mt-6 text-[14px] text-muted-foreground">
          Nothing to try right now — new experiments show up here as they ship.
        </p>;
      } @else {
        <ul className="mt-4 divide-y divide-border">
          @for (const exp of listQ.data.experiments; key exp.key) {
            <li className="flex items-start justify-between gap-6 py-4">
              <div className="min-w-0">
                <div className="flex items-center gap-2">
                  <Label
                    htmlFor={`exp-${exp.key}`}
                    className="text-[15px] font-medium text-foreground"
                  >
                    {exp.title}
                  </Label>
                  <Badge variant={exp.status === "beta" ? "secondary" : "outline"}>
                    {exp.status}
                  </Badge>
                </div>
                <p className="mt-1 text-[14px] text-muted-foreground">
                  {exp.description}
                </p>
                <p className="mt-1 font-mono text-[12px] text-muted-foreground">
                  {exp.key}
                </p>
              </div>
              <Switch
                id={`exp-${exp.key}`}
                checked={exp.enabled}
                disabled={pendingKey !== null}
                onCheckedChange={(v) => void toggle(exp, !!v)}
                data-testid={`experiment-toggle-${exp.key}`}
                aria-label={`Enable ${exp.title}`}
              />
            </li>;
          }
        </ul>;
      }
    </AccountSettingsLayout>;
  }
}
