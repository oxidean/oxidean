/**
 * Chromium gate for /setup/credentials — "Keep current password" Switch (Base UI)
 * must reveal the password fields through the real DOM without Octane overlay races.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanupBrowserMount,
  debugBody,
  expectNoOctaneOverlayInDocument,
  mountWithQueryClient,
} from "@/test/browser-mount";
import { trackDomErrors } from "@/test/dom-errors";
import { act } from "octane";

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

import { CredentialsPage } from "./setup.credentials";

const pendingAdmin = {
  id: "u1",
  email: "admin@example.com",
  username: "admin",
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
  window.history.pushState({}, "", "/setup/credentials");
  meMock.mockReset();
  confirmMock.mockReset();
  meMock.mockResolvedValue({ ok: true, data: pendingAdmin });
  confirmMock.mockResolvedValue({ ok: true, data: {} });
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

describe("/setup/credentials browser", () => {
  it("flipping the keep-password switch reveals password and confirm fields", async () => {
    const races = trackDomErrors();
    await mountWithQueryClient(CredentialsPage);

    const toggle = (await waitForSelector("#credentials-keep-password")) as HTMLElement;
    expect(document.querySelector("#credentials-password")).toBeNull();

    await act(async () => {
      toggle.click();
    });

    await waitForSelector("#credentials-password");
    await waitForSelector("#credentials-confirm");
    expectNoOctaneOverlayInDocument();
    expect(races.errors).toEqual([]);
  });
});
