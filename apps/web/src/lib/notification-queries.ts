import { queryOptions, type QueryClient } from "@octanejs/tanstack-query";
import type {
  NotificationListResponse,
  NotificationPublic,
  NotificationUnreadCountResponse,
} from "@oxidean/api-client";
import { apiClient } from "@/lib/api-client";

const notificationUnreadCountQueryKey = ["notification", "unreadCount"] as const;
function notificationListQueryKey(filter: "unread" | "all", offset = 0) {
  return ["notification", "list", filter, offset] as const;
}
/** Soft unread badge — unauthenticated → 0 (chrome must not throw). */
export function notificationUnreadCountQueryOptions() {
  return queryOptions({
    queryKey: notificationUnreadCountQueryKey,
    queryFn: async (): Promise<number> => {
      const res = await apiClient.notification.unreadCount({});
      if (!res.ok) {
        if (res.error.code === "auth.unauthenticated" || res.error.code === "auth.setup_required") {
          return 0;
        }
        throw new Error(`${res.error.code}: ${res.error.message}`);
      }
      return (res.data as NotificationUnreadCountResponse).count;
    },
    retry: false,
    staleTime: 15_000,
    refetchInterval: 30_000,
  });
}

export function notificationListQueryOptions(filter: "unread" | "all", offset = 0, limit = 30) {
  return queryOptions({
    queryKey: notificationListQueryKey(filter, offset),
    queryFn: async (): Promise<NotificationListResponse> => {
      const res = await apiClient.notification.list({ filter, offset, limit });
      if (!res.ok) {
        throw new Error(`${res.error.code}: ${res.error.message}`);
      }
      return res.data;
    },
    retry: false,
    staleTime: 10_000,
  });
}

export function invalidateNotificationQueries(qc: QueryClient) {
  void qc.invalidateQueries({ queryKey: ["notification"] });
}

export function subjectHref(n: NotificationPublic): string {
  switch (n.subject_kind) {
    case "pull_request":
      return `/${n.owner}/${n.repo}/pull/${n.subject_number}`;
    case "release":
      return n.subject_ref
        ? `/${n.owner}/${n.repo}/releases/${n.subject_ref}`
        : `/${n.owner}/${n.repo}/releases`;
    case "workflow_run":
      return n.subject_ref
        ? `/${n.owner}/${n.repo}/actions/${n.subject_ref}`
        : `/${n.owner}/${n.repo}/actions`;
    case "push":
      return n.subject_ref
        ? `/${n.owner}/${n.repo}/commits/${n.subject_ref}`
        : `/${n.owner}/${n.repo}`;
    default:
      return `/${n.owner}/${n.repo}/issues/${n.subject_number}`;
  }
}

/** Display suffix after `owner/repo` — `#42` for numbered subjects, the
 * release tag / ref for the rest (DEBT-06). */
export function subjectTail(n: NotificationPublic): string {
  if (n.subject_kind === "issue" || n.subject_kind === "pull_request") {
    return `#${n.subject_number}`;
  }
  // Workflow-run refs carry the run UUID purely for deep links — never display it.
  if (n.subject_kind === "workflow_run") return "";
  return n.subject_ref ?? "";
}

export function reasonLabel(reason: string): string {
  switch (reason) {
    case "issue_opened":
      return "opened an issue";
    case "issue_closed":
      return "closed an issue";
    case "issue_reopened":
      return "reopened an issue";
    case "issue_comment":
      return "commented on";
    case "issue_assigned":
      return "assigned you";
    case "issue_unassigned":
      return "unassigned you";
    case "issue_mention":
      return "mentioned you";
    case "pr_opened":
      return "opened a pull request";
    case "pr_closed":
      return "closed a pull request";
    case "pr_reopened":
      return "reopened a pull request";
    case "pr_merged":
      return "merged a pull request";
    case "pr_review":
      return "reviewed";
    case "pr_comment":
      return "commented on";
    case "pr_review_requested":
      return "requested your review";
    case "pr_mention":
      return "mentioned you";
    case "release_published":
      return "published a release on";
    case "release_edited":
      return "edited a release on";
    case "release_deleted":
      return "deleted a release on";
    case "workflow_run_success":
      return "workflow run succeeded on";
    case "workflow_run_failure":
      return "workflow run failed on";
    case "workflow_run_cancelled":
      return "workflow run cancelled on";
    default:
      return reason.replace(/_/g, " ");
  }
}
