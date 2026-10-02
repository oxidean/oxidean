/**
 * Chromium gate for /invites/$token — link-invite email field + form.Subscribe
 * labels mount over phase switches without DOM races.
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
const invitesGetMock = vi.fn();
const invitesAcceptMock = vi.fn();

const paramsState = vi.hoisted(() => {
  let token = "tok-link";
  return {
    get: () => ({ token }),
    set: (next: string) => {
      token = next;
    },
  };
});

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    auth: {
      me: (...args: unknown[]) => meMock(...args),
    },
    invites: {
      get: (...args: unknown[]) => invitesGetMock(...args),
      accept: (...args: unknown[]) => invitesAcceptMock(...args),
    },
  },
}));

vi.mock("@octanejs/tanstack-router", () => ({
  createFileRoute: () => (opts: unknown) => opts,
  useParams: () => paramsState.get(),
  // No RouterProvider — AppLink must see "no router" and render its <a> fallback.
  useRouter: () => undefined,
  Link: (props: { href?: string; children?: unknown }) =>
    createElement("a", { href: props.href }, props.children as never),
}));

import { InviteAcceptPage } from "./invites.$token";

beforeEach(() => {
  meMock.mockReset();
  invitesGetMock.mockReset();
  invitesAcceptMock.mockReset();
  paramsState.set("tok-link");

  meMock.mockResolvedValue({ ok: false, error: { code: "auth.unauthenticated", message: "" } });
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

async function typeIntoId(id: string, value: string): Promise<void> {
  const el = await waitForSelector(`#${id}`);
  await act(async () => {
    const input = el as HTMLInputElement;
    input.focus();
    input.value = value;
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

const linkOrgInvite = {
  kind: "org",
  email: null,
  expires_at: null,
  seats_remaining: 4,
  org_slug: "acme",
  org_display_name: "Acme Corp",
  repo_owner: null,
  repo_name: null,
  grant: "member",
  acceptable: true,
  reason: null,
};

describe("InviteAcceptPage browser DOM races", () => {
  it("unbound link invite mounts the anon email field and submits without races", async () => {
    invitesGetMock.mockResolvedValue({ ok: true, data: linkOrgInvite });
    invitesAcceptMock.mockResolvedValue({
      ok: false,
      error: { code: "auth.taken", message: "username taken" },
    });

    const tracker = trackDomErrors();
    try {
      await mountWithQueryClient(InviteAcceptPage, {});

      // Link invite (email: null) → anon form includes the email field.
      const emailInput = await waitForSelector("#invite-email");
      expect(emailInput).toBeTruthy();
      expect(document.body.textContent).toContain("Acme Corp");

      // Fill all required fields (native constraint validation blocks empty
      // submits in Chromium — the empty-email branch lives in happy-dom tests).
      await typeIntoId("invite-email", "newbie@example.com");
      await typeIntoId("invite-username", "newbie");
      await typeIntoId("invite-password", "passw0rd-passw0rd");
      await typeIntoId("invite-confirm", "passw0rd-passw0rd");

      const submit = document.querySelector("form button[type='submit']");
      expect(submit).toBeTruthy();
      await act(async () => {
        (submit as HTMLElement).click();
      });
      await new Promise((r) => setTimeout(r, 100));
      expect(invitesAcceptMock).toHaveBeenCalledWith({
        token: "tok-link",
        email: "newbie@example.com",
        username: "newbie",
        password: "passw0rd-passw0rd",
      });
      expect(document.body.textContent).toContain("username is already taken");

      tracker.expectNoDomRaces();
      expectNoOctaneOverlayInDocument();
    } finally {
      tracker.dispose();
    }
  }, 30_000);

  it("bound + logged-in invite renders accept form; exhausted link shows invalid state", async () => {
    const boundInvite = {
      kind: "repo" as const,
      email: "ada@example.com",
      expires_at: "2026-12-31T00:00:00Z",
      seats_remaining: null,
      org_slug: null,
      org_display_name: null,
      repo_owner: "acme",
      repo_name: "app",
      grant: "write",
      acceptable: true,
      reason: null,
    };
    invitesGetMock.mockResolvedValue({ ok: true, data: boundInvite });
    meMock.mockResolvedValue({
      ok: true,
      data: {
        id: "u9",
        email: "ada@example.com",
        username: "ada",
        display_name: "Ada",
        bio: "",
        avatar_url: null,
        role: "user",
        profile_incomplete: false,
        email_verified: true,
        must_change_credentials: false,
        default_branch: "main",
      },
    });

    const tracker = trackDomErrors();
    try {
      await mountWithQueryClient(InviteAcceptPage, {});

      await waitForSelector("form button[type='submit']");
      expect(document.body.textContent).toContain("Signed in as");
      expect(document.body.textContent).toContain("ada@example.com");
      expect(document.body.textContent).toContain("acme/app");
      // Bound email invite → no anon email field.
      expect(document.querySelector("#invite-email")).toBeNull();
      expect(document.querySelector("#invite-username")).toBeNull();

      tracker.expectNoDomRaces();
      expectNoOctaneOverlayInDocument();
    } finally {
      tracker.dispose();
    }

    // Second mount on the same suite's next lifecycle: exhausted seats.
    invitesGetMock.mockResolvedValue({
      ok: true,
      data: { ...linkOrgInvite, seats_remaining: 0, acceptable: false, reason: "exhausted" },
    });
    meMock.mockResolvedValue({ ok: false, error: { code: "x", message: "" } });

    const tracker2 = trackDomErrors();
    try {
      await mountWithQueryClient(InviteAcceptPage, {});
      await waitForSelector("[role='alert'], .text-destructive");
      const deadline = Date.now() + 5_000;
      while (Date.now() < deadline) {
        if (document.body.textContent?.includes("no seats left")) break;
        await new Promise((r) => setTimeout(r, 50));
      }
      expect(document.body.textContent).toContain("no seats left");
      expect(document.querySelector("form")).toBeNull();

      tracker2.expectNoDomRaces();
      expectNoOctaneOverlayInDocument();
    } finally {
      tracker2.dispose();
    }
  }, 30_000);
});
