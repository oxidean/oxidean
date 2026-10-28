/**
 * Chromium gates for the `/oxpilot` dialog surfaces — the Base UI
 * `Select` portal, `RadioGroup`/`Checkbox` indicators, and `Dialog`
 * portal mount/unmount next to Octane reconciliation are the DOM-race
 * class the browser-coverage manifest tracks (rows:
 * `components/oxpilot/new-session-dialog.tsrx`,
 * `components/oxpilot/connect-dialog.tsrx`).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act } from "octane";
import {
  cleanupBrowserMount,
  clickAriaLabel,
  debugBody,
  expectNoOctaneOverlayInDocument,
  mountWithQueryClient,
  pickSelectOptionByTestId,
} from "@/test/browser-mount";
import { trackDomErrors } from "@/test/dom-errors";

const repoListMineMock = vi.fn();
const repoRefsMock = vi.fn();
const targetListMock = vi.fn();
const sessionCreateMock = vi.fn();
const pairingCreateMock = vi.fn();
const grantListMock = vi.fn();
const grantRespondMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    oxpilot: {
      targetList: (...args: unknown[]) => targetListMock(...args),
      sessionCreate: (...args: unknown[]) => sessionCreateMock(...args),
      pairingCreate: (...args: unknown[]) => pairingCreateMock(...args),
      grantList: (...args: unknown[]) => grantListMock(...args),
      grantRespond: (...args: unknown[]) => grantRespondMock(...args),
    },
    repo: {
      listMine: (...args: unknown[]) => repoListMineMock(...args),
      refs: (...args: unknown[]) => repoRefsMock(...args),
    },
  },
}));

import { NewSessionDialog } from "@/components/oxpilot/new-session-dialog";
import { ConnectDialog } from "@/components/oxpilot/connect-dialog";

const repo = {
  id: "r1",
  name: "proj",
  owner_username: "ada",
  visibility: "public",
  default_branch: "main",
};

const connectedTarget = {
  id: "t-1",
  label: "desk-box",
  status: "online",
  lifecycle: "connected",
  pairing_id: "p-old",
  granted_caps: ["exec_shell"],
};

async function waitForSelector(selector: string, ms = 10_000): Promise<Element> {
  const deadline = Date.now() + ms;
  while (Date.now() < deadline) {
    const el = document.querySelector(selector);
    if (el) return el;
    await new Promise((r) => setTimeout(r, 50));
  }
  throw new Error(`${selector} never appeared. ${debugBody()}`);
}

async function waitForText(text: string, ms = 10_000): Promise<void> {
  const deadline = Date.now() + ms;
  while (Date.now() < deadline) {
    if (document.body.textContent?.includes(text)) return;
    await new Promise((r) => setTimeout(r, 50));
  }
  throw new Error(`text "${text}" never appeared. ${debugBody()}`);
}

async function typeIntoId(id: string, value: string): Promise<void> {
  const el = await waitForSelector(`#${id}`);
  await act(async () => {
    const input = el as HTMLInputElement | HTMLTextAreaElement;
    input.focus();
    input.value = value;
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

function buttonByText(text: string): HTMLButtonElement {
  const found = [...document.querySelectorAll("button")].find(
    (b) => b.textContent?.trim() === text,
  );
  if (!found) throw new Error(`button "${text}" not found. ${debugBody()}`);
  return found as HTMLButtonElement;
}

beforeEach(() => {
  for (const m of [
    repoListMineMock,
    repoRefsMock,
    targetListMock,
    sessionCreateMock,
    pairingCreateMock,
    grantListMock,
    grantRespondMock,
  ]) {
    m.mockReset();
  }
  repoListMineMock.mockResolvedValue({ ok: true, data: { repos: [repo] } });
  repoRefsMock.mockResolvedValue({
    ok: true,
    data: {
      refs: [
        { name: "refs/heads/main", oid: "a1" },
        { name: "refs/heads/dev", oid: "d2" },
      ],
    },
  });
  targetListMock.mockResolvedValue({ ok: true, data: { targets: [] } });
  sessionCreateMock.mockResolvedValue({ ok: true, data: { id: "s-new" } });
  grantListMock.mockResolvedValue({ ok: true, data: { grants: [] } });
  grantRespondMock.mockResolvedValue({ ok: true, data: { state: "allowed" } });
});

describe("/oxpilot dialog surfaces (Chromium DOM-race gates)", () => {
  afterEach(async () => {
    await cleanupBrowserMount();
  });

  it("oxpilot-repo-trigger (rebound): repo Select open + pick inside the Dialog", async () => {
    const tracker = trackDomErrors();
    try {
      await mountWithQueryClient(NewSessionDialog, {
        open: true,
        onClose: () => {},
        onCreated: () => {},
      });
      await waitForSelector('[data-testid="oxpilot-new-session"]');
      await waitForSelector('[data-testid="oxpilot-repo-trigger"]');
      await pickSelectOptionByTestId("oxpilot-repo-trigger", "oxpilot-repo-option");
      expectNoOctaneOverlayInDocument();
      tracker.expectNoDomRaces();
    } finally {
      tracker.dispose();
    }
  });

  it("oxpilot-new-session: repo Select + connected-target RadioGroup picks", async () => {
    const tracker = trackDomErrors();
    try {
      targetListMock.mockResolvedValue({
        ok: true,
        data: { targets: [connectedTarget] },
      });
      await mountWithQueryClient(NewSessionDialog, {
        open: true,
        onClose: () => {},
        onCreated: () => {},
      });
      await waitForSelector('[data-testid="oxpilot-new-session"]');
      await pickSelectOptionByTestId("oxpilot-repo-trigger", "oxpilot-repo-option");
      // The connected target radio mounts once targetList resolves.
      await waitForText("desk-box");
      const radios = document.querySelectorAll('[data-slot="radio-group-item"]');
      expect(radios.length).toBe(2); // "No target" + the connected target
      await act(async () => {
        (radios[1] as HTMLElement).click();
      });
      await waitForSelector('[data-slot="radio-group-item"][aria-checked="true"]');
      expect(radios[0].getAttribute("aria-checked")).toBe("false");
      expect(radios[1].getAttribute("aria-checked")).toBe("true");
      expectNoOctaneOverlayInDocument();
      tracker.expectNoDomRaces();
    } finally {
      tracker.dispose();
    }
  });

  it("oxpilot-connect: step-1 gating → pairing → step-2 pair-command swap", async () => {
    const tracker = trackDomErrors();
    try {
      pairingCreateMock.mockResolvedValue({
        ok: true,
        data: {
          pairing_id: "pair-1",
          pairing_code: "PAIR-TEST-42",
          // Far-future expiry keeps the step-2 wait phase live for the test.
          expires_at: new Date(Date.now() + 10 * 60 * 1000).toISOString(),
        },
      });
      await mountWithQueryClient(ConnectDialog, { open: true, onClose: () => {} });
      await waitForSelector('[data-testid="oxpilot-connect"]');
      await waitForSelector('section[aria-label="Connect a system"]');

      // [Continue] stays disabled until name + ≥1 cap + acknowledgement.
      expect(buttonByText("Continue").disabled).toBe(true);
      await typeIntoId("oxpilot-target-name", "desk-box");
      expect(buttonByText("Continue").disabled).toBe(true);
      await clickAriaLabel("shell.exec");
      expect(buttonByText("Continue").disabled).toBe(true);
      await clickAriaLabel("I understand the granted capabilities");
      await waitForSelector('[aria-label="shell.exec"][aria-checked="true"]');
      await waitForSelector(
        '[aria-label="I understand the granted capabilities"][aria-checked="true"]',
      );
      expect(buttonByText("Continue").disabled).toBe(false);

      await act(async () => {
        buttonByText("Continue").click();
      });
      await vi.waitFor(() => {
        expect(pairingCreateMock).toHaveBeenCalledWith({
          target_name: "desk-box",
          caps: ["exec_shell"],
        });
      });

      // Step-2 swap — only `ox pilot pair <code>` surfaces; the code
      // stays inside CopyCommand's read-only input, never a transcript.
      await waitForSelector('section[aria-label="Pair the connector"]');
      const cmd = (await waitForSelector(
        'input[aria-label="Pairing command"]',
      )) as HTMLInputElement;
      expect(cmd.value).toBe("ox pilot pair PAIR-TEST-42");
      await waitForText("Waiting for the connector to check in");
      expectNoOctaneOverlayInDocument();
      tracker.expectNoDomRaces();
    } finally {
      tracker.dispose();
    }
  });
});
