import type {
  CollaboratorPermission,
  RepoCollaboratorPublic,
  RepoInvitePublic,
} from "@oxidean/api-client";
import { Check, ChevronDown } from "@octanejs/lucide";
import { useQuery, useQueryClient } from "@octanejs/tanstack-query";
import { useForm } from "@octanejs/tanstack-form";
import { useState } from "octane";
import { AuthErrorBanner } from "@/components/auth-shell";
import { MemberLookup } from "@/components/org/member-lookup";
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

function collaboratorsQueryKey(owner: string, name: string) {
  return ["repo", "collaborators", owner, name] as const;
}

function invitesQueryKey(owner: string, name: string) {
  return ["repo", "invites", owner, name] as const;
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

export type CollaboratorsPanelProps = {
  owner: string;
  name: string;
};

/**
 * Repo Collaborators settings section (ORG-03 / D-ORG-02c / D-ORG-04).
 * Parent route must gate on repo.can_admin — this panel assumes admin access.
 */
export function CollaboratorsPanel({ owner, name }: CollaboratorsPanelProps) @{
  const queryClient = useQueryClient();
  const listQ = useQuery({
    queryKey: collaboratorsQueryKey(owner, name),
    queryFn: async () => {
      const res = await apiClient.repo.collaborators.list({ owner, name });
      if (!res.ok) {
        throw new Error(res.error.message || NETWORK_ERROR);
      }
      return res.data;
    },
  });

  const invitesQ = useQuery({
    queryKey: invitesQueryKey(owner, name),
    queryFn: async () => {
      const res = await apiClient.repo.invites.list({ owner, name });
      if (!res.ok) {
        throw new Error(res.error.message || NETWORK_ERROR);
      }
      return res.data;
    },
  });

  const collaborators: RepoCollaboratorPublic[] = listQ.data?.collaborators ?? [];
  const invites: RepoInvitePublic[] = invitesQ.data?.invites ?? [];

  const [addError, setAddError] = useState("");
  const [addPending, setAddPending] = useState(false);
  const [showAdd, setShowAdd] = useState(false);

  const [inviteError, setInviteError] = useState("");
  const [inviteSuccess, setInviteSuccess] = useState("");
  const [invitePending, setInvitePending] = useState(false);
  const [createdInviteUrl, setCreatedInviteUrl] = useState("");
  const [copyState, setCopyState] = useState<"idle" | "copied" | "error">("idle");

  const addForm = useForm({
    defaultValues: {
      username: "",
      permission: "write" as CollaboratorPermission,
    },
    onSubmit: async ({ value }) => {
      setAddError("");
      const username = value.username.trim();
      if (!username) {
        setAddError("Username is required.");
        return;
      }
      setAddPending(true);
      try {
        const res = await apiClient.repo.collaborators.add({
          owner,
          name,
          username,
          permission: value.permission,
        });
        if (!res.ok) {
          setAddError(res.error.message || NETWORK_ERROR);
          setAddPending(false);
          return;
        }
        addForm.reset();
        setShowAdd(false);
        setAddPending(false);
        await refreshCollabs();
      } catch {
        setAddError(NETWORK_ERROR);
        setAddPending(false);
      }
    },
  });

  const inviteForm = useForm({
    defaultValues: {
      email: "",
      permission: "write" as CollaboratorPermission,
    },
    onSubmit: async ({ value }) => {
      setInviteError("");
      setInviteSuccess("");
      setCreatedInviteUrl("");
      setCopyState("idle");
      const email = value.email.trim();
      if (!email) {
        setInviteError("Email is required.");
        return;
      }
      setInvitePending(true);
      try {
        const res = await apiClient.repo.invites.create({
          owner,
          name,
          email,
          permission: value.permission,
        });
        if (!res.ok) {
          setInviteError(res.error.message || NETWORK_ERROR);
          setInvitePending(false);
          return;
        }
        inviteForm.reset();
        setInviteSuccess("Invitation sent.");
        setCreatedInviteUrl(res.data.invite_url);
        setInvitePending(false);
        await refreshInvites();
      } catch {
        setInviteError(NETWORK_ERROR);
        setInvitePending(false);
      }
    },
  });

  const [rowError, setRowError] = useState("");
  const [removeTarget, setRemoveTarget] = useState<RepoCollaboratorPublic | null>(null);
  const [removePending, setRemovePending] = useState(false);
  const [removeError, setRemoveError] = useState("");
  const [revokeTarget, setRevokeTarget] = useState<RepoInvitePublic | null>(null);
  const [revokePending, setRevokePending] = useState(false);
  const [revokeError, setRevokeError] = useState("");

  async function refreshCollabs() {
    await queryClient.invalidateQueries({
      queryKey: collaboratorsQueryKey(owner, name),
    });
  }

  async function refreshInvites() {
    await queryClient.invalidateQueries({
      queryKey: invitesQueryKey(owner, name),
    });
  }

  async function copyInviteUrl() {
    if (!createdInviteUrl) return;
    try {
      if (!navigator.clipboard?.writeText) {
        throw new Error("clipboard unavailable");
      }
      await navigator.clipboard.writeText(createdInviteUrl);
      setCopyState("copied");
      window.setTimeout(() => setCopyState("idle"), 2000);
    } catch {
      setCopyState("error");
    }
  }

  async function updatePermission(row: RepoCollaboratorPublic, permission: CollaboratorPermission) {
    if (permission === row.permission) return;
    setRowError("");
    try {
      const res = await apiClient.repo.collaborators.update({
        owner,
        name,
        user_id: row.user_id,
        permission,
      });
      if (!res.ok) {
        setRowError(res.error.message || NETWORK_ERROR);
        return;
      }
      await refreshCollabs();
    } catch {
      setRowError(NETWORK_ERROR);
    }
  }

  async function confirmRemove() {
    if (!removeTarget) return;
    setRemoveError("");
    setRemovePending(true);
    try {
      const res = await apiClient.repo.collaborators.remove({
        owner,
        name,
        user_id: removeTarget.user_id,
      });
      if (!res.ok) {
        setRemoveError(res.error.message || NETWORK_ERROR);
        setRemovePending(false);
        return;
      }
      setRemovePending(false);
      setRemoveTarget(null);
      await refreshCollabs();
    } catch {
      setRemoveError(NETWORK_ERROR);
      setRemovePending(false);
    }
  }

  async function confirmRevoke() {
    if (!revokeTarget || revokePending) return;
    setRevokeError("");
    setRevokePending(true);
    try {
      const res = await apiClient.repo.invites.revoke({
        owner,
        name,
        invite_id: revokeTarget.id,
      });
      if (!res.ok) {
        setRevokeError(res.error.message || NETWORK_ERROR);
        setRevokePending(false);
        return;
      }
      setRevokePending(false);
      setRevokeTarget(null);
      await refreshInvites();
    } catch {
      setRevokeError(NETWORK_ERROR);
      setRevokePending(false);
    }
  }

  <section
    className="mt-12 flex flex-col gap-4"
    aria-labelledby="collaborators-heading"
    data-testid="collaborators-panel"
  >
    <h2
      id="collaborators-heading"
      className="font-[family-name:var(--font-display)] text-[18px] font-semibold text-foreground"
    >
      Collaborators
    </h2>
    <p className="text-[14px] text-muted-foreground">
      Grant people direct access to this repository with read, write, or admin permission.
    </p>

    @if (rowError) {
      <AuthErrorBanner message={rowError} />;
    }

    @if (listQ.isPending) {
      <p className="text-[14px] text-muted-foreground">Loading collaborators…</p>;
    } @else if (listQ.isError) {
      <p role="alert" className="text-[14px] text-destructive">
        {(listQ.error instanceof Error ? listQ.error.message : NETWORK_ERROR) as string}
      </p>;
    } @else if (collaborators.length === 0 && !showAdd) {
      <>
        <p className="text-[14px] text-muted-foreground">
          No collaborators yet. Add someone by username to share access.
        </p>
        <Button
          type="button"
          className="h-11 w-fit"
          data-testid="collab-add-open"
          onClick={() => setShowAdd(true)}
        >
          Add collaborator
        </Button>
      </>;
    } @else {
      <>
        <div className="overflow-x-auto">
          <table className="w-full min-w-[28rem] border-collapse text-left text-[14px]">
            <thead>
              <tr className="border-b border-border text-muted-foreground">
                <th className="py-2 pr-4 font-medium">User</th>
                <th className="py-2 pr-4 font-medium">Permission</th>
                <th className="py-2 font-medium">
                  <span className="sr-only">Actions</span>
                </th>
              </tr>
            </thead>
            <tbody>
              @for (const c of collaborators; key c.user_id) {
                <tr className="border-b border-border">
                  <td className="py-3 pr-4 font-medium text-foreground">
                    {"@" + (c.username as string)}
                  </td>
                  <td className="py-3 pr-4">
                    <PermissionSelect
                      id={`collab-perm-${c.user_id}`}
                      value={c.permission}
                      onChange={(p) => void updatePermission(c, p)}
                    />
                  </td>
                  <td className="py-3">
                    <Button
                      type="button"
                      variant="ghost"
                      className="h-11 text-destructive"
                      onClick={() => {
                        setRemoveError("");
                        setRemoveTarget(c);
                      }}
                    >
                      Remove
                    </Button>
                  </td>
                </tr>;
              }
            </tbody>
          </table>
        </div>
        @if (!showAdd) {
          <Button type="button" className="h-11 w-fit" onClick={() => setShowAdd(true)}>
            Add collaborator
          </Button>;
        }
      </>;
    }

    @if (showAdd) {
      <form
        className="mt-2 flex flex-col gap-4 rounded-md border border-border p-4"
        method="post"
        action="#"
        onSubmit={(e) => {
          e.preventDefault();
          void addForm.handleSubmit();
        }}
      >
        <h3 className="text-[16px] font-semibold text-foreground">Add collaborator</h3>
        @if (addError) {
          <AuthErrorBanner message={addError} />;
        }
        <addForm.Field name="username">
          {(field) => (
            <MemberLookup
              id="collab-lookup"
              label="Username"
              value={field.state.value}
              onValueChange={field.handleChange}
              context={{ kind: "repo", owner, name }}
            />
          )}
        </addForm.Field>
        <addForm.Subscribe selector={(s) => s.values.permission}>
          {(addPerm) => (
            <div className="flex flex-col gap-2">
              <Label htmlFor="collab-add-perm">Permission</Label>
              <PermissionSelect
                id="collab-add-perm"
                value={addPerm}
                onChange={(p) => addForm.setFieldValue("permission", p)}
                ariaLabel="Permission"
              />
            </div>
          )}
        </addForm.Subscribe>
        <div className="flex flex-wrap gap-2">
          <Button type="submit" className="h-11" disabled={addPending}>
            @if (addPending) {
              <>Adding…</>;
            } @else {
              <>Add collaborator</>;
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
              addForm.reset();
            }}
          >
            Cancel
          </Button>
        </div>
      </form>;
    }

    <section className="mt-8 flex flex-col gap-4" aria-labelledby="repo-invites-heading">
      <h3 id="repo-invites-heading" className="text-[16px] font-semibold text-foreground">
        Email invitations
      </h3>
      <p className="text-[14px] text-muted-foreground">
        Invite someone by email. They can join even when public sign-up is closed.
      </p>

      <form
        className="flex flex-col gap-4"
        method="post"
        action="#"
        onSubmit={(e) => {
          e.preventDefault();
          void inviteForm.handleSubmit();
        }}
      >
        @if (inviteError) {
          <AuthErrorBanner message={inviteError} />;
        }
        @if (inviteSuccess) {
          <p role="status" className="text-[14px] text-foreground">
            {inviteSuccess as string}
          </p>;
        }
        @if (createdInviteUrl) {
          <div className="flex flex-col gap-2 rounded-md border border-border p-3">
            <Label htmlFor="repo-invite-url">Invite link</Label>
            <div className="flex flex-col gap-2 sm:flex-row">
              <Input
                id="repo-invite-url"
                readOnly
                value={createdInviteUrl as string}
                className="font-mono text-[13px]"
              />
              <Button
                type="button"
                variant="secondary"
                className="h-11 shrink-0"
                onClick={() => void copyInviteUrl()}
              >
                {(copyState === "copied" ? "Copied" : "Copy link") as string}
              </Button>
            </div>
            @if (copyState === "error") {
              <p role="alert" className="text-[13px] text-destructive">
                Could not copy. Select the link and copy it manually.
              </p>;
            }
          </div>;
        }
        <inviteForm.Field name="email">
          {(field) => (
            <div className="flex flex-col gap-2">
              <Label htmlFor="repo-invite-email">Email</Label>
              <Input
                id="repo-invite-email"
                name="email"
                type="email"
                autoComplete="email"
                value={field.state.value}
                onBlur={field.handleBlur}
                onInput={(ev) => field.handleChange((ev.currentTarget as HTMLInputElement).value)}
              />
            </div>
          )}
        </inviteForm.Field>
        <inviteForm.Subscribe selector={(s) => s.values.permission}>
          {(invitePerm) => (
            <div className="flex flex-col gap-2">
              <Label htmlFor="repo-invite-perm">Permission</Label>
              <PermissionSelect
                id="repo-invite-perm"
                value={invitePerm}
                onChange={(p) => inviteForm.setFieldValue("permission", p)}
                ariaLabel="Invite permission"
              />
            </div>
          )}
        </inviteForm.Subscribe>
        <Button type="submit" className="h-11 w-fit" disabled={invitePending}>
          @if (invitePending) {
            <>Sending…</>;
          } @else {
            <>Send invitation</>;
          }
        </Button>
      </form>

      <h4 className="mt-2 text-[15px] font-semibold text-foreground">Pending invitations</h4>
      @if (invitesQ.isPending) {
        <p className="text-[14px] text-muted-foreground">Loading…</p>;
      } @else if (invitesQ.isError) {
        <p role="alert" className="text-[14px] text-destructive">
          {
            (invitesQ.error instanceof Error
              ? invitesQ.error.message
              : NETWORK_ERROR) as string
          }
        </p>;
      } @else if (invites.length === 0) {
        <p className="text-[14px] text-muted-foreground">No pending invitations.</p>;
      } @else {
        <ul className="divide-y divide-border border-y border-border">
          @for (const inv of invites; key inv.id) {
            <li className="flex flex-wrap items-center justify-between gap-3 py-3">
              <div>
                <p className="text-[14px] font-medium text-foreground">
                  {inv.email as string}
                </p>
                <p className="text-[13px] text-muted-foreground">
                  {
                    (permLabel(inv.permission) +
                      " · expires " +
                      inv.expires_at) as string
                  }
                </p>
              </div>
              <Button
                type="button"
                variant="outline"
                className="h-11"
                onClick={() => {
                  setRevokeError("");
                  setRevokeTarget(inv);
                }}
              >
                Revoke
              </Button>
            </li>;
          }
        </ul>;
      }
    </section>

    <AlertDialog
      open={!!removeTarget}
      onOpenChange={(open) => {
        if (removePending) return;
        if (!open) {
          setRemoveTarget(null);
          setRemoveError("");
        }
      }}
    >
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>Remove collaborator?</AlertDialogTitle>
          <AlertDialogDescription>
            {
              (removeTarget
                ? `Remove @${removeTarget.username} from this repository?`
                : "") as string
            }
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
            onClick={() => void confirmRemove()}
          >
            {(removePending ? "Removing…" : "Remove collaborator") as string}
          </Button>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>

    <AlertDialog
      open={revokeTarget != null}
      onOpenChange={(next) => {
        if (revokePending) return;
        if (!next) {
          setRevokeError("");
          setRevokeTarget(null);
        }
      }}
    >
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>Revoke invite?</AlertDialogTitle>
          <AlertDialogDescription>
            {
              (revokeTarget
                ? `Revokes the invitation for ${revokeTarget.email}. They will no longer be able to join with this link.`
                : "Revoke this invitation?") as string
            }
          </AlertDialogDescription>
        </AlertDialogHeader>
        @if (revokeError) {
          <p role="alert" className="text-[14px] text-destructive">
            {revokeError as string}
          </p>;
        }
        <AlertDialogFooter>
          <AlertDialogCancel disabled={revokePending}>Keep invite</AlertDialogCancel>
          <Button
            type="button"
            variant="destructive"
            disabled={revokePending}
            onClick={() => void confirmRevoke()}
          >
            {(revokePending ? "Revoking…" : "Revoke invite") as string}
          </Button>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  </section>;
}
