/**
 * Chromium gate for /admin/templates — instance-pack enable Switch mounts and
 * toggles without insertBefore races (surface moved off skip-only in #104).
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
const listPacksMock = vi.fn();
const setEnabledMock = vi.fn();
const deletePackMock = vi.fn();
const createDefaultsMock = vi.fn();

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
      me: (...args: unknown[]) => meMock(...args),
    },
    admin: {
      templates: {
        list: (...args: unknown[]) => listPacksMock(...args),
        setEnabled: (...args: unknown[]) => setEnabledMock(...args),
        delete: (...args: unknown[]) => deletePackMock(...args),
      },
    },
    repo: {
      createDefaults: (...args: unknown[]) => createDefaultsMock(...args),
    },
  },
}));

vi.mock("@/lib/toast", () => ({
  toastSuccess: vi.fn(),
  toastError: vi.fn(),
}));

// Route module imports ssr-auth → tanstack-start; stub so Chromium Vite never loads Start.
vi.mock("@/lib/ssr-auth", () => ({
  fetchSessionMe: vi.fn(),
  requireSessionRedirect: vi.fn(),
}));

// FileDropzone pulls attr-accept, whose es/ build is CJS under the hood —
// "exports is not defined" under Chromium ESM. Stub it; the Switch is the
// coverage subject here.
vi.mock("@/components/ui/file-dropzone", () => ({
  FileDropzone: () => createElement("div", { "data-testid": "file-dropzone" }),
}));

// No RouterProvider — useAppNavigate must see "no router" and fall back.
vi.mock("@octanejs/tanstack-router", () => ({
  createFileRoute: () => (opts: unknown) => opts,
  useLoaderData: () => loaderState.get(),
  useRouter: () => undefined,
  Link: (props: { href?: string; children?: unknown }) =>
    createElement("a", { href: props.href }, props.children as never),
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

const instancePack = {
  id: "pack-1",
  slug: "acme-node",
  label: "Acme Node",
  group: "Custom",
  description: "Starter",
  default_gitignore: null as null,
  enabled: true,
  byte_size: 1024,
  content_digest: "sha256:x",
  uploaded_by_user_id: "u1",
  created_at: "2026-01-01T00:00:00Z",
  updated_at: "2026-01-01T00:00:00Z",
};

beforeEach(() => {
  meMock.mockReset();
  listPacksMock.mockReset();
  setEnabledMock.mockReset();
  deletePackMock.mockReset();
  createDefaultsMock.mockReset();

  meMock.mockResolvedValue({ ok: true, data: sysAdmin });
  listPacksMock.mockResolvedValue({ ok: true, data: { packs: [instancePack] } });
  setEnabledMock.mockResolvedValue({ ok: true, data: { ok: true } });
  deletePackMock.mockResolvedValue({ ok: true, data: { ok: true } });
  createDefaultsMock.mockResolvedValue({
    ok: true,
    data: {
      default_visibility: "public",
      stacks: [
        {
          id: "node",
          label: "Node",
          group: "Language",
          description: "Node starter",
          provenance: "builtin",
        },
      ],
      gitignores: [{ id: "Node", label: "Node", group: "", description: "" }],
    },
  });

  loaderState.set({ kind: "ready", me: sysAdmin });
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

describe("AdminTemplatesPage browser DOM races", () => {
  it("instance pack Switch toggles enabled without insertBefore races", async () => {
    const tracker = trackDomErrors();
    try {
      await mountWithQueryClient(AdminTemplatesPage, {});

      await waitForSelector("#tpl-slug");
      const toggle = await waitForSelector("#tpl-en-pack-1");

      await act(async () => {
        (toggle as HTMLElement).click();
      });
      await new Promise((r) => setTimeout(r, 100));
      expect(setEnabledMock).toHaveBeenCalledWith({ id: "pack-1", enabled: false });

      tracker.expectNoDomRaces();
      expectNoOctaneOverlayInDocument();
    } finally {
      tracker.dispose();
    }
  }, 30_000);
});
