import { useQuery, useQueryClient } from "@octanejs/tanstack-query";
import type {
  CollaboratorPermission,
  OrgTeamMemberPublic,
  OrgTeamRepoPublic,
  TeamRole,
} from "@oxidean/api-client";
import { Check, ChevronDown } from "@octanejs/lucide";
import { useForm } from "@octanejs/tanstack-form";
import { useState } from "octane";
import { navigate as astroNavigate } from "astro:transitions/client";
import { AuthErrorBanner } from "@/components/auth-shell";
import { MemberLookup } from "@/components/org/member-lookup";
import { AppLink } from "@/components/ui/app-link";
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
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Textarea } from "@/components/ui/textarea";
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
import { AppPage } from "@/components/app-page";
import {
  OrgSettingsShell,
  orgSettingsAccessQueryOptions,
  orgSettingsOwner,
} from "@/components/org/org-settings-shell.tsrx";
import { usePathname } from "@/lib/pathname.tsrx";
import { teamsQueryKey } from "@/routes/$owner.settings.teams.tsrx";

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";

const TEAM_ROLE_OPTIONS: { value: TeamRole; label: string }[] = [
  { value: "member", label: "Member" },
  { value: "maintainer", label: "Maintainer" },
];

const PERM_OPTIONS: { value: CollaboratorPermission; label: string }[] = [
  { value: "read", label: "Read" },
  { value: "write", label: "Write" },
  { value: "admin", label: "Admin" },
];

function teamRoleLabel(r: TeamRole): string {
  return TEAM_ROLE_OPTIONS.find((o) => o.value === r)?.label ?? r;
}

function permLabel(p: CollaboratorPermission): string {
  return PERM_OPTIONS.find((o) => o.value === p)?.label ?? p;
}

function teamQueryKey(slug: string, team: string) {
  return ["org", "team", slug, team] as const;
}
function teamMembersQueryKey(slug: string, team: string) {
  return ["org", "teamMembers", slug, team] as const;
}
function teamReposQueryKey(slug: string, team: string) {
  return ["org", "teamRepos", slug, team] as const;
}

function TeamRoleSelect({
  id,
  value,
  onChange,
  disabled,
}: {
  id: string;
  value: TeamRole;
  onChange: (r: TeamRole) => void;
  disabled?: boolean;
}) @{
  <SelectRoot
    value={value}
    onValueChange={(next) => {
      if (typeof next === "string") onChange(next as TeamRole);
    }}
  >
    <SelectTrigger
      id={id}
      data-testid={id}
      className="min-w-[8rem]"
      aria-label="Team role"
      disabled={disabled}
    >
      <SelectValue>{teamRoleLabel(value) as string}</SelectValue>
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
            @for (const opt of TEAM_ROLE_OPTIONS; key opt.value) {
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

function PermissionSelect({
  id,
  value,
  onChange,
  disabled,
}: {
  id: string;
  value: CollaboratorPermission;
  onChange: (p: CollaboratorPermission) => void;
  disabled?: boolean;
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
      aria-label="Permission"
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

/** `/{owner}/settings/teams/{team}` — team detail (COL-05). */
export function OrgTeamDetailPage() @{
  <AppPage>
    <OrgSettingsShell active="teams">
      <OrgTeamDetailContent />
    </OrgSettingsShell>
  </AppPage>;
}

function OrgTeamDetailContent() @{
  const pathname = usePathname();
  const owner = orgSettingsOwner(pathname);
  const segs = pathname.split("/").filter(Boolean);
  const teamSlug = decodeURIComponent(segs[3] ?? "");
  const data = useQuery(orgSettingsAccessQueryOptions(owner)).data;
  const queryClient = useQueryClient();

  const org = data?.kind === "ready" ? data.org : null;
  const slug = org?.slug ?? owner;
  const canAdmin = data?.kind === "ready" && data.canAdmin;

  const teamQ = useQuery({
    queryKey: teamQueryKey(slug, teamSlug),
    enabled: !!slug && !!teamSlug,
    queryFn: async () => {
      const res = await apiClient.org.teams.get({ slug, team_slug: teamSlug });
      if (!res.ok) throw new Error(`${res.error.code}: ${res.error.message}`);
      return res.data;
    },
  });
  const membersQ = useQuery({
    queryKey: teamMembersQueryKey(slug, teamSlug),
    enabled: !!slug && !!teamSlug,
    queryFn: async () => {
      const res = await apiClient.org.teams.members.list({ slug, team_slug: teamSlug });
      if (!res.ok) throw new Error(`${res.error.code}: ${res.error.message}`);
      return res.data;
    },
  });
  const reposQ = useQuery({
    queryKey: teamReposQueryKey(slug, teamSlug),
    enabled: !!slug && !!teamSlug,
    queryFn: async () => {
      const res = await apiClient.org.teams.repos.list({ slug, team_slug: teamSlug });
      if (!res.ok) throw new Error(`${res.error.code}: ${res.error.message}`);
      return res.data;
    },
  });

  const team = teamQ.data ?? null;
  const members: OrgTeamMemberPublic[] = membersQ.data?.members ?? [];
  const repos: OrgTeamRepoPublic[] = reposQ.data?.repos ?? [];

  const [actionError, setActionError] = useState("");
  const [memberError, setMemberError] = useState("");
  const [repoError, setRepoError] = useState("");
  const [showAddMember, setShowAddMember] = useState(false);
  const [showAddRepo, setShowAddRepo] = useState(false);
  const [showEdit, setShowEdit] = useState(false);
  const [editPending, setEditPending] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [deletePending, setDeletePending] = useState(false);
  const [deleteError, setDeleteError] = useState("");
  const [removeMember, setRemoveMember] = useState<OrgTeamMemberPublic | null>(null);
  const [removeMemberPending, setRemoveMemberPending] = useState(false);
  const [removeMemberError, setRemoveMemberError] = useState("");
  const [removeRepo, setRemoveRepo] = useState<OrgTeamRepoPublic | null>(null);
  const [removeRepoPending, setRemoveRepoPending] = useState(false);
  const [removeRepoError, setRemoveRepoError] = useState("");

  async function refreshAll() {
    await Promise.all([
      queryClient.invalidateQueries({ queryKey: teamQueryKey(slug, teamSlug) }),
      queryClient.invalidateQueries({ queryKey: teamMembersQueryKey(slug, teamSlug) }),
      queryClient.invalidateQueries({ queryKey: teamReposQueryKey(slug, teamSlug) }),
      queryClient.invalidateQueries({ queryKey: teamsQueryKey(slug) }),
    ]);
  }

  const memberForm = useForm({
    defaultValues: { username: "", role: "member" as TeamRole },
    onSubmit: async ({ value }) => {
      setMemberError("");
      const username = value.username.trim();
      if (!username) {
        setMemberError("Username is required.");
        return;
      }
      try {
        const res = await apiClient.org.teams.members.add({
          slug,
          team_slug: teamSlug,
          username,
          role: value.role,
        });
        if (!res.ok) {
          setMemberError(res.error.message || NETWORK_ERROR);
          return;
        }
        memberForm.reset();
        setShowAddMember(false);
        await refreshAll();
      } catch {
        setMemberError(NETWORK_ERROR);
      }
    },
  });

  const repoForm = useForm({
    defaultValues: { repo: "", permission: "read" as CollaboratorPermission },
    onSubmit: async ({ value }) => {
      setRepoError("");
      const repo = value.repo.trim();
      if (!repo) {
        setRepoError("Repository name is required.");
        return;
      }
      try {
        const res = await apiClient.org.teams.repos.add({
          slug,
          team_slug: teamSlug,
          repo,
          permission: value.permission,
        });
        if (!res.ok) {
          setRepoError(res.error.message || NETWORK_ERROR);
          return;
        }
        repoForm.reset();
        setShowAddRepo(false);
        await refreshAll();
      } catch {
        setRepoError(NETWORK_ERROR);
      }
    },
  });

  const editForm = useForm({
    defaultValues: { name: "", description: "", new_slug: "" },
    onSubmit: async ({ value }) => {
      setActionError("");
      setEditPending(true);
      try {
        const res = await apiClient.org.teams.update({
          slug,
          team_slug: teamSlug,
          name: value.name.trim() || null,
          description: value.description,
          new_slug: value.new_slug.trim() || null,
        });
        if (!res.ok) {
          setActionError(res.error.message || NETWORK_ERROR);
          setEditPending(false);
          return;
        }
        setEditPending(false);
        setShowEdit(false);
        const newSlug = res.data.slug;
        await queryClient.invalidateQueries({ queryKey: teamsQueryKey(slug) });
        if (newSlug !== teamSlug) {
          void astroNavigate(`/${slug}/settings/teams/${newSlug}`);
          return;
        }
        await refreshAll();
      } catch {
        setActionError(NETWORK_ERROR);
        setEditPending(false);
      }
    },
  });

  async function setMemberRole(member: OrgTeamMemberPublic, role: TeamRole) {
    if (role === member.role) return;
    setMemberError("");
    try {
      const res = await apiClient.org.teams.members.setRole({
        slug,
        team_slug: teamSlug,
        user_id: member.user_id,
        role,
      });
      if (!res.ok) {
        setMemberError(res.error.message || NETWORK_ERROR);
        return;
      }
      await refreshAll();
    } catch {
      setMemberError(NETWORK_ERROR);
    }
  }

  async function updateRepoPerm(repo: OrgTeamRepoPublic, permission: CollaboratorPermission) {
    if (permission === repo.permission) return;
    setRepoError("");
    try {
      const res = await apiClient.org.teams.repos.update({
        slug,
        team_slug: teamSlug,
        repo: repo.repo_name,
        permission,
      });
      if (!res.ok) {
        setRepoError(res.error.message || NETWORK_ERROR);
        return;
      }
      await refreshAll();
    } catch {
      setRepoError(NETWORK_ERROR);
    }
  }

  async function confirmRemoveMember() {
    if (!removeMember) return;
    setRemoveMemberError("");
    setRemoveMemberPending(true);
    try {
      const res = await apiClient.org.teams.members.remove({
        slug,
        team_slug: teamSlug,
        user_id: removeMember.user_id,
      });
      if (!res.ok) {
        setRemoveMemberError(res.error.message || NETWORK_ERROR);
        setRemoveMemberPending(false);
        return;
      }
      setRemoveMemberPending(false);
      setRemoveMember(null);
      await refreshAll();
    } catch {
      setRemoveMemberError(NETWORK_ERROR);
      setRemoveMemberPending(false);
    }
  }

  async function confirmRemoveRepo() {
    if (!removeRepo) return;
    setRemoveRepoError("");
    setRemoveRepoPending(true);
    try {
      const res = await apiClient.org.teams.repos.remove({
        slug,
        team_slug: teamSlug,
        repo: removeRepo.repo_name,
      });
      if (!res.ok) {
        setRemoveRepoError(res.error.message || NETWORK_ERROR);
        setRemoveRepoPending(false);
        return;
      }
      setRemoveRepoPending(false);
      setRemoveRepo(null);
      await refreshAll();
    } catch {
      setRemoveRepoError(NETWORK_ERROR);
      setRemoveRepoPending(false);
    }
  }

  async function confirmDeleteTeam() {
    setDeleteError("");
    setDeletePending(true);
    try {
      const res = await apiClient.org.teams.delete({ slug, team_slug: teamSlug });
      if (!res.ok) {
        setDeleteError(res.error.message || NETWORK_ERROR);
        setDeletePending(false);
        return;
      }
      await queryClient.invalidateQueries({ queryKey: teamsQueryKey(slug) });
      void astroNavigate(`/${slug}/settings/teams`);
    } catch {
      setDeleteError(NETWORK_ERROR);
      setDeletePending(false);
    }
  }

  @if (teamQ.isPending) {
    <p className="text-[14px] text-muted-foreground">Loading team…</p>;
  } @else if (teamQ.isError || !team) {
    <p role="alert" className="text-[14px] text-destructive">
      {(teamQ.error instanceof Error ? teamQ.error.message : "Team not found.") as string}
    </p>;
  } @else {
    <section className="flex flex-col gap-8" data-testid="org-team-detail">
      <div className="flex flex-wrap items-start justify-between gap-2">
        <div>
          <p className="text-[13px] text-muted-foreground">
            <AppLink href={`/${slug}/settings/teams`} className="no-underline hover:underline">
              Teams
            </AppLink>
            {" / " + (team.slug as string)}
          </p>
          <h2
            className="font-[family-name:var(--font-display)] text-[18px] font-semibold text-foreground"
            data-testid="team-name"
          >
            {team.name as string}
          </h2>
          @if (team.description) {
            <p className="mt-1 text-[14px] text-muted-foreground">
              {team.description as string}
            </p>;
          }
        </div>
        @if (canAdmin) {
          <div className="flex gap-2">
            <Button
              type="button"
              variant="secondary"
              className="h-11"
              data-testid="team-edit-open"
              onClick={() => {
                editForm.setFieldValue("name", team.name);
                editForm.setFieldValue("description", team.description);
                editForm.setFieldValue("new_slug", team.slug);
                setShowEdit(true);
                setActionError("");
              }}
            >
              Edit
            </Button>
            <Button
              type="button"
              variant="destructive"
              className="h-11"
              data-testid="team-delete-open"
              onClick={() => {
                setDeleteError("");
                setConfirmDelete(true);
              }}
            >
              Delete team
            </Button>
          </div>;
        }
      </div>

      @if (showEdit && canAdmin) {
        <form
          className="flex flex-col gap-4 rounded-md border border-border p-4"
          method="post"
          action="#"
          data-testid="team-edit-form"
          onSubmit={(e) => {
            e.preventDefault();
            void editForm.handleSubmit();
          }}
        >
          <h3 className="text-[16px] font-semibold text-foreground">Edit team</h3>
          @if (actionError) {
            <AuthErrorBanner message={actionError} />;
          }
          <editForm.Field name="name">
            {(field) => (
              <div className="flex flex-col gap-2">
                <Label htmlFor="team-edit-name">Name</Label>
                <Input
                  id="team-edit-name"
                  value={field.state.value}
                  onInput={(e) => field.handleChange((e.target as HTMLInputElement).value)}
                  autoComplete="off"
                />
              </div>
            )}
          </editForm.Field>
          <editForm.Field name="new_slug">
            {(field) => (
              <div className="flex flex-col gap-2">
                <Label htmlFor="team-edit-slug">Slug</Label>
                <Input
                  id="team-edit-slug"
                  value={field.state.value}
                  onInput={(e) => field.handleChange((e.target as HTMLInputElement).value)}
                  autoComplete="off"
                />
              </div>
            )}
          </editForm.Field>
          <editForm.Field name="description">
            {(field) => (
              <div className="flex flex-col gap-2">
                <Label htmlFor="team-edit-description">Description</Label>
                <Textarea
                  id="team-edit-description"
                  value={field.state.value}
                  onInput={(e) => field.handleChange((e.target as HTMLTextAreaElement).value)}
                  rows={2}
                />
              </div>
            )}
          </editForm.Field>
          <div className="flex flex-wrap gap-2">
            <Button type="submit" className="h-11" disabled={editPending}>
              @if (editPending) {
                <>Saving…</>;
              } @else {
                <>Save changes</>;
              }
            </Button>
            <Button
              type="button"
              variant="secondary"
              className="h-11"
              disabled={editPending}
              onClick={() => setShowEdit(false)}
            >
              Cancel
            </Button>
          </div>
        </form>;
      }

      <div className="flex flex-col gap-3" data-testid="team-members-section">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <h3 className="text-[16px] font-semibold text-foreground">Members</h3>
          @if (canAdmin && !showAddMember) {
            <Button
              type="button"
              variant="secondary"
              className="h-11 w-fit"
              data-testid="member-add-open"
              onClick={() => setShowAddMember(true)}
            >
              Add member
            </Button>;
          }
        </div>
        @if (memberError) {
          <AuthErrorBanner message={memberError} />;
        }
        @if (showAddMember && canAdmin) {
          <form
            className="flex flex-col gap-4 rounded-md border border-border p-4"
            method="post"
            action="#"
            onSubmit={(e) => {
              e.preventDefault();
              void memberForm.handleSubmit();
            }}
          >
            <memberForm.Field name="username">
              {(field) => (
                <MemberLookup
                  id="team-member-lookup"
                  label="Username"
                  value={field.state.value}
                  onValueChange={field.handleChange}
                  context={{ kind: "org", slug }}
                />
              )}
            </memberForm.Field>
            <memberForm.Subscribe selector={(s) => s.values.role}>
              {(role) => (
                <div className="flex flex-col gap-2">
                  <Label htmlFor="team-member-role">Team role</Label>
                  <TeamRoleSelect
                    id="team-member-role"
                    value={role}
                    onChange={(r) => memberForm.setFieldValue("role", r)}
                  />
                </div>
              )}
            </memberForm.Subscribe>
            <div className="flex flex-wrap gap-2">
              <Button type="submit" className="h-11" data-testid="member-add-submit">
                Add member
              </Button>
              <Button
                type="button"
                variant="secondary"
                className="h-11"
                onClick={() => {
                  setShowAddMember(false);
                  memberForm.reset();
                }}
              >
                Cancel
              </Button>
            </div>
          </form>;
        }
        @if (membersQ.isPending) {
          <p className="text-[14px] text-muted-foreground">Loading members…</p>;
        } @else if (members.length === 0) {
          <p className="text-[14px] text-muted-foreground">
            No members yet. Team members must already belong to the organization.
          </p>;
        } @else {
          <div className="overflow-x-auto">
            <table className="w-full min-w-[28rem] border-collapse text-left text-[14px]">
              <thead>
                <tr className="border-b border-border text-muted-foreground">
                  <th className="py-2 pr-4 font-medium">User</th>
                  <th className="py-2 pr-4 font-medium">Team role</th>
                  <th className="py-2 font-medium">
                    <span className="sr-only">Actions</span>
                  </th>
                </tr>
              </thead>
              <tbody>
                @for (const m of members; key m.user_id) {
                  <tr className="border-b border-border">
                    <td className="py-3 pr-4 font-medium text-foreground">
                      {"@" + (m.username as string)}
                    </td>
                    <td className="py-3 pr-4">
                      @if (canAdmin) {
                        <TeamRoleSelect
                          id={`member-role-${m.user_id}`}
                          value={m.role}
                          onChange={(r) => void setMemberRole(m, r)}
                        />;
                      } @else {
                        <span className="text-muted-foreground">
                          {teamRoleLabel(m.role) as string}
                        </span>;
                      }
                    </td>
                    <td className="py-3">
                      @if (canAdmin) {
                        <Button
                          type="button"
                          variant="ghost"
                          className="h-11 text-destructive"
                          onClick={() => {
                            setRemoveMemberError("");
                            setRemoveMember(m);
                          }}
                        >
                          Remove
                        </Button>;
                      }
                    </td>
                  </tr>;
                }
              </tbody>
            </table>
          </div>;
        }
      </div>

      <div className="flex flex-col gap-3" data-testid="team-repos-section">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <h3 className="text-[16px] font-semibold text-foreground">Repositories</h3>
          @if (canAdmin && !showAddRepo) {
            <Button
              type="button"
              variant="secondary"
              className="h-11 w-fit"
              data-testid="repo-add-open"
              onClick={() => setShowAddRepo(true)}
            >
              Add repository
            </Button>;
          }
        </div>
        @if (repoError) {
          <AuthErrorBanner message={repoError} />;
        }
        @if (showAddRepo && canAdmin) {
          <form
            className="flex flex-col gap-4 rounded-md border border-border p-4"
            method="post"
            action="#"
            onSubmit={(e) => {
              e.preventDefault();
              void repoForm.handleSubmit();
            }}
          >
            <repoForm.Field name="repo">
              {(field) => (
                <div className="flex flex-col gap-2">
                  <Label htmlFor="team-repo-name">Repository name</Label>
                  <Input
                    id="team-repo-name"
                    data-testid="team-repo-name"
                    value={field.state.value}
                    onInput={(e) => field.handleChange((e.target as HTMLInputElement).value)}
                    placeholder="repo-name"
                    autoComplete="off"
                  />
                </div>
              )}
            </repoForm.Field>
            <repoForm.Subscribe selector={(s) => s.values.permission}>
              {(perm) => (
                <div className="flex flex-col gap-2">
                  <Label htmlFor="team-repo-perm">Permission</Label>
                  <PermissionSelect
                    id="team-repo-perm"
                    value={perm}
                    onChange={(p) => repoForm.setFieldValue("permission", p)}
                  />
                </div>
              )}
            </repoForm.Subscribe>
            <div className="flex flex-wrap gap-2">
              <Button type="submit" className="h-11" data-testid="repo-add-submit">
                Grant access
              </Button>
              <Button
                type="button"
                variant="secondary"
                className="h-11"
                onClick={() => {
                  setShowAddRepo(false);
                  repoForm.reset();
                }}
              >
                Cancel
              </Button>
            </div>
          </form>;
        }
        @if (reposQ.isPending) {
          <p className="text-[14px] text-muted-foreground">Loading repositories…</p>;
        } @else if (repos.length === 0) {
          <p className="text-[14px] text-muted-foreground">
            No repositories granted to this team.
          </p>;
        } @else {
          <div className="overflow-x-auto">
            <table className="w-full min-w-[28rem] border-collapse text-left text-[14px]">
              <thead>
                <tr className="border-b border-border text-muted-foreground">
                  <th className="py-2 pr-4 font-medium">Repository</th>
                  <th className="py-2 pr-4 font-medium">Permission</th>
                  <th className="py-2 font-medium">
                    <span className="sr-only">Actions</span>
                  </th>
                </tr>
              </thead>
              <tbody>
                @for (const r of repos; key r.repo_id) {
                  <tr
                    className="border-b border-border"
                    data-testid={`team-repo-${r.repo_name}`}
                  >
                    <td className="py-3 pr-4 font-medium text-foreground">
                      <AppLink
                        href={`/${slug}/${r.repo_name}`}
                        className="text-foreground no-underline hover:underline"
                      >
                        {r.repo_name as string}
                      </AppLink>
                    </td>
                    <td className="py-3 pr-4">
                      @if (canAdmin) {
                        <PermissionSelect
                          id={`repo-perm-${r.repo_id}`}
                          value={r.permission}
                          onChange={(p) => void updateRepoPerm(r, p)}
                        />;
                      } @else {
                        <span className="text-muted-foreground">
                          {permLabel(r.permission) as string}
                        </span>;
                      }
                    </td>
                    <td className="py-3">
                      @if (canAdmin) {
                        <Button
                          type="button"
                          variant="ghost"
                          className="h-11 text-destructive"
                          onClick={() => {
                            setRemoveRepoError("");
                            setRemoveRepo(r);
                          }}
                        >
                          Remove
                        </Button>;
                      }
                    </td>
                  </tr>;
                }
              </tbody>
            </table>
          </div>;
        }
      </div>

      <AlertDialog open={!!removeMember} onOpenChange={(o) => !o && setRemoveMember(null)}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Remove member</AlertDialogTitle>
            <AlertDialogDescription>
              {removeMember
                ? `Remove @${removeMember.username} from ${team.name}? Their repository access through this team ends immediately.`
                : ""}
            </AlertDialogDescription>
          </AlertDialogHeader>
          @if (removeMemberError) {
            <p role="alert" className="text-[14px] text-destructive">
              {removeMemberError as string}
            </p>;
          }
          <AlertDialogFooter>
            <AlertDialogCancel disabled={removeMemberPending}>Cancel</AlertDialogCancel>
            <Button
              type="button"
              variant="destructive"
              disabled={removeMemberPending}
              data-testid="member-remove-confirm"
              onClick={() => void confirmRemoveMember()}
            >
              Remove
            </Button>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      <AlertDialog open={!!removeRepo} onOpenChange={(o) => !o && setRemoveRepo(null)}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Remove repository access</AlertDialogTitle>
            <AlertDialogDescription>
              {removeRepo ? `Remove ${team.name}'s access to ${removeRepo.repo_name}?` : ""}
            </AlertDialogDescription>
          </AlertDialogHeader>
          @if (removeRepoError) {
            <p role="alert" className="text-[14px] text-destructive">
              {removeRepoError as string}
            </p>;
          }
          <AlertDialogFooter>
            <AlertDialogCancel disabled={removeRepoPending}>Cancel</AlertDialogCancel>
            <Button
              type="button"
              variant="destructive"
              disabled={removeRepoPending}
              data-testid="repo-remove-confirm"
              onClick={() => void confirmRemoveRepo()}
            >
              Remove
            </Button>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      <AlertDialog open={confirmDelete} onOpenChange={(o) => !o && setConfirmDelete(false)}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Delete team</AlertDialogTitle>
            <AlertDialogDescription>
              {`Delete ${team.name}? Members keep their organization membership but lose all repository access granted through this team.`}
            </AlertDialogDescription>
          </AlertDialogHeader>
          @if (deleteError) {
            <p role="alert" className="text-[14px] text-destructive">
              {deleteError as string}
            </p>;
          }
          <AlertDialogFooter>
            <AlertDialogCancel disabled={deletePending}>Cancel</AlertDialogCancel>
            <Button
              type="button"
              variant="destructive"
              disabled={deletePending}
              data-testid="team-delete-confirm"
              onClick={() => void confirmDeleteTeam()}
            >
              Delete team
            </Button>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </section>;
  }
}
