import { cleanup, fireEvent, screen, waitFor } from "@octanejs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { CollaboratorsPanel } from "@/components/repo/collaborators-panel";
import { renderWithQueryClient } from "@/test/render-with-query";

const listCollabsMock = vi.fn();
const listInvitesMock = vi.fn();
const createInviteMock = vi.fn();
const createLinkMock = vi.fn();
const revokeInviteMock = vi.fn();
const addCollabMock = vi.fn();
const lookupMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    repo: {
      collaborators: {
        list: (...args: unknown[]) => listCollabsMock(...args),
        add: (...args: unknown[]) => addCollabMock(...args),
        update: vi.fn(),
        remove: vi.fn(),
      },
      invites: {
        list: (...args: unknown[]) => listInvitesMock(...args),
        create: (...args: unknown[]) => createInviteMock(...args),
        createLink: (...args: unknown[]) => createLinkMock(...args),
        revoke: (...args: unknown[]) => revokeInviteMock(...args),
      },
    },
    user: {
      lookup: (...args: unknown[]) => lookupMock(...args),
    },
  },
}));

describe("repo settings Collaborators (ORG-03 / D-ORG-02c / D-ORG-04)", () => {
  afterEach(() => {
    cleanup();
  });

  beforeEach(() => {
    listCollabsMock.mockReset();
    listInvitesMock.mockReset();
    createInviteMock.mockReset();
    createLinkMock.mockReset();
    revokeInviteMock.mockReset();
    addCollabMock.mockReset();
    lookupMock.mockReset();
    listCollabsMock.mockResolvedValue({ ok: true, data: { collaborators: [] } });
    listInvitesMock.mockResolvedValue({ ok: true, data: { invites: [] } });
    lookupMock.mockResolvedValue({ ok: true, data: { users: [] } });
    createInviteMock.mockResolvedValue({
      ok: true,
      data: {
        results: [
          {
            email: "new@example.com",
            ok: true,
            invite: {
              id: "ri1",
              email: "new@example.com",
              permission: "write",
              expires_at: "2026-10-07T00:00:00Z",
              invited_by: "u1",
              created_at: "2026-09-30T00:00:00Z",
              max_uses: 1,
              use_count: 0,
            },
            invite_url: "https://oxidean.example/invites/repo-tok",
          },
        ],
      },
    });
    createLinkMock.mockResolvedValue({
      ok: true,
      data: {
        invite: {
          id: "ri2",
          email: null,
          permission: "read",
          expires_at: null,
          invited_by: "u1",
          created_at: "2026-09-30T00:00:00Z",
          max_uses: null,
          use_count: 0,
        },
        invite_url: "https://oxidean.example/invites/repo-link-tok",
      },
    });
  });

  it("settings gate uses can_admin — not me.id === owner_id", async () => {
    const settingsSrc = await import("./$owner.$repo.settings.tsrx?raw").then((m) =>
      String((m as { default: string }).default),
    );
    expect(settingsSrc).toMatch(/can_admin/);
    expect(settingsSrc).not.toMatch(/me\.id\s*===\s*repo\.owner_id/);
  });

  it("renders empty collaborators state and opens add form", async () => {
    renderWithQueryClient(CollaboratorsPanel, { props: { owner: "ada", name: "hello" } });

    await waitFor(() => {
      expect(screen.getByTestId("collaborators-panel")).toBeTruthy();
      expect(screen.getByText(/No collaborators yet/i)).toBeTruthy();
      expect(screen.getByTestId("collab-add-open")).toBeTruthy();
    });

    fireEvent.click(screen.getByTestId("collab-add-open"));

    await waitFor(() => {
      expect(screen.getByTestId("collab-add-perm")).toBeTruthy();
      expect(document.getElementById("collab-lookup")).toBeTruthy();
    });
  });

  it("username lookup uses repo context and never emails", async () => {
    const src = await import("../components/repo/collaborators-panel.tsrx?raw").then((m) =>
      String((m as { default: string }).default),
    );
    expect(src).toMatch(/MemberLookup/);
    expect(src).toMatch(/kind:\s*"repo"/);
    expect(src).not.toMatch(/hit\.email/);
  });

  it("email invite create shows per-recipient result with copy link", async () => {
    listInvitesMock.mockResolvedValueOnce({ ok: true, data: { invites: [] } }).mockResolvedValue({
      ok: true,
      data: {
        invites: [
          {
            id: "ri1",
            email: "new@example.com",
            permission: "write",
            expires_at: "2026-10-07T00:00:00Z",
            invited_by: "u1",
            created_at: "2026-09-30T00:00:00Z",
            max_uses: 1,
            use_count: 0,
          },
        ],
      },
    });

    renderWithQueryClient(CollaboratorsPanel, { props: { owner: "ada", name: "hello" } });

    await waitFor(() => {
      expect(document.getElementById("repo-invite-emails")).toBeTruthy();
    });

    const emailInput = document.getElementById("repo-invite-emails") as HTMLTextAreaElement;
    fireEvent.input(emailInput, { target: { value: "new@example.com" } });
    fireEvent.click(screen.getByRole("button", { name: /^Send invitations$/i }));

    await waitFor(() => {
      expect(createInviteMock).toHaveBeenCalledWith({
        owner: "ada",
        name: "hello",
        emails: ["new@example.com"],
        permission: "write",
      });
      expect(screen.getByTestId("repo-invite-results")).toBeTruthy();
      expect(screen.getByTestId("repo-invite-result-copy-new@example.com")).toBeTruthy();
    });
  });

  it("link mode creates a reusable invite link with options", async () => {
    renderWithQueryClient(CollaboratorsPanel, { props: { owner: "ada", name: "hello" } });

    await waitFor(() => {
      expect(screen.getByTestId("repo-invite-mode-link")).toBeTruthy();
    });

    fireEvent.click(screen.getByTestId("repo-invite-mode-link"));

    await waitFor(() => {
      expect(document.getElementById("repo-invite-link-expiry")).toBeTruthy();
    });
    fireEvent.input(document.getElementById("repo-invite-link-expiry")!, {
      target: { value: "2026-12-31" },
    });
    fireEvent.input(document.getElementById("repo-invite-link-seats")!, {
      target: { value: "3" },
    });
    fireEvent.click(screen.getByRole("button", { name: /^Create invite link$/i }));

    await waitFor(() => {
      expect(createLinkMock).toHaveBeenCalledWith({
        owner: "ada",
        name: "hello",
        permission: "write",
        expires_at: "2026-12-31",
        max_uses: 3,
      });
      expect(
        screen.getByDisplayValue("https://oxidean.example/invites/repo-link-tok"),
      ).toBeTruthy();
      expect(screen.getByRole("button", { name: /^Copy link$/i })).toBeTruthy();
    });
  });

  it("revokes a pending repo invite via confirm dialog", async () => {
    listInvitesMock.mockResolvedValue({
      ok: true,
      data: {
        invites: [
          {
            id: "ri1",
            email: "pending@example.com",
            permission: "write",
            expires_at: "2026-10-07T00:00:00Z",
            invited_by: "u1",
            created_at: "2026-09-30T00:00:00Z",
            max_uses: 1,
            use_count: 0,
          },
        ],
      },
    });
    revokeInviteMock.mockResolvedValue({ ok: true, data: { ok: true } });

    renderWithQueryClient(CollaboratorsPanel, { props: { owner: "ada", name: "hello" } });

    await waitFor(() => {
      expect(screen.getByText("pending@example.com")).toBeTruthy();
    });

    fireEvent.click(screen.getByRole("button", { name: /^Revoke$/i }));

    await waitFor(() => {
      expect(screen.getByText(/Revoke invite\?/i)).toBeTruthy();
    });

    fireEvent.click(screen.getByRole("button", { name: /^Revoke invite$/i }));

    await waitFor(() => {
      expect(revokeInviteMock).toHaveBeenCalledWith({
        owner: "ada",
        name: "hello",
        invite_id: "ri1",
      });
    });
  });
});
