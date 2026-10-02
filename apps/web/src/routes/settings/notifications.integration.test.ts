import { createElement } from "octane";
import { cleanup, fireEvent, screen, waitFor } from "@octanejs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderWithQueryClient } from "@/test/render-with-query";

/**
 * DEBT-06 — /settings/notifications per-repo watch matrix.
 * Renders watched repos with level selectors; changing a select calls
 * repo.watch with the new level.
 */

const meMock = vi.fn();
const listWatchedMock = vi.fn();
const watchMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    auth: {
      me: (...args: unknown[]) => meMock(...args),
    },
    user: {
      listWatched: (...args: unknown[]) => listWatchedMock(...args),
    },
    repo: {
      watch: (...args: unknown[]) => watchMock(...args),
    },
  },
}));

type LoaderShape =
  | { kind: "unauthenticated" }
  | { kind: "error"; message: string }
  | {
      kind: "ready";
      user: { id: string; username: string };
      repos: unknown[];
    };

let loaderData: LoaderShape | undefined;

vi.mock("@octanejs/tanstack-router", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@octanejs/tanstack-router")>();
  return {
    ...actual,
    useLoaderData: () => loaderData,
    Link: (props: { to?: string; children?: unknown; className?: string }) =>
      createElement(
        "a",
        { href: props.to ?? "#", className: props.className },
        props.children as never,
      ),
  };
});

const sessionUser = {
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

function watchedRepo(level: string, name = "demo") {
  return {
    id: `r-${name}`,
    owner_id: "o1",
    owner_type: "user",
    owner_username: "octo",
    name,
    description: "Demo repo",
    visibility: "public",
    default_branch: "main",
    updated_at: "2026-09-14T00:00:00Z",
    viewer_is_watching: level !== "ignore",
    viewer_watch_level: level,
    watch_count: 2,
  };
}

import { NotificationsSettingsPage } from "./notifications.tsrx";

afterEach(cleanup);

describe("/settings/notifications watch matrix", () => {
  beforeEach(() => {
    loaderData = undefined;
    meMock.mockReset();
    listWatchedMock.mockReset();
    watchMock.mockReset();
  });

  it("renders watched repos with their current levels", async () => {
    loaderData = {
      kind: "ready",
      user: sessionUser,
      repos: [
        watchedRepo("all"),
        watchedRepo("participating", "quiet"),
        watchedRepo("ignore", "muted"),
      ],
    };
    renderWithQueryClient(NotificationsSettingsPage);

    await waitFor(() => {
      expect(screen.getByText("octo/demo")).toBeTruthy();
      expect(screen.getByText("octo/quiet")).toBeTruthy();
      expect(screen.getByText("octo/muted")).toBeTruthy();
    });

    const demo = screen.getByLabelText("Notifications for octo/demo") as HTMLSelectElement;
    const quiet = screen.getByLabelText("Notifications for octo/quiet") as HTMLSelectElement;
    const muted = screen.getByLabelText("Notifications for octo/muted") as HTMLSelectElement;
    expect(demo.value).toBe("all");
    expect(quiet.value).toBe("participating");
    expect(muted.value).toBe("ignore");
  });

  it("calls repo.watch with the chosen level on change", async () => {
    loaderData = {
      kind: "ready",
      user: sessionUser,
      repos: [watchedRepo("all")],
    };
    watchMock.mockResolvedValue({ ok: true, data: watchedRepo("ignore") });
    renderWithQueryClient(NotificationsSettingsPage);

    const select = (await screen.findByLabelText(
      "Notifications for octo/demo",
    )) as HTMLSelectElement;
    fireEvent.change(select, { target: { value: "ignore" } });

    await waitFor(() => {
      expect(watchMock).toHaveBeenCalledWith({
        owner: "octo",
        name: "demo",
        level: "ignore",
      });
    });
    await waitFor(() => {
      expect(select.value).toBe("ignore");
    });
  });

  it("shows an empty state when nothing is watched", async () => {
    loaderData = { kind: "ready", user: sessionUser, repos: [] };
    renderWithQueryClient(NotificationsSettingsPage);
    await waitFor(() => {
      expect(screen.getByText("No watched repositories")).toBeTruthy();
    });
  });

  it("falls back to the client when no loader data is present", async () => {
    loaderData = undefined;
    meMock.mockResolvedValue({ ok: true, data: sessionUser });
    listWatchedMock.mockResolvedValue({
      ok: true,
      data: { repos: [watchedRepo("participating")] },
    });
    renderWithQueryClient(NotificationsSettingsPage);

    await waitFor(() => {
      expect(screen.getByText("octo/demo")).toBeTruthy();
    });
    expect(listWatchedMock).toHaveBeenCalled();
  });

  it("renders an error state when the loader fails", async () => {
    loaderData = { kind: "error", message: "Could not load watched repositories." };
    renderWithQueryClient(NotificationsSettingsPage);
    await waitFor(() => {
      expect(screen.getByRole("alert").textContent).toContain(
        "Could not load watched repositories.",
      );
    });
  });
});
