/**
 * Chromium gate for the Deploy keys panel RadioGroup + list churn
 * (happy-dom cannot catch insertBefore races).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { DeployKeysPanel } from "@/components/repo/deploy-keys-panel";
import {
  cleanupBrowserMount,
  clickTestId,
  debugBody,
  expectNoOctaneOverlayInDocument,
  mountWithQueryClient,
} from "@/test/browser-mount";
import { trackDomErrors } from "@/test/dom-errors";

const listMock = vi.fn();
const createMock = vi.fn();
const deleteMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    repo: {
      deployKey: {
        list: (...args: unknown[]) => listMock(...args),
        create: (...args: unknown[]) => createMock(...args),
        delete: (...args: unknown[]) => deleteMock(...args),
      },
    },
  },
}));

vi.mock("@/lib/toast", () => ({
  toastSuccess: vi.fn(),
  toastError: vi.fn(),
}));

beforeEach(() => {
  listMock.mockReset();
  createMock.mockReset();
  deleteMock.mockReset();
  listMock.mockResolvedValue({ ok: true, data: { keys: [] } });
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

describe("DeployKeysPanel browser DOM races", () => {
  it("read/write radio toggles + list churn do not throw insertBefore", async () => {
    const tracker = trackDomErrors();
    try {
      // First list returns one key; the post-delete refetch returns empty.
      listMock.mockResolvedValueOnce({
        ok: true,
        data: {
          keys: [
            {
              id: "dk1",
              repo_id: "r1",
              title: "CI",
              fingerprint: "SHA256:aaaa",
              key_type: "ssh-ed25519",
              can_write: false,
              created_by: "u1",
              created_at: "2026-01-01T00:00:00Z",
            },
          ],
        },
      });

      await mountWithQueryClient(DeployKeysPanel, {
        owner: "ada",
        name: "hello",
        can_admin: true,
      });

      await waitForTestId("deploy-keys-settings");
      await waitForTestId("dk-scope-read");

      // Toggle the scope radio repeatedly — Indicator mounts must not race siblings.
      await clickTestId("dk-scope-write");
      await clickTestId("dk-scope-read");
      await clickTestId("dk-scope-write");

      // Delete churns the @for list while the radio group stays mounted.
      deleteMock.mockResolvedValue({ ok: true, data: { ok: true } });
      await clickTestId("dk-delete-dk1");
      await waitForTestId("dk-empty");

      expect(document.querySelector('[data-testid="dk-scope-read"]')).toBeTruthy();
      tracker.expectNoDomRaces();
      expectNoOctaneOverlayInDocument();
    } finally {
      tracker.dispose();
    }
  }, 30_000);
});
