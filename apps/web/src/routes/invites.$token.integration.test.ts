import { cleanup, screen, waitFor } from "@octanejs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderWithQueryClient } from "@/test/render-with-query";

const meMock = vi.fn();
const acceptMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    auth: {
      me: (...args: unknown[]) => meMock(...args),
    },
    invites: {
      accept: (...args: unknown[]) => acceptMock(...args),
    },
  },
}));

vi.mock("@octanejs/tanstack-router", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@octanejs/tanstack-router")>();
  return {
    ...actual,
    useParams: () => ({ token: "invite-token-abc" }),
  };
});

import { InviteAcceptPage } from "./invites.$token";

afterEach(cleanup);

describe("/invites/$token (G-11.1-15)", () => {
  beforeEach(() => {
    meMock.mockReset();
    acceptMock.mockReset();
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
    meMock.mockResolvedValue({
      ok: true,
      data: {
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
      },
    });

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
});
