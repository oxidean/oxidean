/**
 * Chromium check for the job-annotations panel — severity icons, message,
 * title, and `file#L` location rendering; empty state stays hidden.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ActionsAnnotations } from "@/components/repo/actions-annotations";
import {
  cleanupBrowserMount,
  debugBody,
  expectNoOctaneOverlayInDocument,
  mountWithQueryClient,
} from "@/test/browser-mount";

const annsMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    repo: {
      actions: {
        jobAnnotations: (...args: unknown[]) => annsMock(...args),
      },
    },
  },
}));

const PROPS = { owner: "ada", name: "hello", runId: "run-1", jobId: "job-1" };

async function waitForTestId(testId: string, ms = 10_000): Promise<Element> {
  const deadline = Date.now() + ms;
  while (Date.now() < deadline) {
    const el = document.querySelector(`[data-testid="${testId}"]`);
    if (el) return el;
    await new Promise((r) => setTimeout(r, 50));
  }
  throw new Error(`[data-testid="${testId}"] never appeared. ${debugBody()}`);
}

beforeEach(() => {
  annsMock.mockReset();
});

afterEach(async () => {
  await cleanupBrowserMount();
});

describe("ActionsAnnotations", () => {
  it("renders level, title, message, and file location per annotation", async () => {
    annsMock.mockResolvedValue({
      ok: true,
      data: {
        annotations: [
          {
            annotation_level: "error",
            message: "boom",
            path: "src/main.rs",
            start_line: 3,
            end_line: 4,
            start_column: 1,
            end_column: 9,
            title: "Lint",
            created_at: "2025-01-01T00:00:00Z",
          },
          {
            annotation_level: "warning",
            message: "careful",
            path: "src/lib.rs",
            start_line: 7,
            created_at: "2025-01-01T00:00:01Z",
          },
          {
            annotation_level: "notice",
            message: "fyi",
            created_at: "2025-01-01T00:00:02Z",
          },
        ],
      },
    });
    await mountWithQueryClient(ActionsAnnotations, PROPS);

    const section = await waitForTestId("actions-annotations");
    expect(section.textContent).toContain("boom");
    expect(section.textContent).toContain("careful");
    expect(section.textContent).toContain("fyi");
    expect(section.textContent).toContain("Lint");
    expect(section.textContent).toContain("src/main.rs#L3-L4");
    expect(section.textContent).toContain("src/lib.rs#L7");

    expect(document.querySelector('[data-testid="actions-annotation-error"]')).not.toBeNull();
    expect(document.querySelector('[data-testid="actions-annotation-warning"]')).not.toBeNull();
    expect(document.querySelector('[data-testid="actions-annotation-notice"]')).not.toBeNull();
    expectNoOctaneOverlayInDocument();
  });

  it("stays hidden when the job produced no annotations", async () => {
    annsMock.mockResolvedValue({ ok: true, data: { annotations: [] } });
    await mountWithQueryClient(ActionsAnnotations, PROPS);
    await new Promise((r) => setTimeout(r, 300));
    expect(document.querySelector('[data-testid="actions-annotations"]')).toBeNull();
    expectNoOctaneOverlayInDocument();
  });
});
