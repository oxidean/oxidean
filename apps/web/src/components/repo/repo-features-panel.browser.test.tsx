/**
 * Chromium gate for the per-repo unit toggles (COL-13): Switch flips drive
 * repo.issues.setEnabled / repo.pulls.setEnabled and update the repo store so
 * chrome tabs react without a reload.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { RepoFeaturesPanel } from "@/components/repo/repo-features-panel";
import {
  cleanupBrowserMount,
  clickAriaLabel,
  debugBody,
  expectNoOctaneOverlayInDocument,
  mountWithQueryClient,
} from "@/test/browser-mount";
import { trackDomErrors } from "@/test/dom-errors";
import type { RepoPublic } from "@oxidean/api-client";

const issuesSetMock = vi.fn();
const pullsSetMock = vi.fn();
const wikiSetMock = vi.fn();
const projectsSetMock = vi.fn();
const discussionsSetMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    repo: {
      issues: {
        setEnabled: (...args: unknown[]) => issuesSetMock(...args),
      },
      pulls: {
        setEnabled: (...args: unknown[]) => pullsSetMock(...args),
      },
      wiki: {
        setEnabled: (...args: unknown[]) => wikiSetMock(...args),
      },
      projects: {
        setEnabled: (...args: unknown[]) => projectsSetMock(...args),
      },
      discussions: {
        setEnabled: (...args: unknown[]) => discussionsSetMock(...args),
      },
    },
  },
}));

vi.mock("@/lib/toast", () => ({
  toastSuccess: vi.fn(),
}));

const repo: RepoPublic = {
  id: "r1",
  owner_id: "u1",
  owner_type: "user",
  owner_username: "ada",
  name: "hello",
  description: "",
  visibility: "public",
  default_branch: "main",
  updated_at: "2026-10-02T00:00:00Z",
  can_admin: true,
  can_write: true,
} as RepoPublic;

async function waitForTestId(testId: string, ms = 10_000): Promise<Element> {
  const deadline = Date.now() + ms;
  while (Date.now() < deadline) {
    const el = document.querySelector(`[data-testid="${testId}"]`);
    if (el) return el;
    await new Promise((r) => setTimeout(r, 50));
  }
  throw new Error(`[data-testid="${testId}"] never appeared. ${debugBody()}`);
}

beforeEach(() => {
  issuesSetMock.mockReset();
  pullsSetMock.mockReset();
  wikiSetMock.mockReset();
  projectsSetMock.mockReset();
  discussionsSetMock.mockReset();
});

afterEach(async () => {
  await cleanupBrowserMount();
});

describe("RepoFeaturesPanel browser DOM races", () => {
  it("issues/pulls switches call setEnabled and do not throw DOM races", async () => {
    const tracker = trackDomErrors();
    try {
      issuesSetMock.mockResolvedValue({ ok: true, data: { enabled: false } });
      pullsSetMock.mockResolvedValue({ ok: true, data: { enabled: false } });
      wikiSetMock.mockResolvedValue({ ok: true, data: { enabled: false } });
      projectsSetMock.mockResolvedValue({ ok: true, data: { enabled: false } });
      discussionsSetMock.mockResolvedValue({ ok: true, data: { enabled: false } });

      await mountWithQueryClient(RepoFeaturesPanel, {
        owner: "ada",
        name: "hello",
        can_admin: true,
        repo,
      });

      await waitForTestId("repo-features-settings");
      await waitForTestId("repo-issues-enabled");
      await waitForTestId("repo-pulls-enabled");
      await waitForTestId("repo-wiki-enabled");
      await waitForTestId("repo-projects-enabled");
      await waitForTestId("repo-discussions-enabled");

      await clickAriaLabel("Enable Issues");
      const issuesDeadline = Date.now() + 5_000;
      while (issuesSetMock.mock.calls.length === 0 && Date.now() < issuesDeadline) {
        await new Promise((r) => setTimeout(r, 50));
      }
      expect(issuesSetMock).toHaveBeenCalledWith({
        owner: "ada",
        name: "hello",
        enabled: false,
      });

      await clickAriaLabel("Enable pull requests");
      const pullsDeadline = Date.now() + 5_000;
      while (pullsSetMock.mock.calls.length === 0 && Date.now() < pullsDeadline) {
        await new Promise((r) => setTimeout(r, 50));
      }
      expect(pullsSetMock).toHaveBeenCalledWith({
        owner: "ada",
        name: "hello",
        enabled: false,
      });

      await clickAriaLabel("Enable wiki");
      const wikiDeadline = Date.now() + 5_000;
      while (wikiSetMock.mock.calls.length === 0 && Date.now() < wikiDeadline) {
        await new Promise((r) => setTimeout(r, 50));
      }
      expect(wikiSetMock).toHaveBeenCalledWith({
        owner: "ada",
        name: "hello",
        enabled: false,
      });

      await clickAriaLabel("Enable projects");
      const projectsDeadline = Date.now() + 5_000;
      while (projectsSetMock.mock.calls.length === 0 && Date.now() < projectsDeadline) {
        await new Promise((r) => setTimeout(r, 50));
      }
      expect(projectsSetMock).toHaveBeenCalledWith({
        owner: "ada",
        name: "hello",
        enabled: false,
      });

      await clickAriaLabel("Enable discussions");
      const discussionsDeadline = Date.now() + 5_000;
      while (discussionsSetMock.mock.calls.length === 0 && Date.now() < discussionsDeadline) {
        await new Promise((r) => setTimeout(r, 50));
      }
      expect(discussionsSetMock).toHaveBeenCalledWith({
        owner: "ada",
        name: "hello",
        enabled: false,
      });

      tracker.expectNoDomRaces();
      expectNoOctaneOverlayInDocument();
    } finally {
      tracker.dispose();
    }
  }, 30_000);

  it("error response surfaces role=alert without throwing", async () => {
    const tracker = trackDomErrors();
    try {
      issuesSetMock.mockResolvedValue({
        ok: false,
        error: { code: "repo.forbidden", message: "nope" },
      });
      await mountWithQueryClient(RepoFeaturesPanel, {
        owner: "ada",
        name: "hello",
        can_admin: true,
        repo,
      });
      await waitForTestId("repo-issues-enabled");
      await clickAriaLabel("Enable Issues");
      const deadline = Date.now() + 5_000;
      let alert: Element | null = null;
      while (!alert && Date.now() < deadline) {
        alert = document.querySelector('[role="alert"]');
        await new Promise((r) => setTimeout(r, 50));
      }
      expect(alert?.textContent).toContain("nope");
      tracker.expectNoDomRaces();
    } finally {
      tracker.dispose();
    }
  }, 30_000);
});
