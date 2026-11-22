import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act } from "octane";
import { BranchProtectionPanel } from "@/components/repo/branch-protection-panel";
import {
  cleanupBrowserMount,
  clickTestId,
  debugBody,
  expectNoOctaneOverlayInDocument,
  mountWithQueryClient,
} from "@/test/browser-mount";
import { trackDomErrors } from "@/test/dom-errors";

const branchProtectionList = vi.fn();
const branchProtectionCreate = vi.fn();
const tagProtectionList = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    repo: {
      branchProtection: {
        list: (...args: unknown[]) => branchProtectionList(...args),
        create: (...args: unknown[]) => branchProtectionCreate(...args),
        update: vi.fn(),
        delete: vi.fn(),
      },
      tagProtection: {
        list: (...args: unknown[]) => tagProtectionList(...args),
        create: vi.fn(),
        update: vi.fn(),
        delete: vi.fn(),
      },
    },
  },
}));

vi.mock("@/lib/toast", () => ({
  toastSuccess: vi.fn(),
}));

async function waitForTestId(testId: string, ms = 5_000): Promise<Element> {
  const deadline = Date.now() + ms;
  while (Date.now() < deadline) {
    const element = document.querySelector(`[data-testid="${testId}"]`);
    if (element) return element;
    await new Promise((resolve) => setTimeout(resolve, 25));
  }
  throw new Error(`[data-testid="${testId}"] never appeared. ${debugBody()}`);
}

beforeEach(() => {
  branchProtectionList.mockReset();
  branchProtectionCreate.mockReset();
  tagProtectionList.mockReset();
  branchProtectionList.mockResolvedValue({ ok: true, data: { rules: [] } });
  tagProtectionList.mockResolvedValue({ ok: true, data: { rules: [] } });
  branchProtectionCreate.mockResolvedValue({ ok: true, data: { rule: {} } });
});

afterEach(cleanupBrowserMount);

describe("branch-protection-panel", () => {
  it("defaults code-owner reviews off and saves the enabled value", async () => {
    const tracker = trackDomErrors();
    try {
      await mountWithQueryClient(BranchProtectionPanel, {
        owner: "ada",
        name: "hello",
        can_admin: true,
      });
      await waitForTestId("bp-add-form");
      const checkbox = (await waitForTestId("bp-code-owner-reviews")) as HTMLInputElement;
      expect(checkbox.checked).toBe(false);
      await clickTestId("bp-code-owner-reviews");
      expect(checkbox.checked).toBe(true);
      const save = Array.from(document.querySelectorAll("button")).find((button) =>
        /Add rule/.test(button.textContent ?? ""),
      );
      if (!save) throw new Error(`Add rule button never appeared. ${debugBody()}`);
      await act(async () => {
        (save as HTMLButtonElement).click();
      });
      const deadline = Date.now() + 5_000;
      while (branchProtectionCreate.mock.calls.length === 0 && Date.now() < deadline) {
        await new Promise((resolve) => setTimeout(resolve, 25));
      }
      expect(branchProtectionCreate).toHaveBeenCalledWith(
        expect.objectContaining({
          owner: "ada",
          name: "hello",
          pattern: "main",
          require_code_owner_reviews: true,
        }),
      );
      expectNoOctaneOverlayInDocument();
      tracker.expectNoDomRaces();
    } finally {
      tracker.dispose();
    }
  });
});
