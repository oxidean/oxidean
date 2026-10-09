import { useForm } from "@octanejs/tanstack-form";
import { queryOptions, useQuery, useQueryClient } from "@octanejs/tanstack-query";
import { AppPage } from "@/components/app-page";
import type { PackagesAdminUsageResponse, UserPublic } from "@oxidean/api-client";
import { packagesAdminUsageQueryOptions } from "@oxidean/api-client";
import { useEffect, useState } from "octane";
import { ByteQuotaField } from "@/components/admin/byte-quota-field";
import { AdminSettingsLayout, SettingsShell } from "@/components/settings/settings-shell";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { apiClient } from "@/lib/api-client";
import {
  bytesToUnitString,
  convertUnitDisplay,
  formatBytes,
  pickDisplayUnit,
  unitValueToBytes,
  type ByteUnit,
} from "@/lib/byte-units";
import {
  DEFAULT_MAX_BLOB_LABEL,
  DEFAULT_OWNER_QUOTA_LABEL,
  packageFormatLabel,
} from "@/lib/package-quota-copy";
import { authSessionQueryOptions, setAuthMeCache } from "@/lib/session-queries";
import { fetchSessionMe } from "@/lib/ssr-auth";
import { toastError, toastSuccess, toastWarning } from "@/lib/toast";
import { AppLink } from "@/components/ui/app-link";

type AdminGateLoader =
  | { kind: "unauthenticated" }
  | { kind: "forbidden" }
  | { kind: "error"; message: string }
  | { kind: "ready"; me: UserPublic };

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";
const FORBIDDEN = "You need admin access to manage package storage.";

function adminPackagesQueryOptions() {
  return queryOptions({
    queryKey: ["admin", "adminPackages"],
    queryFn: async (): Promise<AdminGateLoader> => {
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
    retry: false,
    staleTime: 30_000,
  });
}

type QuotaDraft = {
  amount: string;
  unit: ByteUnit;
};

function draftFromBytes(bytes: number): QuotaDraft {
  const unit = pickDisplayUnit(bytes);
  return { amount: bytesToUnitString(bytes, unit), unit };
}

function usagePercent(used: number, quota: number): number {
  if (!Number.isFinite(used) || !Number.isFinite(quota) || quota <= 0) return 0;
  return Math.min(100, Math.round((used / quota) * 100));
}

/** Admin package storage quota/usage (D-PKG-09). */
export function AdminPackagesPage() @{
  <AppPage>
    <AdminPackagesContent />
  </AppPage>;
}

function AdminPackagesContent() @{
  const [activeOwner, setActiveOwner] = useState("");
  const [pending, setPending] = useState(false);
  const [draft, setDraft] = useState<QuotaDraft>({ amount: "", unit: "GiB" });
  const qc = useQueryClient();
  const loaderData = useQuery(adminPackagesQueryOptions()).data;

  useEffect(() => {
    if (loaderData?.kind === "unauthenticated") {
      window.location.assign("/login?returnTo=/admin/packages");
      return;
    }
    if (loaderData?.kind === "ready") {
      setAuthMeCache(qc, loaderData.me);
    }
  }, [loaderData, qc]);

  const me = useQuery({
    ...authSessionQueryOptions(),
    ...(loaderData?.kind === "ready"
      ? { initialData: loaderData.me }
      : loaderData?.kind === "unauthenticated"
        ? { initialData: null as UserPublic | null }
        : {}),
  });
  const isSysAdmin = me.isSuccess && me.data?.role === "sys-admin";
  const signedOut = loaderData?.kind === "unauthenticated" || (me.isSuccess && me.data === null);
  const forbidden =
    loaderData?.kind === "forbidden" ||
    (me.isSuccess && me.data !== null && me.data.role !== "sys-admin");

  const lookupForm = useForm({
    defaultValues: { owner: "" },
    onSubmit: ({ value }) => {
      const next = value.owner.trim();
      if (!next) {
        toastWarning("Enter a username or organization slug.");
        return;
      }
      setActiveOwner(next);
    },
  });

  const usageQ = useQuery({
    ...packagesAdminUsageQueryOptions(apiClient, { owner: activeOwner }),
    enabled: isSysAdmin && activeOwner.length > 0,
  });

  useEffect(() => {
    if (!usageQ.isSuccess || !usageQ.data) return;
    setDraft(draftFromBytes(usageQ.data.quota_bytes));
  }, [usageQ.isSuccess, usageQ.dataUpdatedAt, activeOwner]);

  const usage: PackagesAdminUsageResponse | undefined = usageQ.data;
  const customQuota = !!usage && usage.quota_bytes !== usage.default_quota_bytes;
  const percent = usage ? usagePercent(usage.used_bytes, usage.quota_bytes) : 0;
  const ownerKindLabel =
    usage?.owner_type === "org" ? "Organization" : usage?.owner_type === "user" ? "User" : "";

  async function saveQuota() {
    if (!isSysAdmin || !activeOwner || pending) return;
    const max_bytes = unitValueToBytes(draft.amount, draft.unit);
    if (max_bytes === null) {
      toastWarning("Enter a valid quota amount.");
      return;
    }
    setPending(true);
    try {
      const res = await apiClient.packages.adminSetQuota({
        owner: activeOwner,
        max_bytes,
      });
      if (!res.ok) {
        toastError(res.error.message || "Could not save quota.");
        setPending(false);
        return;
      }
      await qc.invalidateQueries({ queryKey: ["packages", "adminUsage"] });
      toastSuccess("Owner quota updated.");
      setPending(false);
    } catch {
      toastError(NETWORK_ERROR);
      setPending(false);
    }
  }

  async function applyInstanceDefault() {
    if (!isSysAdmin || !activeOwner || !usage || pending) return;
    setPending(true);
    try {
      const res = await apiClient.packages.adminSetQuota({
        owner: activeOwner,
        max_bytes: usage.default_quota_bytes,
      });
      if (!res.ok) {
        toastError(res.error.message || "Could not apply instance default.");
        setPending(false);
        return;
      }
      await qc.invalidateQueries({ queryKey: ["packages", "adminUsage"] });
      toastSuccess("Quota set to the instance default.");
      setPending(false);
    } catch {
      toastError(NETWORK_ERROR);
      setPending(false);
    }
  }

  @if (signedOut) {
    <SettingsShell>
      <p className="text-[15px] text-muted-foreground">
        Sign in as a system administrator to manage package storage.
      </p>
      <AppLink
        href="/login?returnTo=/admin/packages"
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
      @if (loaderData?.kind === "error") {
        <SettingsShell>
          <p className="text-[15px] text-destructive" role="alert">
            {loaderData.message as string}
          </p>
        </SettingsShell>;
      } @else {
        <AdminSettingsLayout active="packages" data-testid="admin-packages">
          <h1 className="font-[family-name:var(--font-display)] text-[24px] font-semibold leading-[1.2] text-foreground">
            Package storage
          </h1>
          <p className="mt-2 max-w-2xl text-[15px] text-muted-foreground">
            Package registries (OCI, npm, and generic) share one storage pool per user or
            organization. Look up an owner to see usage and set a custom quota. Instance-wide
            defaults come from environment configuration.
          </p>

          <section className="mt-10" aria-labelledby="packages-defaults-heading">
            <h2
              id="packages-defaults-heading"
              className="font-[family-name:var(--font-display)] text-[18px] font-semibold text-foreground"
            >
              Instance defaults
            </h2>
            <dl className="mt-4 grid gap-4 sm:grid-cols-2">
              <div className="rounded-md border border-border bg-card px-4 py-3">
                <dt className="text-[13px] font-medium text-muted-foreground">Per-owner quota</dt>
                <dd className="mt-1 text-[16px] font-semibold text-foreground">
                  {DEFAULT_OWNER_QUOTA_LABEL as string}
                </dd>
                <dd className="mt-1 text-[12px] text-muted-foreground">
                  Unless a custom quota is set for that owner
                </dd>
              </div>
              <div className="rounded-md border border-border bg-card px-4 py-3">
                <dt className="text-[13px] font-medium text-muted-foreground">Max upload size</dt>
                <dd className="mt-1 text-[16px] font-semibold text-foreground">
                  {DEFAULT_MAX_BLOB_LABEL as string}
                </dd>
                <dd className="mt-1 text-[12px] text-muted-foreground">
                  Rejects individual blobs larger than this
                </dd>
              </div>
            </dl>
          </section>

          <section className="mt-10" aria-labelledby="packages-lookup-heading">
            <h2
              id="packages-lookup-heading"
              className="font-[family-name:var(--font-display)] text-[18px] font-semibold text-foreground"
            >
              Owner usage &amp; quota
            </h2>
            <p className="mt-1 text-[14px] text-muted-foreground">
              Enter a username or organization slug, then review storage and adjust the quota if
              needed.
            </p>
            <form
              className="mt-4 flex flex-wrap items-end gap-3"
              method="post"
              action="#"
              onSubmit={(e) => {
                e.preventDefault();
                void lookupForm.handleSubmit();
              }}
            >
              <lookupForm.Field name="owner">
                {(field) => (
                  <div className="flex min-w-[14rem] flex-1 flex-col gap-1">
                    <Label htmlFor="admin-packages-owner">Username or organization</Label>
                    <Input
                      id="admin-packages-owner"
                      data-testid="admin-packages-owner"
                      placeholder="e.g. alice or acme"
                      autoComplete="off"
                      value={field.state.value as string}
                      onInput={(e) => field.handleChange((e.target as HTMLInputElement).value)}
                    />
                  </div>
                )}
              </lookupForm.Field>
              <Button type="submit" data-testid="admin-packages-lookup">
                Show usage
              </Button>
            </form>
          </section>

          @if (activeOwner && usageQ.isPending) {
            <p className="mt-8 text-[14px] text-muted-foreground">Loading package usage…</p>;
          }
          @if (usageQ.isError) {
            <p className="mt-8 text-[14px] text-destructive" role="alert">
              {(usageQ.error as Error)?.message || "Could not load usage for that owner."}
            </p>;
          }
          @if (usage) {
            <div className="mt-8 space-y-8" data-testid="admin-packages-usage">
              <div className="rounded-md border border-border bg-card p-4">
                <div className="flex flex-wrap items-baseline justify-between gap-2">
                  <div>
                    <p className="text-[13px] font-medium uppercase tracking-wide text-muted-foreground">
                      {(ownerKindLabel || "Owner") as string}
                    </p>
                    <p className="mt-0.5 font-mono text-[18px] font-semibold text-foreground">
                      {activeOwner as string}
                    </p>
                  </div>
                  @if (customQuota) {
                    <span className="rounded-md bg-muted px-2 py-1 text-[12px] font-medium text-foreground">
                      Custom quota
                    </span>;
                  } @else {
                    <span className="rounded-md bg-muted px-2 py-1 text-[12px] font-medium text-muted-foreground">
                      Instance default
                    </span>;
                  }
                </div>

                <div className="mt-5">
                  <div className="flex flex-wrap items-baseline justify-between gap-2 text-[14px]">
                    <span className="font-medium text-foreground">
                      {formatBytes(usage.used_bytes) as string}
                      <span className="font-normal text-muted-foreground">
                        {" used of "}
                        {formatBytes(usage.quota_bytes) as string}
                      </span>
                    </span>
                    <span className="text-muted-foreground">{`${percent}%` as string}</span>
                  </div>
                  <div
                    className="mt-2 h-2 overflow-hidden rounded-full bg-muted"
                    role="progressbar"
                    aria-valuemin={0}
                    aria-valuemax={100}
                    aria-valuenow={percent}
                    aria-label="Package storage used"
                  >
                    <div
                      className="h-full rounded-full bg-primary transition-[width]"
                      style={{ width: `${percent}%` }}
                    />
                  </div>
                  <p className="mt-2 text-[13px] text-muted-foreground">
                    Instance default for new owners:{" "}
                    {formatBytes(usage.default_quota_bytes) as string}
                  </p>
                </div>
              </div>

              <section aria-labelledby="packages-quota-edit-heading">
                <h3
                  id="packages-quota-edit-heading"
                  className="text-[16px] font-semibold text-foreground"
                >
                  Set custom quota
                </h3>
                <p className="mt-1 text-[14px] text-muted-foreground">
                  Overrides the instance default for this owner only. Uploads that would
                  exceed the quota are rejected.
                </p>
                <form
                  className="mt-4 flex flex-col gap-4"
                  method="post"
                  action="#"
                  onSubmit={(e) => {
                    e.preventDefault();
                    void saveQuota();
                  }}
                >
                  <ByteQuotaField
                    id="admin-packages-quota"
                    label="Storage limit"
                    value={draft.amount}
                    unit={draft.unit}
                    disabled={pending}
                    overridden={customQuota}
                    onValueChange={(next) => setDraft({ ...draft, amount: next })}
                    onUnitChange={(next) =>
                      setDraft({
                        amount: convertUnitDisplay(draft.amount, draft.unit, next),
                        unit: next,
                      })
                    }
                  />
                  <div className="flex flex-wrap gap-2">
                    <Button
                      type="submit"
                      disabled={pending}
                      data-testid="admin-packages-set-quota"
                    >
                      {(pending ? "Saving…" : "Save quota") as string}
                    </Button>
                    <Button
                      type="button"
                      variant="secondary"
                      disabled={pending || !customQuota}
                      onClick={() => void applyInstanceDefault()}
                      data-testid="admin-packages-use-default"
                    >
                      Use instance default
                    </Button>
                  </div>
                </form>
              </section>

              <section aria-labelledby="packages-by-format-heading">
                <h3
                  id="packages-by-format-heading"
                  className="text-[16px] font-semibold text-foreground"
                >
                  By format
                </h3>
                @if (usage.by_format.length === 0) {
                  <p className="mt-2 text-[14px] text-muted-foreground">
                    No package blobs stored for this owner yet.
                  </p>;
                } @else {
                  <ul className="mt-3 divide-y divide-border rounded-md border border-border">
                    @for (const row of usage.by_format; key row.format) {
                      <li className="flex items-center justify-between gap-3 px-3 py-2.5 text-[14px]">
                        <span className="font-medium text-foreground">
                          {packageFormatLabel(row.format) as string}
                        </span>
                        <span className="text-muted-foreground">
                          {formatBytes(row.bytes) as string}
                        </span>
                      </li>;
                    }
                  </ul>;
                }
              </section>

              <section aria-labelledby="packages-list-heading">
                <h3
                  id="packages-list-heading"
                  className="text-[16px] font-semibold text-foreground"
                >
                  Packages
                </h3>
                @if (usage.packages.length === 0) {
                  <p className="mt-2 text-[14px] text-muted-foreground">
                    This owner has no packages yet.
                  </p>;
                } @else {
                  <ul
                    className="mt-3 divide-y divide-border rounded-md border border-border"
                    data-testid="admin-packages-list"
                  >
                    @for (const pkg of usage.packages; key pkg.package_id) {
                      <li className="flex flex-wrap items-center justify-between gap-2 px-3 py-2.5 text-[14px]">
                        <span className="min-w-0">
                          <span className="font-medium text-foreground">
                            {pkg.name as string}
                          </span>
                          <span className="ml-2 text-muted-foreground">
                            {packageFormatLabel(pkg.format) as string}
                          </span>
                        </span>
                        <span className="shrink-0 text-muted-foreground">
                          {formatBytes(pkg.bytes) as string}
                        </span>
                      </li>;
                    }
                  </ul>;
                }
              </section>
            </div>;
          }
        </AdminSettingsLayout>;
      }
    }
  }
}
