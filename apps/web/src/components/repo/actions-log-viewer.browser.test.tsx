/**
 * Chromium mount/interaction proof for the step-aware Actions log viewer —
 * step collapse, search filtering, and the raw toggle (issue #44).
 */
import { afterEach, describe, expect, it } from "vitest";
import { act } from "octane";
import { ActionsLogViewer } from "@/components/repo/actions-log-viewer";
import {
  cleanupBrowserMount,
  clickTestId,
  debugBody,
  expectNoOctaneOverlayInDocument,
  mountComponent,
} from "@/test/browser-mount";
import { trackDomErrors } from "@/test/dom-errors";

const LOG = [
  "Job test — ci / 2 step(s) [runner]",
  "##[step]actions/checkout@v4",
  "Cloning ada/hello",
  "checkout ok",
  "##[step-done]actions/checkout@v4",
  "##[step]marker",
  "oxidean-stack-hello",
  "##[step-done]marker",
].join("\n");

async function waitForTestId(testId: string, ms = 5_000): Promise<Element> {
  const deadline = Date.now() + ms;
  while (Date.now() < deadline) {
    const el = document.querySelector(`[data-testid="${testId}"]`);
    if (el) return el;
    await new Promise((r) => setTimeout(r, 50));
  }
  throw new Error(`[data-testid="${testId}"] never appeared. ${debugBody()}`);
}

afterEach(async () => {
  await cleanupBrowserMount();
});

describe("ActionsLogViewer browser mount", () => {
  it("renders step rail + lines; search filters; collapse + raw toggle work", async () => {
    const tracker = trackDomErrors();
    try {
      await mountComponent(ActionsLogViewer, {
        log: LOG,
        jobStatus: "success",
        downloadName: "test",
      });
      await waitForTestId("actions-log-viewer");
      await waitForTestId("actions-log-steps");

      // The streamed marker text is visible inside the step body.
      const pane = await waitForTestId("repo-actions-job-log");
      expect(pane.textContent).toContain("oxidean-stack-hello");

      // Step rail lists both steps.
      const steps = document.querySelector('[data-testid="actions-log-steps"]');
      expect(steps?.textContent).toContain("actions/checkout@v4");
      expect(steps?.textContent).toContain("marker");

      // Collapse the first step: its body lines hide.
      await clickTestId("actions-log-step-2");
      expect(pane.textContent).not.toContain("Cloning ada/hello");
      await clickTestId("actions-log-step-2");
      expect(pane.textContent).toContain("Cloning ada/hello");

      // Search narrows to matching lines only.
      const search = await waitForTestId("actions-log-search");
      await act(async () => {
        const input = search as HTMLInputElement;
        input.focus();
        input.value = "oxidean-stack";
        input.dispatchEvent(new Event("input", { bubbles: true }));
      });
      expect(pane.textContent).toContain("oxidean-stack-hello");
      expect(pane.textContent).not.toContain("Cloning ada/hello");

      // Clear search, then raw view shows the unmodified blob.
      await act(async () => {
        const input = search as HTMLInputElement;
        input.value = "";
        input.dispatchEvent(new Event("input", { bubbles: true }));
      });
      await clickTestId("actions-log-raw");
      expect(pane.textContent).toContain("##[step]actions/checkout@v4");

      tracker.expectNoDomRaces();
      expectNoOctaneOverlayInDocument();
    } finally {
      tracker.dispose();
    }
  }, 30_000);
});
