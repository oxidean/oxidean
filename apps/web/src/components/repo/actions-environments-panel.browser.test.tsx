/**
 * Chromium gate for the environments panel — EnvEditor mount/dismount cycles,
 * the prevent-self-review Checkbox, and per-env expand secrets.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ActionsEnvironmentsPanel } from "@/components/repo/actions-environments-panel";
import {
  cleanupBrowserMount,
  clickTestId,
  debugBody,
  expectNoOctaneOverlayInDocument,
  mountWithQueryClient,
} from "@/test/browser-mount";
import { trackDomErrors } from "@/test/dom-errors";

const envsMock = vi.fn();
const secretsMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    repo: {
      actions: {
        environments: {
          list: (...args: unknown[]) => envsMock(...args),
          secretsList: (...args: unknown[]) => secretsMock(...args),
          upsert: vi.fn(async () => ({ ok: true, data: {} })),
          delete: vi.fn(async () => ({ ok: true, data: {} })),
          secretsPut: vi.fn(async () => ({ ok: true, data: {} })),
          secretsDelete: vi.fn(async () => ({ ok: true, data: {} })),
        },
      },
    },
  },
}));

const ENV = {
  name: "staging",
  protection: {
    wait_timer_minutes: 5,
    required_reviewers: ["ada"],
    prevent_self_review: false,
    deployment_branches: [],
  },
};

beforeEach(() => {
  envsMock.mockReset().mockResolvedValue({ ok: true, data: { environments: [ENV] } });
  secretsMock.mockReset().mockResolvedValue({ ok: true, data: { secrets: [] } });
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

describe("ActionsEnvironmentsPanel browser DOM races", () => {
  it("create editor, env expand, and edit editor mount cleanly", async () => {
    const tracker = trackDomErrors();
    try {
      await mountWithQueryClient(ActionsEnvironmentsPanel, {
        owner: "ada",
        name: "hello",
        can_admin: true,
      });

      await waitForTestId("actions-environments-panel");

      // Expand a row — EnvSecrets mounts inside it.
      await waitForTestId("env-toggle-staging");
      await clickTestId("env-toggle-staging");
      await waitForTestId("env-secrets-staging");

      // Edit editor mounts for the same row; save unmounts it on success.
      await waitForTestId("env-edit-staging");
      await clickTestId("env-edit-staging");
      await waitForTestId("env-editor-staging");
      await waitForTestId("env-editor-prevent-self");
      await clickTestId("env-editor-prevent-self");
      await clickTestId("env-editor-save");

      // Create editor mounts fresh after the edit editor dismounts.
      await waitForTestId("env-new");
      await clickTestId("env-new");
      await waitForTestId("env-editor-new");
      await waitForTestId("env-editor-prevent-self");
      await clickTestId("env-editor-prevent-self");
      expect(document.querySelector('[data-testid="env-editor-name-input"]')).toBeTruthy();

      tracker.expectNoDomRaces();
      expectNoOctaneOverlayInDocument();
    } finally {
      tracker.dispose();
    }
  }, 30_000);
});
