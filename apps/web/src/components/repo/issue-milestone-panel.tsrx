import type { IssuePublic, MilestonePublic } from "@oxidean/api-client";
import { useQuery, useQueryClient } from "@octanejs/tanstack-query";
import { useEffect, useState } from "octane";
import { AuthErrorBanner } from "@/components/auth-shell";
import { Button } from "@/components/ui/button";
import { apiClient } from "@/lib/api-client";

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";

type IssueMilestonePanelProps = {
  owner: string;
  repoName: string;
  number: number;
  issue: IssuePublic;
  canWrite: boolean;
};

/** Issue sidebar milestone — Write+ assigns a single milestone (D-ISS milestones). */
export function IssueMilestonePanel({
  owner,
  repoName,
  number,
  issue,
  canWrite,
}: IssueMilestonePanelProps) @{
  const queryClient = useQueryClient();
  const [selected, setSelected] = useState<number | null>(() => issue.milestone?.number ?? null);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState("");

  useEffect(() => {
    setSelected(issue.milestone?.number ?? null);
  }, [issue.id, issue.updated_at]);

  const catalogQ = useQuery({
    queryKey: ["milestone", "list", owner, repoName] as const,
    queryFn: async () => {
      const res = await apiClient.milestone.list({
        owner,
        name: repoName,
        state: "open",
      });
      if (!res.ok) throw new Error(res.error.message || NETWORK_ERROR);
      return res.data.milestones;
    },
    enabled: !!owner && !!repoName && canWrite,
  });

  const current: MilestonePublic | null = issue.milestone ?? null;
  const catalog: MilestonePublic[] = catalogQ.data ?? [];
  const dirty = selected !== (current?.number ?? null);

  async function save() {
    setError("");
    setPending(true);
    try {
      const res = await apiClient.issue.milestone.set({
        owner,
        name: repoName,
        number,
        milestone: selected,
      });
      if (!res.ok) {
        setError(res.error.message || "Could not update milestone.");
        setPending(false);
        return;
      }
      await queryClient.invalidateQueries({
        queryKey: ["issue", "get", owner, repoName, number],
      });
      await queryClient.invalidateQueries({
        queryKey: ["issue", "list", owner, repoName],
      });
      setPending(false);
    } catch {
      setError(NETWORK_ERROR);
      setPending(false);
    }
  }

  <section aria-labelledby="issue-milestone-heading">
    <h2 id="issue-milestone-heading" className="text-[14px] font-semibold text-foreground">
      Milestone
    </h2>
    @if (error) {
      <div className="mt-2">
        <AuthErrorBanner message={error} />
      </div>;
    }
    @if (canWrite) {
      <div className="mt-2 flex flex-col gap-2">
        @if (catalogQ.isPending) {
          <p className="text-[13px] text-muted-foreground">Loading milestones…</p>;
        } @else {
          @if (catalog.length === 0) {
            <p className="text-[13px] text-muted-foreground">
              No milestones in this repository yet.
            </p>;
          } @else {
            <select
              aria-label="Milestone"
              className="h-9 rounded-md border border-border bg-background px-3 text-[13px] text-foreground"
              value={selected === null ? "" : String(selected)}
              disabled={pending}
              onChange={(e) => {
                const v = (e.target as HTMLSelectElement).value;
                setSelected(v === "" ? null : Number(v));
              }}
            >
              <option value="">No milestone</option>
              @for (const m of catalog; key m.id) {
                <option value={String(m.number) as string}>
                  {m.title as string}
                </option>;
              }
            </select>;
          }
        }
        <Button
          type="button"
          size="sm"
          variant="outline"
          disabled={pending || catalogQ.isPending || !dirty}
          onClick={save}
        >
          Save milestone
        </Button>
      </div>;
    } @else {
      <div className="mt-2">
        @if (!current) {
          <p className="text-[13px] text-muted-foreground">No milestone</p>;
        } @else {
          <p className="text-[13px] text-foreground">{current.title as string}</p>;
        }
      </div>;
    }
  </section>;
}
