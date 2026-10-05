import type {
  CollaboratorPermission,
  RepoCollaboratorPublic,
  RepoInvitePublic,
  RepoInvitesCreateItemResult,
} from "@oxidean/api-client";
import { Check, ChevronDown, Link2 } from "@octanejs/lucide";
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

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";

const PERM_OPTIONS: { value: CollaboratorPermission; label: string }[] = [
  { value: "read", label: "Read" },
  { value: "write", label: "Write" },
  { value: "admin", label: "Admin" },
];

function permLabel(p: CollaboratorPermission): string {
  return PERM_OPTIONS.find((o) => o.value === p)?.label ?? p;
}

function inviteExpiryLabel(inv: { expires_at?: string | null }): string {
  return inv.expires_at ? `expires ${inv.expires_at.slice(0, 10)}` : "never expires";
}

function inviteSeatsLabel(inv: { max_uses?: number | null; use_count: number }): string {
  return inv.max_uses == null
    ? `${inv.use_count} used · unlimited seats`
    : `${inv.use_count}/${inv.max_uses} seats used`;
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

  const [inviteMode, setInviteMode] = useState<"emails" | "link">("emails");
  const [inviteEmails, setInviteEmails] = useState("");
  const [inviteLinkExpiry, setInviteLinkExpiry] = useState("");
  const [inviteLinkMaxUses, setInviteLinkMaxUses] = useState("");
  const [inviteResults, setInviteResults] = useState<RepoInvitesCreateItemResult[]>([]);
  const [inviteError, setInviteError] = useState("");
  const [inviteSuccess, setInviteSuccess] = useState("");
  const [invitePending, setInvitePending] = useState(false);
  const [createdInviteUrl, setCreatedInviteUrl] = useState("");
  const [copyState, setCopyState] = useState<"idle" | "copied" | "error">("idle");
  const [copiedResultUrl, setCopiedResultUrl] = useState("");

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
      permission: "write" as CollaboratorPermission,
    },
    onSubmit: async ({ value }) => {
      setInviteError("");
      setInviteSuccess("");
      setCreatedInviteUrl("");
      setCopyState("idle");
      setCopiedResultUrl("");

      if (inviteMode === "link") {
        const maxUsesRaw = inviteLinkMaxUses.trim();
        let maxUses: number | null = null;
        if (maxUsesRaw) {
          const parsed = Number(maxUsesRaw);
          if (!Number.isInteger(parsed) || parsed <= 0) {
            setInviteError("Max seats must be a positive whole number.");
            return;
          }
          maxUses = parsed;
        }
        setInviteResults([]);
        setInvitePending(true);
        try {
          const res = await apiClient.repo.invites.createLink({
            owner,
            name,
            permission: value.permission,
            expires_at: inviteLinkExpiry || null,
            max_uses: maxUses,
          });
          if (!res.ok) {
            setInviteError(res.error.message || NETWORK_ERROR);
            setInvitePending(false);
            return;
          }
          setCreatedInviteUrl(res.data.invite_url);
          setInviteLinkExpiry("");
          setInviteLinkMaxUses("");
          setInvitePending(false);
          await refreshInvites();
        } catch {
          setInviteError(NETWORK_ERROR);
          setInvitePending(false);
        }
        return;
      }

      const emails = inviteEmails
        .split(/[\s,;]+/)
        .map((e) => e.trim())
        .filter(Boolean);
      if (emails.length === 0) {
        setInviteError("Enter at least one email address.");
        return;
      }
      setInviteResults([]);
      setInvitePending(true);
      try {
        const res = await apiClient.repo.invites.create({
          owner,
          name,
          emails,
          permission: value.permission,
        });
        if (!res.ok) {
          setInviteError(res.error.message || NETWORK_ERROR);
          setInvitePending(false);
          return;
        }
        setInviteResults(res.data.results);
        setInviteEmails("");
        const ok = res.data.results.filter((r) => r.ok).length;
        const failed = res.data.results.length - ok;
        setInviteSuccess(
          failed > 0
            ? `Created ${ok} invite${ok === 1 ? "" : "s"}; ${failed} recipient${failed === 1 ? "" : "s"} failed.`
            : `Created ${ok} invite${ok === 1 ? "" : "s"}.`,
        );
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

  async function copyResultUrl(url: string) {
    try {
      if (!navigator.clipboard?.writeText) {
        throw new Error("clipboard unavailable");
      }
      await navigator.clipboard.writeText(url);
      setCopiedResultUrl(url);
      window.setTimeout(() => setCopiedResultUrl(""), 2000);
    } catch {
      setCopiedResultUrl("");
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
        Invitations
      </h3>
      <p className="text-[14px] text-muted-foreground">
        Invite people by email or share a reusable link. They can join even when public sign-up is
        closed.
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
        <div className="flex gap-2" role="group" aria-label="Invite mode">
          <button
            type="button"
            data-testid="repo-invite-mode-emails"
            aria-pressed={inviteMode === "emails"}
            className={
              "rounded-full border px-3 py-1 text-[13px] transition-colors " +
              (inviteMode === "emails"
                ? "border-primary bg-primary/10 text-foreground"
                : "border-border text-muted-foreground hover:text-foreground")
            }
            onClick={() => {
              setInviteMode("emails");
              setInviteError("");
            }}
          >
            By email
          </button>
          <button
            type="button"
            data-testid="repo-invite-mode-link"
            aria-pressed={inviteMode === "link"}
            className={
              "rounded-full border px-3 py-1 text-[13px] transition-colors " +
              (inviteMode === "link"
                ? "border-primary bg-primary/10 text-foreground"
                : "border-border text-muted-foreground hover:text-foreground")
            }
            onClick={() => {
              setInviteMode("link");
              setInviteError("");
            }}
          >
            Invite link
          </button>
        </div>
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
                className="shrink-0"
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
        @if (inviteResults.length > 0) {
          <div
            className="max-h-64 overflow-y-auto rounded-md border border-border"
            data-testid="repo-invite-results"
          >
            <ul className="divide-y divide-border">
              @for (const r of inviteResults; key r.email) {
                <li className="flex flex-col gap-1 px-4 py-3">
                  <div className="flex items-center justify-between gap-2">
                    <p className="truncate text-[14px] font-medium text-foreground">
                      {r.email as string}
                    </p>
                    @if (r.ok && r.invite_url) {
                      <Button
                        type="button"
                        variant="outline"
                        size="sm"
                        className="shrink-0"
                        data-testid={`repo-invite-result-copy-${r.email}`}
                        onClick={() => void copyResultUrl(r.invite_url!)}
                      >
                        {
                          (copiedResultUrl === r.invite_url
                            ? "Copied"
                            : "Copy link") as string
                        }
                      </Button>;
                    }
                  </div>
                  @if (!r.ok) {
                    <p role="alert" className="text-[13px] text-destructive">
                      {(r.error ?? "Failed") as string}
                    </p>;
                  }
                </li>;
              }
            </ul>
          </div>;
        }
        @if (inviteMode === "emails") {
          <div className="flex flex-col gap-2">
            <Label htmlFor="repo-invite-emails">Emails</Label>
            <Textarea
              id="repo-invite-emails"
              data-testid="repo-invite-emails"
              placeholder={"one@example.com\ntwo@example.com" as string}
              value={inviteEmails as string}
              onInput={(ev) =>
                setInviteEmails((ev.currentTarget as HTMLTextAreaElement).value)
              }
              disabled={invitePending}
            />
            <p className="text-[13px] text-muted-foreground">
              Separate addresses with commas, spaces, or new lines (max 50).
            </p>
          </div>;
        } @else {
          <>
            <div className="flex flex-col gap-2">
              <Label htmlFor="repo-invite-link-expiry">Expires on (optional)</Label>
              <Input
                id="repo-invite-link-expiry"
                data-testid="repo-invite-link-expiry"
                type="date"
                value={inviteLinkExpiry as string}
                onInput={(ev) =>
                  setInviteLinkExpiry((ev.currentTarget as HTMLInputElement).value)
                }
                disabled={invitePending}
              />
              <p className="text-[13px] text-muted-foreground">
                Leave empty for a link that never expires.
              </p>
            </div>
            <div className="flex flex-col gap-2">
              <Label htmlFor="repo-invite-link-seats">Max seats (optional)</Label>
              <Input
                id="repo-invite-link-seats"
                data-testid="repo-invite-link-seats"
                type="number"
                min={1}
                step={1}
                placeholder="Unlimited"
                value={inviteLinkMaxUses as string}
                onInput={(ev) =>
                  setInviteLinkMaxUses((ev.currentTarget as HTMLInputElement).value)
                }
                disabled={invitePending}
              />
            </div>
          </>;
        }
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
            <>
              {(inviteMode === "link" ? "Create invite link" : "Send invitations") as string}
            </>;
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
                <p className="flex items-center gap-1.5 text-[14px] font-medium text-foreground">
                  @if (inv.email == null) {
                    <Link2
                      aria-hidden
                      size={14}
                      className="text-muted-foreground"
                    />;
                  }
                  {(inv.email ?? "Invite link") as string}
                </p>
                <p className="text-[13px] text-muted-foreground">
                  {
                    (permLabel(inv.permission) +
                      " · " +
                      inviteExpiryLabel(inv) +
                      " · " +
                      inviteSeatsLabel(inv)) as string
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
                ? revokeTarget.email
                  ? `Revokes the invitation for ${revokeTarget.email}. They will no longer be able to join with this link.`
                  : "Revokes this invite link. It will no longer be usable."
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
