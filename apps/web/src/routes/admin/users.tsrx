import { createFileRoute, useLoaderData } from "@octanejs/tanstack-router";
import { useQuery, useQueryClient } from "@octanejs/tanstack-query";
import type {
  AdminUserAccessOrg,
  AdminUserAccessRepo,
  AdminUserPublic,
  AdminUsersGetAccessResponse,
  AdminUsersListResponse,
  InstanceInvitePublic,
  UserPublic,
  UserRole,
} from "@oxidean/api-client";
import { useEffect, useState } from "octane";
import { AdminSettingsLayout, SettingsShell } from "@/components/settings/settings-shell";
import {
  AlertDialog,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { apiClient } from "@/lib/api-client";
import { formatRelative } from "@/lib/relative-time";
import { authSessionQueryOptions, setAuthMeCache } from "@/lib/session-queries";
import { fetchAdminInvitesList, fetchAdminUsersList, fetchSessionMe } from "@/lib/ssr-auth";
import { toastSuccess } from "@/lib/toast";

export type AdminUsersLoaderData =
  | { kind: "unauthenticated" }
  | { kind: "forbidden" }
  | { kind: "error"; message: string }
  | {
      kind: "ready";
      me: UserPublic;
      users: AdminUsersListResponse;
      invites: InstanceInvitePublic[];
    };

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";
const FORBIDDEN = "You need admin access to manage users.";
const PAGE_SIZE = 50;

const usersQueryKey = (query: string, offset: number) =>
  ["admin", "users", "list", query, offset] as const;
const invitesQueryKey = ["admin", "invites", "list"] as const;

type ConfirmKind =
  | { kind: "role"; user: AdminUserPublic; nextRole: UserRole }
  | { kind: "revoke-sessions"; user: AdminUserPublic }
  | { kind: "ban"; user: AdminUserPublic }
  | { kind: "unban"; user: AdminUserPublic }
  | { kind: "revoke-invite"; invite: InstanceInvitePublic };

function roleLabel(role: UserRole): string {
  if (role === "sys-admin") return "System admin";
  if (role === "admin") return "Admin";
  return "User";
}

function isBanned(user: AdminUserPublic): boolean {
  return user.banned_at != null && user.banned_at !== "";
}

type AccessSummaryView = {
  orgs: AdminUserAccessOrg[];
  repos: AdminUserAccessRepo[];
  error?: string;
  loading: boolean;
};

function AdminUserAccessPanel({
  userId,
  summary,
}: {
  userId: string;
  summary: AccessSummaryView;
}) @{
  <div
    className="mt-3 max-w-xl rounded-md border border-border bg-muted/30 p-3 text-[13px]"
    data-testid={`admin-user-access-${userId}`}
  >
    @if (summary.loading) {
      <p className="text-muted-foreground">Loading access…</p>;
    } @else {
      @if (summary.error) {
        <p role="alert" className="text-destructive">
          {summary.error as string}
        </p>;
      } @else {
        <>
          <p className="font-medium text-foreground">Organizations</p>
          @if (summary.orgs.length === 0) {
            <p className="mt-1 text-muted-foreground">None</p>;
          } @else {
            <ul className="mt-1 list-inside list-disc text-muted-foreground">
              @for (const org of summary.orgs; key org.slug) {
                <li>
                  <a
                    href={`/${org.slug}`}
                    className="text-primary underline-offset-2 hover:underline"
                  >
                    {org.slug as string}
                  </a>
                  {` · ${org.role} · ${org.display_name}` as string}
                </li>;
              }
            </ul>;
          }
          <p className="mt-3 font-medium text-foreground">Repository grants</p>
          @if (summary.repos.length === 0) {
            <p className="mt-1 text-muted-foreground">None</p>;
          } @else {
            <ul className="mt-1 list-inside list-disc text-muted-foreground">
              @for (const repo of summary.repos; key `${repo.owner}/${repo.name}`) {
                <li>
                  <a
                    href={`/${repo.owner}/${repo.name}`}
                    className="text-primary underline-offset-2 hover:underline"
                  >
                    {`${repo.owner}/${repo.name}` as string}
                  </a>
                  {` · ${repo.permission}` as string}
                </li>;
              }
            </ul>;
          }
        </>;
      }
    }
  </div>;
}

function formatShortDate(iso: string): string {
  const ms = Date.parse(iso);
  if (Number.isNaN(ms)) return iso;
  return new Date(ms).toLocaleDateString(undefined, {
    year: "numeric",
    month: "short",
    day: "numeric",
  });
}

export const Route = createFileRoute("/admin/users")({
  component: AdminUsersPage,
  head: () => ({ meta: [{ title: "Users · Oxidean" }] }),
  loader: async (): Promise<AdminUsersLoaderData> => {
    try {
      const meRes = await fetchSessionMe();
      if (!meRes.ok) {
        if (meRes.error.code === "auth.unauthenticated") {
          return { kind: "unauthenticated" };
        }
        return {
          kind: "error",
          message: meRes.error.message || "Could not load session.",
        };
      }
      if (meRes.data.role !== "sys-admin") {
        return { kind: "forbidden" };
      }
      const [usersRes, invitesRes] = await Promise.all([
        fetchAdminUsersList({ data: { query: null, limit: PAGE_SIZE, offset: 0 } }),
        fetchAdminInvitesList(),
      ]);
      if (!usersRes.ok) {
        if (usersRes.error.code === "admin.forbidden") {
          return { kind: "forbidden" };
        }
        return {
          kind: "error",
          message: usersRes.error.message || FORBIDDEN,
        };
      }
      if (!invitesRes.ok) {
        if (invitesRes.error.code === "admin.forbidden") {
          return { kind: "forbidden" };
        }
        return {
          kind: "error",
          message: invitesRes.error.message || "Could not load invites.",
        };
      }
      return {
        kind: "ready",
        me: meRes.data,
        users: usersRes.data,
        invites: invitesRes.data.invites,
      };
    } catch {
      return { kind: "error", message: NETWORK_ERROR };
    }
  },
});

/** Exported for admin/users integration tests. */
export function AdminUsersPage() @{
  const queryClient = useQueryClient();
  const loaderData = useLoaderData({ from: "/admin/users" }) as AdminUsersLoaderData | undefined;

  const [searchDraft, setSearchDraft] = useState("");
  const [query, setQuery] = useState("");
  const [offset, setOffset] = useState(0);

  const [confirm, setConfirm] = useState<ConfirmKind | null>(null);
  const [confirmPending, setConfirmPending] = useState(false);
  const [confirmError, setConfirmError] = useState("");

  const [deleteTarget, setDeleteTarget] = useState<AdminUserPublic | null>(null);
  const [deletePhrase, setDeletePhrase] = useState("");
  const [deletePending, setDeletePending] = useState(false);
  const [deleteError, setDeleteError] = useState("");

  const [inviteEmail, setInviteEmail] = useState("");
  const [invitePending, setInvitePending] = useState(false);
  const [inviteError, setInviteError] = useState("");
  const [createdInviteUrl, setCreatedInviteUrl] = useState("");
  const [copyState, setCopyState] = useState<"idle" | "copied" | "error">("idle");

  const [accessUserId, setAccessUserId] = useState<string | null>(null);
  const [accessCache, setAccessCache] = useState<
    Record<string, AdminUsersGetAccessResponse | { error: string }>
  >({});
  const [accessLoadingId, setAccessLoadingId] = useState<string | null>(null);

  useEffect(() => {
    if (loaderData?.kind === "unauthenticated") {
      window.location.assign("/login?returnTo=/admin/users");
      return;
    }
    if (loaderData?.kind === "ready") {
      setAuthMeCache(queryClient, loaderData.me);
      queryClient.setQueryData(usersQueryKey("", 0), loaderData.users);
      queryClient.setQueryData(invitesQueryKey, loaderData.invites);
    }
  }, [loaderData, queryClient]);

  const me = useQuery({
    ...authSessionQueryOptions(),
    ...(loaderData?.kind === "ready"
      ? { initialData: loaderData.me }
      : loaderData?.kind === "unauthenticated"
        ? { initialData: null as UserPublic | null }
        : {}),
  });
  const isSysAdmin = me.isSuccess && me.data?.role === "sys-admin";
  const selfId = me.data?.id ?? "";

  const usersQ = useQuery({
    queryKey: usersQueryKey(query, offset),
    enabled: isSysAdmin || loaderData?.kind === "ready",
    initialData:
      loaderData?.kind === "ready" && query === "" && offset === 0 ? loaderData.users : undefined,
    queryFn: async (): Promise<AdminUsersListResponse> => {
      const res = await apiClient.admin.users.list({
        query: query || null,
        limit: PAGE_SIZE,
        offset,
      });
      if (!res.ok) throw new Error(res.error.message || FORBIDDEN);
      return res.data;
    },
  });

  const invitesQ = useQuery({
    queryKey: invitesQueryKey,
    enabled: isSysAdmin || loaderData?.kind === "ready",
    initialData: loaderData?.kind === "ready" ? loaderData.invites : undefined,
    queryFn: async (): Promise<InstanceInvitePublic[]> => {
      const res = await apiClient.admin.invites.list();
      if (!res.ok) throw new Error(res.error.message || FORBIDDEN);
      return res.data.invites;
    },
  });

  useEffect(() => {
    if (loaderData) return;
    if (me.isPending) return;
    if (me.isError) return;
    if (me.data === null) {
      window.location.assign("/login?returnTo=/admin/users");
    }
  }, [loaderData, me.isPending, me.isError, me.data]);

  async function refreshUsers() {
    await queryClient.invalidateQueries({ queryKey: ["admin", "users", "list"] });
  }

  async function refreshInvites() {
    await queryClient.invalidateQueries({ queryKey: invitesQueryKey });
  }

  function openConfirm(next: ConfirmKind) {
    setConfirmError("");
    setConfirm(next);
  }

  async function runConfirm() {
    if (!confirm || confirmPending) return;
    const action = confirm;
    setConfirmError("");
    setConfirmPending(true);
    try {
      if (action.kind === "role") {
        const res = await apiClient.admin.users.updateRole({
          user_id: action.user.id,
          role: action.nextRole,
        });
        if (!res.ok) {
          setConfirmError(res.error.message || NETWORK_ERROR);
          setConfirmPending(false);
          return;
        }
        toastSuccess(
          action.nextRole === "sys-admin"
            ? `Granted system admin to @${action.user.username}.`
            : `Demoted @${action.user.username} to user.`,
        );
      } else if (action.kind === "revoke-sessions") {
        const res = await apiClient.admin.users.revokeSessions({
          user_id: action.user.id,
        });
        if (!res.ok) {
          setConfirmError(res.error.message || NETWORK_ERROR);
          setConfirmPending(false);
          return;
        }
        toastSuccess(`Revoked sessions for @${action.user.username}.`);
      } else if (action.kind === "ban") {
        const res = await apiClient.admin.users.ban({ user_id: action.user.id });
        if (!res.ok) {
          setConfirmError(res.error.message || NETWORK_ERROR);
          setConfirmPending(false);
          return;
        }
        toastSuccess(`Banned @${action.user.username}.`);
      } else if (action.kind === "unban") {
        const res = await apiClient.admin.users.unban({ user_id: action.user.id });
        if (!res.ok) {
          setConfirmError(res.error.message || NETWORK_ERROR);
          setConfirmPending(false);
          return;
        }
        toastSuccess(`Unbanned @${action.user.username}.`);
      } else {
        const res = await apiClient.admin.invites.revoke({
          invite_id: action.invite.id,
        });
        if (!res.ok) {
          setConfirmError(res.error.message || NETWORK_ERROR);
          setConfirmPending(false);
          return;
        }
        toastSuccess(`Revoked invite for ${action.invite.email}.`);
        await refreshInvites();
      }
      setConfirmPending(false);
      setConfirm(null);
      if (action.kind !== "revoke-invite") {
        await refreshUsers();
      }
    } catch {
      setConfirmError(NETWORK_ERROR);
      setConfirmPending(false);
    }
  }

  async function runDelete() {
    if (!deleteTarget || deletePending) return;
    setDeleteError("");
    setDeletePending(true);
    try {
      const res = await apiClient.admin.users.delete({
        user_id: deleteTarget.id,
        confirmation: deletePhrase.trim(),
      });
      if (!res.ok) {
        setDeleteError(res.error.message || NETWORK_ERROR);
        setDeletePending(false);
        return;
      }
      toastSuccess(
        `Deleted @${deleteTarget.username} (${res.data.deleted_repos} repos, ${res.data.deleted_orgs} orgs).`,
      );
      setDeletePending(false);
      setDeleteTarget(null);
      setDeletePhrase("");
      await refreshUsers();
    } catch {
      setDeleteError(NETWORK_ERROR);
      setDeletePending(false);
    }
  }

  async function createInvite() {
    const email = inviteEmail.trim();
    if (!email) {
      setInviteError("Email is required.");
      return;
    }
    setInviteError("");
    setInvitePending(true);
    setCreatedInviteUrl("");
    setCopyState("idle");
    try {
      const res = await apiClient.admin.invites.create({ email });
      if (!res.ok) {
        setInviteError(res.error.message || NETWORK_ERROR);
        setInvitePending(false);
        return;
      }
      setCreatedInviteUrl(res.data.invite_url);
      setInviteEmail("");
      setInvitePending(false);
      toastSuccess(`Invite created for ${res.data.invite.email}.`);
      await refreshInvites();
    } catch {
      setInviteError(NETWORK_ERROR);
      setInvitePending(false);
    }
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

  async function toggleAccess(user: AdminUserPublic) {
    if (accessUserId === user.id) {
      setAccessUserId(null);
      return;
    }
    setAccessUserId(user.id);
    if (accessCache[user.id]) return;
    setAccessLoadingId(user.id);
    try {
      const res = await apiClient.admin.users.getAccess({ user_id: user.id });
      if (!res.ok) {
        setAccessCache((prev) => ({
          ...prev,
          [user.id]: { error: res.error.message || NETWORK_ERROR },
        }));
      } else {
        setAccessCache((prev) => ({ ...prev, [user.id]: res.data }));
      }
    } catch {
      setAccessCache((prev) => ({
        ...prev,
        [user.id]: { error: NETWORK_ERROR },
      }));
    } finally {
      setAccessLoadingId(null);
    }
  }

  function accessSummary(userId: string): {
    orgs: AdminUserAccessOrg[];
    repos: AdminUserAccessRepo[];
    error?: string;
    loading: boolean;
  } {
    const cached = accessCache[userId];
    const loading = accessLoadingId === userId;
    if (!cached) return { orgs: [], repos: [], loading };
    if ("error" in cached) return { orgs: [], repos: [], error: cached.error, loading };
    return { orgs: cached.orgs, repos: cached.repos, loading };
  }

  const readyFromLoader = loaderData?.kind === "ready";
  const sessionKnown =
    readyFromLoader ||
    loaderData?.kind === "unauthenticated" ||
    loaderData?.kind === "forbidden" ||
    loaderData?.kind === "error" ||
    !me.isPending;
  const signedOut =
    loaderData?.kind === "unauthenticated" ||
    (sessionKnown && !me.isPending && (!me.isSuccess || !me.data));
  const forbidden =
    loaderData?.kind === "forbidden" || (sessionKnown && me.isSuccess && !!me.data && !isSysAdmin);
  const loaderError = loaderData?.kind === "error" ? loaderData.message : "";
  const usersPage =
    usersQ.data ?? (readyFromLoader && query === "" && offset === 0 ? loaderData.users : undefined);
  const invites = invitesQ.data ?? (readyFromLoader ? loaderData.invites : []);
  const total = usersPage?.total ?? 0;
  const users = usersPage?.users ?? [];
  const canPrev = offset > 0;
  const canNext = offset + PAGE_SIZE < total;

  const confirmTitle =
    confirm?.kind === "role"
      ? confirm.nextRole === "sys-admin"
        ? "Grant system admin?"
        : "Demote to user?"
      : confirm?.kind === "revoke-sessions"
        ? "Revoke sessions?"
        : confirm?.kind === "ban"
          ? "Ban user?"
          : confirm?.kind === "unban"
            ? "Unban user?"
            : confirm?.kind === "revoke-invite"
              ? "Revoke invite?"
              : "Confirm";

  const confirmDescription =
    confirm?.kind === "role"
      ? confirm.nextRole === "sys-admin"
        ? `Grant system administrator access to @${confirm.user.username}.`
        : `Remove system administrator access from @${confirm.user.username}.`
      : confirm?.kind === "revoke-sessions"
        ? `Force-logout @${confirm.user.username} on all devices. They can sign in again.`
        : confirm?.kind === "ban"
          ? `Ban @${confirm.user.username}. Sessions end immediately; the account stays reserved.`
          : confirm?.kind === "unban"
            ? `Restore access for @${confirm.user.username}.`
            : confirm?.kind === "revoke-invite"
              ? `Revoke the invitation for ${confirm.invite.email}. They will no longer be able to join with this link.`
              : "";

  const confirmActionLabel =
    confirm?.kind === "role"
      ? confirm.nextRole === "sys-admin"
        ? "Grant system admin"
        : "Demote to user"
      : confirm?.kind === "revoke-sessions"
        ? "Revoke sessions"
        : confirm?.kind === "ban"
          ? "Ban user"
          : confirm?.kind === "unban"
            ? "Unban user"
            : confirm?.kind === "revoke-invite"
              ? "Revoke invite"
              : "Confirm";

  @if (!sessionKnown) {
    <></>;
  } @else {
    @if (signedOut) {
      <SettingsShell bare>
        <div className="text-center">
          <p className="text-[16px] text-muted-foreground">
            Sign in as a system administrator to manage users.
          </p>
          <a
            href="/login?returnTo=/admin/users"
            className="mt-4 inline-block text-primary underline-offset-2 hover:underline"
          >
            Sign in
          </a>
        </div>
      </SettingsShell>;
    } @else {
      @if (forbidden) {
        <SettingsShell bare>
          <p role="alert" className="text-center text-[16px] text-destructive">
            {FORBIDDEN as string}
          </p>
        </SettingsShell>;
      } @else {
        @if (loaderError && !usersPage) {
          <SettingsShell bare>
            <p role="alert" className="text-center text-[16px] text-destructive">
              {loaderError as string}
            </p>
          </SettingsShell>;
        } @else {
          <AdminSettingsLayout active="users" data-testid="admin-users-page">
            <h1 className="font-[family-name:var(--font-display)] text-[24px] font-semibold leading-[1.2] text-foreground">
              Users
            </h1>
            <p className="mt-2 text-[16px] text-muted-foreground">
              Search accounts, change roles, revoke sessions, ban or delete users, and invite people
              when public sign-up is closed.
            </p>

            <section className="mt-8" aria-labelledby="admin-users-table-heading">
              <h2
                id="admin-users-table-heading"
                className="font-[family-name:var(--font-display)] text-[18px] font-semibold"
              >
                Accounts
              </h2>
              <form
                method="post"
                action="#"
                className="mt-4 flex flex-col gap-3 sm:flex-row sm:items-end"
                onSubmit={(e) => {
                  e.preventDefault();
                  setOffset(0);
                  setQuery(searchDraft.trim());
                }}
              >
                <div className="flex min-w-0 flex-1 flex-col gap-2">
                  <Label htmlFor="admin-users-search">Search</Label>
                  <Input
                    id="admin-users-search"
                    data-testid="admin-users-search"
                    value={searchDraft as string}
                    onInput={(e) => setSearchDraft((e.currentTarget as HTMLInputElement).value)}
                    placeholder="Username, display name, or email"
                    autoComplete="off"
                  />
                </div>
                <Button type="submit" className="h-11 shrink-0">
                  Search
                </Button>
              </form>

              @if (usersQ.isError && !usersPage) {
                <p role="alert" className="mt-4 text-[14px] text-destructive">
                  {
                    (usersQ.error instanceof Error
                      ? usersQ.error.message
                      : FORBIDDEN) as string
                  }
                </p>;
              } @else {
                <div className="mt-4 overflow-x-auto">
                  <table className="w-full min-w-[48rem] border-collapse text-left text-[14px]">
                    <thead>
                      <tr className="border-b border-border text-muted-foreground">
                        <th className="py-2 pr-3 font-medium">Username</th>
                        <th className="py-2 pr-3 font-medium">Display name</th>
                        <th className="py-2 pr-3 font-medium">Email</th>
                        <th className="py-2 pr-3 font-medium">Role</th>
                        <th className="py-2 pr-3 font-medium">Verified</th>
                        <th className="py-2 pr-3 font-medium">Created</th>
                        <th className="py-2 font-medium">
                          <span className="sr-only">Actions</span>
                        </th>
                      </tr>
                    </thead>
                    <tbody>
                      @for (const user of users; key user.id) {
                        <tr className="border-b border-border align-top">
                          <td className="py-3 pr-3 font-medium text-foreground">
                            <span className="inline-flex flex-wrap items-center gap-2">
                              {("@" + user.username) as string}
                              @if (isBanned(user)) {
                                <Badge variant="destructive">Banned</Badge>;
                              }
                            </span>
                          </td>
                          <td className="py-3 pr-3 text-foreground">
                            {(user.display_name || "—") as string}
                          </td>
                          <td className="py-3 pr-3 text-muted-foreground">
                            {user.email as string}
                          </td>
                          <td className="py-3 pr-3">
                            {roleLabel(user.role) as string}
                          </td>
                          <td className="py-3 pr-3">
                            {(user.email_verified ? "Yes" : "No") as string}
                          </td>
                          <td className="py-3 pr-3 text-muted-foreground">
                            {formatRelative(user.created_at) as string}
                          </td>
                          <td className="py-3">
                            <div className="flex flex-wrap gap-1">
                              @if (
                                user.role === "sys-admin"
                              ) {
                                <Button
                                  type="button"
                                  variant="ghost"
                                  className="h-9 px-2"
                                  disabled={user.id === selfId}
                                  onClick={() =>
                                    openConfirm({
                                      kind: "role",
                                      user,
                                      nextRole: "user",
                                    })
                                  }
                                >
                                  Make user
                                </Button>;
                              } @else {
                                <Button
                                  type="button"
                                  variant="ghost"
                                  className="h-9 px-2"
                                  onClick={() =>
                                    openConfirm({
                                      kind: "role",
                                      user,
                                      nextRole: "sys-admin",
                                    })
                                  }
                                >
                                  Make sys-admin
                                </Button>;
                              }
                              <Button
                                type="button"
                                variant="ghost"
                                className="h-9 px-2"
                                onClick={() =>
                                  openConfirm({ kind: "revoke-sessions", user })
                                }
                              >
                                Revoke sessions
                              </Button>
                              @if (isBanned(user)) {
                                <Button
                                  type="button"
                                  variant="ghost"
                                  className="h-9 px-2"
                                  onClick={() =>
                                    openConfirm({ kind: "unban", user })
                                  }
                                >
                                  Unban
                                </Button>;
                              } @else {
                                <Button
                                  type="button"
                                  variant="ghost"
                                  className="h-9 px-2"
                                  disabled={
                                    user.id === selfId ||
                                    user.role === "sys-admin"
                                  }
                                  onClick={() =>
                                    openConfirm({ kind: "ban", user })
                                  }
                                >
                                  Ban
                                </Button>;
                              }
                              <Button
                                type="button"
                                variant="ghost"
                                className="h-9 px-2"
                                onClick={() => void toggleAccess(user)}
                              >
                                {
                                  (accessUserId === user.id
                                    ? "Hide access"
                                    : "View access") as string
                                }
                              </Button>
                              <Button
                                type="button"
                                variant="ghost"
                                className="h-9 px-2 text-destructive"
                                disabled={user.id === selfId}
                                onClick={() => {
                                  setDeleteError("");
                                  setDeletePhrase("");
                                  setDeleteTarget(user);
                                }}
                              >
                                Delete
                              </Button>
                            </div>
                            @if (
                              accessUserId === user.id
                            ) {
                              <AdminUserAccessPanel
                                userId={user.id}
                                summary={accessSummary(user.id)}
                              />;
                            }
                          </td>
                        </tr>;
                      } @empty {
                        <tr>
                          <td
                            colSpan={7}
                            className="py-6 text-center text-muted-foreground"
                          >
                            No users match this search.
                          </td>
                        </tr>;
                      }
                    </tbody>
                  </table>
                </div>;
                <div className="mt-4 flex flex-wrap items-center justify-between gap-3">
                  <p className="text-[13px] text-muted-foreground">
                    {
                      (total === 0
                        ? "0 users"
                        : `Showing ${offset + 1}–${Math.min(offset + PAGE_SIZE, total)} of ${total}`) as string
                    }
                  </p>
                  <div className="flex gap-2">
                    <Button
                      type="button"
                      variant="secondary"
                      disabled={!canPrev}
                      onClick={() => setOffset(Math.max(0, offset - PAGE_SIZE))}
                    >
                      Previous
                    </Button>
                    <Button
                      type="button"
                      variant="secondary"
                      disabled={!canNext}
                      onClick={() => setOffset(offset + PAGE_SIZE)}
                    >
                      Next
                    </Button>
                  </div>
                </div>;
              }
            </section>

            <section className="mt-12" aria-labelledby="admin-invites-heading">
              <h2
                id="admin-invites-heading"
                className="font-[family-name:var(--font-display)] text-[18px] font-semibold"
              >
                Invite people
              </h2>
              <p className="mt-1 text-[14px] text-muted-foreground">
                Send an instance invite by email. Works even when public sign-up is closed. Copy the
                link if email delivery is the log sink.
              </p>

              <form
                method="post"
                action="#"
                className="mt-4 flex flex-col gap-4"
                onSubmit={(e) => {
                  e.preventDefault();
                  void createInvite();
                }}
              >
                @if (inviteError) {
                  <p role="alert" className="text-[14px] text-destructive">
                    {inviteError as string}
                  </p>;
                }
                <div className="flex flex-col gap-2 sm:flex-row sm:items-end">
                  <div className="flex min-w-0 flex-1 flex-col gap-2">
                    <Label htmlFor="admin-invite-email">Email</Label>
                    <Input
                      id="admin-invite-email"
                      data-testid="admin-invite-email"
                      type="email"
                      autoComplete="email"
                      value={inviteEmail as string}
                      onInput={(e) => setInviteEmail((e.currentTarget as HTMLInputElement).value)}
                      disabled={invitePending}
                    />
                  </div>
                  <Button
                    type="submit"
                    className="h-11 shrink-0"
                    disabled={invitePending}
                    data-testid="admin-invite-create"
                  >
                    {(invitePending ? "Creating…" : "Create invite") as string}
                  </Button>
                </div>
              </form>

              @if (createdInviteUrl) {
                <div
                  className="mt-4 rounded-md border border-border p-4"
                  data-testid="admin-invite-url-panel"
                >
                  <p className="text-[14px] font-medium text-foreground">
                    Invite link (copy now)
                  </p>
                  <div className="mt-3 flex flex-col gap-3 sm:flex-row sm:items-stretch">
                    <Input
                      readOnly
                      value={createdInviteUrl as string}
                      aria-label="Invite URL"
                      data-testid="admin-invite-url"
                      className="min-w-0 flex-1 font-mono text-[13px]"
                      onFocus={(e) => {
                        const el = e.currentTarget as HTMLInputElement;
                        el.select();
                      }}
                    />
                    <Button
                      type="button"
                      variant="outline"
                      className="h-11 shrink-0"
                      data-testid="admin-invite-url-copy"
                      aria-label="Copy invite URL"
                      onClick={() => void copyInviteUrl()}
                    >
                      {(copyState === "copied" ? "Copied" : "Copy link") as string}
                    </Button>
                  </div>
                  @if (copyState === "error") {
                    <p role="alert" className="mt-2 text-[14px] text-destructive">
                      Couldn’t copy. Select the link and copy manually.
                    </p>;
                  }
                </div>;
              }

              <h3 className="mt-8 text-[16px] font-semibold text-foreground">
                Pending invitations
              </h3>
              @if (invitesQ.isError) {
                <p role="alert" className="mt-2 text-[14px] text-destructive">
                  {
                    (invitesQ.error instanceof Error
                      ? invitesQ.error.message
                      : "Could not load invites.") as string
                  }
                </p>;
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
                          <p className="text-[14px] font-medium text-foreground">
                            {inv.email as string}
                          </p>
                          <p className="text-[13px] text-muted-foreground">
                            {("expires " + formatShortDate(inv.expires_at)) as string}
                          </p>
                        </div>
                        <Button
                          type="button"
                          variant="outline"
                          className="h-11"
                          onClick={() =>
                            openConfirm({ kind: "revoke-invite", invite: inv })
                          }
                        >
                          Revoke
                        </Button>
                      </li>;
                    }
                  </ul>;
                }
              }
            </section>

            <AlertDialog
              open={confirm != null}
              onOpenChange={(next) => {
                if (confirmPending) return;
                if (!next) {
                  setConfirmError("");
                  setConfirm(null);
                }
              }}
            >
              <AlertDialogContent>
                <AlertDialogHeader>
                  <AlertDialogTitle>{confirmTitle as string}</AlertDialogTitle>
                  <AlertDialogDescription>{confirmDescription as string}</AlertDialogDescription>
                </AlertDialogHeader>
                @if (confirmError) {
                  <p role="alert" className="text-[14px] text-destructive">
                    {confirmError as string}
                  </p>;
                }
                <AlertDialogFooter>
                  <AlertDialogCancel disabled={confirmPending}>Cancel</AlertDialogCancel>
                  <Button
                    type="button"
                    variant={
                      confirm?.kind === "ban" || confirm?.kind === "revoke-invite"
                        ? "destructive"
                        : "default"
                    }
                    disabled={confirmPending}
                    onClick={() => void runConfirm()}
                  >
                    {(confirmPending ? "Working…" : confirmActionLabel) as string}
                  </Button>
                </AlertDialogFooter>
              </AlertDialogContent>
            </AlertDialog>

            <Dialog
              open={deleteTarget != null}
              onOpenChange={(next) => {
                if (deletePending) return;
                if (!next) {
                  setDeleteTarget(null);
                  setDeletePhrase("");
                  setDeleteError("");
                }
              }}
            >
              <DialogContent showCloseButton={!deletePending}>
                <DialogHeader>
                  <DialogTitle>Delete user?</DialogTitle>
                  <DialogDescription>
                    {
                      (deleteTarget
                        ? `Permanently deletes @${deleteTarget.username}, their personal repositories, and sole-owner organizations. Type the username to confirm.`
                        : "Permanently delete this account.") as string
                    }
                  </DialogDescription>
                </DialogHeader>
                <div className="flex flex-col gap-2">
                  <Label htmlFor="admin-delete-confirm">Username</Label>
                  <Input
                    id="admin-delete-confirm"
                    data-testid="admin-delete-confirm"
                    value={deletePhrase as string}
                    onInput={(e) => setDeletePhrase((e.currentTarget as HTMLInputElement).value)}
                    autoComplete="off"
                    placeholder={deleteTarget?.username ?? ""}
                    disabled={deletePending}
                  />
                </div>
                @if (deleteError) {
                  <p role="alert" className="text-[14px] text-destructive">
                    {deleteError as string}
                  </p>;
                }
                <DialogFooter>
                  <Button
                    type="button"
                    variant="secondary"
                    disabled={deletePending}
                    onClick={() => {
                      setDeleteTarget(null);
                      setDeletePhrase("");
                      setDeleteError("");
                    }}
                  >
                    Cancel
                  </Button>
                  <Button
                    type="button"
                    variant="destructive"
                    disabled={
                      deletePending ||
                      !deleteTarget ||
                      deletePhrase.trim() !== deleteTarget.username
                    }
                    onClick={() => void runDelete()}
                  >
                    {(deletePending ? "Deleting…" : "Delete user") as string}
                  </Button>
                </DialogFooter>
              </DialogContent>
            </Dialog>
          </AdminSettingsLayout>;
        }
      }
    }
  }
}
