import { createFileRoute, useLoaderData } from "@octanejs/tanstack-router";
import { useQuery, useQueryClient } from "@octanejs/tanstack-query";
import type {
  AdminInvitesCreateItemResult,
  AdminSessionPublic,
  AdminUserActivityItem,
  AdminUserPublic,
  AdminUsersListResponse,
  InstanceInvitePublic,
  UserPublic,
  UserRole,
} from "@oxidean/api-client";
import {
  Ban,
  EllipsisVertical,
  Eye,
  Link2,
  LogOut,
  RotateCcw,
  Search,
  ShieldMinus,
  ShieldPlus,
  Trash2,
  UserPlus,
} from "@octanejs/lucide";
import { useEffect, useState } from "octane";
import { AvatarPreview } from "@/components/avatar-preview";
import { SettingsPageSkeleton } from "@/components/page-skeletons";
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
import { Button, buttonVariants } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { EmptyState } from "@/components/ui/empty-state";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Textarea } from "@/components/ui/textarea";
import { List, ListHeader, ListRow } from "@/components/ui/list";
import { PageHeader } from "@/components/ui/page-header";
import { apiClient } from "@/lib/api-client";
import { useAppNavigate } from "@/lib/app-navigate";
import { formatRelative } from "@/lib/relative-time";
import { authSessionQueryOptions, setAuthMeCache } from "@/lib/session-queries";
import {
  fetchAdminInvitesList,
  fetchAdminUsersList,
  fetchSessionMe,
  requireSessionRedirect,
} from "@/lib/ssr-auth";
import { toastSuccess, toastWarning } from "@/lib/toast";
import { cn } from "@/lib/utils";
import { AppLink } from "@/components/ui/app-link";

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

function isBanned(user: AdminUserPublic): boolean {
  return user.banned_at != null && user.banned_at !== "";
}

function inviteListLabel(inv: InstanceInvitePublic): string {
  return inv.email ?? "Invite link";
}

function inviteExpiryLabel(inv: { expires_at?: string | null }): string {
  return inv.expires_at ? `Expires ${formatShortDate(inv.expires_at)}` : "Never expires";
}

function inviteSeatsLabel(inv: { max_uses?: number | null; use_count: number }): string {
  return inv.max_uses == null
    ? `${inv.use_count} used · unlimited seats`
    : `${inv.use_count}/${inv.max_uses} seats used`;
}

const ACTIVITY_SOURCES = [
  { value: "all", label: "All" },
  { value: "audit", label: "Audit log" },
  { value: "repository", label: "Repository" },
] as const;

type ActivitySource = (typeof ACTIVITY_SOURCES)[number]["value"];

function activityItemText(item: AdminUserActivityItem): string {
  if (item.source === "repository") {
    const repo =
      item.repo_owner && item.repo_name ? `${item.repo_owner}/${item.repo_name}` : "repository";
    const bits = [repo, item.ref_name ?? "", item.commit_message ?? ""].filter(Boolean);
    return bits.join(" · ");
  }
  const bits = [
    item.target_type && item.target_id ? `${item.target_type} ${item.target_id}` : "",
    item.detail ?? "",
  ].filter(Boolean);
  return bits.join(" · ");
}

function sessionAgentLabel(ua?: string | null): string {
  if (!ua) return "Unknown client";
  return ua.length > 80 ? `${ua.slice(0, 80)}…` : ua;
}

/** Expanded row detail: access grants + sessions (client metadata) + activity log. */
function AdminUserDetailPanel({ userId }: { userId: string }) @{
  const [actSource, setActSource] = useState<ActivitySource>("all");
  const [actType, setActType] = useState("");

  const accessQ = useQuery({
    queryKey: ["admin", "users", "access", userId] as const,
    queryFn: async () => {
      const res = await apiClient.admin.users.getAccess({ user_id: userId });
      if (!res.ok) throw new Error(res.error.message || NETWORK_ERROR);
      return res.data;
    },
  });
  const sessionsQ = useQuery({
    queryKey: ["admin", "users", "sessions", userId] as const,
    queryFn: async () => {
      const res = await apiClient.admin.users.listSessions({ user_id: userId });
      if (!res.ok) throw new Error(res.error.message || NETWORK_ERROR);
      return res.data;
    },
  });
  const activityQ = useQuery({
    queryKey: ["admin", "users", "activity", userId, actSource, actType] as const,
    queryFn: async () => {
      const res = await apiClient.admin.users.getActivity({
        user_id: userId,
        source: actSource === "all" ? null : actSource,
        event_type: actType || null,
        limit: 100,
      });
      if (!res.ok) throw new Error(res.error.message || NETWORK_ERROR);
      return res.data;
    },
  });

  const sessions: AdminSessionPublic[] = sessionsQ.data?.sessions ?? [];
  const activityItems: AdminUserActivityItem[] = activityQ.data?.items ?? [];
  const eventTypes: string[] = activityQ.data?.event_types ?? [];

  <div
    className="border-t border-border/60 bg-muted/20 px-4 py-3 text-[13px] sm:pl-[4.25rem]"
    data-testid={`admin-user-access-${userId}`}
  >
    <p className="font-medium text-foreground">Organizations</p>
    @if (accessQ.isPending) {
      <p className="mt-1 text-muted-foreground">Loading access…</p>;
    } @else {
      @if (accessQ.isError) {
        <p role="alert" className="mt-1 text-destructive">
          {NETWORK_ERROR as string}
        </p>;
      } @else {
        <>
          @if ((accessQ.data?.orgs.length ?? 0) === 0) {
            <p className="mt-1 text-muted-foreground">None</p>;
          } @else {
            <ul className="mt-1 list-inside list-disc text-muted-foreground">
              @for (const org of accessQ.data!.orgs; key org.slug) {
                <li>
                  <AppLink
                    href={`/${org.slug}`}
                    className="text-primary underline-offset-2 hover:underline"
                  >
                    {org.slug as string}
                  </AppLink>
                  {` · ${org.role} · ${org.display_name}` as string}
                </li>;
              }
            </ul>;
          }
          <p className="mt-3 font-medium text-foreground">Repository grants</p>
          @if ((accessQ.data?.repos.length ?? 0) === 0) {
            <p className="mt-1 text-muted-foreground">None</p>;
          } @else {
            <ul className="mt-1 list-inside list-disc text-muted-foreground">
              @for (const repo of accessQ.data!.repos; key `${repo.owner}/${repo.name}`) {
                <li>
                  <AppLink
                    href={`/${repo.owner}/${repo.name}`}
                    className="text-primary underline-offset-2 hover:underline"
                  >
                    {`${repo.owner}/${repo.name}` as string}
                  </AppLink>
                  {` · ${repo.permission}` as string}
                </li>;
              }
            </ul>;
          }
        </>;
      }
    }

    <p className="mt-4 font-medium text-foreground">Sessions</p>
    @if (sessionsQ.isPending) {
      <p className="mt-1 text-muted-foreground">Loading sessions…</p>;
    } @else {
      @if (sessionsQ.isError) {
        <p role="alert" className="mt-1 text-destructive">
          {NETWORK_ERROR as string}
        </p>;
      } @else {
        @if (sessions.length === 0) {
          <p className="mt-1 text-muted-foreground">No active sessions.</p>;
        } @else {
          <ul
            className="mt-1 space-y-2 text-muted-foreground"
            data-testid={`admin-user-sessions-${userId}`}
          >
            @for (const s of sessions; key s.id) {
              <li className="rounded-md border border-border/60 bg-background px-3 py-2">
                <p className="font-mono text-[12px] text-foreground">
                  {(s.ip_address ?? "unknown IP") as string}
                  {(s.remember_me ? " · remember me" : "") as string}
                </p>
                <p className="mt-0.5 break-all">
                  {sessionAgentLabel(s.user_agent) as string}
                </p>
                <p className="mt-0.5">
                  {
                    `Last seen ${formatRelative(s.last_seen_at)} · created ${formatRelative(s.created_at)} · expires ${formatShortDate(s.expires_at)}` as string
                  }
                </p>
              </li>;
            }
          </ul>;
        }
      }
    }

    <div className="mt-4 flex flex-wrap items-center gap-x-3 gap-y-1">
      <p className="font-medium text-foreground">Activity</p>
      <div className="flex flex-wrap gap-1" role="group" aria-label="Activity source">
        @for (const src of ACTIVITY_SOURCES; key src.value) {
          <button
            type="button"
            data-testid={`admin-user-activity-src-${src.value}-${userId}`}
            aria-pressed={actSource === src.value}
            className={cn(
              "rounded-full border px-2.5 py-0.5 text-[12px] transition-colors",
              actSource === src.value
                ? "border-primary bg-primary/10 text-foreground"
                : "border-border text-muted-foreground hover:text-foreground",
            )}
            onClick={() => {
              setActSource(src.value);
              setActType("");
            }}
          >
            {src.label as string}
          </button>;
        }
      </div>
    </div>
    @if (eventTypes.length > 0) {
      <div
        className="mt-2 flex flex-wrap gap-1"
        role="group"
        aria-label="Event type filter"
        data-testid={`admin-user-activity-types-${userId}`}
      >
        <button
          type="button"
          aria-pressed={actType === ""}
          className={cn(
            "rounded-full border px-2.5 py-0.5 text-[12px] transition-colors",
            actType === ""
              ? "border-primary bg-primary/10 text-foreground"
              : "border-border text-muted-foreground hover:text-foreground",
          )}
          onClick={() => setActType("")}
        >
          All types
        </button>
        @for (const t of eventTypes; key t) {
          <button
            type="button"
            data-testid={`admin-user-activity-type-${t}-${userId}`}
            aria-pressed={actType === t}
            className={cn(
              "rounded-full border px-2.5 py-0.5 font-mono text-[12px] transition-colors",
              actType === t
                ? "border-primary bg-primary/10 text-foreground"
                : "border-border text-muted-foreground hover:text-foreground",
            )}
            onClick={() => setActType(t)}
          >
            {t as string}
          </button>;
        }
      </div>;
    }
    @if (activityQ.isPending) {
      <p className="mt-1 text-muted-foreground">Loading activity…</p>;
    } @else {
      @if (activityQ.isError) {
        <p role="alert" className="mt-1 text-destructive">
          {NETWORK_ERROR as string}
        </p>;
      } @else {
        @if (activityItems.length === 0) {
          <p className="mt-1 text-muted-foreground">No activity recorded.</p>;
        } @else {
          <ul
            className="mt-1 space-y-1.5 text-muted-foreground"
            data-testid={`admin-user-activity-${userId}`}
          >
            @for (const item of activityItems; key `${item.source}-${item.id}`) {
              <li className="rounded-md border border-border/60 bg-background px-3 py-2">
                <p className="flex flex-wrap items-center gap-x-2 text-foreground">
                  <span className="font-mono text-[12px]">
                    {item.event_type as string}
                  </span>
                  <span className="text-[12px] text-muted-foreground">
                    {formatRelative(item.created_at) as string}
                  </span>
                  <Badge variant="outline">{item.source as string}</Badge>
                </p>
                @if (activityItemText(item)) {
                  <p className="mt-0.5 break-all">
                    {activityItemText(item) as string}
                  </p>;
                }
                @if (
                  item.ip_address ||
                  item.user_agent
                ) {
                  <p className="mt-0.5 break-all text-[12px]">
                    {
                      [
                        item.ip_address ?? "",
                        item.user_agent ? sessionAgentLabel(item.user_agent) : "",
                      ]
                        .filter(Boolean)
                        .join(" · ") as string
                    }
                  </p>;
                }
              </li>;
            }
          </ul>;
        }
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
  beforeLoad: () => requireSessionRedirect("/admin/users"),
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
  const navigateApp = useAppNavigate();
  const loaderData = useLoaderData({ from: "/admin/users" }) as AdminUsersLoaderData | undefined;

  const [searchDraft, setSearchDraft] = useState("");
  const [query, setQuery] = useState("");
  const [offset, setOffset] = useState(0);

  const [confirm, setConfirm] = useState<ConfirmKind | null>(null);
  const [confirmPending, setConfirmPending] = useState(false);
  const [confirmError, setConfirmError] = useState("");

  const [deleteTarget, setDeleteTarget] = useState<AdminUserPublic | null>(null);
  const [deletePhrase, setDeletePhrase] = useState("");
  const [deleteOrgs, setDeleteOrgs] = useState(false);
  const [deleteOrgsRequired, setDeleteOrgsRequired] = useState(false);
  const [deletePending, setDeletePending] = useState(false);
  const [deleteError, setDeleteError] = useState("");

  const [inviteOpen, setInviteOpen] = useState(false);
  const [inviteMode, setInviteMode] = useState<"emails" | "link">("emails");
  const [inviteEmails, setInviteEmails] = useState("");
  const [inviteLinkExpiry, setInviteLinkExpiry] = useState("");
  const [inviteLinkMaxUses, setInviteLinkMaxUses] = useState("");
  const [invitePending, setInvitePending] = useState(false);
  const [inviteError, setInviteError] = useState("");
  const [inviteResults, setInviteResults] = useState<AdminInvitesCreateItemResult[]>([]);
  const [createdInviteUrl, setCreatedInviteUrl] = useState("");
  const [copyState, setCopyState] = useState<"idle" | "copied" | "error">("idle");
  const [copiedResultUrl, setCopiedResultUrl] = useState("");

  const [accessUserId, setAccessUserId] = useState<string | null>(null);

  useEffect(() => {
    if (loaderData?.kind === "unauthenticated") {
      void navigateApp("/login?returnTo=/admin/users");
      return;
    }
    if (loaderData?.kind === "ready") {
      setAuthMeCache(queryClient, loaderData.me);
      queryClient.setQueryData(usersQueryKey("", 0), loaderData.users);
      queryClient.setQueryData(invitesQueryKey, loaderData.invites);
    }
  }, [loaderData, queryClient, navigateApp]);

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
      void navigateApp("/login?returnTo=/admin/users");
    }
  }, [loaderData, me.isPending, me.isError, me.data, navigateApp]);

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
        toastSuccess(`Revoked invite for ${action.invite.email ?? "link"}.`);
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
        delete_orgs: deleteOrgs,
      });
      if (!res.ok) {
        if (res.error.code === "admin.delete_orgs_confirm") {
          setDeleteOrgsRequired(true);
        }
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
      setDeleteOrgs(false);
      setDeleteOrgsRequired(false);
      await refreshUsers();
    } catch {
      setDeleteError(NETWORK_ERROR);
      setDeletePending(false);
    }
  }

  async function submitInvite() {
    if (inviteMode === "link") {
      await createLinkInvite();
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
    setInviteError("");
    setInvitePending(true);
    setCreatedInviteUrl("");
    setCopyState("idle");
    setCopiedResultUrl("");
    try {
      const res = await apiClient.admin.invites.create({ emails });
      if (!res.ok) {
        setInviteError(res.error.message || NETWORK_ERROR);
        setInvitePending(false);
        return;
      }
      setInviteResults(res.data.results);
      setInviteEmails("");
      setInvitePending(false);
      const ok = res.data.results.filter((r) => r.ok).length;
      const failed = res.data.results.length - ok;
      if (failed > 0) {
        toastWarning(
          `Created ${ok} invite${ok === 1 ? "" : "s"}; ${failed} recipient${failed === 1 ? "" : "s"} failed.`,
        );
      } else {
        toastSuccess(`Created ${ok} invite${ok === 1 ? "" : "s"}.`);
      }
      await refreshInvites();
    } catch {
      setInviteError(NETWORK_ERROR);
      setInvitePending(false);
    }
  }

  async function createLinkInvite() {
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
    setInviteError("");
    setInvitePending(true);
    setCreatedInviteUrl("");
    setCopyState("idle");
    setInviteResults([]);
    setCopiedResultUrl("");
    try {
      const res = await apiClient.admin.invites.createLink({
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
      toastSuccess("Invite link created.");
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

  function closeInviteDialog() {
    setInviteOpen(false);
    setInviteMode("emails");
    setInviteEmails("");
    setInviteLinkExpiry("");
    setInviteLinkMaxUses("");
    setInviteError("");
    setInviteResults([]);
    setCreatedInviteUrl("");
    setCopyState("idle");
    setCopiedResultUrl("");
  }

  async function toggleAccess(user: AdminUserPublic) {
    if (accessUserId === user.id) {
      setAccessUserId(null);
      return;
    }
    setAccessUserId(user.id);
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
              ? `Revoke ${confirm.invite.email ? `the invitation for ${confirm.invite.email}` : "this invite link"}. It will no longer be usable.`
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
    <AdminSettingsLayout active="users">
      <SettingsPageSkeleton />
    </AdminSettingsLayout>;
  } @else {
    @if (signedOut) {
      <SettingsShell bare>
        <div className="text-center">
          <p className="text-[16px] text-muted-foreground">
            Sign in as a system administrator to manage users.
          </p>
          <AppLink
            href="/login?returnTo=/admin/users"
            className="mt-4 inline-block text-primary underline-offset-2 hover:underline"
          >
            Sign in
          </AppLink>
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
            <PageHeader
              title="Users"
              description="Search accounts, change roles, revoke sessions, ban or delete users, and invite people when public sign-up is closed."
              actions={
                <Button
                  type="button"
                  data-testid="admin-invite-open"
                  onClick={() => {
                    setInviteError("");
                    setCreatedInviteUrl("");
                    setCopyState("idle");
                    setInviteOpen(true);
                  }}
                >
                  <UserPlus aria-hidden size={16} />
                  Invite people
                </Button>
              }
            />

            <section className="mt-6" aria-labelledby="admin-users-table-heading">
              <h2
                id="admin-users-table-heading"
                className="font-[family-name:var(--font-display)] text-[18px] font-semibold"
              >
                Accounts
              </h2>

              @if (usersQ.isError && !usersPage) {
                <p role="alert" className="mt-4 text-[14px] text-destructive">
                  {
                    (usersQ.error instanceof Error
                      ? usersQ.error.message
                      : FORBIDDEN) as string
                  }
                </p>;
              } @else {
                <>
                  <List className="mt-4">
                    <ListHeader>
                      <span>
                        {
                          (total === 0
                            ? "0 accounts"
                            : `${total} ${total === 1 ? "account" : "accounts"}`) as string
                        }
                      </span>
                      <form
                        method="post"
                        action="#"
                        className="ml-auto flex min-w-0 flex-1 items-center justify-end gap-2 font-normal sm:flex-none"
                        onSubmit={(e) => {
                          e.preventDefault();
                          setOffset(0);
                          setQuery(searchDraft.trim());
                        }}
                      >
                        <Label htmlFor="admin-users-search" className="sr-only">
                          Search
                        </Label>
                        <Input
                          id="admin-users-search"
                          data-testid="admin-users-search"
                          value={searchDraft as string}
                          onInput={(e) =>
                            setSearchDraft((e.currentTarget as HTMLInputElement).value)
                          }
                          placeholder="Username, display name, or email"
                          autoComplete="off"
                          className="h-9 w-full min-w-0 sm:w-64"
                        />
                        <Button type="submit" variant="outline" className="h-9 shrink-0 px-3">
                          Search
                        </Button>
                      </form>
                    </ListHeader>

                    @for (const user of users; key user.id) {
                      <div
                        className="border-t border-border first:border-t-0"
                        data-testid={`admin-user-row-${user.id}`}
                      >
                        <div className="flex items-center gap-3 px-4 py-3 transition-colors hover:bg-muted/40">
                          <AvatarPreview
                            user={{
                              avatar_url: null,
                              display_name: user.display_name,
                              username: user.username,
                            }}
                            size={36}
                          />
                          <div className="min-w-0 flex-1">
                            <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
                              <span className="font-medium text-foreground">
                                {("@" + user.username) as string}
                              </span>
                              @if (
                                user.display_name &&
                                user.display_name !== user.username
                              ) {
                                <span className="truncate text-[13px] text-muted-foreground">
                                  {user.display_name as string}
                                </span>;
                              }
                              @if (
                                user.role === "sys-admin"
                              ) {
                                <Badge variant="primary">System admin</Badge>;
                              } @else {
                                @if (user.role === "admin") {
                                  <Badge variant="secondary">Admin</Badge>;
                                }
                              }
                              @if (isBanned(user)) {
                                <Badge variant="destructive">Banned</Badge>;
                              }
                              @if (
                                !user.email_verified
                              ) {
                                <Badge variant="outline">Unverified</Badge>;
                              }
                              @if (user.id === selfId) {
                                <Badge variant="outline">You</Badge>;
                              }
                            </div>
                            <p className="mt-0.5 truncate text-[13px] text-muted-foreground">
                              {
                                `${user.email} · Joined ${formatRelative(user.created_at)}` as string
                              }
                            </p>
                          </div>
                          {/* MenuPrimitive.Portal keepMounted is inside DropdownMenuContent. */}
                          <DropdownMenu>
                            <DropdownMenuTrigger
                              type="button"
                              aria-label={`Actions for @${user.username}`}
                              data-testid={`admin-user-menu-${user.id}`}
                              className={cn(
                                buttonVariants({ variant: "ghost", size: "icon-sm" }),
                                "shrink-0",
                              )}
                            >
                              <EllipsisVertical aria-hidden size={16} />
                            </DropdownMenuTrigger>
                            <DropdownMenuContent align="end" className="min-w-48">
                              <DropdownMenuItem
                                data-testid={`admin-user-access-toggle-${user.id}`}
                                onClick={() => void toggleAccess(user)}
                              >
                                <Eye
                                  aria-hidden
                                  size={15}
                                  className="text-muted-foreground"
                                />
                                {
                                  (accessUserId === user.id
                                    ? "Hide details"
                                    : "View details") as string
                                }
                              </DropdownMenuItem>
                              <DropdownMenuSeparator />
                              @if (
                                user.role === "sys-admin"
                              ) {
                                <DropdownMenuItem
                                  disabled={user.id === selfId}
                                  onClick={() =>
                                    openConfirm({
                                      kind: "role",
                                      user,
                                      nextRole: "user",
                                    })
                                  }
                                >
                                  <ShieldMinus
                                    aria-hidden
                                    size={15}
                                    className="text-muted-foreground"
                                  />
                                  Demote to user
                                </DropdownMenuItem>;
                              } @else {
                                <DropdownMenuItem
                                  data-testid={`admin-user-make-sysadmin-${user.id}`}
                                  onClick={() =>
                                    openConfirm({
                                      kind: "role",
                                      user,
                                      nextRole: "sys-admin",
                                    })
                                  }
                                >
                                  <ShieldPlus
                                    aria-hidden
                                    size={15}
                                    className="text-muted-foreground"
                                  />
                                  Make sys-admin
                                </DropdownMenuItem>;
                              }
                              <DropdownMenuItem
                                data-testid={`admin-user-revoke-sessions-${user.id}`}
                                onClick={() =>
                                  openConfirm({ kind: "revoke-sessions", user })
                                }
                              >
                                <LogOut
                                  aria-hidden
                                  size={15}
                                  className="text-muted-foreground"
                                />
                                Revoke sessions
                              </DropdownMenuItem>
                              @if (isBanned(user)) {
                                <DropdownMenuItem
                                  onClick={() =>
                                    openConfirm({ kind: "unban", user })
                                  }
                                >
                                  <RotateCcw
                                    aria-hidden
                                    size={15}
                                    className="text-muted-foreground"
                                  />
                                  Unban
                                </DropdownMenuItem>;
                              } @else {
                                <DropdownMenuItem
                                  data-testid={`admin-user-ban-${user.id}`}
                                  disabled={
                                    user.id === selfId ||
                                    user.role === "sys-admin"
                                  }
                                  onClick={() =>
                                    openConfirm({ kind: "ban", user })
                                  }
                                >
                                  <Ban
                                    aria-hidden
                                    size={15}
                                    className="text-muted-foreground"
                                  />
                                  Ban
                                </DropdownMenuItem>;
                              }
                              <DropdownMenuSeparator />
                              <DropdownMenuItem
                                className="text-destructive data-[highlighted]:bg-destructive/10 data-[highlighted]:text-destructive"
                                data-testid={`admin-user-delete-${user.id}`}
                                disabled={user.id === selfId}
                                onClick={() => {
                                  setDeleteError("");
                                  setDeletePhrase("");
                                  setDeleteOrgs(false);
                                  setDeleteOrgsRequired(false);
                                  setDeleteTarget(user);
                                }}
                              >
                                <Trash2 aria-hidden size={15} />
                                Delete
                              </DropdownMenuItem>
                            </DropdownMenuContent>
                          </DropdownMenu>
                        </div>
                        @if (accessUserId === user.id) {
                          <AdminUserDetailPanel userId={user.id} />;
                        }
                      </div>;
                    } @empty {
                      <EmptyState
                        compact
                        className="rounded-none border-0 border-t bg-transparent"
                        icon={<Search aria-hidden size={20} />}
                        title={
                          (query
                            ? "No users match this search"
                            : "No accounts yet") as string
                        }
                        description={
                          (query
                            ? `Nothing found for “${query}”. Check the spelling or clear the search.`
                            : "Invite people to create the first account.") as string
                        }
                        action={
                          query ? (
                            <Button
                              type="button"
                              variant="outline"
                              size="sm"
                              onClick={() => {
                                setSearchDraft("");
                                setOffset(0);
                                setQuery("");
                              }}
                            >
                              Clear search
                            </Button>
                          ) : undefined
                        }
                      />;
                    }
                  </List>
                  <div className="mt-3 flex flex-wrap items-center justify-between gap-3">
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
                        variant="outline"
                        size="sm"
                        disabled={!canPrev}
                        onClick={() => setOffset(Math.max(0, offset - PAGE_SIZE))}
                      >
                        Previous
                      </Button>
                      <Button
                        type="button"
                        variant="outline"
                        size="sm"
                        disabled={!canNext}
                        onClick={() => setOffset(offset + PAGE_SIZE)}
                      >
                        Next
                      </Button>
                    </div>
                  </div>
                </>;
              }
            </section>

            <section className="mt-10" aria-labelledby="admin-invites-heading">
              <h2
                id="admin-invites-heading"
                className="font-[family-name:var(--font-display)] text-[18px] font-semibold"
              >
                Pending invitations
              </h2>
              <p className="mt-1 text-[14px] text-muted-foreground">
                Invite links work even when public sign-up is closed. Create one with “Invite
                people”.
              </p>
              @if (invitesQ.isError) {
                <p role="alert" className="mt-3 text-[14px] text-destructive">
                  {
                    (invitesQ.error instanceof Error
                      ? invitesQ.error.message
                      : "Could not load invites.") as string
                  }
                </p>;
              } @else {
                @if (invites.length === 0) {
                  <p className="mt-3 text-[14px] text-muted-foreground">
                    No pending invitations.
                  </p>;
                } @else {
                  <List className="mt-3">
                    @for (const inv of invites; key inv.id) {
                      <ListRow>
                        <div className="min-w-0 flex-1">
                          <p className="flex items-center gap-1.5 truncate text-[14px] font-medium text-foreground">
                            @if (inv.email == null) {
                              <Link2
                                aria-hidden
                                size={14}
                                className="text-muted-foreground"
                              />;
                            }
                            {inviteListLabel(inv) as string}
                          </p>
                          <p className="text-[13px] text-muted-foreground">
                            {
                              (inviteExpiryLabel(inv) +
                                " · " +
                                inviteSeatsLabel(inv)) as string
                            }
                          </p>
                        </div>
                        <Button
                          type="button"
                          variant="outline"
                          size="sm"
                          onClick={() =>
                            openConfirm({ kind: "revoke-invite", invite: inv })
                          }
                        >
                          Revoke
                        </Button>
                      </ListRow>;
                    }
                  </List>;
                }
              }
            </section>

            {/* AlertDialogPortal keepMounted is inside AlertDialogContent (browser-coverage). */}
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
              <AlertDialogContent data-testid="admin-users-confirm-dialog">
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

            {/* DialogPortal keepMounted is inside DialogContent (browser-coverage). */}
            <Dialog
              open={deleteTarget != null}
              onOpenChange={(next) => {
                if (deletePending) return;
                if (!next) {
                  setDeleteTarget(null);
                  setDeletePhrase("");
                  setDeleteOrgs(false);
                  setDeleteOrgsRequired(false);
                  setDeleteError("");
                }
              }}
            >
              <DialogContent
                showCloseButton={!deletePending}
                data-testid="admin-users-delete-dialog"
              >
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
                @if (deleteOrgsRequired) {
                  <div className="flex min-h-11 items-center gap-3">
                    <Checkbox
                      id="admin-delete-orgs"
                      data-testid="admin-delete-orgs"
                      checked={deleteOrgs}
                      disabled={deletePending}
                      onCheckedChange={(checked) => setDeleteOrgs(checked === true)}
                    />
                    <Label htmlFor="admin-delete-orgs">
                      Also delete the listed organizations and their repositories
                    </Label>
                  </div>;
                }
                <DialogFooter>
                  <Button
                    type="button"
                    variant="secondary"
                    disabled={deletePending}
                    onClick={() => {
                      setDeleteTarget(null);
                      setDeletePhrase("");
                      setDeleteOrgs(false);
                      setDeleteOrgsRequired(false);
                      setDeleteError("");
                    }}
                  >
                    Cancel
                  </Button>
                  <Button
                    type="button"
                    variant="destructive"
                    data-testid="admin-users-delete-submit"
                    disabled={
                      deletePending ||
                      !deleteTarget ||
                      deletePhrase.trim() !== deleteTarget.username ||
                      (deleteOrgsRequired && !deleteOrgs)
                    }
                    onClick={() => void runDelete()}
                  >
                    {(deletePending ? "Deleting…" : "Delete user") as string}
                  </Button>
                </DialogFooter>
              </DialogContent>
            </Dialog>

            {/* DialogPortal keepMounted is inside DialogContent (browser-coverage). */}
            <Dialog
              open={inviteOpen}
              onOpenChange={(next) => {
                if (invitePending) return;
                if (next) {
                  setInviteOpen(true);
                } else {
                  closeInviteDialog();
                }
              }}
            >
              <DialogContent data-testid="admin-invite-dialog">
                <DialogHeader>
                  <DialogTitle>Invite people</DialogTitle>
                  <DialogDescription>
                    Send instance invites by email, or share a reusable link. Invites work even when
                    public sign-up is closed.
                  </DialogDescription>
                </DialogHeader>
                @if (createdInviteUrl || inviteResults.length > 0) {
                  <div>
                    @if (createdInviteUrl) {
                      <div
                        className="rounded-md border border-border p-4"
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
                            className="shrink-0"
                            data-testid="admin-invite-url-copy"
                            aria-label="Copy invite URL"
                            onClick={() => void copyInviteUrl()}
                          >
                            {
                              (copyState === "copied"
                                ? "Copied"
                                : "Copy link") as string
                            }
                          </Button>
                        </div>
                        @if (copyState === "error") {
                          <p
                            role="alert"
                            className="mt-2 text-[14px] text-destructive"
                          >
                            Couldn’t copy. Select the link and copy manually.
                          </p>;
                        }
                      </div>;
                    } @else {
                      <div
                        className="max-h-64 overflow-y-auto rounded-md border border-border"
                        data-testid="admin-invite-results"
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
                                    data-testid={`admin-invite-result-copy-${r.email}`}
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
                    <DialogFooter className="mt-4">
                      <Button
                        type="button"
                        variant="secondary"
                        onClick={() => closeInviteDialog()}
                      >
                        Done
                      </Button>
                    </DialogFooter>
                  </div>;
                } @else {
                  <form
                    method="post"
                    action="#"
                    className="flex flex-col gap-4"
                    onSubmit={(e) => {
                      e.preventDefault();
                      void submitInvite();
                    }}
                  >
                    <div className="flex gap-2" role="group" aria-label="Invite mode">
                      <button
                        type="button"
                        data-testid="admin-invite-mode-emails"
                        aria-pressed={inviteMode === "emails"}
                        className={cn(
                          "rounded-full border px-3 py-1 text-[13px] transition-colors",
                          inviteMode === "emails"
                            ? "border-primary bg-primary/10 text-foreground"
                            : "border-border text-muted-foreground hover:text-foreground",
                        )}
                        onClick={() => {
                          setInviteMode("emails");
                          setInviteError("");
                        }}
                      >
                        By email
                      </button>
                      <button
                        type="button"
                        data-testid="admin-invite-mode-link"
                        aria-pressed={inviteMode === "link"}
                        className={cn(
                          "rounded-full border px-3 py-1 text-[13px] transition-colors",
                          inviteMode === "link"
                            ? "border-primary bg-primary/10 text-foreground"
                            : "border-border text-muted-foreground hover:text-foreground",
                        )}
                        onClick={() => {
                          setInviteMode("link");
                          setInviteError("");
                        }}
                      >
                        Invite link
                      </button>
                    </div>
                    @if (inviteError) {
                      <p role="alert" className="text-[14px] text-destructive">
                        {inviteError as string}
                      </p>;
                    }
                    @if (inviteMode === "emails") {
                      <div className="flex flex-col gap-2">
                        <Label htmlFor="admin-invite-email">Emails</Label>
                        <Textarea
                          id="admin-invite-email"
                          data-testid="admin-invite-email"
                          placeholder={"one@example.com\ntwo@example.com" as string}
                          value={inviteEmails as string}
                          onInput={(e) =>
                            setInviteEmails(
                              (e.currentTarget as HTMLTextAreaElement).value,
                            )
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
                          <Label htmlFor="admin-invite-link-expiry">
                            Expires on (optional)
                          </Label>
                          <Input
                            id="admin-invite-link-expiry"
                            data-testid="admin-invite-link-expiry"
                            type="date"
                            value={inviteLinkExpiry as string}
                            onInput={(e) =>
                              setInviteLinkExpiry(
                                (e.currentTarget as HTMLInputElement).value,
                              )
                            }
                            disabled={invitePending}
                          />
                          <p className="text-[13px] text-muted-foreground">
                            Leave empty for a link that never expires.
                          </p>
                        </div>
                        <div className="flex flex-col gap-2">
                          <Label htmlFor="admin-invite-link-seats">
                            Max seats (optional)
                          </Label>
                          <Input
                            id="admin-invite-link-seats"
                            data-testid="admin-invite-link-seats"
                            type="number"
                            min={1}
                            step={1}
                            placeholder="Unlimited"
                            value={inviteLinkMaxUses as string}
                            onInput={(e) =>
                              setInviteLinkMaxUses(
                                (e.currentTarget as HTMLInputElement).value,
                              )
                            }
                            disabled={invitePending}
                          />
                          <p className="text-[13px] text-muted-foreground">
                            Number of accounts that can join with this link.
                          </p>
                        </div>
                      </>;
                    }
                    <DialogFooter>
                      <Button
                        type="submit"
                        disabled={invitePending}
                        data-testid="admin-invite-create"
                      >
                        {
                          (invitePending
                            ? "Creating…"
                            : inviteMode === "link"
                              ? "Create link"
                              : "Create invites") as string
                        }
                      </Button>
                    </DialogFooter>
                  </form>;
                }
              </DialogContent>
            </Dialog>
          </AdminSettingsLayout>;
        }
      }
    }
  }
}
