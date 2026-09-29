/**
 * Chromium proof for Checkbox / Radio / Select / Switch beside sibling panels.
 */
import { afterEach, describe, expect, it } from "vitest";
import { UiControlsBrowserHarness } from "@/components/ui/ui-controls.browser-harness";
import {
  cleanupBrowserMount,
  clickTestId,
  debugBody,
  expectNoOctaneOverlayInDocument,
  mountComponent,
  pickSelectOptionByTestId,
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

  it("Select option pick commits via pointerdown+click without overlay", async () => {
    const tracker = trackDomErrors();
    try {
      await mountComponent(UiControlsBrowserHarness);

      await pickSelectOptionByTestId("ui-harness-select", "ui-harness-select-two");

      const value = document.querySelector('[data-testid="ui-harness-select-value"]');
      expect(value?.textContent).toBe("two");
      tracker.expectNoDomRaces();
      expectNoOctaneOverlayInDocument();
    } finally {
      tracker.dispose();
    }
  }, 30_000);

  it("Switch toggle does not throw insertBefore / overlay", async () => {
    const tracker = trackDomErrors();
    try {
      await mountComponent(UiControlsBrowserHarness);

      await clickTestId("ui-harness-switch");
      expect(document.querySelector('[data-testid="ui-harness-switch-value"]')?.textContent).toBe(
        "on",
      );
      await clickTestId("ui-harness-switch");
      expect(document.querySelector('[data-testid="ui-harness-switch-value"]')?.textContent).toBe(
        "off",
      );
      tracker.expectNoDomRaces();
      expectNoOctaneOverlayInDocument();
    } finally {
      tracker.dispose();
    }
  }, 30_000);
});
