/**
 * Chromium gate: OAuth developer settings + consent-adjacent surfaces —
 * two-step delete/revoke buttons and the edit toggle must not throw
 * insertBefore/HierarchyRequestError when sibling trees swap (API-03).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { OAuthAppCreateForm } from "@/components/settings/oauth-app-create-form";
import { OAuthAppItem } from "@/components/settings/oauth-app-item";
import { OAuthGrants } from "@/components/settings/oauth-grants";
import { OAuthSecretReveal } from "@/components/settings/oauth-secret-reveal";
import {
  cleanupBrowserMount,
  clickAriaLabel,
  debugBody,
  expectNoOctaneOverlayInDocument,
  mountWithQueryClient,
} from "@/test/browser-mount";
import { act } from "octane";
import { trackDomErrors } from "@/test/dom-errors";

const createMock = vi.fn();
const updateMock = vi.fn();
const deleteMock = vi.fn();
const rotateMock = vi.fn();
const revokeMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    oauthApp: {
      create: (...args: unknown[]) => createMock(...args),
      update: (...args: unknown[]) => updateMock(...args),
      delete: (...args: unknown[]) => deleteMock(...args),
      regenerateSecret: (...args: unknown[]) => rotateMock(...args),
      revoke: (...args: unknown[]) => revokeMock(...args),
      list: vi.fn(async () => ({ ok: true, data: [] })),
      listGrants: vi.fn(async () => ({ ok: true, data: [] })),
    },
  },
}));

const APP = {
  id: "app-1",
  name: "test-cli",
  client_id: "oxidean_oc_0123456789abcdef0123456789abcdef",
  client_secret_prefix: "oxidean_osec_ab12cd34",
  redirect_uris: ["https://app.example/callback"],
  created_at: new Date().toISOString(),
  updated_at: new Date().toISOString(),
};

const GRANT = {
  application_id: "app-1",
  app_name: "test-cli",
  client_id: APP.client_id,
  scopes: ["repo", "read:user"],
  granted_at: new Date().toISOString(),
  last_used_at: null,
};

function setInput(el: Element | null, value: string) {
  if (!el) throw new Error(`input not mounted. ${debugBody()}`);
  (el as HTMLInputElement | HTMLTextAreaElement).value = value;
  el.dispatchEvent(new InputEvent("input", { bubbles: true }));
}

async function waitFor(pred: () => boolean, label: string, ms = 3_000) {
  const deadline = Date.now() + ms;
  while (Date.now() < deadline) {
    if (pred()) return;
    await new Promise((r) => setTimeout(r, 25));
  }
  throw new Error(`timed out waiting for ${label}. ${debugBody()}`);
}

function buttonByText(text: string): HTMLButtonElement | null {
  for (const b of document.querySelectorAll("button")) {
    if (b.textContent?.trim() === text) return b as HTMLButtonElement;
  }
  return null;
}

async function clickButton(text: string) {
  const b = buttonByText(text);
  if (!b) throw new Error(`button "${text}" not found. ${debugBody()}`);
  await act(async () => {
    b.click();
  });
}

beforeEach(() => {
  createMock.mockReset();
  updateMock.mockReset();
  deleteMock.mockReset();
  rotateMock.mockReset();
  revokeMock.mockReset();
});

afterEach(async () => {
  await cleanupBrowserMount();
});

describe("OAuthAppCreateForm browser DOM races", () => {
  it("submit → onCreated without DOM races", async () => {
    const tracker = trackDomErrors();
    let createdSecret = "";
    try {
      createMock.mockResolvedValue({
        ok: true,
        data: { app: APP, client_secret: "oxidean_osec_secret" },
      });
      await mountWithQueryClient(OAuthAppCreateForm, {
        onCreated: (_app: unknown, secret: string) => {
          createdSecret = secret;
        },
      });

      setInput(document.querySelector("#oauth-app-name"), "cli-bot");
      setInput(document.querySelector("#oauth-app-redirect-uris"), "https://app.example/cb");

      await clickButton("Register application");
      await waitFor(() => createdSecret.length > 0, "onCreated");
      expect(createMock).toHaveBeenCalledOnce();
      tracker.expectNoDomRaces();
      expectNoOctaneOverlayInDocument();
    } finally {
      tracker.dispose();
    }
  }, 30_000);
});

describe("OAuthAppItem browser DOM races", () => {
  it("edit toggle + delete two-step do not throw", async () => {
    const tracker = trackDomErrors();
    try {
      updateMock.mockResolvedValue({ ok: true, data: APP });
      deleteMock.mockResolvedValue({ ok: true, data: { ok: true } });
      await mountWithQueryClient(OAuthAppItem, { app: APP });

      await clickButton("Edit");
      await waitFor(
        () => Boolean(document.querySelector(`#oauth-app-name-${APP.id}`)),
        "inline edit form",
      );
      await clickButton("Close");

      await clickButton("Delete");
      await waitFor(() => Boolean(buttonByText("Confirm delete")), "confirm");
      await clickButton("Confirm delete");
      await waitFor(() => deleteMock.mock.calls.length > 0, "delete call");
      tracker.expectNoDomRaces();
      expectNoOctaneOverlayInDocument();
    } finally {
      tracker.dispose();
    }
  }, 30_000);

  it("rotate secret reveals the one-time panel", async () => {
    const tracker = trackDomErrors();
    try {
      rotateMock.mockResolvedValue({
        ok: true,
        data: { app: APP, client_secret: "oxidean_osec_newsecret" },
      });
      await mountWithQueryClient(OAuthAppItem, { app: APP });
      await clickButton("Rotate secret");
      await waitFor(
        () => Boolean(document.querySelector('[aria-label="Client secret"]')),
        "secret reveal input",
      );
      tracker.expectNoDomRaces();
      expectNoOctaneOverlayInDocument();
    } finally {
      tracker.dispose();
    }
  }, 30_000);
});

describe("OAuthGrants browser DOM races", () => {
  it("revoke two-step swaps siblings cleanly", async () => {
    const tracker = trackDomErrors();
    let revoked = false;
    try {
      revokeMock.mockResolvedValue({ ok: true, data: { ok: true } });
      await mountWithQueryClient(OAuthGrants, {
        grants: [GRANT],
        onRevoked: () => {
          revoked = true;
        },
      });
      await clickButton("Revoke");
      await waitFor(() => Boolean(buttonByText("Confirm revoke")), "confirm");
      await clickButton("Confirm revoke");
      await waitFor(() => revoked, "onRevoked");
      expect(revokeMock).toHaveBeenCalledOnce();
      tracker.expectNoDomRaces();
      expectNoOctaneOverlayInDocument();
    } finally {
      tracker.dispose();
    }
  }, 30_000);
});

describe("OAuthSecretReveal browser DOM races", () => {
  it("copy click swaps button label without races", async () => {
    const tracker = trackDomErrors();
    try {
      await mountWithQueryClient(OAuthSecretReveal, {
        secret: "oxidean_osec_secret",
      });
      await clickAriaLabel("Copy client secret");
      tracker.expectNoDomRaces();
      expectNoOctaneOverlayInDocument();
    } finally {
      tracker.dispose();
    }
  }, 30_000);
});
