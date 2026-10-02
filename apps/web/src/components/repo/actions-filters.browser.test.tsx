/**
 * Chromium gate for the Actions run filters bar — two Base UI Selects +
 * submit-applied text inputs (issue #44).
 */
import { afterEach, describe, expect, it } from "vitest";
import { act } from "octane";
import {
  ActionsFiltersBar,
  EMPTY_FILTERS,
  type ActionsFilterDraft,
} from "@/components/repo/actions-filters";
import {
  cleanupBrowserMount,
  debugBody,
  expectNoOctaneOverlayInDocument,
  mountComponent,
  pickSelectOptionByTestId,
} from "@/test/browser-mount";
import { trackDomErrors } from "@/test/dom-errors";

async function waitForTestId(testId: string, ms = 5_000): Promise<Element> {
  const deadline = Date.now() + ms;
  while (Date.now() < deadline) {
    const el = document.querySelector(`[data-testid="${testId}"]`);
    if (el) return el;
    await new Promise((r) => setTimeout(r, 50));
  }
  throw new Error(`[data-testid="${testId}"] never appeared. ${debugBody()}`);
}

async function typeIntoTestId(testId: string, value: string): Promise<void> {
  const el = await waitForTestId(testId);
  await act(async () => {
    const input = el as HTMLInputElement;
    input.focus();
    input.value = value;
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

afterEach(async () => {
  await cleanupBrowserMount();
});

describe("ActionsFiltersBar browser DOM races", () => {
  it("status/event selects apply immediately; text inputs apply on submit", async () => {
    const tracker = trackDomErrors();
    const applied: ActionsFilterDraft[] = [];
    try {
      await mountComponent(ActionsFiltersBar, {
        filters: EMPTY_FILTERS,
        onApply: (next: ActionsFilterDraft) => applied.push(next),
      });
      await waitForTestId("actions-filters");

      await pickSelectOptionByTestId("actions-filter-status", "actions-filter-status-failure");
      expect(applied.at(-1)?.status).toBe("failure");

      await pickSelectOptionByTestId("actions-filter-event", "actions-filter-event-push");
      expect(applied.at(-1)?.event).toBe("push");

      await typeIntoTestId("actions-filter-branch", "main");
      await typeIntoTestId("actions-filter-query", "nightly");
      const submit = await waitForTestId("actions-filter-apply");
      await act(async () => {
        (submit as HTMLElement).click();
      });
      const last = applied.at(-1);
      expect(last?.branch).toBe("main");
      expect(last?.q).toBe("nightly");

      tracker.expectNoDomRaces();
      expectNoOctaneOverlayInDocument();
    } finally {
      tracker.dispose();
    }
  }, 30_000);
});
