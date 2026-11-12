/**
 * Chromium proof for the pull-v2 review bar — RadioGroup option picks and the
 * disabled-author path must not race sibling re-renders (Base UI Indicator
 * mount race class of bug). The pending-draft list mounts/unmounts a sibling
 * subtree on top of the same radios.
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

const PENDING = [
  {
    id: "pc1",
    pull_id: "p1",
    author_id: "u1",
    author_username: "ada",
    body: "draft note on main",
    path: "src/main.rs",
    side: "RIGHT",
    line: 3,
    pending: true,
    resolved: false,
    outdated: false,
    created_at: "2026-03-04T10:00:00Z",
    updated_at: "2026-03-04T10:00:00Z",
  },
  {
    id: "pc2",
    pull_id: "p1",
    author_id: "u1",
    author_username: "ada",
    body: "second draft",
    path: "docs/README.md",
    side: "RIGHT",
    line: 1,
    start_line: 1,
    pending: true,
    resolved: false,
    outdated: false,
    created_at: "2026-03-04T10:05:00Z",
    updated_at: "2026-03-04T10:05:00Z",
  },
];

describe("pull-v2 review bar browser DOM races", () => {
  it("switching review states keeps the bar mounted without insertBefore races", async () => {
    const tracker = trackDomErrors();
    try {
      const onSubmit = vi.fn();
      await mountComponent(PullV2ReviewBar, {
        pending: PENDING,
        isAuthor: false,
        busy: false,
        error: null,
        onSubmit,
        onDiscardPending: vi.fn(),
      });

      const bar = document.querySelector('[data-testid="pull-v2-review-bar"]');
      if (!bar) throw new Error(`review bar not mounted. ${debugBody()}`);

      // Two pending items list under the header.
      const items = document.querySelectorAll('[data-testid="pull-v2-review-pending-item"]');
      expect(items.length).toBe(2);

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
        pending: [],
        isAuthor: true,
        busy: false,
        error: null,
        onSubmit: vi.fn(),
        onDiscardPending: vi.fn(),
      });
      const approve = document.querySelector('[data-testid="pull-v2-review-approve"]');
      if (!approve) throw new Error(`approve radio missing. ${debugBody()}`);
      expect(approve.closest("[data-disabled]")).not.toBeNull();
      expectNoOctaneOverlayInDocument();
    } finally {
      tracker.dispose();
    }
  });

  it("discards a pending draft via its trash button", async () => {
    const tracker = trackDomErrors();
    try {
      const onDiscard = vi.fn();
      await mountComponent(PullV2ReviewBar, {
        pending: PENDING,
        isAuthor: false,
        busy: false,
        error: null,
        onSubmit: vi.fn(),
        onDiscardPending: onDiscard,
      });
      await clickTestId("pull-v2-review-discard-pending");
      expect(onDiscard).toHaveBeenCalledWith("pc1");
      expectNoOctaneOverlayInDocument();
    } finally {
      tracker.dispose();
    }
  });
});
