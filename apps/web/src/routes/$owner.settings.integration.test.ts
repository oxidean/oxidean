/**
 * Org settings layout + General / Members / Labels render coverage.
 */
import { createElement } from "octane";
import { cleanup, fireEvent, screen, waitFor } from "@octanejs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderWithQueryClient } from "@/test/render-with-query";

const updateSettingsMock = vi.fn();
const membersListMock = vi.fn();
const invitesListMock = vi.fn();
const invitesCreateMock = vi.fn();
const invitesCreateLinkMock = vi.fn();
const invitesRevokeMock = vi.fn();
const labelsListMock = vi.fn();
const authMeMock = vi.fn();
const bootstrapStatusMock = vi.fn();
const orgGetMock = vi.fn();
const orgListMineMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    auth: {
      me: (...args: unknown[]) => authMeMock(...args),
      bootstrapStatus: (...args: unknown[]) => bootstrapStatusMock(...args),
    },
    org: {
      get: (...args: unknown[]) => orgGetMock(...args),
      listMine: (...args: unknown[]) => orgListMineMock(...args),
      updateSettings: (...args: unknown[]) => updateSettingsMock(...args),
      members: {
        list: (...args: unknown[]) => membersListMock(...args),
        add: vi.fn(),
        updateRole: vi.fn(),
        remove: vi.fn(),
      },
      invites: {
        list: (...args: unknown[]) => invitesListMock(...args),
        create: (...args: unknown[]) => invitesCreateMock(...args),
        createLink: (...args: unknown[]) => invitesCreateLinkMock(...args),
        revoke: (...args: unknown[]) => invitesRevokeMock(...args),
      },
    },
    label: {
      listForOrg: (...args: unknown[]) => labelsListMock(...args),
      create: vi.fn(),
      delete: vi.fn(),
    },
    user: {
      lookup: vi.fn().mockResolvedValue({ ok: true, data: { users: [] } }),
    },
  },
}));

const org = {
  id: "o1",
  slug: "acme",
  display_name: "Acme",
  member_base_permission: "none" as const,
  created_at: "2026-01-01T00:00:00Z",
};

/**
 * Access resolution moved from the `$owner.settings` layout loader to
 * `orgSettingsAccessQueryOptions` — drive it by mocking `apiClient.auth.me`,
 * `org.get`, and `org.listMine`. A never-resolving `org.get` simulates the
 * loader-less "loading" state.
 */
let accessPending = false;

import { OrgSettingsShell } from "@/components/org/org-settings-shell";
import { OrgSettingsPage } from "./$owner.settings.index";
import { OrgMembersPage } from "./$owner.settings.members";
import { OrgLabelsPage } from "./$owner.settings.labels";

function setPathname(path: string) {
  window.history.pushState({}, "", path);
}

function mockAccessReady() {
  authMeMock.mockResolvedValue({ ok: true, data: ME });
  bootstrapStatusMock.mockResolvedValue({ ok: true, data: { needs_setup: false } });
  orgGetMock.mockImplementation(() =>
    accessPending ? new Promise(() => {}) : Promise.resolve({ ok: true, data: org }),
  );
  orgListMineMock.mockResolvedValue({
    ok: true,
    data: {
      orgs: [{ slug: "acme", role: "owner", display_name: "Acme" }],
    },
  });
}

const ME = {
  id: "u1",
  email: "ada@example.com",
  username: "ada",
  display_name: "Ada",
  bio: "",
  avatar_url: null,
  role: "user",
  profile_incomplete: false,
  email_verified: true,
  must_change_credentials: false,
};

beforeEach(() => {
  accessPending = false;
  setPathname("/acme/settings");
  mockAccessReady();
  membersListMock.mockResolvedValue({
    ok: true,
    data: {
      members: [
        {
          user_id: "u1",
          username: "owner1",
          role: "owner",
          created_at: "2026-01-01T00:00:00Z",
        },
      ],
    },
  });
  invitesListMock.mockResolvedValue({ ok: true, data: { invites: [] } });
  invitesCreateMock.mockReset();
  invitesCreateLinkMock.mockReset();
  invitesRevokeMock.mockReset();
  invitesCreateMock.mockResolvedValue({
    ok: true,
    data: {
      results: [
        {
          email: "new@example.com",
          ok: true,
          invite: {
            id: "oi1",
            email: "new@example.com",
            role: "member",
            expires_at: "2026-10-07T00:00:00Z",
            invited_by: "u1",
            created_at: "2026-09-30T00:00:00Z",
            max_uses: 1,
            use_count: 0,
          },
          invite_url: "https://oxidean.example/invites/org-tok",
        },
      ],
    },
  });
  invitesCreateLinkMock.mockResolvedValue({
    ok: true,
    data: {
      invite: {
        id: "oi2",
        email: null,
        role: "member",
        expires_at: null,
        invited_by: "u1",
        created_at: "2026-09-30T00:00:00Z",
        max_uses: null,
        use_count: 0,
      },
      invite_url: "https://oxidean.example/invites/org-link-tok",
    },
  });
  invitesRevokeMock.mockResolvedValue({ ok: true, data: { ok: true } });
  labelsListMock.mockResolvedValue({
    ok: true,
    data: {
      labels: [{ id: "l1", name: "bug", color: "d73a4a", description: "Something broken" }],
    },
  });
  updateSettingsMock.mockReset();
});

afterEach(cleanup);

describe("org settings layout", () => {
  it("happy: sidebar links for General, Members, Labels", async () => {
    renderWithQueryClient(OrgSettingsShell, {
      props: {
        active: "general",
        children: createElement("div", { "data-testid": "org-settings-outlet" }, "outlet"),
      },
    });

    await waitFor(() => {
      expect(screen.getByTestId("org-settings-layout")).toBeInTheDocument();
    });
    const nav = screen.getByRole("navigation", { name: "Organization settings" });
    expect(nav.querySelector('a[href="/acme/settings"]')).toBeTruthy();
    expect(nav.querySelector('a[href="/acme/settings/members"]')).toBeTruthy();
    expect(nav.querySelector('a[href="/acme/settings/labels"]')).toBeTruthy();
    expect(screen.getByTestId("org-settings-outlet")).toBeInTheDocument();
  });

  it("edge: highlights Members when pathname ends with /members", async () => {
    setPathname("/acme/settings/members");
    renderWithQueryClient(OrgSettingsShell, {
      props: { active: "members", children: createElement("div") },
    });

    await waitFor(() => {
      expect(screen.getByRole("link", { name: "Members" })).toHaveAttribute("aria-current", "page");
    });
  });

  it("unhappy: loading state when org missing", async () => {
    accessPending = true;
    renderWithQueryClient(OrgSettingsShell, {
      props: { active: "general", children: createElement("div") },
    });
    expect(screen.getByText("Loading…")).toBeInTheDocument();
  });
});

describe("org settings general", () => {
  it("happy: display name + member_base select", async () => {
    renderWithQueryClient(OrgSettingsPage);

    await waitFor(() => {
      expect(screen.getByTestId("org-settings-general")).toBeInTheDocument();
    });
    expect(screen.getByLabelText("Display name")).toHaveValue("Acme");
    expect(screen.getByLabelText("Base permission for Members")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Save settings" })).toBeInTheDocument();
  });
});

describe("org settings members", () => {
  it("happy: members table + add member controls", async () => {
    renderWithQueryClient(OrgMembersPage);

    await waitFor(() => {
      expect(screen.getByTestId("org-settings-members")).toBeInTheDocument();
    });
    await waitFor(() => {
      expect(screen.getByText("@owner1")).toBeInTheDocument();
    });
    expect(screen.getByRole("heading", { name: "Members" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Add member" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Invitations" })).toBeInTheDocument();
  });

  it("creates org invites in bulk and shows per-recipient results", async () => {
    invitesListMock.mockResolvedValueOnce({ ok: true, data: { invites: [] } }).mockResolvedValue({
      ok: true,
      data: {
        invites: [
          {
            id: "oi1",
            email: "new@example.com",
            role: "member",
            expires_at: "2026-10-07T00:00:00Z",
            invited_by: "u1",
            created_at: "2026-09-30T00:00:00Z",
            max_uses: 1,
            use_count: 0,
          },
        ],
      },
    });

    renderWithQueryClient(OrgMembersPage);

    await waitFor(() => {
      expect(document.getElementById("invite-emails")).toBeTruthy();
    });

    fireEvent.input(document.getElementById("invite-emails") as HTMLTextAreaElement, {
      target: { value: "new@example.com" },
    });
    fireEvent.click(screen.getByRole("button", { name: /^Send invitations$/i }));

    await waitFor(() => {
      expect(invitesCreateMock).toHaveBeenCalledWith({
        slug: "acme",
        emails: ["new@example.com"],
        role: "member",
      });
      expect(screen.getByTestId("org-invite-results")).toBeTruthy();
      expect(screen.getByTestId("org-invite-result-copy-new@example.com")).toBeTruthy();
    });
  });

  it("link mode creates a reusable org invite link", async () => {
    renderWithQueryClient(OrgMembersPage);

    await waitFor(() => {
      expect(screen.getByTestId("org-invite-mode-link")).toBeTruthy();
    });

    fireEvent.click(screen.getByTestId("org-invite-mode-link"));

    await waitFor(() => {
      expect(document.getElementById("invite-link-seats")).toBeTruthy();
    });
    fireEvent.input(document.getElementById("invite-link-seats")!, {
      target: { value: "5" },
    });
    fireEvent.click(screen.getByRole("button", { name: /^Create invite link$/i }));

    await waitFor(() => {
      expect(invitesCreateLinkMock).toHaveBeenCalledWith({
        slug: "acme",
        role: "member",
        expires_at: null,
        max_uses: 5,
      });
      expect(screen.getByDisplayValue("https://oxidean.example/invites/org-link-tok")).toBeTruthy();
      expect(screen.getByRole("button", { name: /^Copy link$/i })).toBeTruthy();
    });
  });

  it("revokes a pending org invite via confirm dialog", async () => {
    invitesListMock.mockResolvedValue({
      ok: true,
      data: {
        invites: [
          {
            id: "oi1",
            email: "pending@example.com",
            role: "member",
            expires_at: "2026-10-07T00:00:00Z",
            invited_by: "u1",
            created_at: "2026-09-30T00:00:00Z",
            max_uses: 1,
            use_count: 0,
          },
        ],
      },
    });

    renderWithQueryClient(OrgMembersPage);

    await waitFor(() => {
      expect(screen.getByText("pending@example.com")).toBeTruthy();
    });

    fireEvent.click(screen.getByRole("button", { name: /^Revoke$/i }));

    await waitFor(() => {
      expect(screen.getByText(/Revoke invite\?/i)).toBeTruthy();
    });

    fireEvent.click(screen.getByRole("button", { name: /^Revoke invite$/i }));

    await waitFor(() => {
      expect(invitesRevokeMock).toHaveBeenCalledWith({
        slug: "acme",
        invite_id: "oi1",
      });
    });
  });

  it("unhappy: shows loading when org missing", () => {
    accessPending = true;
    renderWithQueryClient(OrgMembersPage);
    expect(screen.getByText("Loading…")).toBeInTheDocument();
  });
});

describe("org settings labels", () => {
  it("happy: create form + existing label list", async () => {
    renderWithQueryClient(OrgLabelsPage);

    await waitFor(() => {
      expect(screen.getByTestId("org-settings-labels")).toBeInTheDocument();
    });
    expect(screen.getByRole("heading", { name: "Labels" })).toBeInTheDocument();
    expect(screen.getByLabelText("Name")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Create label" })).toBeInTheDocument();
    await waitFor(() => {
      expect(screen.getByText("bug")).toBeInTheDocument();
    });
  });

  it("unhappy: empty catalog message", async () => {
    labelsListMock.mockResolvedValue({ ok: true, data: { labels: [] } });
    renderWithQueryClient(OrgLabelsPage);

    await waitFor(() => {
      expect(screen.getByText("No org labels yet")).toBeInTheDocument();
    });
  });
});
