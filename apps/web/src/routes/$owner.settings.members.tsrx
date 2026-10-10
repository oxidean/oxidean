import { useQuery, useQueryClient } from "@octanejs/tanstack-query";
import type {
  OrgInvitePublic,
  OrgInvitesCreateItemResult,
  OrgMemberPublic,
  OrgRole,
} from "@oxidean/api-client";
import { Check, ChevronDown, Link2 } from "@octanejs/lucide";
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
import { AppPage } from "@/components/app-page";
import {
  OrgSettingsShell,
  orgSettingsAccessQueryOptions,
  orgSettingsOwner,
} from "@/components/org/org-settings-shell.tsrx";
import { usePathname } from "@/lib/pathname.tsrx";

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";
const LAST_OWNER_ERROR = "Cannot demote or remove the last organization owner.";
const FORBIDDEN = "You need organization admin access to manage members.";

const ROLE_OPTIONS: { value: OrgRole; label: string }[] = [
  { value: "owner", label: "Owner" },
  { value: "admin", label: "Admin" },
  { value: "member", label: "Member" },
];

function roleLabel(role: OrgRole): string {
  return ROLE_OPTIONS.find((o) => o.value === role)?.label ?? role;
}

function inviteExpiryLabel(inv: { expires_at?: string | null }): string {
  return inv.expires_at ? `expires ${inv.expires_at.slice(0, 10)}` : "never expires";
}

function inviteSeatsLabel(inv: { max_uses?: number | null; use_count: number }): string {
  return inv.max_uses == null
    ? `${inv.use_count} used · unlimited seats`
    : `${inv.use_count}/${inv.max_uses} seats used`;
}

function membersQueryKey(slug: string) {
  return ["org", "members", slug] as const;
}

function invitesQueryKey(slug: string) {
  return ["org", "invites", slug] as const;
}

function RoleSelect({
  id,
  value,
  onChange,
  disabled,
}: {
  id: string;
  value: OrgRole;
  onChange: (role: OrgRole) => void;
  disabled?: boolean;
}) @{
  <SelectRoot
    value={value}
    onValueChange={(next) => {
      if (typeof next === "string") onChange(next as OrgRole);
    }}
  >
    <SelectTrigger id={id} className="min-w-[8rem]" aria-label="Role" disabled={disabled}>
      <SelectValue>{roleLabel(value) as string}</SelectValue>
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
            @for (const opt of ROLE_OPTIONS; key opt.value) {
              <SelectItem value={opt.value}>
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

/** Exported for members Wave 0 tests (ORG-01 / D-ORG-03). */
/** `/{owner}/settings/members` — org membership admin (title via shell). */
export function OrgMembersPage() @{
  <AppPage>
    <OrgSettingsShell active="members">
      <OrgMembersContent />
    </OrgSettingsShell>
  </AppPage>;
}

/** Exported for members Wave 0 tests (ORG-01 / D-ORG-03). */
function OrgMembersContent() @{
  const pathname = usePathname();
  const owner = orgSettingsOwner(pathname);
  const data = useQuery(orgSettingsAccessQueryOptions(owner)).data;
  const queryClient = useQueryClient();

  const org = data?.kind === "ready" ? data.org : null;
  const slug = org?.slug ?? owner;
  const canAdmin = data?.kind === "ready" && data.canAdmin;

  const membersQ = useQuery({
    queryKey: membersQueryKey(slug),
    enabled: !!slug,
    queryFn: async () => {
      const res = await apiClient.org.members.list({ slug });
      if (!res.ok) throw new Error(`${res.error.code}: ${res.error.message}`);
      return res.data;
    },
  });

  const invitesQ = useQuery({
    queryKey: invitesQueryKey(slug),
    enabled: !!slug && canAdmin,
    queryFn: async () => {
      const res = await apiClient.org.invites.list({ slug });
      if (!res.ok) throw new Error(`${res.error.code}: ${res.error.message}`);
      return res.data;
    },
  });

  const members: OrgMemberPublic[] = membersQ.data?.members ?? [];
  const invites: OrgInvitePublic[] = invitesQ.data?.invites ?? [];

  const [addError, setAddError] = useState("");
  const [addPending, setAddPending] = useState(false);

  const [inviteMode, setInviteMode] = useState<"emails" | "link">("emails");
  const [inviteEmails, setInviteEmails] = useState("");
  const [inviteLinkExpiry, setInviteLinkExpiry] = useState("");
  const [inviteLinkMaxUses, setInviteLinkMaxUses] = useState("");
  const [inviteResults, setInviteResults] = useState<OrgInvitesCreateItemResult[]>([]);
  const [inviteError, setInviteError] = useState("");
  const [inviteSuccess, setInviteSuccess] = useState("");
  const [invitePending, setInvitePending] = useState(false);
  const [createdInviteUrl, setCreatedInviteUrl] = useState("");
  const [copyState, setCopyState] = useState<"idle" | "copied" | "error">("idle");
  const [copiedResultUrl, setCopiedResultUrl] = useState("");

  const addForm = useForm({
    defaultValues: {
      username: "",
      role: "member" as OrgRole,
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
        const res = await apiClient.org.members.add({
          slug,
          username,
          role: value.role,
        });
        if (!res.ok) {
          if (res.error.code === "org.last_owner") {
            setAddError(LAST_OWNER_ERROR);
          } else if (res.error.code === "org.forbidden") {
            setAddError(FORBIDDEN);
          } else {
            setAddError(res.error.message || NETWORK_ERROR);
          }
          setAddPending(false);
          return;
        }
        addForm.reset();
        setAddPending(false);
        await refreshMembers();
      } catch {
        setAddError(NETWORK_ERROR);
        setAddPending(false);
      }
    },
  });

  const inviteForm = useForm({
    defaultValues: {
      role: "member" as OrgRole,
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
          const res = await apiClient.org.invites.createLink({
            slug,
            role: value.role,
            expires_at: inviteLinkExpiry || null,
            max_uses: maxUses,
          });
          if (!res.ok) {
            setInviteError(
              res.error.code === "org.forbidden" ? FORBIDDEN : res.error.message || NETWORK_ERROR,
            );
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
        const res = await apiClient.org.invites.create({
          slug,
          emails,
          role: value.role,
        });
        if (!res.ok) {
          if (res.error.code === "org.forbidden") {
            setInviteError(FORBIDDEN);
          } else {
            setInviteError(res.error.message || NETWORK_ERROR);
          }
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
  const [revokeTarget, setRevokeTarget] = useState<OrgInvitePublic | null>(null);
  const [revokePending, setRevokePending] = useState(false);
  const [revokeError, setRevokeError] = useState("");

  async function refreshMembers() {
    await queryClient.invalidateQueries({ queryKey: membersQueryKey(slug) });
  }

  async function refreshInvites() {
    await queryClient.invalidateQueries({ queryKey: invitesQueryKey(slug) });
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

  async function updateRole(member: OrgMemberPublic, role: OrgRole) {
    if (role === member.role) return;
    setRowError("");
    try {
      const res = await apiClient.org.members.updateRole({
        slug,
        user_id: member.user_id,
        role,
      });
      if (!res.ok) {
        if (res.error.code === "org.last_owner") {
          setRowError(LAST_OWNER_ERROR);
        } else {
          setRowError(res.error.message || NETWORK_ERROR);
        }
        return;
      }
      await refreshMembers();
    } catch {
      setRowError(NETWORK_ERROR);
    }
  }

  async function removeMember(member: OrgMemberPublic) {
    setRowError("");
    try {
      const res = await apiClient.org.members.remove({
        slug,
        user_id: member.user_id,
      });
      if (!res.ok) {
        if (res.error.code === "org.last_owner") {
          setRowError(LAST_OWNER_ERROR);
        } else {
          setRowError(res.error.message || NETWORK_ERROR);
        }
        return;
      }
      await refreshMembers();
    } catch {
      setRowError(NETWORK_ERROR);
    }
  }

  async function confirmRevoke() {
    if (!revokeTarget || revokePending) return;
    setRevokeError("");
    setRevokePending(true);
    try {
      const res = await apiClient.org.invites.revoke({
        slug,
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

  @if (!org) {
    <p className="text-[16px] text-muted-foreground">Loading…</p>;
  } @else {
    <div data-testid="org-settings-members">
      @if (rowError) {
        <div className="mb-4">
          <AuthErrorBanner message={rowError} />
        </div>;
      }

      <section aria-labelledby="members-table-heading" className="mb-12">
        <h2 id="members-table-heading" className="text-[18px] font-semibold text-foreground">
          Members
        </h2>
        @if (membersQ.isPending) {
          <p className="mt-4 text-[14px] text-muted-foreground">Loading members…</p>;
        } @else {
          <div className="mt-4 overflow-x-auto">
            <table className="w-full min-w-[28rem] border-collapse text-left text-[14px]">
              <thead>
                <tr className="border-b border-border text-muted-foreground">
                  <th className="py-2 pr-4 font-medium">User</th>
                  <th className="py-2 pr-4 font-medium">Role</th>
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
                        <RoleSelect
                          id={`role-${m.user_id}`}
                          value={m.role}
                          onChange={(role) => void updateRole(m, role)}
                        />;
                      } @else {
                        <span>{roleLabel(m.role) as string}</span>;
                      }
                    </td>
                    <td className="py-3">
                      @if (canAdmin) {
                        <Button
                          type="button"
                          variant="ghost"
                          className="h-11 text-destructive"
                          onClick={() => void removeMember(m)}
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
      </section>

      @if (canAdmin) {
        <div>
          <section aria-labelledby="add-member-heading" className="mb-12">
            <h2 id="add-member-heading" className="text-[18px] font-semibold text-foreground">
              Add member
            </h2>
            <p className="mt-1 text-[14px] text-muted-foreground">
              Add an existing user by username. Works even when public sign-up is closed.
            </p>
            <form
              className="mt-4 flex flex-col gap-4"
              method="post"
              action="#"
              onSubmit={(e) => {
                e.preventDefault();
                void addForm.handleSubmit();
              }}
            >
              @if (addError) {
                <AuthErrorBanner message={addError} />;
              }
              <addForm.Field name="username">
                {(field) => (
                  <MemberLookup
                    value={field.state.value}
                    onValueChange={field.handleChange}
                    context={{ kind: "org", slug }}
                  />
                )}
              </addForm.Field>
              <addForm.Subscribe selector={(s) => s.values.role}>
                {(addRole) => (
                  <div className="flex flex-col gap-2">
                    <Label htmlFor="add-member-role">Role</Label>
                    <RoleSelect
                      id="add-member-role"
                      value={addRole}
                      onChange={(role) => addForm.setFieldValue("role", role)}
                    />
                  </div>
                )}
              </addForm.Subscribe>
              <Button type="submit" className="h-11 w-fit" disabled={addPending}>
                @if (addPending) {
                  <>Adding…</>;
                } @else {
                  <>Add member</>;
                }
              </Button>
            </form>
          </section>

          <section aria-labelledby="invites-heading">
            <h2 id="invites-heading" className="text-[18px] font-semibold text-foreground">
              Invitations
            </h2>
            <p className="mt-1 text-[14px] text-muted-foreground">
              Invite people by email or share a reusable link. They can join even when public
              sign-up is closed.
            </p>

            <form
              className="mt-4 flex flex-col gap-4"
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
                  data-testid="org-invite-mode-emails"
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
                  data-testid="org-invite-mode-link"
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
                  <Label htmlFor="org-invite-url">Invite link</Label>
                  <div className="flex flex-col gap-2 sm:flex-row">
                    <Input
                      id="org-invite-url"
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
                  data-testid="org-invite-results"
                >
                  <ul className="divide-y divide-border">
                    @for (const r of inviteResults; key r.email) {
                      <li className="flex flex-col gap-1 px-4 py-3">
                        <div className="flex items-center justify-between gap-2">
                          <p className="truncate text-[14px] font-medium text-foreground">
                            {r.email as string}
                          </p>
                          @if (
                            r.ok &&
                            r.invite_url
                          ) {
                            <Button
                              type="button"
                              variant="outline"
                              size="sm"
                              className="shrink-0"
                              data-testid={`org-invite-result-copy-${r.email}`}
                              onClick={() =>
                                void copyResultUrl(r.invite_url!)
                              }
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
                          <p
                            role="alert"
                            className="text-[13px] text-destructive"
                          >
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
                  <Label htmlFor="invite-emails">Emails</Label>
                  <Textarea
                    id="invite-emails"
                    data-testid="invite-emails"
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
                    <Label htmlFor="invite-link-expiry">Expires on (optional)</Label>
                    <Input
                      id="invite-link-expiry"
                      data-testid="invite-link-expiry"
                      type="date"
                      value={inviteLinkExpiry as string}
                      onInput={(ev) =>
                        setInviteLinkExpiry(
                          (ev.currentTarget as HTMLInputElement).value,
                        )
                      }
                      disabled={invitePending}
                    />
                    <p className="text-[13px] text-muted-foreground">
                      Leave empty for a link that never expires.
                    </p>
                  </div>
                  <div className="flex flex-col gap-2">
                    <Label htmlFor="invite-link-seats">Max seats (optional)</Label>
                    <Input
                      id="invite-link-seats"
                      data-testid="invite-link-seats"
                      type="number"
                      min={1}
                      step={1}
                      placeholder="Unlimited"
                      value={inviteLinkMaxUses as string}
                      onInput={(ev) =>
                        setInviteLinkMaxUses(
                          (ev.currentTarget as HTMLInputElement).value,
                        )
                      }
                      disabled={invitePending}
                    />
                  </div>
                </>;
              }
              <inviteForm.Subscribe selector={(s) => s.values.role}>
                {(inviteRole) => (
                  <div className="flex flex-col gap-2">
                    <Label htmlFor="invite-role">Role</Label>
                    <RoleSelect
                      id="invite-role"
                      value={inviteRole}
                      onChange={(role) => inviteForm.setFieldValue("role", role)}
                    />
                  </div>
                )}
              </inviteForm.Subscribe>
              <Button type="submit" className="h-11 w-fit" disabled={invitePending}>
                @if (invitePending) {
                  <>Sending…</>;
                } @else {
                  <>
                    {
                      (inviteMode === "link"
                        ? "Create invite link"
                        : "Send invitations") as string
                    }
                  </>;
                }
              </Button>
            </form>

            <h3 className="mt-8 text-[16px] font-semibold text-foreground">
              Pending invitations
            </h3>
            @if (invitesQ.isPending) {
              <p className="mt-2 text-[14px] text-muted-foreground">Loading…</p>;
            } @else {
              @if (invites.length === 0) {
                <p className="mt-2 text-[14px] text-muted-foreground">
                  No pending invitations.
                </p>;
              } @else {
                <ul className="mt-3 divide-y divide-border border-y border-border">
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
                            (roleLabel(inv.role) +
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
            }
          </section>
        </div>;
      }

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
              @if (revokePending) {
                <>Revoking…</>;
              } @else {
                <>Revoke invite</>;
              }
            </Button>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>;
  }
}

/** Alias for Wave 0 import flexibility. */
