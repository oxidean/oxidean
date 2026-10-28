/**
 * Chromium gate for the `/oxpilot` new-session repo Select — the Base UI
 * portal mount/unmount next to Octane reconciliation is the DOM-race class
 * the browser-coverage manifest tracks.
 */
import { afterEach, describe, it, vi } from "vitest";
import {
  cleanupBrowserMount,
  debugBody,
  expectNoOctaneOverlayInDocument,
  mountWithQueryClient,
  pickSelectOptionByTestId,
} from "@/test/browser-mount";
import { trackDomErrors } from "@/test/dom-errors";

const meMock = vi.fn();
const statusMock = vi.fn();
const sessionListMock = vi.fn();
const sessionCreateMock = vi.fn();
const repoListMineMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    auth: {
      me: (...args: unknown[]) => meMock(...args),
    },
    oxpilot: {
      status: (...args: unknown[]) => statusMock(...args),
      sessionList: (...args: unknown[]) => sessionListMock(...args),
      sessionCreate: (...args: unknown[]) => sessionCreateMock(...args),
    },
    repo: {
      listMine: (...args: unknown[]) => repoListMineMock(...args),
    },
    org: {
      listMine: () => Promise.resolve({ ok: true, data: { orgs: [] } }),
    },
  },
}));

import { OxpilotPage } from "./oxpilot.tsrx";

const user = {
  id: "u1",
  email: "ada@example.com",
  username: "ada",
  display_name: "Ada",
  bio: "",
  avatar_url: null as null,
  role: "user",
  profile_incomplete: false,
  email_verified: true,
  must_change_credentials: false,
};

async function waitForTestId(testId: string, ms = 10_000): Promise<Element> {
  const deadline = Date.now() + ms;
  while (Date.now() < deadline) {
    const el = document.querySelector(`[data-testid="${testId}"]`);
    if (el) return el;
    await new Promise((r) => setTimeout(r, 50));
  }
  throw new Error(`[data-testid="${testId}"] never appeared. ${debugBody()}`);
}

describe("/oxpilot new-session Select", () => {
  afterEach(async () => {
    await cleanupBrowserMount();
  });

  it("repo picker open + pick commits without insertBefore / overlay", async () => {
    const tracker = trackDomErrors();
    try {
      window.history.pushState({}, "", "/oxpilot");
      meMock.mockResolvedValue({ ok: true, data: user });
      statusMock.mockResolvedValue({
        ok: true,
        data: { enabled: true, provider: { id: "stub", model: "stub-1" }, reason: null },
      });
      sessionListMock.mockResolvedValue({ ok: true, data: { sessions: [] } });
      sessionCreateMock.mockResolvedValue({ ok: true, data: { id: "s-1" } });
      repoListMineMock.mockResolvedValue({
        ok: true,
        data: {
          repos: [
            {
              id: "r1",
              name: "proj",
              owner_username: "ada",
              visibility: "public",
              default_branch: "main",
            },
          ],
        },
      });

      await mountWithQueryClient(OxpilotPage);
      await waitForTestId("oxpilot-repo-trigger");
      await pickSelectOptionByTestId("oxpilot-repo-trigger", "oxpilot-repo-option");
      expectNoOctaneOverlayInDocument();
      tracker.expectNoDomRaces();
    } finally {
      tracker.dispose();
    }
  });
});
