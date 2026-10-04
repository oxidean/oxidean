/**
 * Chromium gate for the Actions "Re-run" DropdownMenu — happy-dom cannot
 * prove the portal/menu click path is race-free (issue #44).
 */
import { afterEach, describe, expect, it, vi } from "vitest";
import { act } from "octane";
import { ActionsRerunMenu } from "@/components/repo/actions-rerun-menu";
import {
  cleanupBrowserMount,
  debugBody,
  expectNoOctaneOverlayInDocument,
  mountComponent,
} from "@/test/browser-mount";
import { trackDomErrors } from "@/test/dom-errors";

const JOBS = [
  {
    id: "job-1",
    run_id: "run-1",
    job_key: "build",
    name: "build",
    status: "failure",
    runs_on: [],
  },
  {
    id: "job-2",
    run_id: "run-1",
    job_key: "test",
    name: "test",
    status: "success",
    runs_on: [],
  },
];

async function waitForTestId(testId: string, ms = 5_000): Promise<Element> {
  const deadline = Date.now() + ms;
  while (Date.now() < deadline) {
    const el = document.querySelector(`[data-testid="${testId}"]`);
    if (el) return el;
    await new Promise((r) => setTimeout(r, 50));
  }
  throw new Error(`[data-testid="${testId}"] never appeared. ${debugBody()}`);
}

async function openMenu(): Promise<void> {
  const trigger = await waitForTestId("actions-rerun-menu");
  for (let attempt = 0; attempt < 2; attempt++) {
    await act(async () => {
      (trigger as HTMLElement).click();
    });
    const deadline = Date.now() + 2_000;
    while (Date.now() < deadline) {
      if (trigger.hasAttribute("data-popup-open")) return;
      await new Promise((r) => setTimeout(r, 50));
    }
  }
  throw new Error("rerun menu trigger never reported data-popup-open");
}

afterEach(async () => {
  await cleanupBrowserMount();
});

describe("ActionsRerunMenu browser DOM races", () => {
  it("menu opens and re-run items call back with the right target", async () => {
    const tracker = trackDomErrors();
    const onRerun = vi.fn();
    try {
      await mountComponent(ActionsRerunMenu, { jobs: JOBS, onRerun });

      await openMenu();
      await act(async () => {
        (await waitForTestId("actions-rerun-all")).dispatchEvent(
          new MouseEvent("click", { bubbles: true }),
        );
      });
      expect(onRerun).toHaveBeenCalledWith({});

      await openMenu();
      await act(async () => {
        (await waitForTestId("actions-rerun-failed")).dispatchEvent(
          new MouseEvent("click", { bubbles: true }),
        );
      });
      expect(onRerun).toHaveBeenCalledWith({ failed_only: true });

      await openMenu();
      await act(async () => {
        (await waitForTestId("actions-rerun-job-build")).dispatchEvent(
          new MouseEvent("click", { bubbles: true }),
        );
      });
      expect(onRerun).toHaveBeenCalledWith({ job_id: "job-1" });

      tracker.expectNoDomRaces();
      expectNoOctaneOverlayInDocument();
    } finally {
      tracker.dispose();
    }
  }, 30_000);

  it("re-run failed is disabled when no job failed", async () => {
    const onRerun = vi.fn();
    await mountComponent(ActionsRerunMenu, {
      jobs: [{ ...JOBS[1] }],
      onRerun,
    });
    await openMenu();
    const failed = await waitForTestId("actions-rerun-failed");
    expect(failed.hasAttribute("disabled") || failed.getAttribute("aria-disabled") === "true").toBe(
      true,
    );
    expectNoOctaneOverlayInDocument();
  }, 30_000);
});
