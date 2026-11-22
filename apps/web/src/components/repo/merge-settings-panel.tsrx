import type { RepoMergeSettings } from "@oxidean/api-client";
import { useQuery, useQueryClient } from "@octanejs/tanstack-query";
import { useState } from "octane";
import { apiClient } from "@/lib/api-client";
import { toastSuccess } from "@/lib/toast";

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";

function mergeSettingsQueryKey(owner: string, name: string) {
  return ["mergeSettings", owner, name] as const;
}

/** Repo Settings merge strategy toggles (PR-07 / D-PR-18). */
export function MergeSettingsPanel(props: {
  owner: string;
  name: string;
  /** Server Capability Admin — toggle only when true. */
  can_admin: boolean;
}) @{
  const owner = props.owner;
  const name = props.name;
  const canAdmin = props.can_admin;
  const queryClient = useQueryClient();

  const settingsQ = useQuery({
    queryKey: mergeSettingsQueryKey(owner, name),
    queryFn: async (): Promise<RepoMergeSettings> => {
      const res = await apiClient.mergeSettings.get({ owner, name });
      if (!res.ok) throw new Error(res.error.message || "Could not load merge settings.");
      return res.data;
    },
    enabled: canAdmin,
  });

  const [pending, setPending] = useState(false);
  const [error, setError] = useState("");

  async function saveNext(next: RepoMergeSettings) {
    if (!canAdmin) return;
    setError("");
    setPending(true);
    try {
      const res = await apiClient.mergeSettings.update({
        owner,
        name,
        allow_merge_commit: next.allow_merge_commit,
        allow_squash_merge: next.allow_squash_merge,
        allow_rebase_merge: next.allow_rebase_merge,
        allow_auto_merge: next.allow_auto_merge,
      });
      if (!res.ok) {
        setError(res.error.message || "Could not update merge settings.");
        setPending(false);
        return;
      }
      await queryClient.invalidateQueries({ queryKey: mergeSettingsQueryKey(owner, name) });
      toastSuccess("Merge settings updated.");
      setPending(false);
    } catch {
      setError(NETWORK_ERROR);
      setPending(false);
    }
  }

  const s = settingsQ.data;

  <section
    className="mt-12 flex flex-col gap-4"
    aria-labelledby="merge-settings-heading"
    data-testid="merge-settings"
  >
    <h2
      id="merge-settings-heading"
      className="font-[family-name:var(--font-display)] text-[18px] font-semibold text-foreground"
    >
      Pull request merges
    </h2>
    <p className="text-[14px] text-muted-foreground">
      Choose which merge strategies are available on this repository.
    </p>

    @if (!canAdmin) {
      <p className="text-[14px] text-muted-foreground">
        Only repository admins can change merge strategies.
      </p>;
    } @else if (settingsQ.isPending) {
      <p className="text-[14px] text-muted-foreground">Loading merge settings…</p>;
    } @else if (settingsQ.isError) {
      <p role="alert" className="text-[14px] text-destructive">
        {
          (settingsQ.error instanceof Error
            ? settingsQ.error.message
            : "Could not load merge settings.") as string
        }
      </p>;
    } @else {
      <>
        @if (s) {
          <div className="flex flex-col gap-2 text-[14px]">
            <label className="flex items-center gap-2">
              <input
                type="checkbox"
                checked={s.allow_merge_commit}
                disabled={pending}
                onChange={(e) =>
                  void saveNext({
                    allow_merge_commit: (e.target as HTMLInputElement).checked,
                    allow_squash_merge: s.allow_squash_merge,
                    allow_rebase_merge: s.allow_rebase_merge,
                    allow_auto_merge: s.allow_auto_merge,
                  })
                }
              />
              Allow merge commits
            </label>
            <label className="flex items-center gap-2">
              <input
                type="checkbox"
                checked={s.allow_squash_merge}
                disabled={pending}
                onChange={(e) =>
                  void saveNext({
                    allow_merge_commit: s.allow_merge_commit,
                    allow_squash_merge: (e.target as HTMLInputElement).checked,
                    allow_rebase_merge: s.allow_rebase_merge,
                    allow_auto_merge: s.allow_auto_merge,
                  })
                }
              />
              Allow squash merging
            </label>
            <label className="flex items-center gap-2">
              <input
                type="checkbox"
                checked={s.allow_rebase_merge}
                disabled={pending}
                onChange={(e) =>
                  void saveNext({
                    allow_merge_commit: s.allow_merge_commit,
                    allow_squash_merge: s.allow_squash_merge,
                    allow_rebase_merge: (e.target as HTMLInputElement).checked,
                    allow_auto_merge: s.allow_auto_merge,
                  })
                }
              />
              Allow rebase merging
            </label>
            <label className="flex items-center gap-2">
              <input
                type="checkbox"
                checked={s.allow_auto_merge}
                disabled={pending}
                onChange={(e) =>
                  void saveNext({
                    allow_merge_commit: s.allow_merge_commit,
                    allow_squash_merge: s.allow_squash_merge,
                    allow_rebase_merge: s.allow_rebase_merge,
                    allow_auto_merge: (e.target as HTMLInputElement).checked,
                  })
                }
              />
              Allow auto-merge
            </label>
            <p className="text-[13px] text-muted-foreground">
              Auto-merge merges a pull request automatically once required checks and
              approvals pass. Pushing new commits disables it.
            </p>
          </div>;
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
