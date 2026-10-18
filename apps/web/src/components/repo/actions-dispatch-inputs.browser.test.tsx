/**
 * Chromium gate for the typed workflow_dispatch input controls — Checkbox and
 * Base UI Selects (choice + environment) across mount and option commits.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act } from "octane";
import {
  ActionsDispatchInputs,
  initialDispatchInputs,
} from "@/components/repo/actions-dispatch-inputs";
import {
  cleanupBrowserMount,
  debugBody,
  expectNoOctaneOverlayInDocument,
  mountWithQueryClient,
  pickSelectOptionByTestId,
} from "@/test/browser-mount";
import { trackDomErrors } from "@/test/dom-errors";

const envsMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    repo: {
      actions: {
        environments: {
          list: (...args: unknown[]) => envsMock(...args),
        },
      },
    },
  },
}));

const INPUTS: import("@oxidean/api-client").ActionDispatchInputPublic[] = [
  { name: "reason", type: "string", required: true, description: "Why now" },
  { name: "dry_run", type: "boolean", required: false },
  { name: "channel", type: "choice", required: false, options: ["stable", "beta"] },
  { name: "target", type: "environment", required: false },
];

beforeEach(() => {
  envsMock.mockReset().mockResolvedValue({
    ok: true,
    data: { environments: [{ name: "staging" }, { name: "production" }] },
  });
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

describe("ActionsDispatchInputs browser DOM races", () => {
  it("checkbox + choice/environment Selects report changes without remounts", async () => {
    const tracker = trackDomErrors();
    const changes: Array<[string, unknown]> = [];
    const values = initialDispatchInputs(INPUTS);
    try {
      await mountWithQueryClient(ActionsDispatchInputs, {
        owner: "ada",
        name: "hello",
        inputs: INPUTS,
        values,
        onChange: (n: string, v: unknown) => changes.push([n, v]),
      });

      await waitForTestId("dispatch-input-field-reason");
      await waitForTestId("dispatch-input-field-dry_run");
      const checkbox = await waitForTestId("dispatch-input-field-dry_run");
      await act(async () => {
        (checkbox as HTMLElement).click();
      });
      expect(changes.at(-1)).toEqual(["dry_run", true]);

      await pickSelectOptionByTestId(
        "dispatch-input-field-channel",
        "dispatch-input-field-channel-option-beta",
      );
      expect(changes.at(-1)).toEqual(["channel", "beta"]);

      // Environment options hydrate from the environments query.
      await pickSelectOptionByTestId(
        "dispatch-input-field-target",
        "dispatch-input-field-target-option-staging",
      );
      expect(changes.at(-1)).toEqual(["target", "staging"]);

      tracker.expectNoDomRaces();
      expectNoOctaneOverlayInDocument();
    } finally {
      tracker.dispose();
    }
  }, 30_000);
});
