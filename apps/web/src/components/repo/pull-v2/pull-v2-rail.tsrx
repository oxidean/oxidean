import { useQuery, useQueryClient } from "@octanejs/tanstack-query";
import type { IssueAssigneePublic, LabelPublic, PullPublic } from "@oxidean/api-client";
import { useEffect, useState } from "octane";
import { CircleSlash, GitPullRequest, GitPullRequestDraft, Settings, X } from "@octanejs/lucide";
import { AppLink } from "@/components/ui/app-link";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { apiClient } from "@/lib/api-client";
import { cn } from "@/lib/utils";

type PullV2RailProps = {
  owner: string;
  repoName: string;
  number: number;
  pull: PullPublic;
  canWrite: boolean;
  /** Author-or-write — draft/close footer actions (can_edit_pull). */
  canEdit?: boolean;
};

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";

function swatchStyle(color: string): { backgroundColor: string } {
  const hex = color.startsWith("#") ? color : `#${color}`;
  return { backgroundColor: hex };
}

function pullLabels(pull: PullPublic): LabelPublic[] {
  return pull.labels ?? [];
}

/** Gear button that toggles a section's picker — GitHub's rail gear affordance. */
function RailGear(props: {
  label: string;
  open: boolean;
  onToggle: () => void;
  testid: string;
}) @{
  <button
    type="button"
    className="inline-flex size-5 items-center justify-center rounded-sm text-muted-foreground hover:bg-muted hover:text-foreground"
    aria-label={props.label as string}
    aria-expanded={props.open}
    data-testid={props.testid as string}
    onClick={() => props.onToggle()}
  >
    <Settings aria-hidden size={13} />
  </button>;
}

/** Reviewers section — pull.reviewRequests.* (existing RPCs). */
function RailReviewers(props: PullV2RailProps) @{
  const queryClient = useQueryClient();
  const [requestUser, setRequestUser] = useState("");
  const [pickerOpen, setPickerOpen] = useState(false);
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
    <div className="flex items-center justify-between gap-2">
      <h3 id="pull-v2-reviewers-heading" className="text-[13px] font-semibold text-foreground">
        Reviewers
      </h3>
      @if (props.canWrite) {
        <RailGear
          label="Edit reviewers"
          open={pickerOpen}
          onToggle={() => setPickerOpen(!pickerOpen)}
          testid="pull-v2-reviewers-gear"
        />;
      }
    </div>
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
    @if (props.canWrite && pickerOpen) {
      <div className="mt-2 flex gap-1.5" data-testid="pull-v2-reviewer-picker">
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
  const [pickerOpen, setPickerOpen] = useState(false);
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
    enabled: props.canWrite && pickerOpen,
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
    <div className="flex items-center justify-between gap-2">
      <h3 id="pull-v2-assignees-heading" className="text-[13px] font-semibold text-foreground">
        Assignees
      </h3>
      @if (props.canWrite) {
        <RailGear
          label="Edit assignees"
          open={pickerOpen}
          onToggle={() => setPickerOpen(!pickerOpen)}
          testid="pull-v2-assignees-gear"
        />;
      }
    </div>
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
    @if (props.canWrite && pickerOpen) {
      <div className="mt-2 flex flex-col gap-1.5" data-testid="pull-v2-assignees-picker">
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
  const [pickerOpen, setPickerOpen] = useState(false);
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
    enabled: props.canWrite && pickerOpen,
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
    <div className="flex items-center justify-between gap-2">
      <h3 id="pull-v2-labels-heading" className="text-[13px] font-semibold text-foreground">
        Labels
      </h3>
      @if (props.canWrite) {
        <RailGear
          label="Edit labels"
          open={pickerOpen}
          onToggle={() => setPickerOpen(!pickerOpen)}
          testid="pull-v2-labels-gear"
        />;
      }
    </div>
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
    @if (props.canWrite && pickerOpen) {
      <div className="mt-2 flex flex-col gap-1.5" data-testid="pull-v2-labels-picker">
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

/** Projects section — pull.projects.list; read-only links to each board. */
function RailProjects(props: PullV2RailProps) @{
  const linksQ = useQuery({
    queryKey: ["pull", "projects", props.owner, props.repoName, props.number] as const,
    queryFn: async () => {
      const res = await apiClient.pull.projects.list({
        owner: props.owner,
        name: props.repoName,
        number: props.number,
      });
      if (!res.ok) throw new Error(res.error.message || NETWORK_ERROR);
      return res.data.links;
    },
  });
  const links = linksQ.data ?? [];
  const linksError = linksQ.isError
    ? String(linksQ.error?.message ?? "Could not load projects.")
    : "";

  <section aria-labelledby="pull-v2-projects-heading" data-testid="pull-v2-rail-projects">
    <h3 id="pull-v2-projects-heading" className="text-[13px] font-semibold text-foreground">
      Projects
    </h3>
    <div className="mt-1.5">
      @if (linksQ.isPending) {
        <p className="text-[13px] text-muted-foreground">Loading…</p>;
      } @else {
        @if (linksQ.isError) {
          <p className="text-[13px] text-destructive">{linksError as string}</p>;
        } @else {
          @if (links.length === 0) {
            <p className="text-[13px] text-muted-foreground">None yet</p>;
          } @else {
            <ul className="flex flex-col gap-1">
              @for (const link of links; key link.card_id) {
                <li className="text-[13px]">
                  <AppLink
                    href={link.project_url as string}
                    className="font-medium text-foreground no-underline hover:text-primary hover:underline"
                    data-testid="pull-v2-rail-project-link"
                  >
                    {link.project_title as string}
                  </AppLink>
                  <span className="text-muted-foreground">
                    {` · ${link.column_name}` as string}
                  </span>
                  @if (link.project_state === "closed") {
                    <span className="text-muted-foreground"> (closed)</span>;
                  }
                </li>;
              }
            </ul>;
          }
        }
      }
    </div>
  </section>;
}

/** Milestone section — pull.milestone.set; open-milestone catalog via
 * milestone.list (same query key the issue sidebar uses). */
function RailMilestone(props: PullV2RailProps) @{
  const queryClient = useQueryClient();
  const [selected, setSelected] = useState<number | null>(
    () => props.pull.milestone?.number ?? null,
  );
  const [pickerOpen, setPickerOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  useEffect(() => {
    setSelected(props.pull.milestone?.number ?? null);
  }, [props.pull.id, props.pull.updated_at]);

  const catalogQ = useQuery({
    queryKey: ["milestone", "list", props.owner, props.repoName] as const,
    queryFn: async () => {
      const res = await apiClient.milestone.list({
        owner: props.owner,
        name: props.repoName,
        state: "open",
      });
      if (!res.ok) throw new Error(res.error.message || NETWORK_ERROR);
      return res.data.milestones;
    },
    enabled: props.canWrite && pickerOpen,
  });

  const current = props.pull.milestone ?? null;
  const catalog = catalogQ.data ?? [];
  // Keep a closed current milestone selectable so the picker doesn't
  // silently drop it from the options.
  const options =
    current && !catalog.some((m) => m.number === current.number) ? [current, ...catalog] : catalog;
  const dirty = selected !== (current?.number ?? null);

  async function save() {
    setBusy(true);
    setError("");
    try {
      const res = await apiClient.pull.milestone.set({
        owner: props.owner,
        name: props.repoName,
        number: props.number,
        milestone: selected,
      });
      if (!res.ok) {
        setError(res.error.message || "Could not update milestone.");
        return;
      }
      await queryClient.invalidateQueries({
        queryKey: ["pull", "get", props.owner, props.repoName, props.number],
      });
    } catch {
      setError(NETWORK_ERROR);
    } finally {
      setBusy(false);
    }
  }

  <section aria-labelledby="pull-v2-milestone-heading" data-testid="pull-v2-rail-milestone">
    <div className="flex items-center justify-between gap-2">
      <h3 id="pull-v2-milestone-heading" className="text-[13px] font-semibold text-foreground">
        Milestone
      </h3>
      @if (props.canWrite) {
        <RailGear
          label="Edit milestone"
          open={pickerOpen}
          onToggle={() => setPickerOpen(!pickerOpen)}
          testid="pull-v2-milestone-gear"
        />;
      }
    </div>
    <div className="mt-1.5">
      @if (!current) {
        <p className="text-[13px] text-muted-foreground">No milestone</p>;
      } @else {
        <>
          <p className="text-[13px]">
            <AppLink
              href={`/${props.owner}/${props.repoName}/issues/milestones` as string}
              className="font-medium text-foreground no-underline hover:text-primary hover:underline"
              data-testid="pull-v2-rail-milestone-link"
            >
              {current.title as string}
            </AppLink>
            <span className="text-muted-foreground">{` #${current.number}` as string}</span>
          </p>
          @if (
            current.open_issues != null ||
            current.closed_issues != null
          ) {
            <p className="mt-0.5 text-[12px] text-muted-foreground">
              {
                `${current.open_issues ?? 0} open · ${current.closed_issues ?? 0} closed` as string
              }
            </p>;
          }
        </>;
      }
    </div>
    @if (props.canWrite && pickerOpen) {
      <div className="mt-2 flex flex-col gap-1.5" data-testid="pull-v2-milestone-picker">
        @if (catalogQ.isPending) {
          <p className="text-[12px] text-muted-foreground">Loading milestones…</p>;
        } @else {
          @if (catalogQ.isError) {
            <p className="text-[12px] text-destructive">
              {
                String(
                  catalogQ.error?.message ?? "Could not load milestones.",
                ) as string
              }
            </p>;
          } @else {
            <>
              <select
                aria-label="Milestone"
                className="h-8 rounded-md border border-border bg-background px-2 text-[13px] text-foreground"
                value={selected === null ? "" : String(selected)}
                disabled={busy}
                data-testid="pull-v2-milestone-select"
                onChange={(e) => {
                  const v = (e.target as HTMLSelectElement).value;
                  setSelected(v === "" ? null : Number(v));
                }}
              >
                <option value="">No milestone</option>
                @for (const m of options; key m.id) {
                  <option value={String(m.number) as string}>
                    {`${m.title} (${m.open_issues ?? 0} open)` as string}
                  </option>;
                }
              </select>
              <Button
                type="button"
                size="sm"
                variant="outline"
                disabled={busy || !dirty}
                data-testid="pull-v2-milestone-save"
                onClick={() => void save()}
              >
                Save milestone
              </Button>
            </>;
          }
        }
      </div>;
    }
    <p className={cn("mt-1 text-[12px] text-destructive", !error && "hidden")} role="alert">
      {error as string}
    </p>
  </section>;
}

/** Development section — issues closed by title/body keywords
 * (pull.development.list). */
function RailDevelopment(props: PullV2RailProps) @{
  const devQ = useQuery({
    queryKey: ["pull", "development", props.owner, props.repoName, props.number] as const,
    queryFn: async () => {
      const res = await apiClient.pull.development.list({
        owner: props.owner,
        name: props.repoName,
        number: props.number,
      });
      if (!res.ok) throw new Error(res.error.message || NETWORK_ERROR);
      return res.data.items;
    },
  });
  const items = devQ.data ?? [];
  const devError = devQ.isError
    ? String(devQ.error?.message ?? "Could not load development links.")
    : "";

  <section aria-labelledby="pull-v2-development-heading" data-testid="pull-v2-rail-development">
    <h3 id="pull-v2-development-heading" className="text-[13px] font-semibold text-foreground">
      Development
    </h3>
    <div className="mt-1.5">
      @if (devQ.isPending) {
        <p className="text-[13px] text-muted-foreground">Loading…</p>;
      } @else {
        @if (devQ.isError) {
          <p className="text-[13px] text-destructive">{devError as string}</p>;
        } @else {
          @if (items.length === 0) {
            <p className="text-[13px] text-muted-foreground">None yet</p>;
          } @else {
            <ul className="flex flex-col gap-1">
              @for (const item of items; key `${item.issue.id}-${item.relation}`) {
                <li className="text-[13px]">
                  <span className="text-muted-foreground">
                    {`${item.relation} ` as string}
                  </span>
                  <AppLink
                    href={
                      `/${props.owner}/${props.repoName}/issues/${item.issue.number}` as string
                    }
                    className="font-medium text-foreground no-underline hover:text-primary hover:underline"
                    data-testid="pull-v2-rail-development-link"
                  >
                    {`#${item.issue.number} ${item.issue.title}` as string}
                  </AppLink>
                </li>;
              }
            </ul>;
          }
        }
      }
    </div>
  </section>;
}

function deploymentBadgeVariant(
  state: string | null | undefined,
  fallback: string,
): "success" | "destructive" | "primary" | "default" {
  const s = state ?? fallback;
  if (s === "success") return "success";
  if (s === "failure" || s === "error" || s === "cancelled") return "destructive";
  if (s === "inactive") return "default";
  return "primary";
}

/** Deployments section — pull.deployments.list (latest status per env). */
function RailDeployments(props: PullV2RailProps) @{
  const depsQ = useQuery({
    queryKey: ["pull", "deployments", props.owner, props.repoName, props.number] as const,
    queryFn: async () => {
      const res = await apiClient.pull.deployments.list({
        owner: props.owner,
        name: props.repoName,
        number: props.number,
      });
      if (!res.ok) throw new Error(res.error.message || NETWORK_ERROR);
      return res.data.deployments;
    },
  });
  const deployments = depsQ.data ?? [];
  const depsError = depsQ.isError
    ? String(depsQ.error?.message ?? "Could not load deployments.")
    : "";

  <section aria-labelledby="pull-v2-deployments-heading" data-testid="pull-v2-rail-deployments">
    <h3 id="pull-v2-deployments-heading" className="text-[13px] font-semibold text-foreground">
      Deployments
    </h3>
    <div className="mt-1.5">
      @if (depsQ.isPending) {
        <p className="text-[13px] text-muted-foreground">Loading…</p>;
      } @else {
        @if (depsQ.isError) {
          <p className="text-[13px] text-destructive">{depsError as string}</p>;
        } @else {
          @if (deployments.length === 0) {
            <p className="text-[13px] text-muted-foreground">None yet</p>;
          } @else {
            <ul className="flex flex-col gap-1.5">
              @for (const d of deployments; key d.id) {
                <li className="text-[13px]">
                  <div className="flex items-center gap-1.5">
                    @if (d.url) {
                      {
                        /* raw-anchor-ok: deployment url is external (cloud
                    console, service endpoint) — not an app route. */
                      }
                      <a
                        href={d.url as string}
                        target="_blank"
                        rel="noreferrer"
                        className="font-medium text-foreground no-underline hover:text-primary hover:underline"
                        data-testid="pull-v2-rail-deployment-link"
                      >
                        {d.environment as string}
                      </a>;
                    } @else {
                      <AppLink
                        href={
                          `/${props.owner}/${props.repoName}/actions` as string
                        }
                        className="font-medium text-foreground no-underline hover:text-primary hover:underline"
                        data-testid="pull-v2-rail-deployment-link"
                      >
                        {d.environment as string}
                      </AppLink>;
                    }
                    <Badge variant={deploymentBadgeVariant(d.state, d.job_status)}>
                      {(d.state ?? d.job_status) as string}
                    </Badge>
                  </div>
                  @if (d.description) {
                    <p className="mt-0.5 truncate text-[12px] text-muted-foreground">
                      {d.description as string}
                    </p>;
                  }
                </li>;
              }
            </ul>;
          }
        }
      }
    </div>
  </section>;
}

/** Footer actions — convert-to-draft / ready / close via existing RPCs. */
function RailActions(props: PullV2RailProps) @{
  const queryClient = useQueryClient();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const open = props.pull.state === "open";

  async function invalidate() {
    await queryClient.invalidateQueries({
      queryKey: ["pull", "get", props.owner, props.repoName, props.number],
    });
  }

  async function setDraft(draft: boolean) {
    setBusy(true);
    setError("");
    const res = await apiClient.pull.update({
      owner: props.owner,
      name: props.repoName,
      number: props.number,
      draft,
    });
    setBusy(false);
    if (!res.ok) {
      setError(res.error.message || "Could not update the pull request.");
      return;
    }
    await invalidate();
  }

  async function close() {
    setBusy(true);
    setError("");
    const res = await apiClient.pull.close({
      owner: props.owner,
      name: props.repoName,
      number: props.number,
    });
    setBusy(false);
    if (!res.ok) {
      setError(res.error.message || "Could not close the pull request.");
      return;
    }
    await invalidate();
  }

  <div
    className="flex flex-col gap-1.5 border-t border-border pt-3"
    data-testid="pull-v2-rail-actions"
  >
    @if (open && (props.canEdit ?? props.canWrite)) {
      <>
        @if (props.pull.draft) {
          <button
            type="button"
            disabled={busy}
            className="inline-flex items-center gap-1.5 text-left text-[13px] text-muted-foreground hover:text-foreground disabled:opacity-50"
            data-testid="pull-v2-rail-ready"
            onClick={() => void setDraft(false)}
          >
            <GitPullRequest aria-hidden size={13} />
            Ready for review
          </button>;
        } @else {
          <button
            type="button"
            disabled={busy}
            className="inline-flex items-center gap-1.5 text-left text-[13px] text-muted-foreground hover:text-foreground disabled:opacity-50"
            data-testid="pull-v2-rail-convert-draft"
            onClick={() => void setDraft(true)}
          >
            <GitPullRequestDraft aria-hidden size={13} />
            Convert to draft
          </button>;
        }
        <button
          type="button"
          disabled={busy}
          className="inline-flex items-center gap-1.5 text-left text-[13px] text-muted-foreground hover:text-destructive disabled:opacity-50"
          data-testid="pull-v2-rail-close"
          onClick={() => void close()}
        >
          <CircleSlash aria-hidden size={13} />
          Close pull request
        </button>
      </>;
    }
    <p className={cn("text-[12px] text-destructive", !error && "hidden")} role="alert">
      {error as string}
    </p>
  </div>;
}

/**
 * "Status and details" right rail for the v2 overview (WI-0001 S8),
 * ordered like GitHub's new UI: activity summary first, then the
 * reviewers / assignees / labels pickers, then Projects / Milestone /
 * Development / Deployments, participants, and the draft/close footer
 * actions.
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
  const authorInitial = (props.pull.author_username || "?").slice(0, 1).toUpperCase();

  <div className="space-y-5" data-testid="pull-v2-rail">
    <section aria-labelledby="pull-v2-activity-heading" data-testid="pull-v2-rail-activity">
      <h3 id="pull-v2-activity-heading" className="text-[13px] font-semibold text-foreground">
        Activity
      </h3>
      <p className="mt-0.5 text-[12px] text-muted-foreground">
        since this pull request was created
      </p>
      <ul className="mt-1.5 space-y-0.5 text-[13px] text-muted-foreground">
        <li className="flex items-center justify-between gap-2">
          <span>Commits</span>
          <span className="rounded-full bg-muted px-1.5 py-0.5 text-[11px] leading-none">
            {commitCount as number}
          </span>
        </li>
        <li className="flex items-center justify-between gap-2">
          <span>Comments</span>
          <span className="rounded-full bg-muted px-1.5 py-0.5 text-[11px] leading-none">
            {commentCount as number}
          </span>
        </li>
        <li className="flex items-center justify-between gap-2">
          <span>Reviews</span>
          <span className="rounded-full bg-muted px-1.5 py-0.5 text-[11px] leading-none">
            {reviewCount as number}
          </span>
        </li>
      </ul>
      <AppLink
        href={`/${props.owner}/${props.repoName}/activity` as string}
        className="mt-1.5 inline-block text-[13px] text-primary no-underline hover:underline"
        data-testid="pull-v2-activity-link"
      >
        View all activity
      </AppLink>
    </section>
    <RailReviewers {...props} />
    <RailAssignees {...props} />
    <RailLabels {...props} />
    <RailProjects {...props} />
    <RailMilestone {...props} />
    <RailDevelopment {...props} />
    <RailDeployments {...props} />
    <section data-testid="pull-v2-rail-participants">
      <h3 className="text-[13px] font-semibold text-foreground">
        {
          `${props.pull.author_username ? 1 : 0} participant${props.pull.author_username ? "" : "s"}` as string
        }
      </h3>
      <div className="mt-1.5 flex items-center gap-1.5">
        <span
          className="inline-flex size-5 items-center justify-center rounded-full bg-muted text-[10px] font-semibold text-muted-foreground"
          aria-hidden
        >
          {authorInitial as string}
        </span>
        <span className="text-[13px] text-foreground">{props.pull.author_username as string}</span>
      </div>
    </section>
    <RailActions {...props} />
  </div>;
}
