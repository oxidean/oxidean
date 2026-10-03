import { createFileRoute, useLoaderData } from "@octanejs/tanstack-router";
import { useQuery, useQueryClient } from "@octanejs/tanstack-query";
import type {
  AdminLfsSettingsPublic,
  AdminLfsUsageResponse,
  UserPublic,
} from "@oxidean/api-client";
import { useForm } from "@octanejs/tanstack-form";
import { useEffect, useState } from "octane";
import { ByteQuotaField } from "@/components/admin/byte-quota-field";
import { LfsUsageBarChartGate } from "@/components/admin/lfs-usage-charts-gate";
import { SettingsPageSkeleton } from "@/components/page-skeletons";
import { AdminSettingsLayout, SettingsShell } from "@/components/settings/settings-shell";
import { Button } from "@/components/ui/button";
import { apiClient } from "@/lib/api-client";
import {
  bytesToUnitString,
  convertUnitDisplay,
  formatBytes,
  pickDisplayUnit,
  toLfsUsageChartRows,
  unitValueToBytes,
  type ByteUnit,
} from "@/lib/byte-units";
import { useAppNavigate } from "@/lib/app-navigate";
import { authSessionQueryOptions, setAuthMeCache } from "@/lib/session-queries";
import {
  fetchAdminLfsSettings,
  fetchAdminLfsUsage,
  fetchSessionMe,
  requireSessionRedirect,
} from "@/lib/ssr-auth";
import { toastError, toastSuccess, toastWarning } from "@/lib/toast";
import { AppLink } from "@/components/ui/app-link";

export type AdminLfsLoaderData =
  | { kind: "unauthenticated" }
  | { kind: "forbidden" }
  | { kind: "error"; message: string }
  | {
      kind: "ready";
      me: UserPublic;
      settings: AdminLfsSettingsPublic;
      usage: AdminLfsUsageResponse;
    };

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";
const FORBIDDEN = "You need admin access to manage LFS quotas.";

const settingsQueryKey = ["admin", "lfs", "settings"] as const;
const usageQueryKey = ["admin", "lfs", "usage"] as const;

type QuotaFormValues = {
  maxObject: string;
  maxObjectUnit: ByteUnit;
  quotaRepo: string;
  quotaRepoUnit: ByteUnit;
  quotaUser: string;
  quotaUserUnit: ByteUnit;
};

function settingsToFormValues(settings: AdminLfsSettingsPublic): QuotaFormValues {
  const maxObjectUnit = pickDisplayUnit(settings.max_object_bytes);
  const quotaRepoUnit = pickDisplayUnit(settings.quota_repo_bytes);
  const quotaUserUnit = pickDisplayUnit(settings.quota_user_bytes);
  return {
    maxObject: bytesToUnitString(settings.max_object_bytes, maxObjectUnit),
    maxObjectUnit,
    quotaRepo: bytesToUnitString(settings.quota_repo_bytes, quotaRepoUnit),
    quotaRepoUnit,
    quotaUser: bytesToUnitString(settings.quota_user_bytes, quotaUserUnit),
    quotaUserUnit,
  };
}

export const Route = createFileRoute("/admin/lfs")({
  component: AdminLfsPage,
  head: () => ({ meta: [{ title: "LFS quotas · Oxidean" }] }),
  beforeLoad: () => requireSessionRedirect("/admin/lfs"),
  loader: async (): Promise<AdminLfsLoaderData> => {
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
      const [settingsRes, usageRes] = await Promise.all([
        fetchAdminLfsSettings(),
        fetchAdminLfsUsage(),
      ]);
      if (!settingsRes.ok) {
        return {
          kind: "error",
          message: settingsRes.error.message || FORBIDDEN,
        };
      }
      if (!usageRes.ok) {
        return {
          kind: "error",
          message: usageRes.error.message || "Could not load usage.",
        };
      }
      return {
        kind: "ready",
        me: meRes.data,
        settings: settingsRes.data,
        usage: usageRes.data,
      };
    } catch {
      return { kind: "error", message: NETWORK_ERROR };
    }
  },
});

/** Exported for admin/lfs integration tests (D-LFS-12 / D-LFS-13 / D-LFS-19). */
export function AdminLfsPage() @{
  const queryClient = useQueryClient();
  const navigateApp = useAppNavigate();
  const loaderData = useLoaderData({ from: "/admin/lfs" }) as AdminLfsLoaderData | undefined;

  useEffect(() => {
    if (loaderData?.kind === "unauthenticated") {
      void navigateApp("/login?returnTo=/admin/lfs");
      return;
    }
    if (loaderData?.kind === "ready") {
      setAuthMeCache(queryClient, loaderData.me);
      queryClient.setQueryData(settingsQueryKey, loaderData.settings);
      queryClient.setQueryData(usageQueryKey, loaderData.usage);
    }
  }, [loaderData, queryClient, navigateApp]);

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
    queryFn: async (): Promise<AdminLfsSettingsPublic> => {
      const res = await apiClient.admin.lfs.getSettings();
      if (!res.ok) throw new Error(res.error.message || FORBIDDEN);
      return res.data;
    },
  });

  const usageQ = useQuery({
    queryKey: usageQueryKey,
    enabled: isSysAdmin || loaderData?.kind === "ready",
    initialData: loaderData?.kind === "ready" ? loaderData.usage : undefined,
    queryFn: async (): Promise<AdminLfsUsageResponse> => {
      const res = await apiClient.admin.lfs.getUsage();
      if (!res.ok) throw new Error(res.error.message || FORBIDDEN);
      return res.data;
    },
  });

  const initialSettings = loaderData?.kind === "ready" ? loaderData.settings : null;

  const [pending, setPending] = useState(false);

  const form = useForm({
    defaultValues: initialSettings
      ? settingsToFormValues(initialSettings)
      : {
          maxObject: "",
          maxObjectUnit: "GiB" as ByteUnit,
          quotaRepo: "",
          quotaRepoUnit: "GiB" as ByteUnit,
          quotaUser: "",
          quotaUserUnit: "GiB" as ByteUnit,
        },
    onSubmit: async ({ value }) => {
      const max_object_bytes = unitValueToBytes(value.maxObject, value.maxObjectUnit);
      const quota_repo_bytes = unitValueToBytes(value.quotaRepo, value.quotaRepoUnit);
      const quota_user_bytes = unitValueToBytes(value.quotaUser, value.quotaUserUnit);
      if (max_object_bytes === null || quota_repo_bytes === null || quota_user_bytes === null) {
        toastWarning("Enter valid numeric quota amounts.");
        return;
      }
      setPending(true);
      try {
        const res = await apiClient.admin.lfs.updateSettings({
          max_object_bytes,
          quota_repo_bytes,
          quota_user_bytes,
        });
        if (!res.ok) {
          toastError(res.error.message || "Could not save LFS settings.");
          setPending(false);
          return;
        }
        await queryClient.invalidateQueries({ queryKey: ["admin", "lfs"] });
        toastSuccess("LFS quotas updated.");
        setPending(false);
      } catch {
        toastError(NETWORK_ERROR);
        setPending(false);
      }
    },
  });

  useEffect(() => {
    if (loaderData) return;
    if (me.isPending) return;
    if (me.isError) return;
    if (me.data === null) {
      void navigateApp("/login?returnTo=/admin/lfs");
    }
  }, [loaderData, me.isPending, me.isError, me.data, navigateApp]);

  useEffect(() => {
    if (settingsQ.isSuccess && settingsQ.data) {
      const next = settingsToFormValues(settingsQ.data);
      form.setFieldValue("maxObject", next.maxObject);
      form.setFieldValue("maxObjectUnit", next.maxObjectUnit);
      form.setFieldValue("quotaRepo", next.quotaRepo);
      form.setFieldValue("quotaRepoUnit", next.quotaRepoUnit);
      form.setFieldValue("quotaUser", next.quotaUser);
      form.setFieldValue("quotaUserUnit", next.quotaUserUnit);
    }
  }, [settingsQ.isSuccess, settingsQ.dataUpdatedAt]);

  async function clearOverrides() {
    setPending(true);
    try {
      const res = await apiClient.admin.lfs.updateSettings({
        clear_overrides: true,
      });
      if (!res.ok) {
        toastError(res.error.message || "Could not clear overrides.");
        setPending(false);
        return;
      }
      await queryClient.invalidateQueries({ queryKey: ["admin", "lfs"] });
      toastSuccess("Reverted to environment defaults.");
      setPending(false);
    } catch {
      toastError(NETWORK_ERROR);
      setPending(false);
    }
  }

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
  const usage = usageQ.data ?? (readyFromLoader ? loaderData.usage : undefined);

  const byRepoChart = usage
    ? toLfsUsageChartRows(
        usage.by_repo.map((row) => ({
          label: `${row.owner}/${row.name}`,
          logicalBytes: row.logical_bytes,
        })),
      )
    : [];
  const byOwnerChart = usage
    ? toLfsUsageChartRows(
        usage.by_owner.map((row) => ({
          label: row.owner_slug,
          logicalBytes: row.logical_bytes,
        })),
      )
    : [];

  @if (!sessionKnown) {
    <AdminSettingsLayout active="lfs">
      <SettingsPageSkeleton />
    </AdminSettingsLayout>;
  } @else {
    @if (signedOut) {
      <SettingsShell bare>
        <div className="text-center">
          <p className="text-[16px] text-muted-foreground">
            Sign in as a system administrator to manage LFS quotas.
          </p>
          <AppLink
            href="/login?returnTo=/admin/lfs"
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
        @if (loaderError && !settings) {
          <SettingsShell bare>
            <p role="alert" className="text-center text-[16px] text-destructive">
              {loaderError as string}
            </p>
          </SettingsShell>;
        } @else {
          <AdminSettingsLayout active="lfs" data-testid="admin-lfs-page">
            <h1 className="font-[family-name:var(--font-display)] text-[24px] font-semibold leading-[1.2] text-foreground">
              Git LFS quotas
            </h1>
            <p className="mt-2 text-[16px] text-muted-foreground">
              Override instance defaults for max object size and storage quotas. Choose an amount
              and unit; environment variables remain the fallback when overrides are cleared.
            </p>

            @if (settingsQ.isError && !settings) {
              <p role="alert" className="mt-8 text-[14px] text-destructive">
                {
                  (settingsQ.error instanceof Error
                    ? settingsQ.error.message
                    : FORBIDDEN) as string
                }
              </p>;
            } @else {
              <form
                method="post"
                action="#"
                className="mt-8 flex flex-col gap-4"
                onSubmit={(e) => {
                  e.preventDefault();
                  void form.handleSubmit();
                }}
              >
                <form.Field name="maxObject">
                  {(amountField) => (
                    <form.Field name="maxObjectUnit">
                      {(unitField) => (
                        <ByteQuotaField
                          id="lfs-max-object"
                          label="Max object size"
                          value={amountField.state.value}
                          unit={unitField.state.value}
                          overridden={settings?.max_object_bytes_overridden}
                          disabled={pending}
                          onBlur={amountField.handleBlur}
                          onValueChange={amountField.handleChange}
                          onUnitChange={(next) => {
                            amountField.handleChange(
                              convertUnitDisplay(
                                amountField.state.value,
                                unitField.state.value,
                                next,
                              ),
                            );
                            unitField.handleChange(next);
                          }}
                        />
                      )}
                    </form.Field>
                  )}
                </form.Field>
                <form.Field name="quotaRepo">
                  {(amountField) => (
                    <form.Field name="quotaRepoUnit">
                      {(unitField) => (
                        <ByteQuotaField
                          id="lfs-quota-repo"
                          label="Per-repo quota"
                          value={amountField.state.value}
                          unit={unitField.state.value}
                          overridden={settings?.quota_repo_bytes_overridden}
                          disabled={pending}
                          onBlur={amountField.handleBlur}
                          onValueChange={amountField.handleChange}
                          onUnitChange={(next) => {
                            amountField.handleChange(
                              convertUnitDisplay(
                                amountField.state.value,
                                unitField.state.value,
                                next,
                              ),
                            );
                            unitField.handleChange(next);
                          }}
                        />
                      )}
                    </form.Field>
                  )}
                </form.Field>
                <form.Field name="quotaUser">
                  {(amountField) => (
                    <form.Field name="quotaUserUnit">
                      {(unitField) => (
                        <ByteQuotaField
                          id="lfs-quota-user"
                          label="Per-user quota"
                          value={amountField.state.value}
                          unit={unitField.state.value}
                          overridden={settings?.quota_user_bytes_overridden}
                          disabled={pending}
                          onBlur={amountField.handleBlur}
                          onValueChange={amountField.handleChange}
                          onUnitChange={(next) => {
                            amountField.handleChange(
                              convertUnitDisplay(
                                amountField.state.value,
                                unitField.state.value,
                                next,
                              ),
                            );
                            unitField.handleChange(next);
                          }}
                        />
                      )}
                    </form.Field>
                  )}
                </form.Field>
                <div className="flex flex-wrap gap-2">
                  <Button type="submit" disabled={pending}>
                    {(pending ? "Saving…" : "Save quotas") as string}
                  </Button>
                  <Button
                    type="button"
                    variant="secondary"
                    disabled={pending}
                    onClick={() => void clearOverrides()}
                  >
                    Clear overrides
                  </Button>
                </div>
              </form>;
            }

            <section className="mt-12 flex flex-col gap-4" aria-labelledby="lfs-inst-usage">
              <h2
                id="lfs-inst-usage"
                className="font-[family-name:var(--font-display)] text-[18px] font-semibold"
              >
                Instance usage breakdown
              </h2>
              @if (usageQ.isError && !usage) {
                <p role="alert" className="text-[14px] text-destructive">
                  {
                    (usageQ.error instanceof Error
                      ? usageQ.error.message
                      : "Could not load usage.") as string
                  }
                </p>;
              } @else {
                @if (usage) {
                  <>
                    <p className="text-[14px] text-muted-foreground">
                      {
                        `${usage.object_count} unique OIDs · ${formatBytes(usage.physical_bytes)} physical · ${formatBytes(usage.logical_bytes)} logical` as string
                      }
                    </p>
                    <div className="grid gap-8 lg:grid-cols-2">
                      <LfsUsageBarChartGate
                        title="By repository"
                        testId="lfs-usage-chart-repo"
                        data={byRepoChart}
                        emptyMessage="No linked LFS objects."
                      />
                      <LfsUsageBarChartGate
                        title="By owner"
                        testId="lfs-usage-chart-owner"
                        data={byOwnerChart}
                        emptyMessage="No linked LFS objects."
                      />
                    </div>
                    @if (
                      usage.by_repo.length > 0 ||
                      usage.by_owner.length > 0
                    ) {
                      <div className="grid gap-6 sm:grid-cols-2">
                        @if (usage.by_repo.length > 0) {
                          <div>
                            <h3 className="text-[15px] font-medium">
                              Repositories (detail)
                            </h3>
                            <ul className="mt-2 flex flex-col gap-1 text-[13px] text-muted-foreground">
                              @for (const row of usage.by_repo; key row.repository_id) {
                                <li>
                                  {
                                    `${row.owner}/${row.name} · ${row.object_count} · ${formatBytes(row.logical_bytes)}` as string
                                  }
                                </li>;
                              }
                            </ul>
                          </div>;
                        }
                        @if (usage.by_owner.length > 0) {
                          <div>
                            <h3 className="text-[15px] font-medium">
                              Owners (detail)
                            </h3>
                            <ul className="mt-2 flex flex-col gap-1 text-[13px] text-muted-foreground">
                              @for (const row of usage.by_owner; key row.owner_id) {
                                <li>
                                  {
                                    `${row.owner_slug} · ${row.object_count} · ${formatBytes(row.logical_bytes)}` as string
                                  }
                                </li>;
                              }
                            </ul>
                          </div>;
                        }
                      </div>;
                    }
                  </>;
                }
              }
            </section>
          </AdminSettingsLayout>;
        }
      }
    }
  }
}
