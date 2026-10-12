import { useQuery, useQueryClient } from "@octanejs/tanstack-query";
import type { NotificationPublic } from "@oxidean/api-client";
import { useState } from "octane";
import { BellRing, CheckCheck } from "@octanejs/lucide";
import { AppLink } from "@/components/ui/app-link";
import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/empty-state";
import { List, ListRow } from "@/components/ui/list";
import { PageHeader } from "@/components/ui/page-header";
import { Skeleton } from "@/components/ui/skeleton";
import { apiClient } from "@/lib/api-client";
import { formatRelative } from "@/lib/relative-time";
import {
  invalidateNotificationQueries,
  notificationListQueryOptions,
  notificationUnreadCountQueryOptions,
  reasonLabel,
  subjectHref,
  subjectTail,
} from "@/lib/notification-queries";
import { AppPage } from "@/components/app-page";
import { usePageRedirect } from "@/lib/nav.tsrx";
import { authSessionQueryOptions } from "@/lib/session-queries";
import { cn } from "@/lib/utils";

/** `/notifications` — signed-in only; anonymous hits bounce to /login. */
export function NotificationsPage() @{
  <AppPage>
    <NotificationsContent />
  </AppPage>;
}

function NotificationsContent() @{
  const meQ = useQuery(authSessionQueryOptions());
  usePageRedirect(meQ.data === null || meQ.isError ? "/login?returnTo=/notifications" : null);
  const qc = useQueryClient();
  const [filter, setFilter] = useState<"unread" | "all">("unread");
  const [offset, setOffset] = useState(0);
  const listQ = useQuery(notificationListQueryOptions(filter, offset));
  const unreadQ = useQuery(notificationUnreadCountQueryOptions());
  const rows: NotificationPublic[] = listQ.data?.notifications ?? [];
  const total = listQ.data?.total ?? 0;
  const unreadCountRaw = Number(unreadQ.data ?? 0);
  const unreadCount = Number.isFinite(unreadCountRaw) ? unreadCountRaw : 0;
  const limit = 30;

  async function markAll() {
    const res = await apiClient.notification.markAllRead({});
    if (res.ok) {
      invalidateNotificationQueries(qc);
    }
  }

  function markReadOnActivate(n: NotificationPublic) {
    // Fire-and-forget — the anchor navigates (ClientRouter morph) while the
    // RPC is in flight; module JS stays alive across morphs so it completes.
    if (n.read_at) return;
    void apiClient.notification
      .markRead({ ids: [n.id] })
      .then(() => invalidateNotificationQueries(qc));
  }

  <div className="mx-auto max-w-3xl px-4 py-10">
    <PageHeader
      icon={<BellRing aria-hidden size={22} />}
      title="Notifications"
      description={
        unreadCount > 0
          ? ((String(unreadCount) + " unread") as string)
          : ("You're all caught up" as string)
      }
      actions={
        unreadCount > 0 ? (
          <Button type="button" variant="outline" onClick={() => void markAll()}>
            <CheckCheck aria-hidden size={15} />
            Mark all as read
          </Button>
        ) : undefined
      }
    />

    <div
      role="tablist"
      aria-label="Notification filters"
      className="mt-6 inline-flex rounded-md border border-border p-1"
    >
      <button
        type="button"
        role="tab"
        aria-selected={filter === "unread"}
        className={cn(
          "rounded px-3 py-1.5 text-[14px] font-medium outline-none focus-visible:ring-2 focus-visible:ring-ring",
          filter === "unread" ? "bg-muted text-foreground" : "text-muted-foreground",
        )}
        onClick={() => {
          setFilter("unread");
          setOffset(0);
        }}
      >
        Unread
      </button>
      <button
        type="button"
        role="tab"
        aria-selected={filter === "all"}
        className={cn(
          "rounded px-3 py-1.5 text-[14px] font-medium outline-none focus-visible:ring-2 focus-visible:ring-ring",
          filter === "all" ? "bg-muted text-foreground" : "text-muted-foreground",
        )}
        onClick={() => {
          setFilter("all");
          setOffset(0);
        }}
      >
        All
      </button>
    </div>

    <div className="mt-6">
      @if (listQ.isPending) {
        <List>
          @for (const i of [0, 1, 2, 3, 4]; key i) {
            <ListRow className="items-start gap-3">
              <Skeleton className="mt-1 size-2 rounded-full" />
              <div className="flex flex-1 flex-col gap-2">
                <Skeleton className="h-4 w-2/3" />
                <Skeleton className="h-3 w-1/3" />
              </div>
            </ListRow>;
          }
        </List>;
      } @else {
        @if (rows.length === 0) {
          <EmptyState
            icon={<BellRing aria-hidden size={22} />}
            title={filter === "unread" ? "You're all caught up" : "No notifications yet"}
            description={
              filter === "unread"
                ? "New activity on repositories, issues, and pull requests you follow will show up here."
                : "When you're mentioned or something you follow changes, it will appear here."
            }
          />;
        } @else {
          <>
            <List>
              @for (const n of rows; key n.id) {
                <ListRow className="p-0">
                  <AppLink
                    href={subjectHref(n)}
                    className={cn(
                      "flex w-full items-start gap-3 px-4 py-3 text-left text-current no-underline outline-none",
                      "transition-colors hover:bg-muted/40 focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring",
                    )}
                    aria-label={`${n.actor_username} ${reasonLabel(n.reason)} ${n.owner}/${n.repo} ${subjectTail(n)}`}
                    onClick={() => markReadOnActivate(n)}
                  >
                    <span
                      aria-hidden
                      className={cn(
                        "mt-1.5 size-2 shrink-0 rounded-full",
                        n.read_at ? "bg-transparent" : "bg-primary",
                      )}
                    />
                    <span className="flex min-w-0 flex-1 flex-col gap-0.5">
                      <span
                        className={cn(
                          "text-[14px]",
                          n.read_at
                            ? "text-muted-foreground"
                            : "font-medium text-foreground",
                        )}
                      >
                        <span className="font-semibold text-foreground">
                          {n.actor_username as string}
                        </span>{" "}
                        {reasonLabel(n.reason) as string}{" "}
                        <span className="text-foreground">
                          {n.owner as string}/{n.repo as string}
                          {" " + subjectTail(n)}
                        </span>
                      </span>
                      <span className="truncate text-[13px] text-muted-foreground">
                        {n.subject_title as string}
                        {" · "}
                        {formatRelative(n.created_at) as string}
                      </span>
                    </span>
                  </AppLink>
                </ListRow>;
              }
            </List>
            @if (offset + rows.length < total) {
              <div className="mt-4 flex justify-center">
                <Button
                  type="button"
                  variant="outline"
                  onClick={() => setOffset(offset + limit)}
                >
                  Load older
                </Button>
              </div>;
            }
          </>;
        }
      }
    </div>
  </div>;
}
