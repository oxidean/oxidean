/**
 * Chromium proof for ComposerExtras — saved-replies dropdown (list/insert/
 * delete) and the attachment upload path must mount and interact in a real DOM
 * without Octane overlay/DOM races.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ComposerExtras } from "@/components/repo/pull-v2/pull-v2-composer-extras";
import {
  clickTestId,
  debugBody,
  expectNoOctaneOverlayInDocument,
  mountWithQueryClient,
} from "@/test/browser-mount";
import { trackDomErrors } from "@/test/dom-errors";
import { act } from "octane";

const listMock = vi.fn();
const createMock = vi.fn();
const deleteMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    savedReplies: {
      list: (...args: unknown[]) => listMock(...args),
      create: (...args: unknown[]) => createMock(...args),
      delete: (...args: unknown[]) => deleteMock(...args),
    },
  },
}));

const REPLIES = [
  {
    id: "r1",
    name: "Friendly nudge",
    body: "Thanks for the PR — could you add a test?",
    created_at: "2026-03-04T10:00:00Z",
    updated_at: "2026-03-04T10:00:00Z",
  },
];

async function waitForTestId(testId: string, ms = 3_000): Promise<Element> {
  const deadline = Date.now() + ms;
  for (;;) {
    const el = document.querySelector(`[data-testid="${testId}"]`);
    if (el) return el;
    if (Date.now() > deadline) {
      throw new Error(`waitForTestId: [data-testid="${testId}"] never attached. ${debugBody()}`);
    }
    await new Promise((r) => setTimeout(r, 25));
  }
}

beforeEach(() => {
  listMock.mockReset();
  createMock.mockReset();
  deleteMock.mockReset();
  listMock.mockResolvedValue({ ok: true, data: { replies: REPLIES } });
  deleteMock.mockResolvedValue({ ok: true, data: { id: "r1" } });
});

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("pull-v2 composer extras browser DOM", () => {
  it("lists saved replies, inserts one into the draft, and deletes it", async () => {
    const tracker = trackDomErrors();
    try {
      const onValueChange = vi.fn();
      await mountWithQueryClient(ComposerExtras, {
        value: "existing draft",
        onValueChange,
      });

      await clickTestId("pull-v2-saved-replies");
      await waitForTestId("pull-v2-reply-r1");

      const replyItem = await waitForTestId("pull-v2-reply-r1");
      await act(async () => {
        (replyItem as HTMLElement).click();
      });
      expect(onValueChange).toHaveBeenCalledWith(
        "existing draft\n\nThanks for the PR — could you add a test?",
      );

      // Reopen and delete the reply.
      await clickTestId("pull-v2-saved-replies");
      await waitForTestId("pull-v2-reply-delete-r1");
      await clickTestId("pull-v2-reply-delete-r1");
      await vi.waitFor(() => {
        expect(deleteMock).toHaveBeenCalledWith({ id: "r1" });
      });

      expectNoOctaneOverlayInDocument();
    } finally {
      tracker.dispose();
    }
  });

  it("uploads an attachment and inserts the returned markdown image", async () => {
    const tracker = trackDomErrors();
    try {
      const fetchMock = vi.fn().mockResolvedValue({
        ok: true,
        json: async () => ({
          ok: true,
          data: {
            id: "u1",
            url: "/api/uploads/u1",
            name: "badge.png",
            content_type: "image/png",
            size: 4,
          },
        }),
      });
      vi.stubGlobal("fetch", fetchMock);

      const onValueChange = vi.fn();
      await mountWithQueryClient(ComposerExtras, { value: "", onValueChange });

      const input = (await waitForTestId("pull-v2-attach-input")) as HTMLInputElement;
      const dt = new DataTransfer();
      dt.items.add(new File(["png!"], "badge.png", { type: "image/png" }));
      input.files = dt.files;
      input.dispatchEvent(new Event("change", { bubbles: true }));

      await vi.waitFor(() => {
        expect(onValueChange).toHaveBeenCalledWith("![badge.png](/api/uploads/u1)");
      });
      expect(fetchMock).toHaveBeenCalledWith(
        expect.stringContaining("/api/uploads/comment-attachments"),
        expect.objectContaining({ method: "POST" }),
      );
      expectNoOctaneOverlayInDocument();
    } finally {
      tracker.dispose();
    }
  });

  it("creates a saved reply through the multiline dialog", async () => {
    const tracker = trackDomErrors();
    try {
      createMock.mockResolvedValue({
        ok: true,
        data: {
          id: "r2",
          name: "Repro steps",
          body: "line one\n\n- detail",
          created_at: "2026-03-04T10:00:00Z",
          updated_at: "2026-03-04T10:00:00Z",
        },
      });
      await mountWithQueryClient(ComposerExtras, { value: "", onValueChange: vi.fn() });

      await clickTestId("pull-v2-saved-replies");
      await waitForTestId("pull-v2-reply-new");
      await clickTestId("pull-v2-reply-new");
      // DialogPortal is keepMounted — the popup stays in the DOM and toggles
      // `hidden`, so assert visibility rather than attachment.
      await vi.waitFor(() => {
        const dlg = document.querySelector(
          '[data-testid="pull-v2-reply-create-dialog"]',
        ) as HTMLElement | null;
        expect(dlg).not.toBeNull();
        expect(dlg?.hidden).toBe(false);
      });

      const setField = (testId: string, value: string) => {
        const el = document.querySelector(`[data-testid="${testId}"]`);
        if (!el) throw new Error(`field ${testId} missing. ${debugBody()}`);
        (el as HTMLInputElement | HTMLTextAreaElement).value = value;
        el.dispatchEvent(new InputEvent("input", { bubbles: true }));
      };
      await act(async () => {
        setField("pull-v2-reply-name-input", "Repro steps");
        setField("pull-v2-reply-body-input", "line one\n\n- detail");
      });

      await clickTestId("pull-v2-reply-create-save");
      await vi.waitFor(() => {
        expect(createMock).toHaveBeenCalledWith({
          name: "Repro steps",
          body: "line one\n\n- detail",
        });
      });
      await vi.waitFor(() => {
        const dlg = document.querySelector(
          '[data-testid="pull-v2-reply-create-dialog"]',
        ) as HTMLElement | null;
        expect(dlg === null || dlg.hidden).toBe(true);
      });
      expectNoOctaneOverlayInDocument();
    } finally {
      tracker.dispose();
    }
  });

  it("shows a sign-in hint instead of the picker when saved replies are unauthenticated", async () => {
    const tracker = trackDomErrors();
    try {
      listMock.mockResolvedValue({
        ok: false,
        error: { code: "auth.unauthenticated", message: "sign in" },
      });
      await mountWithQueryClient(ComposerExtras, { value: "", onValueChange: vi.fn() });

      await clickTestId("pull-v2-saved-replies");
      await waitForTestId("pull-v2-reply-new");
      await vi.waitFor(() => {
        expect(document.body.textContent).toContain("Sign in to use saved replies");
      });
      expectNoOctaneOverlayInDocument();
    } finally {
      tracker.dispose();
    }
  });
});
