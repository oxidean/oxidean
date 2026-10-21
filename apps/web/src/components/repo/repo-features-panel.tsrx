import type { RepoPublic } from "@oxidean/api-client";
import { useState } from "octane";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { apiClient } from "@/lib/api-client";
import { useRepoStore } from "@/lib/repo-store";
import { toastSuccess } from "@/lib/toast";

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";

type Unit = "issues" | "pulls" | "wiki" | "projects" | "discussions";

const UNIT_LABEL: Record<Unit, string> = {
  issues: "Issues",
  pulls: "Pull requests",
  wiki: "Wiki",
  projects: "Projects",
  discussions: "Discussions",
};

/**
 * Repo Settings → Features — per-repo unit toggles for Issues / Pulls / Wiki /
 * Projects (COL-13). Writes go through `repo.{unit}.setEnabled` (Admin-only
 * RPCs); the store update keeps the repo chrome tabs in sync immediately.
 */
export function RepoFeaturesPanel(props: {
  owner: string;
  name: string;
  can_admin: boolean;
  repo: RepoPublic;
}) @{
  const owner = props.owner;
  const name = props.name;
  const canAdmin = props.can_admin;
  const storeRepo = useRepoStore((s) => s.repo);
  const setRepo = useRepoStore((s) => s.setRepo);
  // Store is source of truth (same merge as repo-chrome) so toggles here
  // update the Issues/Pulls tabs without a reload.
  const live = storeRepo && storeRepo.id === props.repo.id ? storeRepo : props.repo;
  const issuesEnabled = live.issues_enabled !== false;
  const pullsEnabled = live.pulls_enabled !== false;
  const wikiEnabled = live.wiki_enabled !== false;
  const projectsEnabled = live.projects_enabled !== false;
  const discussionsEnabled = live.discussions_enabled !== false;

  const [pendingUnit, setPendingUnit] = useState<"" | Unit>("");
  const [error, setError] = useState("");

  async function setUnit(unit: Unit, next: boolean) {
    if (!canAdmin || pendingUnit) return;
    setError("");
    setPendingUnit(unit);
    try {
      const res = await apiClient.repo[unit].setEnabled({ owner, name, enabled: next });
      if (!res.ok) {
        setError(res.error.message || "Could not update repository features.");
        setPendingUnit("");
        return;
      }
      const flag = `${unit}_enabled` as keyof RepoPublic;
      setRepo({ ...live, [flag]: res.data.enabled });
      toastSuccess(next ? `${UNIT_LABEL[unit]} enabled.` : `${UNIT_LABEL[unit]} disabled.`);
      setPendingUnit("");
    } catch {
      setError(NETWORK_ERROR);
      setPendingUnit("");
    }
  }

  <section
    id="features"
    className="mt-12 flex flex-col gap-4"
    aria-labelledby="features-heading"
    data-testid="repo-features-settings"
  >
    <h2
      id="features-heading"
      className="font-[family-name:var(--font-display)] text-[18px] font-semibold text-foreground"
    >
      Features
    </h2>
    <p className="text-[14px] text-muted-foreground">
      Turn repository units on or off. Disabling hides the tab and rejects new issue / pull activity
      — nothing is deleted and everything returns when re-enabled.
    </p>

    <div className="flex flex-col gap-4">
      <div className="flex items-center gap-3">
        <Switch
          checked={issuesEnabled}
          disabled={!canAdmin || pendingUnit !== ""}
          onCheckedChange={(v) => void setUnit("issues", !!v)}
          data-testid="repo-issues-enabled"
          aria-label="Enable Issues"
        />
        <Label className="text-[14px]">Issues</Label>
      </div>
      <div className="flex items-center gap-3">
        <Switch
          checked={pullsEnabled}
          disabled={!canAdmin || pendingUnit !== ""}
          onCheckedChange={(v) => void setUnit("pulls", !!v)}
          data-testid="repo-pulls-enabled"
          aria-label="Enable pull requests"
        />
        <Label className="text-[14px]">Pull requests</Label>
      </div>
      <div className="flex items-center gap-3">
        <Switch
          checked={wikiEnabled}
          disabled={!canAdmin || pendingUnit !== ""}
          onCheckedChange={(v) => void setUnit("wiki", !!v)}
          data-testid="repo-wiki-enabled"
          aria-label="Enable wiki"
        />
        <Label className="text-[14px]">Wiki</Label>
      </div>
      <div className="flex items-center gap-3">
        <Switch
          checked={projectsEnabled}
          disabled={!canAdmin || pendingUnit !== ""}
          onCheckedChange={(v) => void setUnit("projects", !!v)}
          data-testid="repo-projects-enabled"
          aria-label="Enable projects"
        />
        <Label className="text-[14px]">Projects</Label>
      </div>
      <div className="flex items-center gap-3">
        <Switch
          checked={discussionsEnabled}
          disabled={!canAdmin || pendingUnit !== ""}
          onCheckedChange={(v) => void setUnit("discussions", !!v)}
          data-testid="repo-discussions-enabled"
          aria-label="Enable discussions"
        />
        <Label className="text-[14px]">Discussions</Label>
      </div>
    </div>

    @if (error) {
      <p className="text-destructive" role="alert">
        {error as string}
      </p>;
    }
  </section>;
}
