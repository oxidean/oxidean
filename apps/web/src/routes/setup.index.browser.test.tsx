/**
 * Chromium gate for /setup/ — provider panels swapped inside form.Subscribe
 * plus the allow-signup Switch must mount without insertBefore DOM races.
 */
import { afterEach, beforeEach, describe, it, vi } from "vitest";
import {
  cleanupBrowserMount,
  debugBody,
  expectNoOctaneOverlayInDocument,
  mountWithQueryClient,
} from "@/test/browser-mount";
import { trackDomErrors } from "@/test/dom-errors";
import { act } from "octane";

const bootstrapMock = vi.fn();

const loaderState = vi.hoisted(() => {
  let data: unknown;
  return {
    get: () => data,
    set: (next: unknown) => {
      data = next;
    },
  };
});

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    auth: {
      bootstrapSetup: (...args: unknown[]) => bootstrapMock(...args),
    },
  },
}));

// Route module imports ssr-auth → tanstack-start; stub so Chromium Vite never loads Start.
vi.mock("@/lib/ssr-auth", () => ({
  fetchBootstrapStatus: vi.fn(),
}));

vi.mock("@octanejs/tanstack-router", () => ({
  createFileRoute: () => (opts: unknown) => opts,
  redirect: (opts: unknown) => opts,
  useLoaderData: () => loaderState.get(),
}));

import { SetupPage } from "./setup.index";

beforeEach(() => {
  bootstrapMock.mockReset();
  bootstrapMock.mockResolvedValue({ ok: true, data: {} });
  loaderState.set({ loadError: "" });
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

async function setNativeSelect(id: string, value: string): Promise<void> {
  const el = await waitForSelector(`#${id}`);
  await act(async () => {
    const select = el as HTMLSelectElement;
    select.value = value;
    select.dispatchEvent(new Event("change", { bubbles: true }));
  });
}

describe("SetupPage browser DOM races", () => {
  it("provider panel swaps inside form.Subscribe and the Switch toggle mount cleanly", async () => {
    const tracker = trackDomErrors();
    try {
      await mountWithQueryClient(SetupPage, {});

      await waitForSelector("#setup-email");
      await waitForSelector("#setup-provider");

      // local → oidc mounts the issuer/client-id fields; back to local unmounts.
      await setNativeSelect("setup-provider", "oidc");
      await waitForSelector("#setup-oidc-issuer");
      await waitForSelector("#setup-oidc-client-id");

      await setNativeSelect("setup-provider", "workos");
      await waitForSelector("#setup-workos-client-id");
      await waitForGone("#setup-oidc-issuer");

      await setNativeSelect("setup-provider", "local");
      await waitForGone("#setup-workos-client-id");

      // Toggle the allow-signup Switch.
      const signupSwitch = await waitForSelector("#setup-allow-signup");
      await act(async () => {
        (signupSwitch as HTMLElement).click();
      });
      await new Promise((r) => setTimeout(r, 50));

      tracker.expectNoDomRaces();
      expectNoOctaneOverlayInDocument();
    } finally {
      tracker.dispose();
    }
  }, 30_000);
});
