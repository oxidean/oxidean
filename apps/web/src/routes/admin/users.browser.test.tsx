/**
 * Chromium gate for /admin/users AlertDialog + Delete Dialog portals (issue #52).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanupBrowserMount,
  clickTestId,
  debugBody,
  expectNoOctaneOverlayInDocument,
  mountWithQueryClient,
} from "@/test/browser-mount";
import { trackDomErrors } from "@/test/dom-errors";
import { act } from "octane";

const meMock = vi.fn();
const listUsersMock = vi.fn();
const listInvitesMock = vi.fn();
const createInviteMock = vi.fn();
const banMock = vi.fn();
const deleteUserMock = vi.fn();
const getAccessMock = vi.fn();

const loaderState = vi.hoisted(() => {
  let data: unknown;
  return {
    get: () => data,
    set: (next: unknown) => {
      data = next;
    },
  };
});

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    auth: {
      me: (...args: unknown[]) => meMock(...args),
    },
    admin: {
      users: {
        list: (...args: unknown[]) => listUsersMock(...args),
        updateRole: vi.fn(),
        revokeSessions: vi.fn(),
        ban: (...args: unknown[]) => banMock(...args),
        unban: vi.fn(),
        delete: (...args: unknown[]) => deleteUserMock(...args),
        getAccess: (...args: unknown[]) => getAccessMock(...args),
      },
      invites: {
        create: (...args: unknown[]) => createInviteMock(...args),
        list: (...args: unknown[]) => listInvitesMock(...args),
        revoke: vi.fn(),
      },
    },
  },
}));

vi.mock("@/lib/toast", () => ({
  toastSuccess: vi.fn(),
  toastError: vi.fn(),
  toastWarning: vi.fn(),
}));

// Route module imports ssr-auth → tanstack-start; stub so Chromium Vite never loads Start.
vi.mock("@/lib/ssr-auth", () => ({
  fetchSessionMe: vi.fn(),
  fetchAdminUsersList: vi.fn(),
  fetchAdminInvitesList: vi.fn(),
}));

// Avoid importing Start/router entry points in the Chromium iframe.
vi.mock("@octanejs/tanstack-router", () => ({
  createFileRoute: () => (opts: unknown) => opts,
  useLoaderData: () => loaderState.get(),
}));

import { AdminUsersPage } from "./users";

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

beforeEach(() => {
  meMock.mockReset();
  listUsersMock.mockReset();
  listInvitesMock.mockReset();
  createInviteMock.mockReset();
  banMock.mockReset();
  deleteUserMock.mockReset();
  getAccessMock.mockReset();

  meMock.mockResolvedValue({ ok: true, data: sysAdmin });
  listUsersMock.mockResolvedValue({
    ok: true,
    data: {
      users: [
        {
          id: sysAdmin.id,
          email: sysAdmin.email,
          username: sysAdmin.username,
          display_name: sysAdmin.display_name,
          role: "sys-admin" as const,
          email_verified: true,
          banned_at: null,
          created_at: "2026-01-01T00:00:00Z",
        },
        listedUser,
      ],
      total: 2,
    },
  });
  listInvitesMock.mockResolvedValue({ ok: true, data: { invites: [] } });
  banMock.mockResolvedValue({
    ok: true,
    data: { ...listedUser, banned_at: "2026-09-30T12:00:00Z" },
  });
  deleteUserMock.mockResolvedValue({
    ok: true,
    data: { ok: true, deleted_repos: 0, deleted_orgs: 0 },
  });
  getAccessMock.mockResolvedValue({
    ok: true,
    data: {
      orgs: [{ slug: "acme", role: "member", display_name: "Acme" }],
      repos: [{ owner: "acme", name: "app", permission: "write" }],
    },
  });

  loaderState.set({
    kind: "ready",
    me: sysAdmin,
    users: {
      users: [
        {
          id: sysAdmin.id,
          email: sysAdmin.email,
          username: sysAdmin.username,
          display_name: sysAdmin.display_name,
          role: "sys-admin" as const,
          email_verified: true,
          banned_at: null,
          created_at: "2026-01-01T00:00:00Z",
        },
        listedUser,
      ],
      total: 2,
    },
    invites: [],
  });
});

afterEach(async () => {
  await cleanupBrowserMount();
});

async function waitForTestId(testId: string, ms = 10_000): Promise<Element> {
  const deadline = Date.now() + ms;
  while (Date.now() < deadline) {
    const el = document.querySelector(`[data-testid="${testId}"]`);
    if (el) return el;
    await new Promise((r) => setTimeout(r, 50));
  }
  throw new Error(`[data-testid="${testId}"] never appeared. ${debugBody()}`);
}

async function typeIntoTestId(testId: string, value: string): Promise<void> {
  const el = await waitForTestId(testId);
  await act(async () => {
    const input = el as HTMLInputElement;
    input.focus();
    input.value = value;
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

// Row actions live behind the row overflow menu. Open it (Base UI toggles the
// menu on click when no pointerdown precedes it) and wait for data-popup-open,
// retrying once — portal open can miss the first click under Chromium.
async function openUserMenu(userId: string): Promise<void> {
  const trigger = await waitForTestId(`admin-user-menu-${userId}`);
  for (let attempt = 0; attempt < 2; attempt++) {
    await act(async () => {
      (trigger as HTMLElement).click();
    });
    const deadline = Date.now() + 2_000;
    while (Date.now() < deadline) {
      if (trigger.hasAttribute("data-popup-open")) return;
      await new Promise((r) => setTimeout(r, 50));
    }
  }
  throw new Error(`admin-user-menu-${userId} did not open. ${debugBody()}`);
}

describe("AdminUsersPage browser DOM races", () => {
  it("ban AlertDialog and delete Dialog portals do not throw insertBefore", async () => {
    const tracker = trackDomErrors();
    try {
      await mountWithQueryClient(AdminUsersPage, {});

      await waitForTestId("admin-users-page");
      await waitForTestId("admin-user-menu-u2");

      await openUserMenu("u2");
      await clickTestId("admin-user-ban-u2");
      await waitForTestId("admin-users-confirm-dialog");
      expect(document.body.textContent).toMatch(/Ban user/i);

      const banConfirm = Array.from(document.querySelectorAll("button")).find((b) =>
        /^Ban user$/i.test((b.textContent ?? "").trim()),
      );
      expect(banConfirm).toBeTruthy();
      await act(async () => {
        banConfirm!.click();
      });
      await new Promise((r) => setTimeout(r, 100));
      expect(banMock).toHaveBeenCalled();

      // First delete is refused for shared orgs; the opt-in Checkbox must
      // mount inside the dialog without an insertBefore race.
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
          data: { ok: true, deleted_repos: 0, deleted_orgs: 1 },
        });

      await openUserMenu("u2");
      await clickTestId("admin-user-delete-u2");
      await waitForTestId("admin-users-delete-dialog");
      await waitForTestId("admin-delete-confirm");

      const submit = document.querySelector(
        '[data-testid="admin-users-delete-submit"]',
      ) as HTMLButtonElement | null;
      expect(submit?.disabled).toBe(true);

      await typeIntoTestId("admin-delete-confirm", "ada");
      await waitForTestId("admin-users-delete-submit");
      expect(
        (document.querySelector('[data-testid="admin-users-delete-submit"]') as HTMLButtonElement)
          .disabled,
      ).toBe(false);

      await clickTestId("admin-users-delete-submit");
      await new Promise((r) => setTimeout(r, 100));
      expect(deleteUserMock).toHaveBeenCalledWith({
        user_id: "u2",
        confirmation: "ada",
        delete_orgs: false,
      });

      await waitForTestId("admin-delete-orgs");
      expect(
        (document.querySelector('[data-testid="admin-users-delete-submit"]') as HTMLButtonElement)
          .disabled,
      ).toBe(true);

      await clickTestId("admin-delete-orgs");
      await new Promise((r) => setTimeout(r, 100));
      expect(
        (document.querySelector('[data-testid="admin-users-delete-submit"]') as HTMLButtonElement)
          .disabled,
      ).toBe(false);

      await clickTestId("admin-users-delete-submit");
      await new Promise((r) => setTimeout(r, 100));
      expect(deleteUserMock).toHaveBeenCalledWith({
        user_id: "u2",
        confirmation: "ada",
        delete_orgs: true,
      });

      tracker.expectNoDomRaces();
      expectNoOctaneOverlayInDocument();
    } finally {
      tracker.dispose();
    }
  }, 45_000);
});
