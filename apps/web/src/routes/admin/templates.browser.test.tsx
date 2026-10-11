/**
 * Chromium gate for /admin/templates — instance pack enable Switch (Base UI)
 * must toggle through the real DOM without Octane overlay/DOM races.
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
const listMock = vi.fn();
const setEnabledMock = vi.fn();
const createDefaultsMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    auth: {
      me: (...args: unknown[]) => meMock(...args),
    },
    admin: {
      templates: {
        list: (...args: unknown[]) => listMock(...args),
        setEnabled: (...args: unknown[]) => setEnabledMock(...args),
        delete: vi.fn(),
        update: vi.fn(),
      },
    },
    repo: {
      createDefaults: (...args: unknown[]) => createDefaultsMock(...args),
    },
  },
}));

vi.mock("@/lib/toast", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/toast")>()),
  toastSuccess: vi.fn(),
  toastError: vi.fn(),
  toastWarning: vi.fn(),
}));

// Route module imports ssr-auth → stub so the Chromium iframe never loads it.
const fetchSessionMeMock = vi.hoisted(() => vi.fn());
vi.mock("@/lib/ssr-auth", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/lib/ssr-auth")>();
  return { ...actual, fetchSessionMe: fetchSessionMeMock };
});

// FileDropzone pulls attr-accept, whose ESM build is broken under vitest
// browser deps — the upload path is not the gated surface here.
vi.mock("@/components/ui/file-dropzone", () => ({
  FileDropzone: () => null,
}));

import { AdminTemplatesPage } from "./templates";

const sysAdmin = {
  id: "u1",
  email: "admin@example.com",
  username: "admin",
  display_name: "Admin",
  bio: "",
  avatar_url: null as null,
  role: "sys-admin" as const,
  profile_incomplete: false,
  email_verified: true,
  must_change_credentials: false,
  default_branch: "main",
};

const pack = {
  id: "p1",
  slug: "acme-node",
  label: "Acme Node",
  group: "Custom",
  description: "Instance starter",
  enabled: true,
  byte_size: 128,
  content_digest: "sha256:abc",
  default_gitignore: null as string | null,
  uploaded_by_user_id: "u1",
  created_at: "2026-01-01T00:00:00Z",
  updated_at: "2026-01-01T00:00:00Z",
};

beforeEach(() => {
  meMock.mockReset();
  listMock.mockReset();
  setEnabledMock.mockReset();
  createDefaultsMock.mockReset();
  fetchSessionMeMock.mockReset();

  meMock.mockResolvedValue({ ok: true, data: sysAdmin });
  fetchSessionMeMock.mockImplementation(() => meMock());
  listMock.mockResolvedValue({ ok: true, data: { packs: [pack] } });
  setEnabledMock.mockResolvedValue({ ok: true, data: { ...pack, enabled: false } });
  createDefaultsMock.mockResolvedValue({
    ok: true,
    data: { default_visibility: "public", stacks: [], licenses: [], gitignores: [] },
  });
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

describe("/admin/templates browser", () => {
  it("toggles a template pack's Enabled switch via setEnabled RPC", async () => {
    const races = trackDomErrors();
    await mountWithQueryClient(AdminTemplatesPage);

    // Wait for the admin gate + pack list to resolve and render the row.
    const toggle = (await waitForSelector("#tpl-en-p1")) as HTMLElement;
    expect(document.body.textContent).toContain("Acme Node");

    await act(async () => {
      toggle.click();
    });

    await vi.waitFor(() => {
      expect(setEnabledMock).toHaveBeenCalledWith({ id: "p1", enabled: false });
    });
    expectNoOctaneOverlayInDocument();
    expect(races.errors).toEqual([]);
  });
});
