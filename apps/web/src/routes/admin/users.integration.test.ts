import { cleanup, fireEvent, screen, waitFor } from "@octanejs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderWithQueryClient } from "@/test/render-with-query";

const meMock = vi.fn();
const listUsersMock = vi.fn();
const listInvitesMock = vi.fn();
const createInviteMock = vi.fn();
const createLinkMock = vi.fn();
const updateRoleMock = vi.fn();
const revokeSessionsMock = vi.fn();
const banMock = vi.fn();
const unbanMock = vi.fn();
const deleteUserMock = vi.fn();
const revokeInviteMock = vi.fn();
const getAccessMock = vi.fn();
const listSessionsMock = vi.fn();
const getActivityMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    auth: {
      me: (...args: unknown[]) => meMock(...args),
    },
    admin: {
      users: {
        list: (...args: unknown[]) => listUsersMock(...args),
        updateRole: (...args: unknown[]) => updateRoleMock(...args),
        revokeSessions: (...args: unknown[]) => revokeSessionsMock(...args),
        ban: (...args: unknown[]) => banMock(...args),
        unban: (...args: unknown[]) => unbanMock(...args),
        delete: (...args: unknown[]) => deleteUserMock(...args),
        getAccess: (...args: unknown[]) => getAccessMock(...args),
        listSessions: (...args: unknown[]) => listSessionsMock(...args),
        getActivity: (...args: unknown[]) => getActivityMock(...args),
      },
      invites: {
        create: (...args: unknown[]) => createInviteMock(...args),
        createLink: (...args: unknown[]) => createLinkMock(...args),
        list: (...args: unknown[]) => listInvitesMock(...args),
        revoke: (...args: unknown[]) => revokeInviteMock(...args),
      },
    },
  },
}));

vi.mock("@/lib/toast", () => ({
  toastSuccess: vi.fn(),
  toastError: vi.fn(),
  toastWarning: vi.fn(),
}));

const sysAdmin = {
  id: "u1",
  email: "admin@example.com",
  username: "admin",
  display_name: "Admin",
  bio: "",
  avatar_url: null as null,
  role: "sys-admin" as const,
  profile_incomplete: false,
  email_verified: true,
  must_change_credentials: false,
  default_branch: "main",
};

const listedUser = {
  id: "u2",
  email: "ada@example.com",
  username: "ada",
  display_name: "Ada Lovelace",
  role: "user" as const,
  email_verified: true,
  banned_at: null as null,
  created_at: "2026-01-15T12:00:00Z",
};

const readyUsers = {
  users: [
    {
      id: sysAdmin.id,
      email: sysAdmin.email,
      username: sysAdmin.username,
      display_name: sysAdmin.display_name,
      role: "sys-admin" as const,
      email_verified: true,
      banned_at: null as null,
      created_at: "2026-01-01T00:00:00Z",
    },
    listedUser,
  ],
  total: 2,
};

const pendingInvite = {
  id: "inv-1",
  email: "new@example.com",
  expires_at: "2026-10-07T00:00:00Z",
  invited_by: sysAdmin.id,
  created_at: "2026-09-30T00:00:00Z",
  max_uses: 1,
  use_count: 0,
};

const pendingLinkInvite = {
  id: "inv-2",
  email: null,
  expires_at: null,
  invited_by: sysAdmin.id,
  created_at: "2026-09-30T00:00:00Z",
  max_uses: 5,
  use_count: 2,
};

type LoaderShape =
  | { kind: "unauthenticated" }
  | { kind: "forbidden" }
  | { kind: "error"; message: string }
  | {
      kind: "ready";
      me: typeof sysAdmin;
      users: typeof readyUsers;
      invites: (typeof pendingInvite | typeof pendingLinkInvite)[];
    };

let loaderData: LoaderShape | undefined;
const assignMock = vi.fn();

vi.mock("@octanejs/tanstack-router", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@octanejs/tanstack-router")>();
  return {
    ...actual,
    useLoaderData: () => loaderData,
  };
});

import { AdminUsersPage } from "./users";

describe("/admin/users", () => {
  afterEach(() => {
    cleanup();
  });

  beforeEach(() => {
    meMock.mockReset();
    listUsersMock.mockReset();
    listInvitesMock.mockReset();
    createInviteMock.mockReset();
    updateRoleMock.mockReset();
    revokeSessionsMock.mockReset();
    banMock.mockReset();
    unbanMock.mockReset();
    deleteUserMock.mockReset();
    revokeInviteMock.mockReset();
    getAccessMock.mockReset();
    assignMock.mockReset();
    Object.defineProperty(window, "location", {
      configurable: true,
      value: { assign: assignMock, href: "http://localhost/" },
    });

    meMock.mockResolvedValue({ ok: true, data: sysAdmin });
    listUsersMock.mockResolvedValue({ ok: true, data: readyUsers });
    listInvitesMock.mockResolvedValue({ ok: true, data: { invites: [pendingInvite] } });
    createInviteMock.mockResolvedValue({
      ok: true,
      data: {
        results: [
          {
            email: "fresh@example.com",
            ok: true,
            invite: pendingInvite,
            invite_url: "https://oxidean.example/invites/tok-abc",
          },
        ],
      },
    });
    createLinkMock.mockResolvedValue({
      ok: true,
      data: {
        invite: pendingLinkInvite,
        invite_url: "https://oxidean.example/invites/tok-link",
      },
    });
    listSessionsMock.mockResolvedValue({
      ok: true,
      data: {
        sessions: [
          {
            id: "s1",
            created_at: "2026-09-29T10:00:00Z",
            last_seen_at: "2026-09-30T08:00:00Z",
            expires_at: "2026-10-30T00:00:00Z",
            remember_me: true,
            ip_address: "203.0.113.7",
            user_agent: "Mozilla/5.0 TestBrowser",
          },
        ],
      },
    });
    getActivityMock.mockResolvedValue({
      ok: true,
      data: {
        items: [
          {
            id: "ae-1",
            source: "audit",
            event_type: "admin.user_ban",
            created_at: "2026-09-30T09:00:00Z",
            target_type: "user",
            target_id: "u9",
            detail: '{"note":"spam"}',
            ip_address: "198.51.100.4",
            user_agent: "curl/8.0",
          },
          {
            id: "ra-1",
            source: "repository",
            event_type: "push",
            created_at: "2026-09-29T12:00:00Z",
            repo_owner: "acme",
            repo_name: "app",
            ref_name: "main",
            commits_count: 3,
            commit_message: "fix things",
            pr_number: null,
          },
        ],
        event_types: ["admin.user_ban", "push"],
      },
    });
    banMock.mockResolvedValue({
      ok: true,
      data: { ...listedUser, banned_at: "2026-09-30T12:00:00Z" },
    });
    deleteUserMock.mockResolvedValue({
      ok: true,
      data: { ok: true, deleted_repos: 1, deleted_orgs: 0 },
    });
    getAccessMock.mockResolvedValue({
      ok: true,
      data: {
        orgs: [{ slug: "acme", display_name: "Acme", role: "member" }],
        repos: [{ owner: "acme", name: "app", permission: "write" }],
      },
    });
    loaderData = {
      kind: "ready",
      me: sysAdmin,
      users: readyUsers,
      invites: [pendingInvite],
    };
  });

  // Row actions live behind the row overflow menu — drive the real flow:
  // open the menu (Base UI toggles on click when no pointerdown precedes it),
  // then click the menu item.
  async function openUserMenu(userId: string) {
    await waitFor(() => {
      expect(screen.getByTestId(`admin-user-menu-${userId}`)).toBeTruthy();
    });
    fireEvent.click(screen.getByTestId(`admin-user-menu-${userId}`));
    await waitFor(() => {
      expect(screen.getByTestId(`admin-user-menu-${userId}`)).toHaveAttribute("data-popup-open");
    });
  }

  it("redirects signed-out sessions toward login", async () => {
    loaderData = { kind: "unauthenticated" };
    meMock.mockResolvedValue({
      ok: false,
      error: { code: "auth.unauthenticated", message: "Not signed in" },
    });

    renderWithQueryClient(AdminUsersPage);

    await waitFor(() => {
      expect(assignMock).toHaveBeenCalledWith("/login?returnTo=/admin/users");
    });
    expect(screen.getByText(/Sign in as a system administrator/i)).toBeTruthy();
    expect(screen.getByRole("link", { name: /Sign in/i })).toHaveAttribute(
      "href",
      "/login?returnTo=/admin/users",
    );
  });

  it("shows forbidden for non sys-admin from loader", async () => {
    loaderData = { kind: "forbidden" };
    meMock.mockResolvedValue({
      ok: true,
      data: { ...sysAdmin, role: "user" },
    });

    renderWithQueryClient(AdminUsersPage);

    await waitFor(() => {
      expect(screen.getByText(/You need admin access to manage users/i)).toBeTruthy();
    });
    expect(listUsersMock).not.toHaveBeenCalled();
    expect(screen.queryByTestId("admin-users-page")).toBeNull();
  });

  it("renders users list and invite action for sys-admin", async () => {
    renderWithQueryClient(AdminUsersPage);

    await waitFor(
      () => {
        expect(screen.getByTestId("admin-users-page")).toBeTruthy();
        expect(screen.getByRole("heading", { name: "Users" })).toBeTruthy();
        expect(screen.getByText("@ada")).toBeTruthy();
        expect(screen.getByText("Ada Lovelace")).toBeTruthy();
        expect(screen.getByText(/ada@example\.com/)).toBeTruthy();
        expect(screen.getByTestId("admin-invite-open")).toBeTruthy();
        expect(screen.getByText("new@example.com")).toBeTruthy();
      },
      { timeout: 10_000 },
    );

    const nav = screen.getByRole("navigation", { name: "Admin settings" });
    expect(nav.querySelector('a[href="/admin/users"]')).toBeTruthy();

    fireEvent.click(screen.getByTestId("admin-invite-open"));

    await waitFor(() => {
      expect(screen.getByTestId("admin-invite-dialog")).toHaveAttribute("data-open");
      expect(screen.getByTestId("admin-invite-email")).toBeTruthy();
    });
  }, 15_000);

  it("creates a bulk invite and shows per-recipient results with copy controls", async () => {
    renderWithQueryClient(AdminUsersPage);

    await waitFor(() => {
      expect(screen.getByTestId("admin-invite-open")).toBeTruthy();
    });

    fireEvent.click(screen.getByTestId("admin-invite-open"));

    await waitFor(() => {
      expect(screen.getByTestId("admin-invite-dialog")).toHaveAttribute("data-open");
    });

    fireEvent.input(screen.getByTestId("admin-invite-email"), {
      target: { value: "fresh@example.com" },
    });
    fireEvent.click(screen.getByTestId("admin-invite-create"));

    await waitFor(() => {
      expect(createInviteMock).toHaveBeenCalledWith({ emails: ["fresh@example.com"] });
      expect(screen.getByTestId("admin-invite-results")).toBeTruthy();
      expect(screen.getByTestId("admin-invite-result-copy-fresh@example.com")).toBeTruthy();
    });
  });

  it("creates a shareable invite link with expiry and seats", async () => {
    renderWithQueryClient(AdminUsersPage);

    await waitFor(() => {
      expect(screen.getByTestId("admin-invite-open")).toBeTruthy();
    });

    fireEvent.click(screen.getByTestId("admin-invite-open"));

    await waitFor(() => {
      expect(screen.getByTestId("admin-invite-dialog")).toHaveAttribute("data-open");
    });

    fireEvent.click(screen.getByTestId("admin-invite-mode-link"));

    await waitFor(() => {
      expect(screen.getByTestId("admin-invite-link-expiry")).toBeTruthy();
    });
    fireEvent.input(screen.getByTestId("admin-invite-link-expiry"), {
      target: { value: "2026-12-31" },
    });
    fireEvent.input(screen.getByTestId("admin-invite-link-seats"), {
      target: { value: "10" },
    });
    fireEvent.click(screen.getByTestId("admin-invite-create"));

    await waitFor(() => {
      expect(createLinkMock).toHaveBeenCalledWith({
        expires_at: "2026-12-31",
        max_uses: 10,
      });
      expect(screen.getByTestId("admin-invite-url-panel")).toBeTruthy();
      expect(screen.getByTestId("admin-invite-url")).toHaveValue(
        "https://oxidean.example/invites/tok-link",
      );
      expect(screen.getByTestId("admin-invite-url-copy")).toBeTruthy();
    });
  });

  it("renders link invites with seat usage and no expiry", async () => {
    listInvitesMock.mockResolvedValue({
      ok: true,
      data: { invites: [pendingInvite, pendingLinkInvite] },
    });
    loaderData = {
      kind: "ready",
      me: sysAdmin,
      users: readyUsers,
      invites: [pendingInvite, pendingLinkInvite],
    };

    renderWithQueryClient(AdminUsersPage);

    await waitFor(() => {
      expect(screen.getByText("new@example.com")).toBeTruthy();
      // The invite dialog's "Invite link" mode button stays mounted (portal),
      // so the pending row's label is not the only match.
      expect(screen.getAllByText("Invite link").length).toBeGreaterThan(0);
      expect(screen.getByText(/Never expires/)).toBeTruthy();
      expect(screen.getByText(/2\/5 seats used/)).toBeTruthy();
    });
  });

  it("opens ban confirm dialog and calls ban RPC", async () => {
    renderWithQueryClient(AdminUsersPage);

    await openUserMenu("u2");
    fireEvent.click(screen.getByTestId("admin-user-ban-u2"));

    await waitFor(() => {
      expect(screen.getByTestId("admin-users-confirm-dialog")).toBeTruthy();
      expect(screen.getByText(/Ban @ada/i)).toBeTruthy();
    });

    fireEvent.click(screen.getByRole("button", { name: /^Ban user$/i }));

    await waitFor(() => {
      expect(banMock).toHaveBeenCalledWith({ user_id: "u2" });
    });
  });

  it("delete dialog requires username confirmation before submit", async () => {
    renderWithQueryClient(AdminUsersPage);

    await openUserMenu("u2");
    fireEvent.click(screen.getByTestId("admin-user-delete-u2"));

    await waitFor(() => {
      expect(screen.getByTestId("admin-users-delete-dialog")).toBeTruthy();
      expect(screen.getByTestId("admin-delete-confirm")).toBeTruthy();
    });

    expect(screen.getByTestId("admin-users-delete-submit")).toBeDisabled();

    fireEvent.input(screen.getByTestId("admin-delete-confirm"), {
      target: { value: "ada" },
    });

    await waitFor(() => {
      expect(screen.getByTestId("admin-users-delete-submit")).not.toBeDisabled();
    });

    fireEvent.click(screen.getByTestId("admin-users-delete-submit"));

    await waitFor(() => {
      expect(deleteUserMock).toHaveBeenCalledWith({
        user_id: "u2",
        confirmation: "ada",
        delete_orgs: false,
      });
    });
  });

  it("delete dialog requires org-deletion opt-in when server refuses shared orgs", async () => {
    deleteUserMock
      .mockResolvedValueOnce({
        ok: false,
        error: {
          code: "admin.delete_orgs_confirm",
          message:
            "Deleting this account also deletes organization(s) that still have other members: shared-org. Confirm again with delete_orgs to proceed.",
        },
      })
      .mockResolvedValueOnce({
        ok: true,
        data: { ok: true, deleted_repos: 2, deleted_orgs: 1 },
      });

    renderWithQueryClient(AdminUsersPage);

    await openUserMenu("u2");
    fireEvent.click(screen.getByTestId("admin-user-delete-u2"));
    fireEvent.input(screen.getByTestId("admin-delete-confirm"), {
      target: { value: "ada" },
    });
    fireEvent.click(screen.getByTestId("admin-users-delete-submit"));

    await waitFor(() => {
      expect(deleteUserMock).toHaveBeenCalledWith({
        user_id: "u2",
        confirmation: "ada",
        delete_orgs: false,
      });
      expect(screen.getByTestId("admin-delete-orgs")).toBeTruthy();
    });

    // Submit stays disabled until the shared-org opt-in is checked.
    expect(screen.getByTestId("admin-users-delete-submit")).toBeDisabled();

    fireEvent.click(screen.getByTestId("admin-delete-orgs"));

    await waitFor(() => {
      expect(screen.getByTestId("admin-users-delete-submit")).not.toBeDisabled();
    });

    fireEvent.click(screen.getByTestId("admin-users-delete-submit"));

    await waitFor(() => {
      expect(deleteUserMock).toHaveBeenCalledWith({
        user_id: "u2",
        confirmation: "ada",
        delete_orgs: true,
      });
    });
  });

  it("view access loads getAccess and shows org/repo grants", async () => {
    renderWithQueryClient(AdminUsersPage);

    await openUserMenu("u2");
    fireEvent.click(screen.getByTestId("admin-user-access-toggle-u2"));

    await waitFor(() => {
      expect(getAccessMock).toHaveBeenCalledWith({ user_id: "u2" });
      const panel = screen.getByTestId("admin-user-access-u2");
      expect(panel).toBeTruthy();
      expect(panel.textContent).toMatch(/Organizations/);
      expect(panel.textContent).toMatch(/acme/);
      expect(panel.textContent).toMatch(/acme\/app/);
      expect(panel.textContent).toMatch(/write/i);
      expect(panel.querySelector('a[href="/acme"]')).toBeTruthy();
      expect(panel.querySelector('a[href="/acme/app"]')).toBeTruthy();
    });
  });

  it("detail panel lists sessions with client metadata", async () => {
    renderWithQueryClient(AdminUsersPage);

    await openUserMenu("u2");
    fireEvent.click(screen.getByTestId("admin-user-access-toggle-u2"));

    await waitFor(() => {
      expect(listSessionsMock).toHaveBeenCalledWith({ user_id: "u2" });
      const sessions = screen.getByTestId("admin-user-sessions-u2");
      expect(sessions.textContent).toMatch(/203\.0\.113\.7/);
      expect(sessions.textContent).toMatch(/TestBrowser/);
      expect(sessions.textContent).toMatch(/remember me/);
      expect(sessions.textContent).toMatch(/Last seen/);
    });
  });

  it("detail panel lists activity and filters by source", async () => {
    renderWithQueryClient(AdminUsersPage);

    await openUserMenu("u2");
    fireEvent.click(screen.getByTestId("admin-user-access-toggle-u2"));

    await waitFor(() => {
      expect(getActivityMock).toHaveBeenCalledWith({
        user_id: "u2",
        source: null,
        event_type: null,
        limit: 100,
      });
      const activity = screen.getByTestId("admin-user-activity-u2");
      expect(activity.textContent).toMatch(/admin\.user_ban/);
      expect(activity.textContent).toMatch(/push/);
      expect(activity.textContent).toMatch(/acme\/app/);
      expect(activity.textContent).toMatch(/fix things/);
      expect(screen.getByTestId("admin-user-activity-types-u2")).toBeTruthy();
    });

    fireEvent.click(screen.getByTestId("admin-user-activity-src-repository-u2"));

    await waitFor(() => {
      expect(getActivityMock).toHaveBeenCalledWith({
        user_id: "u2",
        source: "repository",
        event_type: null,
        limit: 100,
      });
    });
  });

  it("revoke sessions confirm calls revokeSessions RPC", async () => {
    revokeSessionsMock.mockResolvedValue({ ok: true, data: { revoked: 3 } });
    renderWithQueryClient(AdminUsersPage);

    await openUserMenu("u2");
    fireEvent.click(screen.getByTestId("admin-user-revoke-sessions-u2"));

    await waitFor(() => {
      expect(screen.getByTestId("admin-users-confirm-dialog")).toBeTruthy();
      expect(screen.getByText(/Force-logout @ada/i)).toBeTruthy();
    });

    fireEvent.click(screen.getByRole("button", { name: /^Revoke sessions$/i }));

    await waitFor(() => {
      expect(revokeSessionsMock).toHaveBeenCalledWith({ user_id: "u2" });
    });
  });

  it("make sys-admin confirm calls updateRole RPC", async () => {
    updateRoleMock.mockResolvedValue({
      ok: true,
      data: { ...listedUser, role: "sys-admin" },
    });
    renderWithQueryClient(AdminUsersPage);

    await openUserMenu("u2");
    fireEvent.click(screen.getByTestId("admin-user-make-sysadmin-u2"));

    await waitFor(() => {
      expect(screen.getByTestId("admin-users-confirm-dialog")).toBeTruthy();
      expect(screen.getByText(/Grant system administrator access to @ada/i)).toBeTruthy();
    });

    fireEvent.click(screen.getByRole("button", { name: /^Grant system admin$/i }));

    await waitFor(() => {
      expect(updateRoleMock).toHaveBeenCalledWith({
        user_id: "u2",
        role: "sys-admin",
      });
    });
  });
});
