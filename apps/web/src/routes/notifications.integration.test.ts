/**
 * Phase 17 — /notifications inbox (D-09 / D-12 / NOTF-02).
 */
import { cleanup, screen, waitFor } from "@octanejs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderWithQueryClient } from "@/test/render-with-query";

const navigateMock = vi.fn();

vi.mock("@octanejs/tanstack-router", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@octanejs/tanstack-router")>();
  return {
    ...actual,
    useNavigate: () => navigateMock,
    createFileRoute: () => (opts: { component?: unknown }) => opts,
  };
});

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    notification: {
      list: vi.fn(),
      unreadCount: vi.fn(),
      markRead: vi.fn(),
      markAllRead: vi.fn(),
    },
  },
}));

vi.mock("@/lib/ssr-auth", () => ({
  fetchSessionMe: vi.fn(async () => ({
    ok: true,
    data: { id: "u1", username: "ada", email_verified: true },
  })),
}));

import { apiClient } from "@/lib/api-client";
import { NotificationsPage } from "./notifications";

afterEach(cleanup);

beforeEach(() => {
  navigateMock.mockReset();
  vi.mocked(apiClient.notification.unreadCount).mockResolvedValue({
    ok: true,
    data: { count: 1 },
  } as never);
  vi.mocked(apiClient.notification.list).mockResolvedValue({
    ok: true,
    data: {
      total: 1,
      notifications: [
        {
          id: "n1",
          reason: "issue_comment",
          subject_kind: "issue",
          subject_repo_id: "r1",
          owner: "ada",
          repo: "hello",
          subject_number: 1,
          subject_title: "Talk",
          actor_id: "u2",
          actor_username: "bob",
          created_at: "2026-09-16T00:00:00Z",
          read_at: null,
        },
      ],
    },
  } as never);
  vi.mocked(apiClient.notification.markRead).mockResolvedValue({
    ok: true,
    data: { marked: 1 },
  } as never);
  vi.mocked(apiClient.notification.markAllRead).mockResolvedValue({
    ok: true,
    data: { marked: 1 },
  } as never);
});

describe("/notifications page (D-09 / D-12 / NOTF-02)", () => {
  it("renders Unread|All filters and Mark all as read", async () => {
    renderWithQueryClient(NotificationsPage);

    await waitFor(() => {
      expect(screen.getByRole("heading", { name: /notifications/i })).toBeInTheDocument();
    });
    expect(screen.getByRole("tab", { name: /^Unread$/ })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: /^All$/ })).toBeInTheDocument();
    await waitFor(() => {
      expect(screen.getByRole("button", { name: /Mark all as read/i })).toBeInTheDocument();
    });
    await waitFor(() => {
      expect(
        screen.getByRole("button", { name: /bob commented on ada\/hello #1/i }),
      ).toBeInTheDocument();
    });
  });

  it("activating a row marks read and navigates to subject", async () => {
    const assign = vi.fn();
    vi.stubGlobal("location", { assign });

    renderWithQueryClient(NotificationsPage);

    const row = await waitFor(() =>
      screen.getByRole("button", { name: /bob commented on ada\/hello #1/i }),
    );
    row.click();

    await waitFor(() => {
      expect(apiClient.notification.markRead).toHaveBeenCalledWith({ ids: ["n1"] });
    });
    expect(assign).toHaveBeenCalledWith("/ada/hello/issues/1");
  });

  it("renders release and workflow_run subjects with ref deep links (DEBT-06)", async () => {
    vi.mocked(apiClient.notification.list).mockResolvedValue({
      ok: true,
      data: {
        total: 2,
        notifications: [
          {
            id: "n2",
            reason: "release_published",
            subject_kind: "release",
            subject_repo_id: "r1",
            owner: "ada",
            repo: "hello",
            subject_number: 0,
            subject_title: "Ship it",
            subject_ref: "v1.0.0",
            actor_id: "u2",
            actor_username: "bob",
            created_at: "2026-09-17T00:00:00Z",
            read_at: null,
          },
          {
            id: "n3",
            reason: "workflow_run_failure",
            subject_kind: "workflow_run",
            subject_repo_id: "r1",
            owner: "ada",
            repo: "hello",
            subject_number: 0,
            subject_title: "CI",
            subject_ref: "run-42",
            actor_id: "u2",
            actor_username: "bob",
            created_at: "2026-09-18T00:00:00Z",
            read_at: null,
          },
        ],
      },
    } as never);

    renderWithQueryClient(NotificationsPage);

    const release = await waitFor(() =>
      screen.getByRole("button", { name: /bob published a release on ada\/hello v1\.0\.0/i }),
    );
    expect(
      screen.getByRole("button", { name: /bob workflow run failed on ada\/hello/i }),
    ).toBeInTheDocument();

    const assign = vi.fn();
    vi.stubGlobal("location", { assign });
    release.click();
    await waitFor(() => {
      expect(assign).toHaveBeenCalledWith("/ada/hello/releases/v1.0.0");
    });
  });

  it("falls back to the index route when subject_ref is null (DEBT-06 review)", async () => {
    vi.mocked(apiClient.notification.list).mockResolvedValue({
      ok: true,
      data: {
        total: 1,
        notifications: [
          {
            id: "n4",
            reason: "release_deleted",
            subject_kind: "release",
            subject_repo_id: "r1",
            owner: "ada",
            repo: "hello",
            subject_number: 0,
            subject_title: "v0.9",
            subject_ref: null,
            actor_id: "u2",
            actor_username: "bob",
            created_at: "2026-09-19T00:00:00Z",
            read_at: null,
          },
        ],
      },
    } as never);

    renderWithQueryClient(NotificationsPage);

    const row = await waitFor(() =>
      screen.getByRole("button", { name: /bob deleted a release on ada\/hello/i }),
    );

    const assign = vi.fn();
    vi.stubGlobal("location", { assign });
    row.click();
    await waitFor(() => {
      expect(assign).toHaveBeenCalledWith("/ada/hello/releases");
    });
  });
});
