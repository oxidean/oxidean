/**
 * Chromium gate for /setup/credentials — the keep-password Switch swaps the
 * password fields inside form.Subscribe; must mount without insertBefore
 * races (surface moved off skip-only in #104).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanupBrowserMount,
  debugBody,
  expectNoOctaneOverlayInDocument,
  mountWithQueryClient,
} from "@/test/browser-mount";
import { trackDomErrors } from "@/test/dom-errors";
import { act, createElement } from "octane";

const meMock = vi.fn();
const confirmMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    auth: {
      me: (...args: unknown[]) => meMock(...args),
      confirmAdminCredentials: (...args: unknown[]) => confirmMock(...args),
    },
  },
}));

// Route module imports ssr-auth → tanstack-start; stub so Chromium Vite never loads Start.
vi.mock("@/lib/ssr-auth", () => ({
  fetchSessionMe: vi.fn(),
}));

// No RouterProvider — useAppNavigate must see "no router" and fall back.
vi.mock("@octanejs/tanstack-router", () => ({
  createFileRoute: () => (opts: unknown) => opts,
  redirect: (opts: unknown) => opts,
  useRouter: () => undefined,
  Link: (props: { href?: string; children?: unknown }) =>
    createElement("a", { href: props.href }, props.children as never),
}));

import { CredentialsPage } from "./setup.credentials";

const pendingAdmin = {
  id: "u1",
  email: "admin@example.com",
  username: "system-administrator",
  display_name: "Admin",
  bio: "",
  avatar_url: null as null,
  role: "sys-admin" as const,
  profile_incomplete: false,
  email_verified: true,
  must_change_credentials: true,
  default_branch: "main",
};

beforeEach(() => {
  meMock.mockReset();
  confirmMock.mockReset();
  meMock.mockResolvedValue({ ok: true, data: pendingAdmin });
  confirmMock.mockResolvedValue({ ok: true, data: { ok: true } });
});

afterEach(async () => {
  await cleanupBrowserMount();
});

async function waitForSelector(selector: string, ms = 10_000): Promise<Element> {
  const deadline = Date.now() + ms;
  while (Date.now() < deadline) {
    const el = document.querySelector(selector);
    if (el) return el;
    await new Promise((r) => setTimeout(r, 50));
  }
  throw new Error(`${selector} never appeared. ${debugBody()}`);
}

async function waitForGone(selector: string, ms = 10_000): Promise<void> {
  const deadline = Date.now() + ms;
  while (Date.now() < deadline) {
    if (!document.querySelector(selector)) return;
    await new Promise((r) => setTimeout(r, 50));
  }
  throw new Error(`${selector} never detached. ${debugBody()}`);
}

describe("CredentialsPage browser DOM races", () => {
  it("keep-password Switch swaps the password panel without insertBefore races", async () => {
    const tracker = trackDomErrors();
    try {
      await mountWithQueryClient(CredentialsPage, {});

      // Ready once the session fetch populates the form fields.
      const username = (await waitForSelector("#credentials-username")) as HTMLInputElement;
      expect(username.value).toBe("system-administrator");

      const keepSwitch = await waitForSelector("#credentials-keep-password");

      // Off → password + confirm mount inside form.Subscribe.
      await act(async () => {
        (keepSwitch as HTMLElement).click();
      });
      await waitForSelector("#credentials-password");
      await waitForSelector("#credentials-confirm");

      // Back on → both unmount cleanly.
      await act(async () => {
        (keepSwitch as HTMLElement).click();
      });
      await waitForGone("#credentials-password");
      await waitForGone("#credentials-confirm");

      tracker.expectNoDomRaces();
      expectNoOctaneOverlayInDocument();
    } finally {
      tracker.dispose();
    }
  }, 30_000);
});
