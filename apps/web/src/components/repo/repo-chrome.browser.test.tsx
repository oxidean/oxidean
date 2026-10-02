/**
 * Chromium gate for the repo-chrome watch-level DropdownMenu (DEBT-06):
 * menu opens, level items render, and picking a level calls repo.watch
 * without DOM-race errors.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { RepoPublic } from "@oxidean/api-client";
import { RepoChrome } from "@/components/repo/repo-chrome.tsrx";
import {
  cleanupBrowserMount,
  clickTestId,
  debugBody,
  expectNoOctaneOverlayInDocument,
  mountWithQueryClient,
} from "@/test/browser-mount";
import { trackDomErrors } from "@/test/dom-errors";

const watchMock = vi.fn();
const unwatchMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    repo: {
      watch: (...args: unknown[]) => watchMock(...args),
      unwatch: (...args: unknown[]) => unwatchMock(...args),
    },
  },
}));

vi.mock("@/lib/use-chrome-account", () => ({
  useChromeAccountState: () => ({
    pending: false,
    user: {
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
    },
    needsSetup: false,
    allowSignup: true,
  }),
}));

beforeEach(() => {
  watchMock.mockReset();
  unwatchMock.mockReset();
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

function repoFixture(level: string | null): RepoPublic {
  return {
    id: "r1",
    owner_id: "o1",
    owner_type: "user",
    owner_username: "ada",
    name: "hello",
    description: "Demo repo",
    visibility: "public",
    default_branch: "main",
    updated_at: "2026-09-14T00:00:00Z",
    can_admin: false,
    can_write: true,
    star_count: 1,
    viewer_has_starred: false,
    watch_count: 1,
    viewer_is_watching: level != null && level !== "ignore",
    viewer_watch_level: level as RepoPublic["viewer_watch_level"],
    fork_count: 0,
  };
}

describe("RepoChrome watch-level menu (DEBT-06)", () => {
  it("opens the menu and picks Ignore without DOM errors", async () => {
    watchMock.mockResolvedValue({
      ok: true,
      data: { ...repoFixture("ignore") },
    });
    const tracker = trackDomErrors();
    try {
      await mountWithQueryClient(RepoChrome, {
        repo: repoFixture("all"),
        active: "code",
      });

      const trigger = await waitForTestId("repo-watch-menu");
      expect(trigger.textContent).toContain("Watching");

      await clickTestId("repo-watch-menu");
      await waitForTestId("repo-watch-level-ignore");
      await clickTestId("repo-watch-level-ignore");

      await vi.waitFor(() => {
        expect(watchMock).toHaveBeenCalledWith({
          owner: "ada",
          name: "hello",
          level: "ignore",
        });
      });
      const t = await waitForTestId("repo-watch-menu");
      expect(t.textContent).toContain("Ignored");
      expectNoOctaneOverlayInDocument();
      tracker.expectNoDomRaces();
    } finally {
      tracker.dispose();
    }
  });

  it("offers Unwatch for a subscribed viewer", async () => {
    unwatchMock.mockResolvedValue({
      ok: true,
      data: { ...repoFixture(null), watch_count: 0 },
    });
    const tracker = trackDomErrors();
    try {
      await mountWithQueryClient(RepoChrome, {
        repo: repoFixture("participating"),
        active: "code",
      });
      await clickTestId("repo-watch-menu");
      await waitForTestId("repo-watch-unwatch");
      await clickTestId("repo-watch-unwatch");
      await vi.waitFor(() => {
        expect(unwatchMock).toHaveBeenCalledWith({ owner: "ada", name: "hello" });
      });
      expectNoOctaneOverlayInDocument();
      tracker.expectNoDomRaces();
    } finally {
      tracker.dispose();
    }
  });
});
