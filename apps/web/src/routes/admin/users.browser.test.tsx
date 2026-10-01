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
const listSessionsMock = vi.fn();
const getActivityMock = vi.fn();

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
        listSessions: (...args: unknown[]) => listSessionsMock(...args),
        getActivity: (...args: unknown[]) => getActivityMock(...args),
      },
      invites: {
        create: (...args: unknown[]) => createInviteMock(...args),
        createLink: vi.fn(),
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
  listSessionsMock.mockResolvedValue({ ok: true, data: { sessions: [] } });
  getActivityMock.mockResolvedValue({ ok: true, data: { items: [], event_types: [] } });

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

  it("invite dialog modes and user detail panel mount without DOM races", async () => {
    const linkInvite = {
      id: "inv-link",
      email: null,
      expires_at: null,
      invited_by: sysAdmin.id,
      created_at: "2026-01-20T00:00:00Z",
      max_uses: 5,
      use_count: 2,
    };
    listInvitesMock.mockResolvedValue({ ok: true, data: { invites: [linkInvite] } });
    listSessionsMock.mockResolvedValue({
      ok: true,
      data: {
        sessions: [
          {
            id: "s1",
            created_at: "2026-01-20T00:00:00Z",
            last_seen_at: "2026-01-21T00:00:00Z",
            expires_at: "2026-02-20T00:00:00Z",
            remember_me: true,
            ip_address: "203.0.113.7",
            user_agent: "Mozilla/5.0 test-agent",
          },
        ],
      },
    });
    getActivityMock.mockResolvedValue({
      ok: true,
      data: {
        items: [
          {
            id: "a1",
            source: "audit",
            event_type: "auth.login",
            detail: "password",
            target_type: null,
            target_id: null,
            repo_owner: null,
            repo_name: null,
            ref_name: null,
            commit_message: null,
            created_at: "2026-01-21T00:00:00Z",
          },
          {
            id: "r1",
            source: "repository",
            event_type: "push",
            detail: null,
            target_type: null,
            target_id: null,
            repo_owner: "acme",
            repo_name: "app",
            ref_name: "refs/heads/main",
            commit_message: "fix",
            created_at: "2026-01-22T00:00:00Z",
          },
        ],
        event_types: ["auth.login", "push"],
      },
    });
    createInviteMock.mockResolvedValue({
      ok: true,
      data: {
        results: [
          {
            email: "one@example.com",
            ok: true,
            error: null,
            invite: linkInvite,
            invite_url: "https://ox.example/invites/tok1",
          },
          { email: "bad", ok: false, error: "invalid email", invite: null, invite_url: null },
        ],
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
      invites: [linkInvite],
    });

    const tracker = trackDomErrors();
    try {
      await mountWithQueryClient(AdminUsersPage, {});

      await waitForTestId("admin-users-page");

      // Pending link invite row renders its seat/expiry metadata.
      const deadline = Date.now() + 10_000;
      while (Date.now() < deadline) {
        if (document.body.textContent?.includes("2/5 seats used")) break;
        await new Promise((r) => setTimeout(r, 50));
      }
      expect(document.body.textContent).toContain("Invite link");
      expect(document.body.textContent).toContain("Never expires");
      expect(document.body.textContent).toContain("2/5 seats used");

      // Invite dialog: emails → link → emails mode switches mount cleanly.
      await clickTestId("admin-invite-open");
      await waitForTestId("admin-invite-dialog");
      await waitForTestId("admin-invite-email");

      await clickTestId("admin-invite-mode-link");
      await waitForTestId("admin-invite-link-expiry");
      await waitForTestId("admin-invite-link-seats");

      await clickTestId("admin-invite-mode-emails");
      await waitForTestId("admin-invite-email");

      // Bulk textarea accepts multi-address input and submits per-email results.
      await typeIntoTestId("admin-invite-email", "one@example.com, bad");
      await clickTestId("admin-invite-create");
      await waitForTestId("admin-invite-results");
      expect(createInviteMock).toHaveBeenCalledWith({
        emails: ["one@example.com", "bad"],
      });

      // Close the dialog, then open the per-user detail panel.
      await act(async () => {
        document
          .querySelector('[data-testid="admin-invite-dialog"]')
          ?.closest("[data-slot='dialog-content']")
          ?.querySelector("button[data-slot='dialog-close'], button[aria-label='Close']")
          ?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
      });
      await new Promise((r) => setTimeout(r, 150));

      await openUserMenu("u2");
      await clickTestId("admin-user-access-toggle-u2");
      await waitForTestId("admin-user-access-u2");
      await waitForTestId(`admin-user-sessions-u2`);
      await waitForTestId(`admin-user-activity-u2`);

      expect(listSessionsMock).toHaveBeenCalledWith({ user_id: "u2" });
      expect(getActivityMock).toHaveBeenCalledWith(
        expect.objectContaining({ user_id: "u2", source: null, limit: 100 }),
      );
      expect(document.body.textContent).toContain("203.0.113.7");
      expect(document.body.textContent).toContain("auth.login");
      expect(document.body.textContent).toContain("acme/app");

      // Activity source filter buttons swap the query without a DOM race.
      await clickTestId("admin-user-activity-src-audit-u2");
      await new Promise((r) => setTimeout(r, 100));
      expect(getActivityMock).toHaveBeenCalledWith(
        expect.objectContaining({ user_id: "u2", source: "audit" }),
      );

      tracker.expectNoDomRaces();
      expectNoOctaneOverlayInDocument();
    } finally {
      tracker.dispose();
    }
  }, 45_000);
});
