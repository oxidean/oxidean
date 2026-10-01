import { cleanup, fireEvent, screen, waitFor } from "@octanejs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderWithQueryClient } from "@/test/render-with-query";

const meMock = vi.fn();
const listUsersMock = vi.fn();
const listInvitesMock = vi.fn();
const createInviteMock = vi.fn();
const updateRoleMock = vi.fn();
const revokeSessionsMock = vi.fn();
const banMock = vi.fn();
const unbanMock = vi.fn();
const deleteUserMock = vi.fn();
const revokeInviteMock = vi.fn();
const getAccessMock = vi.fn();

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
      },
      invites: {
        create: (...args: unknown[]) => createInviteMock(...args),
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
};

type LoaderShape =
  | { kind: "unauthenticated" }
  | { kind: "forbidden" }
  | { kind: "error"; message: string }
  | {
      kind: "ready";
      me: typeof sysAdmin;
      users: typeof readyUsers;
      invites: (typeof pendingInvite)[];
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
        invite: pendingInvite,
        invite_url: "https://oxidean.example/invites/tok-abc",
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

  it("renders users table and invite panel for sys-admin", async () => {
    renderWithQueryClient(AdminUsersPage);

    await waitFor(
      () => {
        expect(screen.getByTestId("admin-users-page")).toBeTruthy();
        expect(screen.getByRole("heading", { name: "Users" })).toBeTruthy();
        expect(screen.getByText("@ada")).toBeTruthy();
        expect(screen.getByText("Ada Lovelace")).toBeTruthy();
        expect(screen.getByText("ada@example.com")).toBeTruthy();
        expect(screen.getByTestId("admin-invite-email")).toBeTruthy();
        expect(screen.getByText("new@example.com")).toBeTruthy();
      },
      { timeout: 10_000 },
    );

    const nav = screen.getByRole("navigation", { name: "Admin settings" });
    expect(nav.querySelector('a[href="/admin/users"]')).toBeTruthy();
  }, 15_000);

  it("creates an invite and shows a copyable invite URL control", async () => {
    renderWithQueryClient(AdminUsersPage);

    await waitFor(() => {
      expect(screen.getByTestId("admin-invite-email")).toBeTruthy();
    });

    fireEvent.input(screen.getByTestId("admin-invite-email"), {
      target: { value: "fresh@example.com" },
    });
    fireEvent.click(screen.getByTestId("admin-invite-create"));

    await waitFor(() => {
      expect(createInviteMock).toHaveBeenCalledWith({ email: "fresh@example.com" });
      expect(screen.getByTestId("admin-invite-url-panel")).toBeTruthy();
      expect(screen.getByTestId("admin-invite-url")).toHaveValue(
        "https://oxidean.example/invites/tok-abc",
      );
      expect(screen.getByTestId("admin-invite-url-copy")).toBeTruthy();
      expect(screen.getByRole("button", { name: /Copy invite URL|Copy link/i })).toBeTruthy();
    });
  });

  it("opens ban confirm dialog and calls ban RPC", async () => {
    renderWithQueryClient(AdminUsersPage);

    await waitFor(() => {
      expect(screen.getByTestId("admin-user-ban-u2")).toBeTruthy();
    });

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

    await waitFor(() => {
      expect(screen.getByTestId("admin-user-delete-u2")).toBeTruthy();
    });

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
      });
    });
  });

  it("view access loads getAccess and shows org/repo grants", async () => {
    renderWithQueryClient(AdminUsersPage);

    await waitFor(() => {
      expect(screen.getByTestId("admin-user-access-toggle-u2")).toBeTruthy();
    });

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

  it("revoke sessions confirm calls revokeSessions RPC", async () => {
    revokeSessionsMock.mockResolvedValue({ ok: true, data: { revoked: 3 } });
    renderWithQueryClient(AdminUsersPage);

    await waitFor(() => {
      expect(screen.getByTestId("admin-user-revoke-sessions-u2")).toBeTruthy();
    });

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

    await waitFor(() => {
      expect(screen.getByTestId("admin-user-make-sysadmin-u2")).toBeTruthy();
    });

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
