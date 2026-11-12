/**
 * Chromium proof for the pull-v2 review bar — RadioGroup option picks and the
 * disabled-author path must not race sibling re-renders (Base UI Indicator
 * mount race class of bug).
 */
import { describe, expect, it, vi } from "vitest";
import { PullV2ReviewBar } from "@/components/repo/pull-v2/pull-v2-review-bar";
import {
  clickTestId,
  debugBody,
  expectNoOctaneOverlayInDocument,
  mountComponent,
} from "@/test/browser-mount";
import { trackDomErrors } from "@/test/dom-errors";

describe("pull-v2 review bar browser DOM races", () => {
  it("switching review states keeps the bar mounted without insertBefore races", async () => {
    const tracker = trackDomErrors();
    try {
      const onSubmit = vi.fn();
      await mountComponent(PullV2ReviewBar, {
        pendingCount: 2,
        isAuthor: false,
        busy: false,
        error: null,
        onSubmit,
      });

      const bar = document.querySelector('[data-testid="pull-v2-review-bar"]');
      if (!bar) throw new Error(`review bar not mounted. ${debugBody()}`);

      // Comment → Approve → Request-changes → back to Comment: each pick
      // mounts/unmounts the RadioGroup indicator beside a re-rendering label.
      await clickTestId("pull-v2-review-approve");
      await clickTestId("pull-v2-review-request-changes");
      await clickTestId("pull-v2-review-comment");
      expect(document.querySelector('[data-testid="pull-v2-review-bar"]')).not.toBeNull();

      await clickTestId("pull-v2-review-submit");
      expect(onSubmit).toHaveBeenCalledWith("commented", "");

      expectNoOctaneOverlayInDocument();
    } finally {
      tracker.dispose();
    }
  });

  it("author gating disables approve/request-changes without remounting", async () => {
    const tracker = trackDomErrors();
    try {
      await mountComponent(PullV2ReviewBar, {
        pendingCount: 0,
        isAuthor: true,
        busy: false,
        error: null,
        onSubmit: vi.fn(),
      });
      const approve = document.querySelector('[data-testid="pull-v2-review-approve"]');
      if (!approve) throw new Error(`approve radio missing. ${debugBody()}`);
      expect(approve.closest("[data-disabled]")).not.toBeNull();
      expectNoOctaneOverlayInDocument();
    } finally {
      tracker.dispose();
    }
  });
});
