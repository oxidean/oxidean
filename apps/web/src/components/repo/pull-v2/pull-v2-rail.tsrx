import { useQuery, useQueryClient } from "@octanejs/tanstack-query";
import type { IssueAssigneePublic, LabelPublic, PullPublic } from "@oxidean/api-client";
import { useEffect, useState } from "octane";
import { X } from "@octanejs/lucide";
import { AppLink } from "@/components/ui/app-link";
import { Button } from "@/components/ui/button";
import { apiClient } from "@/lib/api-client";
import { cn } from "@/lib/utils";

type PullV2RailProps = {
  owner: string;
  repoName: string;
  number: number;
  pull: PullPublic;
  canWrite: boolean;
};

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";

function swatchStyle(color: string): { backgroundColor: string } {
  const hex = color.startsWith("#") ? color : `#${color}`;
  return { backgroundColor: hex };
}

function pullLabels(pull: PullPublic): LabelPublic[] {
  return pull.labels ?? [];
}

/** Reviewers section — pull.reviewRequests.* (existing RPCs). */
function RailReviewers(props: PullV2RailProps) @{
  const queryClient = useQueryClient();
  const [requestUser, setRequestUser] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  const requestsQ = useQuery({
    queryKey: ["pull", "reviewRequests", props.owner, props.repoName, props.number] as const,
    queryFn: async () => {
      const res = await apiClient.pull.reviewRequests.list({
        owner: props.owner,
        name: props.repoName,
        number: props.number,
      });
      if (!res.ok) throw new Error(res.error.message || NETWORK_ERROR);
      return res.data;
    },
  });
  const requested: string[] = requestsQ.data?.usernames ?? [];

  async function invalidate() {
    await queryClient.invalidateQueries({
      queryKey: ["pull", "reviewRequests", props.owner, props.repoName, props.number],
    });
  }

  async function add() {
    if (!requestUser.trim()) return;
    setBusy(true);
    setError("");
    const res = await apiClient.pull.reviewRequests.add({
      owner: props.owner,
      name: props.repoName,
      number: props.number,
      username: requestUser.trim(),
    });
    setBusy(false);
    if (!res.ok) {
      setError(res.error.message || "Could not request review");
      return;
    }
    setRequestUser("");
    await invalidate();
  }

  async function remove(username: string) {
    setBusy(true);
    setError("");
    const res = await apiClient.pull.reviewRequests.remove({
      owner: props.owner,
      name: props.repoName,
      number: props.number,
      username,
    });
    setBusy(false);
    if (!res.ok) {
      setError(res.error.message || "Could not remove reviewer");
      return;
    }
    await invalidate();
  }

  <section aria-labelledby="pull-v2-reviewers-heading" data-testid="pull-v2-rail-reviewers">
    <h3 id="pull-v2-reviewers-heading" className="text-[13px] font-semibold text-foreground">
      Reviewers
    </h3>
    <div className="mt-1.5">
      @if (requested.length === 0) {
        <p className="text-[13px] text-muted-foreground">No reviewers requested</p>;
      } @else {
        <ul className="flex flex-col gap-1">
          @for (const u of requested; key u) {
            <li className="flex items-center gap-1.5 text-[13px]">
              <span className="min-w-0 flex-1 truncate font-medium text-foreground">
                {`@${u}` as string}
              </span>
              @if (props.canWrite) {
                <button
                  type="button"
                  className="text-muted-foreground hover:text-foreground"
                  aria-label={`Remove reviewer ${u}`}
                  disabled={busy}
                  onClick={() => void remove(u)}
                >
                  <X aria-hidden size={12} />
                </button>;
              }
            </li>;
          }
        </ul>;
      }
    </div>
    @if (props.canWrite) {
      <div className="mt-2 flex gap-1.5">
        <input
          className="min-w-0 flex-1 rounded-md border border-border bg-background px-2 py-1 text-[13px]"
          value={requestUser}
          placeholder="Request reviewer"
          data-testid="pull-v2-reviewer-input"
          onInput={(e) => setRequestUser((e.currentTarget as HTMLInputElement).value)}
        />
        <Button
          type="button"
          size="sm"
          variant="outline"
          disabled={busy || !requestUser.trim()}
          data-testid="pull-v2-reviewer-add"
          onClick={() => void add()}
        >
          Add
        </Button>
      </div>;
    }
    <p className={cn("mt-1 text-[12px] text-destructive", !error && "hidden")}>{error as string}</p>
  </section>;
}

/** Assignees section — issue.assigneeCandidates for the picker, pull.assignees.set to save. */
function RailAssignees(props: PullV2RailProps) @{
  const queryClient = useQueryClient();
  const [selected, setSelected] = useState<string[]>(() =>
    (props.pull.assignees ?? []).map((a) => a.user_id),
  );
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  useEffect(() => {
    setSelected((props.pull.assignees ?? []).map((a) => a.user_id));
  }, [props.pull.id, props.pull.updated_at]);

  const candidatesQ = useQuery({
    queryKey: ["issue", "assigneeCandidates", props.owner, props.repoName] as const,
    queryFn: async () => {
      const res = await apiClient.issue.assigneeCandidates({
        owner: props.owner,
        name: props.repoName,
      });
      if (!res.ok) throw new Error(res.error.message || NETWORK_ERROR);
      return res.data.users;
    },
    enabled: props.canWrite,
  });

  const current: IssueAssigneePublic[] = props.pull.assignees ?? [];
  const catalog: IssueAssigneePublic[] = candidatesQ.data ?? [];

  function toggle(userId: string) {
    setSelected((prev) =>
      prev.includes(userId) ? prev.filter((x) => x !== userId) : [...prev, userId],
    );
  }

  async function save() {
    setBusy(true);
    setError("");
    const res = await apiClient.pull.assignees.set({
      owner: props.owner,
      name: props.repoName,
      number: props.number,
      userIds: selected,
    });
    setBusy(false);
    if (!res.ok) {
      setError(res.error.message || "Could not update assignees.");
      return;
    }
    await queryClient.invalidateQueries({
      queryKey: ["pull", "get", props.owner, props.repoName, props.number],
    });
  }

  <section aria-labelledby="pull-v2-assignees-heading" data-testid="pull-v2-rail-assignees">
    <h3 id="pull-v2-assignees-heading" className="text-[13px] font-semibold text-foreground">
      Assignees
    </h3>
    <div className="mt-1.5">
      @if (current.length === 0) {
        <p className="text-[13px] text-muted-foreground">No one assigned</p>;
      } @else {
        <ul className="flex flex-col gap-1">
          @for (const a of current; key a.user_id) {
            <li className="truncate text-[13px] font-medium text-foreground">
              {`@${a.username}` as string}
            </li>;
          }
        </ul>;
      }
    </div>
    @if (props.canWrite) {
      <div className="mt-2 flex flex-col gap-1.5">
        @if (candidatesQ.isPending) {
          <p className="text-[12px] text-muted-foreground">Loading assignees…</p>;
        } @else {
          @if (catalog.length > 0) {
            <>
              <ul className="flex max-h-40 flex-col gap-1 overflow-y-auto">
                @for (const user of catalog; key user.user_id) {
                  <li>
                    <label className="flex cursor-pointer items-center gap-2 text-[13px] text-foreground">
                      <input
                        type="checkbox"
                        checked={selected.includes(user.user_id)}
                        disabled={busy}
                        onChange={() => toggle(user.user_id)}
                      />
                      <span>{user.username as string}</span>
                      @if (
                        user.display_name &&
                        user.display_name !== user.username
                      ) {
                        <span className="text-muted-foreground">
                          {`(${user.display_name})` as string}
                        </span>;
                      }
                    </label>
                  </li>;
                }
              </ul>
              <Button
                type="button"
                size="sm"
                variant="outline"
                disabled={busy}
                data-testid="pull-v2-assignees-save"
                onClick={() => void save()}
              >
                Save assignees
              </Button>
            </>;
          }
        }
      </div>;
    }
    <p className={cn("mt-1 text-[12px] text-destructive", !error && "hidden")}>{error as string}</p>
  </section>;
}

/** Labels section — label.listForRepo for the catalog, pull.labels.set to save. */
function RailLabels(props: PullV2RailProps) @{
  const queryClient = useQueryClient();
  const [selected, setSelected] = useState<string[]>(() => pullLabels(props.pull).map((l) => l.id));
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  useEffect(() => {
    setSelected(pullLabels(props.pull).map((l) => l.id));
  }, [props.pull.id, props.pull.updated_at]);

  const catalogQ = useQuery({
    queryKey: ["label", "listForRepo", props.owner, props.repoName] as const,
    queryFn: async () => {
      const res = await apiClient.label.listForRepo({
        owner: props.owner,
        name: props.repoName,
      });
      if (!res.ok) throw new Error(res.error.message || NETWORK_ERROR);
      return res.data.labels;
    },
    enabled: props.canWrite,
  });

  const current = pullLabels(props.pull);
  const catalog: LabelPublic[] = catalogQ.data ?? [];

  function toggle(id: string) {
    setSelected((prev) => (prev.includes(id) ? prev.filter((x) => x !== id) : [...prev, id]));
  }

  async function save() {
    setBusy(true);
    setError("");
    const res = await apiClient.pull.labels.set({
      owner: props.owner,
      name: props.repoName,
      number: props.number,
      labelIds: selected,
    });
    setBusy(false);
    if (!res.ok) {
      setError(res.error.message || "Could not update labels.");
      return;
    }
    await queryClient.invalidateQueries({
      queryKey: ["pull", "get", props.owner, props.repoName, props.number],
    });
  }

  <section aria-labelledby="pull-v2-labels-heading" data-testid="pull-v2-rail-labels">
    <h3 id="pull-v2-labels-heading" className="text-[13px] font-semibold text-foreground">
      Labels
    </h3>
    <div className="mt-1.5">
      @if (current.length === 0) {
        <p className="text-[13px] text-muted-foreground">None yet</p>;
      } @else {
        <ul className="flex flex-wrap gap-1.5">
          @for (const lab of current; key lab.id) {
            <li className="inline-flex items-center gap-1.5 rounded-md border border-border px-2 py-0.5 text-[12px] text-foreground">
              <span
                className="inline-block size-2.5 rounded-sm"
                style={swatchStyle(lab.color)}
                aria-hidden
              />
              {lab.name as string}
            </li>;
          }
        </ul>;
      }
    </div>
    @if (props.canWrite) {
      <div className="mt-2 flex flex-col gap-1.5">
        @if (catalogQ.isPending) {
          <p className="text-[12px] text-muted-foreground">Loading labels…</p>;
        } @else {
          @if (catalog.length > 0) {
            <>
              <ul className="flex max-h-40 flex-col gap-1 overflow-y-auto">
                @for (const lab of catalog; key lab.id) {
                  <li>
                    <label className="flex cursor-pointer items-center gap-2 text-[13px] text-foreground">
                      <input
                        type="checkbox"
                        checked={selected.includes(lab.id)}
                        disabled={busy}
                        onChange={() => toggle(lab.id)}
                      />
                      <span
                        className="inline-block size-3 shrink-0 rounded-sm border border-border"
                        style={swatchStyle(lab.color)}
                        aria-hidden
                      />
                      <span>{lab.name as string}</span>
                    </label>
                  </li>;
                }
              </ul>
              <Button
                type="button"
                size="sm"
                variant="outline"
                disabled={busy}
                data-testid="pull-v2-labels-save"
                onClick={() => void save()}
              >
                Save labels
              </Button>
            </>;
          }
        }
      </div>;
    }
    <p className={cn("mt-1 text-[12px] text-destructive", !error && "hidden")}>{error as string}</p>
  </section>;
}

/**
 * "Status and details" right rail for the v2 overview (WI-0001 S8):
 * reviewers / assignees / labels pickers plus a plain activity summary.
 */
export function PullV2Rail(props: PullV2RailProps) @{
  const reviewsQ = useQuery({
    queryKey: ["pull", "reviews", props.owner, props.repoName, props.number] as const,
    queryFn: async () => {
      const res = await apiClient.pull.reviews.list({
        owner: props.owner,
        name: props.repoName,
        number: props.number,
      });
      if (!res.ok) throw new Error(res.error.message || NETWORK_ERROR);
      return res.data;
    },
  });
  const commitsQ = useQuery({
    queryKey: ["pull", "commits", props.owner, props.repoName, props.number] as const,
    queryFn: async () => {
      const res = await apiClient.pull.commits({
        owner: props.owner,
        name: props.repoName,
        number: props.number,
      });
      if (!res.ok) throw new Error(res.error.message || NETWORK_ERROR);
      return res.data;
    },
  });

  const reviewCount = reviewsQ.data?.reviews.length ?? 0;
  const commitCount = commitsQ.data?.commits.length ?? 0;
  const commentCount = props.pull.comment_count ?? 0;

  <div className="space-y-5" data-testid="pull-v2-rail">
    <RailReviewers {...props} />
    <RailAssignees {...props} />
    <RailLabels {...props} />
    <section aria-labelledby="pull-v2-activity-heading" data-testid="pull-v2-rail-activity">
      <h3 id="pull-v2-activity-heading" className="text-[13px] font-semibold text-foreground">
        Activity
      </h3>
      <ul className="mt-1.5 space-y-0.5 text-[13px] text-muted-foreground">
        <li>{`${commitCount} commit${commitCount === 1 ? "" : "s"}` as string}</li>
        <li>{`${commentCount} comment${commentCount === 1 ? "" : "s"}` as string}</li>
        <li>{`${reviewCount} review${reviewCount === 1 ? "" : "s"}` as string}</li>
      </ul>
      <AppLink
        href={`/${props.owner}/${props.repoName}/activity` as string}
        className="mt-1.5 inline-block text-[13px] text-primary no-underline hover:underline"
        data-testid="pull-v2-activity-link"
      >
        View repository activity
      </AppLink>
    </section>
    {/* GitHub parity gap, intentionally absent: Milestone / Projects /
        Development / Deployments rail sections have no pull-scoped RPCs
        (pull.update carries no milestone, no pull-project linkage exists,
        deployments aren't PR-scoped). Add here when the domain lands. */}
  </div>;
}
