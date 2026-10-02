/**
 * Chromium gate for the webhook event Checkboxes — happy-dom cannot catch
 * Octane insertBefore races on the events fieldset. Covers DEBT-04's added
 * `issue_comment` option, the API-04 event breadth (`release`, `star`,
 * `fork`, `create`, `delete`, `workflow_run`, `registry_package`), and the
 * hosting panel (webhooks-panel.tsrx).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { WebhooksPanel } from "@/components/repo/webhooks-panel";
import {
  cleanupBrowserMount,
  clickAriaLabel,
  clickTestId,
  debugBody,
  expectNoOctaneOverlayInDocument,
  mountWithQueryClient,
} from "@/test/browser-mount";
import { trackDomErrors } from "@/test/dom-errors";

const webhookListMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    webhook: {
      list: (...args: unknown[]) => webhookListMock(...args),
      create: vi.fn(),
      update: vi.fn(),
      delete: vi.fn(),
    },
  },
}));

vi.mock("@/lib/toast", () => ({
  toastSuccess: vi.fn(),
}));

beforeEach(() => {
  webhookListMock.mockReset();
  webhookListMock.mockResolvedValue({ ok: true, data: { webhooks: [] } });
});

afterEach(async () => {
  await cleanupBrowserMount();
});

async function waitForTestId(testId: string, ms = 10_000): Promise<Element> {
  const deadline = Date.now() + ms;
  while (Date.now() < deadline) {
    const el = document.querySelector(`[data-testid="${testId}"]`);
    if (el) return el;
    await new Promise((r) => setTimeout(r, 50));
  }
  throw new Error(`[data-testid="${testId}"] never appeared. ${debugBody()}`);
}

describe("WebhooksPanel + WebhookForm browser DOM races", () => {
  it("create flow renders event checkboxes incl. API-04 events without insertBefore races", async () => {
    const tracker = trackDomErrors();
    try {
      await mountWithQueryClient(WebhooksPanel, {
        owner: "ada",
        name: "hello",
        can_admin: true,
      });

      await waitForTestId("repo-webhooks-settings");
      await clickTestId("webhook-add");
      await waitForTestId("webhook-form");

      for (const label of [
        "push",
        "pull_request",
        "issues",
        "issue_comment",
        "release",
        "star",
        "fork",
        "create",
        "delete",
        "workflow_run",
        "registry_package",
      ]) {
        expect(document.querySelector(`[aria-label="${label}"]`)).toBeTruthy();
      }

      // Toggle a DEBT-04 event and an API-04 event off and back on; the
      // events map re-renders the Checkbox state without an Octane DOM race.
      await clickAriaLabel("issue_comment");
      await clickAriaLabel("issue_comment");
      await clickAriaLabel("workflow_run");
      await clickAriaLabel("push");

      tracker.expectNoDomRaces();
      expectNoOctaneOverlayInDocument();
    } finally {
      tracker.dispose();
    }
  });
});
