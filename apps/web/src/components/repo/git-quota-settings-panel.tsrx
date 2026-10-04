import type { RepoQuotaPublic } from "@oxidean/api-client";
import { useQuery, useQueryClient } from "@octanejs/tanstack-query";
import { useEffect, useState } from "octane";
import { ByteQuotaField } from "@/components/admin/byte-quota-field";
import { Button } from "@/components/ui/button";
import { apiClient } from "@/lib/api-client";
import {
  bytesToUnitString,
  convertUnitDisplay,
  formatBytes,
  pickDisplayUnit,
  unitValueToBytes,
  type ByteUnit,
} from "@/lib/byte-units";
import { toastSuccess } from "@/lib/toast";

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";

function gitQuotaQueryKey(owner: string, name: string) {
  return ["repo", "git-quota", owner, name] as const;
}

/** Repo Settings git object size usage + quota override (GIT-25). */
export function GitQuotaSettingsPanel(props: {
  owner: string;
  name: string;
  /** Server Capability Admin — quota edit only when true. */
  can_admin: boolean;
}) @{
  const owner = props.owner;
  const name = props.name;
  const canAdmin = props.can_admin;
  const queryClient = useQueryClient();

  const quotaQ = useQuery({
    queryKey: gitQuotaQueryKey(owner, name),
    queryFn: async (): Promise<RepoQuotaPublic> => {
      const res = await apiClient.repo.quota.get({ owner, name });
      if (!res.ok) throw new Error(res.error.message || "Could not load storage quota.");
      return res.data;
    },
  });

  const [pending, setPending] = useState(false);
  const [error, setError] = useState("");
  const [amount, setAmount] = useState("");
  const [unit, setUnit] = useState<ByteUnit>("GiB");

  const quota = quotaQ.data;

  // Seed the editor from each loaded response (override when set, else the
  // resolved instance default).
  useEffect(() => {
    if (!quotaQ.isSuccess || !quotaQ.data) return;
    const seed = quotaQ.data.size_quota_bytes ?? quotaQ.data.instance_quota_bytes;
    const seedBytes = Math.max(0, seed);
    const seedUnit = pickDisplayUnit(seedBytes);
    setAmount(bytesToUnitString(seedBytes, seedUnit));
    setUnit(seedUnit);
  }, [quotaQ.isSuccess, quotaQ.dataUpdatedAt]);

  async function saveOverride() {
    if (!canAdmin || !quota) return;
    const bytes = unitValueToBytes(amount, unit);
    if (bytes === null) {
      setError("Enter a valid numeric quota amount (0 = unlimited).");
      return;
    }
    setError("");
    setPending(true);
    try {
      const res = await apiClient.repo.quota.set({
        owner,
        name,
        size_quota_bytes: bytes,
      });
      if (!res.ok) {
        setError(res.error.message || "Could not save the storage quota.");
        setPending(false);
        return;
      }
      await queryClient.invalidateQueries({ queryKey: gitQuotaQueryKey(owner, name) });
      toastSuccess("Repository storage quota updated.");
      setPending(false);
    } catch {
      setError(NETWORK_ERROR);
      setPending(false);
    }
  }

  async function clearOverride() {
    if (!canAdmin) return;
    setError("");
    setPending(true);
    try {
      const res = await apiClient.repo.quota.set({
        owner,
        name,
        size_quota_bytes: null,
      });
      if (!res.ok) {
        setError(res.error.message || "Could not revert to the instance default.");
        setPending(false);
        return;
      }
      await queryClient.invalidateQueries({ queryKey: gitQuotaQueryKey(owner, name) });
      toastSuccess("Using the instance storage quota default.");
      setPending(false);
    } catch {
      setError(NETWORK_ERROR);
      setPending(false);
    }
  }

  const sizeBytes = quota?.size_bytes ?? 0;
  const effectiveQuota = quota?.effective_quota_bytes ?? null;
  const hasOverride = quota?.size_quota_bytes != null;
  const usedPct =
    effectiveQuota && effectiveQuota > 0
      ? Math.min(100, Math.round((sizeBytes / effectiveQuota) * 100))
      : null;
  const overQuota = effectiveQuota !== null && sizeBytes > effectiveQuota;

  <section
    className="mt-12 flex flex-col gap-4"
    aria-labelledby="git-quota-heading"
    data-testid="repo-git-quota-settings"
  >
    <h2
      id="git-quota-heading"
      className="font-[family-name:var(--font-display)] text-[18px] font-semibold text-foreground"
    >
      Storage
    </h2>
    <p className="text-[14px] text-muted-foreground">
      Git object data on disk (the bare repository). Pushes are rejected once the repository is over
      quota; deleting refs stays allowed. Enter <code className="text-[13px]">0</code> for an
      unlimited override. LFS and package objects have their own quotas and do not count here.
    </p>

    @if (quotaQ.isPending) {
      <p className="text-[14px] text-muted-foreground">Loading storage usage…</p>;
    } @else if (quotaQ.isError) {
      <p role="alert" className="text-[14px] text-destructive">
        {
          (quotaQ.error instanceof Error
            ? quotaQ.error.message
            : "Could not load storage quota.") as string
        }
      </p>;
    } @else {
      <>
        <div
          className="flex flex-col gap-2 rounded-md border border-border p-4"
          aria-labelledby="git-quota-usage-heading"
        >
          <h3
            id="git-quota-usage-heading"
            className="text-[15px] font-medium text-foreground"
          >
            Usage
          </h3>
          <p className="text-[14px] text-foreground">
            {
              (effectiveQuota === null
                ? `${formatBytes(sizeBytes)} used — unlimited`
                : `${formatBytes(sizeBytes)} of ${formatBytes(effectiveQuota)} used (${usedPct ?? 0}%)`) as string
            }
          </p>
          @if (effectiveQuota !== null) {
            <div
              role="progressbar"
              aria-label="Repository storage used"
              aria-valuemin={0}
              aria-valuemax={100}
              aria-valuenow={usedPct ?? 0}
              className="h-2 w-full overflow-hidden rounded-full bg-muted"
            >
              <div
                className={overQuota ? "h-full bg-destructive" : "h-full bg-primary"}
                style={{ width: `${usedPct ?? 0}%` }}
              />
            </div>;
          }
          @if (overQuota) {
            <p role="alert" className="text-[14px] text-destructive">
              This repository is over its storage quota — new pushes are rejected until
              it is back under quota.
            </p>;
          }
          <p className="text-[13px] text-muted-foreground">
            {
              (hasOverride
                ? "Repository override active."
                : (quota?.instance_quota_bytes ?? 0) > 0
                  ? `Using the instance default (${formatBytes(quota?.instance_quota_bytes ?? 0)}).`
                  : "Using the instance default (unlimited).") as string
            }
          </p>
        </div>

        @if (canAdmin) {
          <div className="flex max-w-md flex-col gap-3">
            <ByteQuotaField
              id="git-quota-override"
              label="Repository quota"
              value={amount}
              unit={unit}
              overridden={hasOverride}
              disabled={pending}
              onValueChange={setAmount}
              onUnitChange={(next) => {
                setAmount(convertUnitDisplay(amount, unit, next));
                setUnit(next);
              }}
            />
            <div className="flex flex-wrap gap-2">
              <Button
                type="button"
                disabled={pending}
                onClick={() => void saveOverride()}
                data-testid="git-quota-save"
              >
                {(pending ? "Saving…" : "Save quota") as string}
              </Button>
              @if (hasOverride) {
                <Button
                  type="button"
                  variant="secondary"
                  disabled={pending}
                  onClick={() => void clearOverride()}
                  data-testid="git-quota-clear"
                >
                  Use instance default
                </Button>;
              }
            </div>
          </div>;
        } @else {
          <p className="text-[14px] text-muted-foreground">
            Only repository admins can change the storage quota.
          </p>;
        }

        @if (error) {
          <p role="alert" className="text-[14px] text-destructive">
            {error as string}
          </p>;
        }
      </>;
    }
  </section>;
}
