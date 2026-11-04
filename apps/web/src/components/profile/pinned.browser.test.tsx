/**
 * Chromium gate: profile pinned-repos customize dialog (Checkbox + Dialog) and
 * the owner contribution-settings DropdownMenu must mount/interact without
 * insertBefore / Octane overlay races (COL-04 / COL-12 surfaces).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanupBrowserMount,
  debugBody,
  expectNoOctaneOverlayInDocument,
  mountWithQueryClient,
} from "@/test/browser-mount";
import { trackDomErrors } from "@/test/dom-errors";
import { act } from "octane";

const meMock = vi.fn();
const listPinnedMock = vi.fn();
const setPinnedMock = vi.fn();
const getContributionsMock = vi.fn();
const updateProfileMock = vi.fn();
const languagesMock = vi.fn();
const orgListPinnedMock = vi.fn();
const orgSetPinnedMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    auth: {
      me: (...args: unknown[]) => meMock(...args),
      providerConfig: vi.fn(async () => ({
        ok: true,
        data: { mode: "local", allow_signup: true },
      })),
      bootstrapStatus: vi.fn(async () => ({ ok: true, data: { needs_setup: false } })),
    },
    user: {
      listPinnedRepos: (...args: unknown[]) => listPinnedMock(...args),
      setPinnedRepos: (...args: unknown[]) => setPinnedMock(...args),
      getContributions: (...args: unknown[]) => getContributionsMock(...args),
      updateProfile: (...args: unknown[]) => updateProfileMock(...args),
    },
    org: {
      listPinnedRepos: (...args: unknown[]) => orgListPinnedMock(...args),
      setPinnedRepos: (...args: unknown[]) => orgSetPinnedMock(...args),
    },
    repo: {
      languages: (...args: unknown[]) => languagesMock(...args),
    },
  },
}));

import { PinnedReposSection } from "./pinned-repos";
import { UserContributionsSection } from "./user-contributions-section";

const user = {
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
  two_factor_enabled: false,
  two_factor_enroll_required: false,
  show_private_contributions: true,
};

const repo = (id: string, name: string) => ({
  id,
  owner_id: "u1",
  owner_type: "user",
  owner_username: "ada",
  name,
  description: `${name} description`,
  visibility: "public",
  default_branch: "main",
  updated_at: "2026-01-01T00:00:00Z",
  can_admin: false,
  can_write: false,
  star_count: 3,
  open_issue_count: 0,
  open_pull_count: 0,
  viewer_has_starred: false,
  fork_count: 1,
});

const contributions = {
  total: 2,
  from: "2026-01-01",
  to: "2026-01-07",
  includes_private: true,
  days: [{ date: "2026-01-02", count: 2 }],
  by_type: { commits: 2, pull_requests: 0, issues: 0, reviews: 0 },
  months: [],
  years: [2026],
  current_streak: 1,
  longest_streak: 1,
};

beforeEach(() => {
  window.history.pushState({}, "", "/ada");
  for (const m of [
    listPinnedMock,
    setPinnedMock,
    getContributionsMock,
    updateProfileMock,
    languagesMock,
  ]) {
    m.mockReset();
  }
  orgListPinnedMock.mockReset();
  orgSetPinnedMock.mockReset();
  meMock.mockReset();
  meMock.mockResolvedValue({ ok: true, data: user });
  updateProfileMock.mockImplementation(async (input: Record<string, unknown>) => ({
    ok: true,
    data: { ...user, ...input },
  }));
  getContributionsMock.mockResolvedValue({ ok: true, data: contributions });
  languagesMock.mockResolvedValue({ ok: true, data: { languages: [] } });
  setPinnedMock.mockImplementation(async (input: { repo_ids: string[] }) => ({
    ok: true,
    data: { repos: (input.repo_ids ?? []).map((id) => repo(id, `r-${id}`)) },
  }));
});

afterEach(async () => {
  await cleanupBrowserMount();
});

async function waitForSelector(selector: string, ms = 15_000): Promise<Element> {
  const deadline = Date.now() + ms;
  while (Date.now() < deadline) {
    const el = document.querySelector(selector);
    if (el) return el;
    await new Promise((r) => setTimeout(r, 50));
  }
  throw new Error(`${selector} never appeared. ${debugBody()}`);
}

describe("profile pinned repos browser DOM races", () => {
  it("customize dialog toggles checkboxes and saves the pin list", async () => {
    const tracker = trackDomErrors();
    try {
      listPinnedMock.mockResolvedValue({
        ok: true,
        data: { repos: [repo("r1", "alpha")] },
      });
      await mountWithQueryClient(PinnedReposSection, {
        kind: "user",
        slug: "ada",
        canCustomize: true,
        candidates: [repo("r1", "alpha"), repo("r2", "beta")],
      });

      const customize = await waitForSelector('[data-testid="customize-pins"]');
      await act(async () => {
        (customize as HTMLElement).click();
      });

      await waitForSelector('[data-testid="customize-pins-dialog"]');
      const beta = await waitForSelector('[aria-label="Pin beta"]');
      await act(async () => {
        (beta as HTMLElement).click();
      });
      await new Promise((r) => setTimeout(r, 100));
      const count = await waitForSelector('[data-testid="pins-selected-count"]');
      expect(count.textContent).toContain("2 of 6");

      const save = await waitForSelector('[data-testid="save-pins-button"]');
      await act(async () => {
        (save as HTMLElement).click();
      });
      await new Promise((r) => setTimeout(r, 200));
      expect(setPinnedMock).toHaveBeenCalledWith({
        repo_ids: expect.arrayContaining(["r1", "r2"]),
      });

      tracker.expectNoDomRaces();
      expectNoOctaneOverlayInDocument();
    } finally {
      tracker.dispose();
    }
  });

  it("org kind lists and saves pins via the org RPCs", async () => {
    const tracker = trackDomErrors();
    try {
      orgListPinnedMock.mockResolvedValue({
        ok: true,
        data: { repos: [repo("r1", "alpha")] },
      });
      orgSetPinnedMock.mockImplementation(async (input: { repo_ids: string[] }) => ({
        ok: true,
        data: { repos: (input.repo_ids ?? []).map((id) => repo(id, `r-${id}`)) },
      }));
      await mountWithQueryClient(PinnedReposSection, {
        kind: "org",
        slug: "acme",
        canCustomize: true,
        candidates: [repo("r1", "alpha"), repo("r2", "beta")],
      });

      const customize = await waitForSelector('[data-testid="customize-pins"]');
      await act(async () => {
        (customize as HTMLElement).click();
      });

      await waitForSelector('[data-testid="customize-pins-dialog"]');
      const beta = await waitForSelector('[aria-label="Pin beta"]');
      await act(async () => {
        (beta as HTMLElement).click();
      });
      const save = await waitForSelector('[data-testid="save-pins-button"]');
      await act(async () => {
        (save as HTMLElement).click();
      });
      await new Promise((r) => setTimeout(r, 200));
      expect(orgSetPinnedMock).toHaveBeenCalledWith({
        slug: "acme",
        repo_ids: expect.arrayContaining(["r1", "r2"]),
      });
      // The user RPCs are untouched on the org path.
      expect(setPinnedMock).not.toHaveBeenCalled();

      tracker.expectNoDomRaces();
      expectNoOctaneOverlayInDocument();
    } finally {
      tracker.dispose();
    }
  });

  it("contribution settings dropdown toggles private contributions", async () => {
    const tracker = trackDomErrors();
    try {
      await mountWithQueryClient(UserContributionsSection, {
        username: "ada",
        isSelf: true,
      });
      await waitForSelector('[data-testid="user-contributions"]');

      const trigger = await waitForSelector('[data-testid="contribution-settings-trigger"]');
      await act(async () => {
        (trigger as HTMLElement).click();
      });

      const item = await waitForSelector('[data-testid="contribution-settings-private"]');
      await act(async () => {
        (item as HTMLElement).click();
      });
      await new Promise((r) => setTimeout(r, 200));
      expect(updateProfileMock).toHaveBeenCalledWith(
        expect.objectContaining({ show_private_contributions: false }),
      );

      tracker.expectNoDomRaces();
      expectNoOctaneOverlayInDocument();
    } finally {
      tracker.dispose();
    }
  });
});
