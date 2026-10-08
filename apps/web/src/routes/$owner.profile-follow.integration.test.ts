/**
 * DEBT-06 — user profile Follow button + follower/following counts.
 */
import { cleanup, fireEvent, screen, waitFor } from "@octanejs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderWithQueryClient } from "@/test/render-with-query";
import { getQueryClient } from "@/lib/query-client";

const followMock = vi.fn();
const unfollowMock = vi.fn();
const authMeMock = vi.fn();
const orgGetMock = vi.fn();
const orgListMineMock = vi.fn();
const profileGetMock = vi.fn();
const listByOwnerMock = vi.fn();
const listStarredMock = vi.fn();
const repoGetMock = vi.fn();
const repoTreeMock = vi.fn();
const repoBlobMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    auth: {
      me: (...args: unknown[]) => authMeMock(...args),
    },
    org: {
      get: (...args: unknown[]) => orgGetMock(...args),
      listMine: (...args: unknown[]) => orgListMineMock(...args),
    },
    user: {
      follow: (...args: unknown[]) => followMock(...args),
      unfollow: (...args: unknown[]) => unfollowMock(...args),
      getPublicProfile: (...args: unknown[]) => profileGetMock(...args),
      listStarred: (...args: unknown[]) => listStarredMock(...args),
    },
    repo: {
      listByOwner: (...args: unknown[]) => listByOwnerMock(...args),
      get: (...args: unknown[]) => repoGetMock(...args),
      tree: (...args: unknown[]) => repoTreeMock(...args),
      blob: (...args: unknown[]) => repoBlobMock(...args),
    },
  },
}));

function setLocation(path: string) {
  window.history.pushState({}, "", path);
}

let accountUser: unknown = {
  id: "u-me",
  email: "me@example.com",
  username: "me",
  display_name: "Me",
  bio: "",
  avatar_url: null,
  role: "user",
  profile_incomplete: false,
  email_verified: true,
  must_change_credentials: false,
};

vi.mock("@/lib/use-chrome-account", () => ({
  useChromeAccountState: () => ({
    pending: false,
    user: accountUser,
    needsSetup: false,
    allowSignup: true,
  }),
}));

function graceProfile(opts: {
  following?: boolean;
  followerCount?: number;
  followingCount?: number;
}) {
  return {
    username: "grace",
    display_name: "Grace Hopper",
    bio: "COBOL pioneer",
    avatar_url: null,
    follower_count: opts.followerCount ?? 3,
    following_count: opts.followingCount ?? 1,
    viewer_is_following: opts.following ?? false,
  };
}

const meUser = {
  id: "u-me",
  email: "me@example.com",
  username: "me",
  display_name: "Me",
  bio: "",
  avatar_url: null,
  role: "user",
  profile_incomplete: false,
  email_verified: true,
  must_change_credentials: false,
};

import { UserProfilePage } from "./$owner.index";

afterEach(cleanup);

describe("/$owner user profile follow (DEBT-06)", () => {
  beforeEach(() => {
    setLocation("/grace");
    followMock.mockReset();
    unfollowMock.mockReset();
    authMeMock.mockReset();
    orgGetMock.mockReset();
    orgListMineMock.mockReset();
    profileGetMock.mockReset();
    listByOwnerMock.mockReset();
    listStarredMock.mockReset();
    repoGetMock.mockReset();
    repoTreeMock.mockReset();
    repoBlobMock.mockReset();
    authMeMock.mockResolvedValue({ ok: true, data: meUser });
    orgGetMock.mockResolvedValue({
      ok: false,
      error: { code: "org.not_found", message: "not found" },
    });
    orgListMineMock.mockResolvedValue({ ok: true, data: { orgs: [] } });
    profileGetMock.mockResolvedValue({ ok: true, data: graceProfile({}) });
    listByOwnerMock.mockResolvedValue({ ok: true, data: { repos: [] } });
    listStarredMock.mockResolvedValue({ ok: true, data: { repos: [] } });
    repoGetMock.mockResolvedValue({
      ok: false,
      error: { code: "repo.not_found", message: "not found" },
    });
    repoTreeMock.mockResolvedValue({
      ok: false,
      error: { code: "repo.not_found", message: "not found" },
    });
    repoBlobMock.mockResolvedValue({
      ok: false,
      error: { code: "repo.not_found", message: "not found" },
    });
    accountUser = {
      id: "u-me",
      email: "me@example.com",
      username: "me",
      display_name: "Me",
      bio: "",
      avatar_url: null,
      role: "user",
      profile_incomplete: false,
      email_verified: true,
      must_change_credentials: false,
    };
  });

  it("renders follower/following counts and a Follow button", async () => {
    profileGetMock.mockResolvedValue({
      ok: true,
      data: graceProfile({ followerCount: 5, followingCount: 2 }),
    });
    renderWithQueryClient(UserProfilePage);

    await waitFor(() => {
      expect(screen.getByText("Grace Hopper")).toBeTruthy();
      const counts = screen.getByTestId("profile-follow-counts");
      expect(counts.textContent).toContain("5");
      expect(counts.textContent).toContain("followers");
      expect(counts.textContent).toContain("2");
      expect(counts.textContent).toContain("following");
      const btn = screen.getByTestId("profile-follow-button") as HTMLButtonElement;
      expect(btn.textContent).toBe("Follow");
    });
  });

  it("calls user.follow and flips to Following with updated count", async () => {
    profileGetMock.mockResolvedValue({ ok: true, data: graceProfile({ followerCount: 5 }) });
    followMock.mockResolvedValue({
      ok: true,
      data: {
        username: "grace",
        display_name: "Grace Hopper",
        bio: "",
        avatar_url: null,
        follower_count: 6,
        following_count: 1,
        viewer_is_following: true,
      },
    });
    renderWithQueryClient(UserProfilePage);

    const btn = (await screen.findByTestId("profile-follow-button")) as HTMLButtonElement;
    fireEvent.click(btn);
    await waitFor(() => {
      expect(followMock).toHaveBeenCalledWith({ username: "grace" });
      expect(btn.textContent).toBe("Following");
      expect(screen.getByTestId("profile-follow-counts").textContent).toContain("6");
    });
  });

  it("calls user.unfollow when already following", async () => {
    profileGetMock.mockResolvedValue({
      ok: true,
      data: graceProfile({ following: true, followerCount: 5 }),
    });
    unfollowMock.mockResolvedValue({
      ok: true,
      data: {
        username: "grace",
        display_name: "Grace Hopper",
        bio: "",
        avatar_url: null,
        follower_count: 4,
        following_count: 1,
        viewer_is_following: false,
      },
    });
    renderWithQueryClient(UserProfilePage);

    const btn = (await screen.findByTestId("profile-follow-button")) as HTMLButtonElement;
    expect(btn.textContent).toBe("Following");
    fireEvent.click(btn);
    await waitFor(() => {
      expect(unfollowMock).toHaveBeenCalledWith({ username: "grace" });
      expect(btn.textContent).toBe("Follow");
    });
  });

  it("hides the Follow button on your own profile and when signed out", async () => {
    authMeMock.mockResolvedValue({ ok: true, data: { ...meUser, username: "grace" } });
    renderWithQueryClient(UserProfilePage);
    await waitFor(() => {
      expect(screen.getByText("Grace Hopper")).toBeTruthy();
    });
    expect(screen.queryByTestId("profile-follow-button")).toBeNull();

    cleanup();
    getQueryClient().clear();
    accountUser = null;
    authMeMock.mockResolvedValue({
      ok: false,
      error: { code: "auth.unauthenticated", message: "signed out" },
    });
    renderWithQueryClient(UserProfilePage);
    await waitFor(() => {
      expect(screen.getByText("Grace Hopper")).toBeTruthy();
    });
    expect(screen.queryByTestId("profile-follow-button")).toBeNull();
  });
});
