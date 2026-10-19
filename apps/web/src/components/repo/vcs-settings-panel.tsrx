import type { VcsStatusResponse } from "@oxidean/api-client";
import { useQuery, useQueryClient } from "@octanejs/tanstack-query";
import { useState } from "octane";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { apiClient } from "@/lib/api-client";
import { toastSuccess } from "@/lib/toast";

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";

const DISABLED_SHAPE: VcsStatusResponse = {
  enabled: false,
  heads: [],
  changeset_count: 0,
  object_count: 0,
  storage_bytes: 0,
};

function vcsStatusQueryKey(owner: string, name: string) {
  return ["repo", "vcs", "status", owner, name] as const;
}

/**
 * Repo Settings "Source control protocols" — the per-repo VCS flag
 * (FR1.1/AC1.1.1). `vcs.status`'s `vcs.disabled` envelope is the
 * never-enabled/disabled wire shape (AC1.2.3) and renders as an unchecked
 * default; only other failures become the alert state.
 */
export function VcsSettingsPanel(props: {
  owner: string;
  name: string;
  /** Server Capability Admin — the settings page itself already gates on it. */
  can_admin: boolean;
  /** Archived repositories are read-only — the control is disabled. */
  archived: boolean;
}) @{
  const owner = props.owner;
  const name = props.name;
  const canAdmin = props.can_admin;
  const archived = props.archived;
  const queryClient = useQueryClient();

  const statusQ = useQuery({
    queryKey: vcsStatusQueryKey(owner, name),
    queryFn: async (): Promise<VcsStatusResponse> => {
      const res = await apiClient.vcs.status({ owner, name });
      if (!res.ok) {
        if (res.error.code === "vcs.disabled") return DISABLED_SHAPE;
        throw new Error(res.error.message || "Could not load VCS status.");
      }
      return res.data;
    },
  });

  // null = follow the server value; set once the admin edits the checkbox.
  const [draft, setDraft] = useState<boolean | null>(null);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState("");

  const enabled = statusQ.data?.enabled ?? false;
  const checked = draft ?? enabled;
  const dirty = draft !== null && draft !== enabled;
  const controlsDisabled = pending || archived || !canAdmin;

  async function save() {
    if (controlsDisabled || !dirty) return;
    const next = checked;
    setError("");
    setPending(true);
    try {
      const res = await apiClient.vcs.repoEnable({ owner, name, enabled: next });
      if (!res.ok) {
        setError(res.error.message || "Could not update VCS setting.");
        setPending(false);
        return;
      }
      setDraft(res.data.enabled);
      await queryClient.invalidateQueries({ queryKey: vcsStatusQueryKey(owner, name) });
      toastSuccess(next ? "VCS protocol enabled." : "VCS protocol disabled.");
      setPending(false);
    } catch {
      setError(NETWORK_ERROR);
      setPending(false);
    }
  }

  <section className="mt-12 flex flex-col gap-4" data-testid="vcs-settings-panel">
    @if (statusQ.isPending) {
      <>
        <h2 className="font-[family-name:var(--font-display)] text-[18px] font-semibold text-foreground">
          Source control protocols
        </h2>
        <p className="text-[14px] text-muted-foreground">Loading VCS status…</p>
      </>;
    } @else if (statusQ.isError) {
      <>
        <h2 className="font-[family-name:var(--font-display)] text-[18px] font-semibold text-foreground">
          Source control protocols
        </h2>
        <p role="alert" className="text-[14px] text-destructive">
          {
            (statusQ.error instanceof Error
              ? statusQ.error.message
              : "Could not load VCS status.") as string
          }
        </p>
      </>;
    } @else {
      <form
        method="post"
        action="#"
        className="flex flex-col gap-4"
        onSubmit={(e) => {
          e.preventDefault();
          void save();
        }}
      >
        <fieldset className="flex flex-col gap-3" aria-describedby="vcs-protocols-desc">
          <legend className="font-[family-name:var(--font-display)] text-[18px] font-semibold text-foreground">
            Source control protocols
          </legend>
          <p id="vcs-protocols-desc" className="text-[14px] text-muted-foreground">
            Enable the Oxidean VCS protocol for this repository. It stores agent-friendly
            changesets server-side next to Git — Git clones, pushes, and the protected-review
            flow keep working unchanged.
          </p>
          <label className="flex cursor-pointer items-center gap-3">
            <Checkbox
              checked={checked}
              disabled={controlsDisabled}
              aria-label="Enable the VCS protocol for this repository"
              onCheckedChange={(v) => setDraft(v === true)}
            />
            <span className="text-[14px] text-foreground">
              Enable the Oxidean VCS protocol
            </span>
          </label>
          @if (archived) {
            <p className="text-[13px] text-muted-foreground">
              Archived repositories are read-only; unarchive to change this setting.
            </p>;
          }
          @if (!canAdmin) {
            <p className="text-[13px] text-muted-foreground">
              Only repository admins can change VCS settings.
            </p>;
          }
        </fieldset>

        @if (error) {
          <p role="alert" className="text-[14px] text-destructive">
            {error as string}
          </p>;
        }

        <div>
          <Button
            type="submit"
            disabled={controlsDisabled || !dirty}
            className="w-full sm:w-auto"
          >
            {(pending ? "Saving…" : "Save protocol setting") as string}
          </Button>
        </div>
      </form>;
    }
  </section>;
}
