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
      },
      invites: {
        create: (...args: unknown[]) => createInviteMock(...args),
        list: (...args: unknown[]) => listInvitesMock(...args),
        revoke: (...args: unknown[]) => revokeInviteMock(...args),
      },
    },
  },
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
});
