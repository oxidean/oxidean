import { cleanup, fireEvent, screen, waitFor } from "@octanejs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderWithQueryClient } from "@/test/render-with-query";

const meMock = vi.fn();
const getMock = vi.fn();
const acceptMock = vi.fn();
const assignMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    auth: {
      me: (...args: unknown[]) => meMock(...args),
    },
    invites: {
      get: (...args: unknown[]) => getMock(...args),
      accept: (...args: unknown[]) => acceptMock(...args),
    },
  },
}));

import { InviteAcceptPage } from "./invites.$token";

const boundInstanceInvite = {
  kind: "instance",
  email: "newbie@example.com",
  expires_at: "2026-10-07T00:00:00Z",
  seats_remaining: 1,
  acceptable: true,
};

const linkInstanceInvite = {
  kind: "instance",
  email: null,
  expires_at: null,
  seats_remaining: null,
  acceptable: true,
};

const signedInUser = {
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
  default_branch: "main",
};

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

/**
 * Simulate the `Set-Cookie` pair a sessioned `invites.accept` response sends:
 * the API always pairs the HttpOnly session cookie with the readable
 * `oxidean_signed_in=1` presence hint the page checks post-accept (#130).
 */
function mintSessionCookies() {
  document.cookie = "oxidean_signed_in=1; path=/";
}

function clearSessionCookies() {
  document.cookie = "oxidean_signed_in=; path=/; max-age=0";
}

describe("/invites/$token (G-11.1-15)", () => {
  beforeEach(() => {
    meMock.mockReset();
    getMock.mockReset();
    acceptMock.mockReset();
    assignMock.mockReset();
    clearSessionCookies();
    window.history.pushState({}, "", "/invites/invite-token-abc");
    vi.spyOn(window.location, "assign").mockImplementation(assignMock);
    getMock.mockResolvedValue({ ok: true, data: boundInstanceInvite });
  });

  it("renders anon signup form when not signed in", async () => {
    meMock.mockResolvedValue({
      ok: false,
      error: { code: "auth.unauthenticated", message: "Not signed in" },
    });

    renderWithQueryClient(InviteAcceptPage);

    await waitFor(
      () => {
        expect(screen.getByText("Accept invitation")).toBeTruthy();
        expect(screen.getByLabelText("Username")).toBeTruthy();
        expect(screen.getByLabelText("Password")).toBeTruthy();
        expect(screen.getByLabelText("Confirm password")).toBeTruthy();
        expect(screen.getByRole("button", { name: "Create account and join" })).toBeTruthy();
      },
      { timeout: 10_000 },
    );
  });

  it("renders logged-in accept flow when session exists", async () => {
    meMock.mockResolvedValue({ ok: true, data: signedInUser });

    renderWithQueryClient(InviteAcceptPage);

    await waitFor(
      () => {
        expect(screen.getByText(/Signed in as/i)).toBeTruthy();
        expect(screen.getByText("ada")).toBeTruthy();
        expect(screen.getByRole("button", { name: "Accept invitation" })).toBeTruthy();
      },
      { timeout: 10_000 },
    );
  });

  it("shows invalid invite when preview reports unacceptable", async () => {
    getMock.mockResolvedValue({
      ok: true,
      data: { kind: "instance", email: null, acceptable: false, reason: "revoked" },
    });
    meMock.mockResolvedValue({
      ok: false,
      error: { code: "auth.unauthenticated", message: "Not signed in" },
    });

    renderWithQueryClient(InviteAcceptPage);

    await waitFor(() => {
      expect(screen.getByText(/revoked/i)).toBeTruthy();
    });
    expect(screen.queryByLabelText("Username")).toBeNull();
  });

  it("link invite shows email field for anonymous signup and sends it", async () => {
    getMock.mockResolvedValue({ ok: true, data: linkInstanceInvite });
    meMock.mockResolvedValue({
      ok: false,
      error: { code: "auth.unauthenticated", message: "Not signed in" },
    });
    acceptMock.mockImplementation(() => {
      mintSessionCookies();
      return Promise.resolve({ ok: true, data: { kind: "instance" } });
    });

    renderWithQueryClient(InviteAcceptPage);

    await waitFor(() => {
      expect(screen.getByLabelText("Email")).toBeTruthy();
      expect(screen.getByLabelText("Username")).toBeTruthy();
    });

    fireEvent.input(screen.getByLabelText("Email"), {
      target: { value: "newbie@example.com" },
    });
    fireEvent.input(screen.getByLabelText("Username"), { target: { value: "newbie" } });
    fireEvent.input(screen.getByLabelText("Password"), { target: { value: "password1" } });
    fireEvent.input(screen.getByLabelText("Confirm password"), {
      target: { value: "password1" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Create account and join" }));

    await waitFor(() => {
      expect(acceptMock).toHaveBeenCalledWith({
        token: "invite-token-abc",
        email: "newbie@example.com",
        username: "newbie",
        password: "password1",
      });
      expect(assignMock).toHaveBeenCalledWith("/");
    });
  });

  it("accepts instance invite and redirects home", async () => {
    meMock.mockResolvedValue({
      ok: false,
      error: { code: "auth.unauthenticated", message: "Not signed in" },
    });
    acceptMock.mockImplementation(() => {
      mintSessionCookies();
      return Promise.resolve({ ok: true, data: { kind: "instance" } });
    });

    renderWithQueryClient(InviteAcceptPage);

    await waitFor(() => {
      expect(screen.getByLabelText("Username")).toBeTruthy();
    });

    fireEvent.input(screen.getByLabelText("Username"), { target: { value: "newbie" } });
    fireEvent.input(screen.getByLabelText("Password"), { target: { value: "password1" } });
    fireEvent.input(screen.getByLabelText("Confirm password"), {
      target: { value: "password1" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Create account and join" }));

    await waitFor(() => {
      expect(acceptMock).toHaveBeenCalledWith({
        token: "invite-token-abc",
        email: null,
        username: "newbie",
        password: "password1",
      });
      expect(assignMock).toHaveBeenCalledWith("/");
    });
  });

  it("accepts org invite and redirects to org", async () => {
    meMock.mockResolvedValue({ ok: true, data: signedInUser });
    acceptMock.mockImplementation(() => {
      mintSessionCookies();
      return Promise.resolve({
        ok: true,
        data: {
          kind: "org",
          org: { id: "o1", slug: "acme", display_name: "Acme", member_base_permission: "none" },
          member: {
            user_id: "u1",
            username: "ada",
            role: "member",
            created_at: "2026-09-30T00:00:00Z",
          },
        },
      });
    });

    renderWithQueryClient(InviteAcceptPage);

    await waitFor(() => {
      expect(screen.getByRole("button", { name: "Accept invitation" })).toBeTruthy();
    });
    fireEvent.click(screen.getByRole("button", { name: "Accept invitation" }));

    await waitFor(() => {
      expect(acceptMock).toHaveBeenCalled();
      expect(assignMock).toHaveBeenCalledWith("/acme");
    });
  });

  it("accepts repo invite and redirects to repository", async () => {
    meMock.mockResolvedValue({ ok: true, data: signedInUser });
    acceptMock.mockImplementation(() => {
      mintSessionCookies();
      return Promise.resolve({
        ok: true,
        data: {
          kind: "repo",
          owner: "acme",
          name: "app",
          permission: "write",
        },
      });
    });

    renderWithQueryClient(InviteAcceptPage);

    await waitFor(() => {
      expect(screen.getByRole("button", { name: "Accept invitation" })).toBeTruthy();
    });
    fireEvent.click(screen.getByRole("button", { name: "Accept invitation" }));

    await waitFor(() => {
      expect(acceptMock).toHaveBeenCalled();
      expect(assignMock).toHaveBeenCalledWith("/acme/app");
    });
  });

  it("routes to login when an enroll-required accept mints no session", async () => {
    meMock.mockResolvedValue({
      ok: false,
      error: { code: "auth.unauthenticated", message: "Not signed in" },
    });
    // Enroll-required accepts skip the session mint (#130): ok response, but
    // no `oxidean_signed_in` cookie — the page must hand off to login (which
    // re-authenticates and opens the enroll challenge), not a protected page.
    acceptMock.mockResolvedValue({ ok: true, data: { kind: "instance" } });

    renderWithQueryClient(InviteAcceptPage);

    await waitFor(() => {
      expect(screen.getByLabelText("Username")).toBeTruthy();
    });

    fireEvent.input(screen.getByLabelText("Username"), { target: { value: "newbie" } });
    fireEvent.input(screen.getByLabelText("Password"), { target: { value: "password1" } });
    fireEvent.input(screen.getByLabelText("Confirm password"), {
      target: { value: "password1" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Create account and join" }));

    await waitFor(() => {
      expect(acceptMock).toHaveBeenCalled();
      expect(assignMock).toHaveBeenCalledWith("/login?returnTo=%2F");
    });
  });

  it("carries the org destination through login when no session was minted", async () => {
    meMock.mockResolvedValue({
      ok: false,
      error: { code: "auth.unauthenticated", message: "Not signed in" },
    });
    getMock.mockResolvedValue({
      ok: true,
      data: { kind: "org", email: "newbie@example.com", org_slug: "acme", acceptable: true },
    });
    acceptMock.mockResolvedValue({
      ok: true,
      data: {
        kind: "org",
        org: { id: "o1", slug: "acme", display_name: "Acme", member_base_permission: "none" },
        member: {
          user_id: "u2",
          username: "newbie",
          role: "member",
          created_at: "2026-09-30T00:00:00Z",
        },
      },
    });

    renderWithQueryClient(InviteAcceptPage);

    await waitFor(() => {
      expect(screen.getByLabelText("Username")).toBeTruthy();
    });

    fireEvent.input(screen.getByLabelText("Username"), { target: { value: "newbie" } });
    fireEvent.input(screen.getByLabelText("Password"), { target: { value: "password1" } });
    fireEvent.input(screen.getByLabelText("Confirm password"), {
      target: { value: "password1" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Create account and join" }));

    await waitFor(() => {
      expect(acceptMock).toHaveBeenCalled();
      expect(assignMock).toHaveBeenCalledWith("/login?returnTo=%2Facme");
    });
  });
});
