import type { CollaboratorPermission, OrgTeamPublic, RepoTeamPublic } from "@oxidean/api-client";
import { Check, ChevronDown } from "@octanejs/lucide";
import { useQuery, useQueryClient } from "@octanejs/tanstack-query";
import { useState } from "octane";
import { AuthErrorBanner } from "@/components/auth-shell";
import {
  AlertDialog,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";
import {
  SelectIcon,
  SelectItem,
  SelectItemIndicator,
  SelectItemText,
  SelectList,
  SelectPopup,
  SelectPortal,
  SelectPositioner,
  SelectRoot,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { apiClient } from "@/lib/api-client";

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";

const PERM_OPTIONS: { value: CollaboratorPermission; label: string }[] = [
  { value: "read", label: "Read" },
  { value: "write", label: "Write" },
  { value: "admin", label: "Admin" },
];

function permLabel(p: CollaboratorPermission): string {
  return PERM_OPTIONS.find((o) => o.value === p)?.label ?? p;
}

function teamsQueryKey(owner: string, name: string) {
  return ["repo", "teams", owner, name] as const;
}
function orgTeamsQueryKey(slug: string) {
  return ["org", "teams", slug] as const;
}

function PermissionSelect({
  id,
  value,
  onChange,
  disabled,
  ariaLabel,
}: {
  id: string;
  value: CollaboratorPermission;
  onChange: (p: CollaboratorPermission) => void;
  disabled?: boolean;
  ariaLabel?: string;
}) @{
  <SelectRoot
    value={value}
    onValueChange={(next) => {
      if (typeof next === "string") onChange(next as CollaboratorPermission);
    }}
  >
    <SelectTrigger
      id={id}
      data-testid={id}
      className="min-w-[8rem]"
      aria-label={ariaLabel ?? "Permission"}
      disabled={disabled}
    >
      <SelectValue>{permLabel(value) as string}</SelectValue>
      <SelectIcon>
        <ChevronDown aria-hidden size={16} />
      </SelectIcon>
    </SelectTrigger>
    {/* octane-dom-race-ok: Base UI SelectPortal has no keepMounted escape
        hatch; popup is anchor-scoped under the Select root. */}
    <SelectPortal>
      <SelectPositioner sideOffset={4}>
        <SelectPopup>
          <SelectList>
            @for (const opt of PERM_OPTIONS; key opt.value) {
              <SelectItem value={opt.value} data-testid={`${id}-option-${opt.value}`}>
                <SelectItemIndicator>
                  <Check aria-hidden size={16} className="text-primary" />
                </SelectItemIndicator>
                <SelectItemText>{opt.label as string}</SelectItemText>
              </SelectItem>;
            }
          </SelectList>
        </SelectPopup>
      </SelectPositioner>
    </SelectPortal>
  </SelectRoot>;
}

type TeamsPanelProps = {
  owner: string;
  name: string;
  /** Repo `owner_type === "org"` — team lists only exist on org-owned repos. */
  orgOwned: boolean;
};

/**
 * Repo Teams settings section (COL-05). Shown on org-owned repos beside
 * Collaborators — grants a whole org team shared read/write/admin access.
 * Parent route gates on repo.can_admin.
 */
export function TeamsPanel({ owner, name, orgOwned }: TeamsPanelProps) @{
  const queryClient = useQueryClient();

  const listQ = useQuery({
    queryKey: teamsQueryKey(owner, name),
    enabled: orgOwned,
    queryFn: async () => {
      const res = await apiClient.repo.teams.list({ owner, name });
      if (!res.ok) throw new Error(res.error.message || NETWORK_ERROR);
      return res.data;
    },
  });

  // Org teams populate the add-select; failure (e.g. caller is repo admin via
  // collaborator but not an org member) just hides the add form.
  const orgTeamsQ = useQuery({
    queryKey: orgTeamsQueryKey(owner),
    enabled: orgOwned,
    queryFn: async () => {
      const res = await apiClient.org.teams.list({ slug: owner });
      if (!res.ok) throw new Error(res.error.message || NETWORK_ERROR);
      return res.data;
    },
  });

  const granted: RepoTeamPublic[] = listQ.data?.teams ?? [];
  const grantedIds = new Set(granted.map((t) => t.team_id));
  const orgTeams: OrgTeamPublic[] = orgTeamsQ.data?.teams ?? [];
  const available = orgTeams.filter((t) => !grantedIds.has(t.id));

  const [rowError, setRowError] = useState("");
  const [addError, setAddError] = useState("");
  const [showAdd, setShowAdd] = useState(false);
  const [addPending, setAddPending] = useState(false);
  const [addTeamId, setAddTeamId] = useState("");
  const [addPerm, setAddPerm] = useState<CollaboratorPermission>("read");
  const [removeTarget, setRemoveTarget] = useState<RepoTeamPublic | null>(null);
  const [removePending, setRemovePending] = useState(false);
  const [removeError, setRemoveError] = useState("");

  async function refresh() {
    await Promise.all([
      queryClient.invalidateQueries({ queryKey: teamsQueryKey(owner, name) }),
      queryClient.invalidateQueries({ queryKey: orgTeamsQueryKey(owner) }),
    ]);
  }

  async function submitAdd() {
    setAddError("");
    const team = available.find((t) => t.id === addTeamId) ?? available[0];
    if (!team) {
      setAddError("Pick a team to grant.");
      return;
    }
    setAddPending(true);
    try {
      const res = await apiClient.repo.teams.add({
        owner,
        name,
        team_slug: team.slug,
        permission: addPerm,
      });
      if (!res.ok) {
        setAddError(res.error.message || NETWORK_ERROR);
        setAddPending(false);
        return;
      }
      setAddPending(false);
      setShowAdd(false);
      setAddTeamId("");
      setAddPerm("read");
      await refresh();
    } catch {
      setAddError(NETWORK_ERROR);
      setAddPending(false);
    }
  }

  async function updatePermission(row: RepoTeamPublic, permission: CollaboratorPermission) {
    if (permission === row.permission) return;
    setRowError("");
    try {
      const res = await apiClient.repo.teams.update({
        owner,
        name,
        team_slug: row.slug,
        permission,
      });
      if (!res.ok) {
        setRowError(res.error.message || NETWORK_ERROR);
        return;
      }
      await refresh();
    } catch {
      setRowError(NETWORK_ERROR);
    }
  }

  async function confirmRemove() {
    if (!removeTarget) return;
    setRemoveError("");
    setRemovePending(true);
    try {
      const res = await apiClient.repo.teams.remove({
        owner,
        name,
        team_slug: removeTarget.slug,
      });
      if (!res.ok) {
        setRemoveError(res.error.message || NETWORK_ERROR);
        setRemovePending(false);
        return;
      }
      setRemovePending(false);
      setRemoveTarget(null);
      await refresh();
    } catch {
      setRemoveError(NETWORK_ERROR);
      setRemovePending(false);
    }
  }

  @if (!orgOwned) {
    <></>;
  } @else {
    <section
      className="mt-12 flex flex-col gap-4"
      aria-labelledby="teams-heading"
      data-testid="teams-panel"
    >
      <h2
        id="teams-heading"
        className="font-[family-name:var(--font-display)] text-[18px] font-semibold text-foreground"
      >
        Teams
      </h2>
      <p className="text-[14px] text-muted-foreground">
        Grant an organization team shared access to this repository. Every team member gets the
        permission.
      </p>

      @if (rowError) {
        <AuthErrorBanner message={rowError} />;
      }

      @if (listQ.isPending) {
        <p className="text-[14px] text-muted-foreground">Loading teams…</p>;
      } @else if (listQ.isError) {
        <p role="alert" className="text-[14px] text-destructive">
          {(listQ.error instanceof Error ? listQ.error.message : NETWORK_ERROR) as string}
        </p>;
      } @else {
        <>
          @if (granted.length === 0) {
            <p className="text-[14px] text-muted-foreground" data-testid="teams-empty">
              No teams have access to this repository.
            </p>;
          } @else {
            <div className="overflow-x-auto">
              <table className="w-full min-w-[28rem] border-collapse text-left text-[14px]">
                <thead>
                  <tr className="border-b border-border text-muted-foreground">
                    <th className="py-2 pr-4 font-medium">Team</th>
                    <th className="py-2 pr-4 font-medium">Permission</th>
                    <th className="py-2 font-medium">
                      <span className="sr-only">Actions</span>
                    </th>
                  </tr>
                </thead>
                <tbody>
                  @for (const t of granted; key t.team_id) {
                    <tr
                      className="border-b border-border"
                      data-testid={`repo-team-${t.slug}`}
                    >
                      <td className="py-3 pr-4">
                        <span className="font-medium text-foreground">
                          {t.name as string}
                        </span>
                        <span className="ml-2 text-[12px] text-muted-foreground">
                          {"@" + (t.slug as string)}
                        </span>
                      </td>
                      <td className="py-3 pr-4">
                        <PermissionSelect
                          id={`team-perm-${t.team_id}`}
                          value={t.permission}
                          onChange={(p) => void updatePermission(t, p)}
                        />
                      </td>
                      <td className="py-3">
                        <Button
                          type="button"
                          variant="ghost"
                          className="h-11 text-destructive"
                          onClick={() => {
                            setRemoveError("");
                            setRemoveTarget(t);
                          }}
                        >
                          Remove
                        </Button>
                      </td>
                    </tr>;
                  }
                </tbody>
              </table>
            </div>;
          }
          @if (!showAdd && available.length > 0) {
            <Button
              type="button"
              className="h-11 w-fit"
              data-testid="team-add-open"
              onClick={() => setShowAdd(true)}
            >
              Add team
            </Button>;
          }
        </>;
      }

      @if (showAdd) {
        <div
          className="mt-2 flex flex-col gap-4 rounded-md border border-border p-4"
          data-testid="team-add-form"
        >
          <h3 className="text-[16px] font-semibold text-foreground">Add team</h3>
          @if (addError) {
            <AuthErrorBanner message={addError} />;
          }
          <div className="flex flex-col gap-2">
            <Label htmlFor="team-add-select">Team</Label>
            <SelectRoot
              value={addTeamId || (available[0]?.id ?? "")}
              onValueChange={(next) => {
                if (typeof next === "string") setAddTeamId(next);
              }}
            >
              <SelectTrigger
                id="team-add-select"
                data-testid="team-add-select"
                className="min-w-[12rem]"
                aria-label="Team"
              >
                <SelectValue>
                  {
                    (available.find((t) => t.id === (addTeamId || available[0]?.id))?.name ??
                      "Pick a team") as string
                  }
                </SelectValue>
                <SelectIcon>
                  <ChevronDown aria-hidden size={16} />
                </SelectIcon>
              </SelectTrigger>
              {/* octane-dom-race-ok: Base UI SelectPortal has no keepMounted
                  escape hatch; popup is anchor-scoped under the Select root. */}
              <SelectPortal>
                <SelectPositioner sideOffset={4}>
                  <SelectPopup>
                    <SelectList>
                      @for (const t of available; key t.id) {
                        <SelectItem
                          value={t.id}
                          data-testid={`team-add-option-${t.slug}`}
                        >
                          <SelectItemIndicator>
                            <Check aria-hidden size={16} className="text-primary" />
                          </SelectItemIndicator>
                          <SelectItemText>{t.name as string}</SelectItemText>
                        </SelectItem>;
                      }
                    </SelectList>
                  </SelectPopup>
                </SelectPositioner>
              </SelectPortal>
            </SelectRoot>
          </div>
          <div className="flex flex-col gap-2">
            <Label htmlFor="team-add-perm">Permission</Label>
            <PermissionSelect id="team-add-perm" value={addPerm} onChange={setAddPerm} />
          </div>
          <div className="flex flex-wrap gap-2">
            <Button
              type="button"
              className="h-11"
              data-testid="team-add-submit"
              disabled={addPending}
              onClick={() => void submitAdd()}
            >
              @if (addPending) {
                <>Adding…</>;
              } @else {
                <>Add team</>;
              }
            </Button>
            <Button
              type="button"
              variant="secondary"
              className="h-11"
              disabled={addPending}
              onClick={() => {
                setShowAdd(false);
                setAddError("");
              }}
            >
              Cancel
            </Button>
          </div>
        </div>;
      }

      <AlertDialog open={!!removeTarget} onOpenChange={(o) => !o && setRemoveTarget(null)}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Remove team access</AlertDialogTitle>
            <AlertDialogDescription>
              {removeTarget
                ? `Remove ${removeTarget.name}'s access to ${name}? Members keep access from other grants.`
                : ""}
            </AlertDialogDescription>
          </AlertDialogHeader>
          @if (removeError) {
            <p role="alert" className="text-[14px] text-destructive">
              {removeError as string}
            </p>;
          }
          <AlertDialogFooter>
            <AlertDialogCancel disabled={removePending}>Cancel</AlertDialogCancel>
            <Button
              type="button"
              variant="destructive"
              disabled={removePending}
              data-testid="team-remove-confirm"
              onClick={() => void confirmRemove()}
            >
              Remove
            </Button>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </section>;
  }
}
