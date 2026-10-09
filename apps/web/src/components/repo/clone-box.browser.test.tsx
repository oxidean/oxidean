/**
 * Chromium gate for the repo "Code" clone DropdownMenu — the menu portal
 * mounts while Octane re-renders siblings, which happy-dom cannot prove
 * race-free (issue #44 class). Also pins the advertised SSH host/port prop
 * contract the meta-injected fallbacks feed (D-SSH-02).
 */
import { afterEach, describe, expect, it } from "vitest";
import { CloneBox } from "@/components/repo/clone-box";
import {
  cleanupBrowserMount,
  clickAriaLabel,
  debugBody,
  expectNoOctaneOverlayInDocument,
  mountComponent,
} from "@/test/browser-mount";
import { trackDomErrors } from "@/test/dom-errors";

async function waitForSelector(sel: string, ms = 5_000): Promise<Element> {
  const deadline = Date.now() + ms;
  while (Date.now() < deadline) {
    const el = document.querySelector(sel);
    if (el) return el;
    await new Promise((r) => setTimeout(r, 50));
  }
  throw new Error(`${sel} never appeared. ${debugBody()}`);
}

afterEach(async () => {
  await cleanupBrowserMount();
});

describe("CloneBox browser", () => {
  it("dropdown opens; HTTPS and SSH clone URLs resolve from props", async () => {
    const tracker = trackDomErrors();
    try {
      await mountComponent(CloneBox, {
        owner: "oxidean",
        repo: "oxidean",
        refName: "main",
        empty: false,
        publicOrigin: "https://forge.example",
        sshHost: "ssh.example.com",
        sshPort: 2222,
      });

      await clickAriaLabel("Clone or download");

      const https = (await waitForSelector(
        "input[aria-label='HTTPS clone URL']",
      )) as HTMLInputElement;
      expect(https.value).toBe("https://forge.example/oxidean/oxidean.git");
      const ssh = (await waitForSelector("input[aria-label='SSH clone URL']")) as HTMLInputElement;
      expect(ssh.value).toBe("git@ssh.example.com:oxidean/oxidean.git");
      // Non-standard SSH port paints the ~/.ssh/config hint.
      expect(document.body.textContent).toContain("Port 2222");

      // Copy path: headless Chromium has no clipboard — the error affordance
      // must render without a DOM race.
      await clickAriaLabel("Copy HTTPS URL");
      await waitForSelector("[role='alert']");

      tracker.expectNoDomRaces();
      expectNoOctaneOverlayInDocument();
    } finally {
      tracker.dispose();
    }
  }, 30_000);

  it("empty repo disables the archive download items", async () => {
    const tracker = trackDomErrors();
    try {
      await mountComponent(CloneBox, {
        owner: "o",
        repo: "r",
        refName: "main",
        empty: true,
        publicOrigin: "https://forge.example",
        sshHost: "localhost",
        sshPort: 22,
      });

      await clickAriaLabel("Clone or download");
      await waitForSelector("input[aria-label='HTTPS clone URL']");

      const zip = Array.from(document.querySelectorAll("[role='menuitem']")).find((el) =>
        el.textContent?.includes("Download ZIP"),
      );
      expect(zip?.getAttribute("aria-disabled")).toBe("true");

      tracker.expectNoDomRaces();
      expectNoOctaneOverlayInDocument();
    } finally {
      tracker.dispose();
    }
  }, 30_000);
});
