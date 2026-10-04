/**
 * DEBT-06 — user profile Follow button + follower/following counts.
 */
import { createElement } from "octane";
import { cleanup, fireEvent, screen, waitFor } from "@octanejs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderWithQueryClient } from "@/test/render-with-query";

const followMock = vi.fn();
const unfollowMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    user: {
      follow: (...args: unknown[]) => followMock(...args),
      unfollow: (...args: unknown[]) => unfollowMock(...args),
    },
  },
}));

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

type LoaderShape = {
  kind: "user";
  user: {
    profile: {
      username: string;
      display_name: string;
      bio: string;
      avatar_url: null;
      follower_count: number;
      following_count: number;
      viewer_is_following: boolean;
    };
    repos: unknown[];
    starred: unknown[];
    isSelf: boolean;
    profileReadme: null;
  };
};

let loaderData: LoaderShape | undefined;

vi.mock("@octanejs/tanstack-router", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@octanejs/tanstack-router")>();
  return {
    ...actual,
    useParams: () => ({ owner: "grace" }),
    useLoaderData: () => loaderData,
    Link: (props: {
      to?: string;
      params?: Record<string, string>;
      children?: unknown;
      className?: string;
    }) =>
      createElement(
        "a",
        { href: "#", className: props.className } as never,
        props.children as never,
      ),
  };
});

function userLoader(opts: {
  following?: boolean;
  followerCount?: number;
  followingCount?: number;
  isSelf?: boolean;
}): LoaderShape {
  return {
    kind: "user",
    user: {
      profile: {
        username: "grace",
        display_name: "Grace Hopper",
        bio: "COBOL pioneer",
        avatar_url: null,
        follower_count: opts.followerCount ?? 3,
        following_count: opts.followingCount ?? 1,
        viewer_is_following: opts.following ?? false,
      },
      repos: [],
      starred: [],
      isSelf: opts.isSelf ?? false,
      profileReadme: null,
    },
  };
}

import { UserProfilePage } from "./$owner.index";

afterEach(cleanup);

describe("/$owner user profile follow (DEBT-06)", () => {
  beforeEach(() => {
    loaderData = undefined;
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
    followMock.mockReset();
    unfollowMock.mockReset();
  });

  it("renders follower/following counts and a Follow button", async () => {
    loaderData = userLoader({ followerCount: 5, followingCount: 2 });
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
    loaderData = userLoader({ followerCount: 5 });
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
    loaderData = userLoader({ following: true, followerCount: 5 });
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
    loaderData = userLoader({ isSelf: true });
    renderWithQueryClient(UserProfilePage);
    await waitFor(() => {
      expect(screen.getByText("Grace Hopper")).toBeTruthy();
    });
    expect(screen.queryByTestId("profile-follow-button")).toBeNull();

    cleanup();
    accountUser = null;
    loaderData = userLoader({ isSelf: false });
    renderWithQueryClient(UserProfilePage);
    await waitFor(() => {
      expect(screen.getByText("Grace Hopper")).toBeTruthy();
    });
    expect(screen.queryByTestId("profile-follow-button")).toBeNull();
  });
});
