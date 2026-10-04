/**
 * Chromium gate for the GIT-19 web file-editing commit form (RadioGroup +
 * hidden-toggled branch panel — DOM races) and the "Add file" DropdownMenu on
 * the repo browse toolbar.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { FileCommitForm } from "@/components/repo/file-commit-form";
import { RepoBrowseToolbar } from "@/components/repo/repo-browse-toolbar";
import {
  cleanupBrowserMount,
  clickTestId,
  debugBody,
  expectNoOctaneOverlayInDocument,
  mountWithQueryClient,
} from "@/test/browser-mount";
import { trackDomErrors } from "@/test/dom-errors";

// CloneBox (mounted inside the toolbar) reads process.env — absent under
// Chromium. Stub the origin helpers it calls.
vi.mock("@/lib/public-origin", () => ({
  resolvePublicOriginClient: () => "http://localhost:3000",
  resolveSshHost: () => "localhost",
  resolveSshPort: () => 22,
  httpsCloneUrl: (origin: string, owner: string, repo: string) => `${origin}/${owner}/${repo}.git`,
  sshCloneUrl: (host: string, _port: number, owner: string, repo: string) =>
    `git@${host}:${owner}/${repo}.git`,
  sshNeedsPortHint: () => false,
}));

async function waitForTestId(testId: string, ms = 10_000): Promise<Element> {
  const deadline = Date.now() + ms;
  while (Date.now() < deadline) {
    const el = document.querySelector(`[data-testid="${testId}"]`);
    if (el) return el;
    await new Promise((r) => setTimeout(r, 50));
  }
  throw new Error(`[data-testid="${testId}"] never appeared. ${debugBody()}`);
}

function branchPanel(): HTMLElement {
  const el = document.querySelector<HTMLElement>('[data-testid="file-commit-branch-panel"]');
  if (!el) throw new Error("branch panel missing");
  return el;
}

function panelVisible(): boolean {
  return !branchPanel().classList.contains("hidden");
}

describe("FileCommitForm browser DOM races", () => {
  beforeEach(() => {});
  afterEach(async () => {
    await cleanupBrowserMount();
  });

  it("radio toggles the branch panel via hidden — no insertBefore races", async () => {
    const tracker = trackDomErrors();
    const onSubmit = vi.fn();
    try {
      await mountWithQueryClient(FileCommitForm, {
        policy: { branch: "main", direct_commit_allowed: true, requires_pr: false },
        policyPending: false,
        baseBranch: "main",
        pending: false,
        error: "",
        submitLabel: "Commit changes",
        cancelHref: "/a/b/tree/main",
        onSubmit,
      });

      await waitForTestId("file-commit-form");
      await waitForTestId("file-commit-mode");
      await waitForTestId("file-commit-branch-panel");
      expect(panelVisible()).toBe(false);

      await clickTestId("file-commit-mode-branch");
      await new Promise((r) => setTimeout(r, 50));
      expect(panelVisible()).toBe(true);

      await clickTestId("file-commit-mode-direct");
      await new Promise((r) => setTimeout(r, 50));
      expect(panelVisible()).toBe(false);

      // back to branch + submit → onSubmit receives branch mode
      await clickTestId("file-commit-mode-branch");
      await new Promise((r) => setTimeout(r, 50));
      await clickTestId("file-commit-submit");
      const deadline = Date.now() + 5_000;
      while (Date.now() < deadline && onSubmit.mock.calls.length === 0) {
        await new Promise((r) => setTimeout(r, 50));
      }
      expect(onSubmit).toHaveBeenCalled();
      expect(onSubmit.mock.calls[0]?.[0]?.mode).toBe("branch");

      tracker.expectNoDomRaces();
      expectNoOctaneOverlayInDocument();
    } finally {
      tracker.dispose();
    }
  }, 30_000);

  it("protected policy forces branch mode and disables direct", async () => {
    const tracker = trackDomErrors();
    try {
      await mountWithQueryClient(FileCommitForm, {
        policy: { branch: "main", direct_commit_allowed: false, requires_pr: true },
        policyPending: false,
        baseBranch: "main",
        pending: false,
        error: "",
        submitLabel: "Commit changes",
        cancelHref: "/a/b/tree/main",
        onSubmit: vi.fn(),
      });

      await waitForTestId("file-commit-form");
      const direct = await waitForTestId("file-commit-mode-direct");
      expect(
        (direct as HTMLElement).getAttribute("aria-disabled") === "true" ||
          (direct as HTMLButtonElement).disabled,
      ).toBe(true);
      // mode forced to branch — panel visible without a click.
      await new Promise((r) => setTimeout(r, 100));
      expect(panelVisible()).toBe(true);

      tracker.expectNoDomRaces();
      expectNoOctaneOverlayInDocument();
    } finally {
      tracker.dispose();
    }
  }, 30_000);
});

describe("RepoBrowseToolbar Add file menu", () => {
  afterEach(async () => {
    await cleanupBrowserMount();
  });

  it("opens the dropdown without DOM races and lists file actions", async () => {
    const tracker = trackDomErrors();
    try {
      await mountWithQueryClient(RepoBrowseToolbar, {
        owner: "ada",
        repo: "hello",
        refName: "main",
        refs: [],
        path: "src",
        showPathCrumbs: true,
        canWrite: true,
      });

      await waitForTestId("add-file-menu");
      await clickTestId("add-file-menu");
      await waitForTestId("add-file-new");
      expect(document.querySelector('[data-testid="add-file-mkdir"]')).toBeTruthy();
      expect(document.querySelector('[data-testid="add-file-upload"]')).toBeTruthy();

      tracker.expectNoDomRaces();
      expectNoOctaneOverlayInDocument();
    } finally {
      tracker.dispose();
    }
  }, 30_000);
});
