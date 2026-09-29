/**
 * Chromium proof that Checkbox / Radio Indicators with keepMounted do not throw
 * insertBefore when toggled beside sibling panels (issues #41–#43).
 */
import { afterEach, describe, expect, it } from "vitest";
import { UiControlsBrowserHarness } from "@/components/ui/ui-controls.browser-harness";
import {
  cleanupBrowserMount,
  clickTestId,
  debugBody,
  expectNoOctaneOverlayInDocument,
  mountComponent,
} from "@/test/browser-mount";
import { trackDomErrors } from "@/test/dom-errors";

afterEach(async () => {
  await cleanupBrowserMount();
});

describe("UI controls browser DOM races", () => {
  it("checkbox + radio toggles do not throw insertBefore / overlay", async () => {
    const tracker = trackDomErrors();
    try {
      await mountComponent(UiControlsBrowserHarness);

      const root = document.querySelector('[data-testid="ui-controls-harness"]');
      if (!root) {
        throw new Error(`ui-controls-harness not mounted. ${debugBody()}`);
      }

      await clickTestId("ui-harness-checkbox");
      await clickTestId("ui-harness-checkbox");
      await clickTestId("ui-harness-radio-b");
      await clickTestId("ui-harness-radio-a");
      await clickTestId("ui-harness-radio-b");

      expect(document.querySelector('[data-testid="ui-harness-panel-b"]')).toBeTruthy();
      tracker.expectNoDomRaces();
      expectNoOctaneOverlayInDocument();
    } finally {
      tracker.dispose();
    }
  }, 30_000);
});
