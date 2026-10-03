/**
 * Chromium gate for CloneBox — the Code dropdown portal (clone URL inputs +
 * archive download items) must mount without insertBefore races.
 */
import { afterEach, describe, expect, it } from "vitest";
import {
  cleanupBrowserMount,
  clickAriaLabel,
  debugBody,
  expectNoOctaneOverlayInDocument,
  mountComponent,
} from "@/test/browser-mount";
import { trackDomErrors } from "@/test/dom-errors";

import { CloneBox } from "@/components/repo/clone-box";

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

async function openCloneMenu(): Promise<void> {
  const trigger = await waitForSelector('[aria-label="Clone or download"]');
  for (let attempt = 0; attempt < 2; attempt++) {
    await clickAriaLabel("Clone or download");
    const deadline = Date.now() + 2_000;
    while (Date.now() < deadline) {
      if (trigger.hasAttribute("data-popup-open")) return;
      if (document.querySelector('[aria-label="HTTPS clone URL"]')) return;
      await new Promise((r) => setTimeout(r, 50));
    }
  }
  throw new Error(`Clone dropdown did not open. ${debugBody()}`);
}

describe("CloneBox browser DOM races", () => {
  it("dropdown mounts clone inputs and archive download items", async () => {
    const tracker = trackDomErrors();
    try {
      await mountComponent(CloneBox, {
        owner: "ada",
        repo: "hello",
        refName: "main",
        empty: false,
        publicOrigin: "https://ox.example",
        sshHost: "git.ox.example",
        sshPort: 22,
      });

      await openCloneMenu();

      const https = (await waitForSelector('[aria-label="HTTPS clone URL"]')) as HTMLInputElement;
      expect(https.value).toBe("https://ox.example/ada/hello.git");

      const ssh = (await waitForSelector('[aria-label="SSH clone URL"]')) as HTMLInputElement;
      expect(ssh.value).toContain("git.ox.example");

      expect(document.body.textContent).toContain("Download ZIP");
      expect(document.body.textContent).toContain("Download tar.gz");

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
        owner: "ada",
        repo: "hello",
        refName: "main",
        empty: true,
        publicOrigin: "https://ox.example",
        sshHost: "git.ox.example",
        sshPort: 22,
      });

      await openCloneMenu();

      const zipItem = Array.from(document.querySelectorAll('[role="menuitem"]')).find(
        (el) => (el.textContent ?? "").trim() === "Download ZIP",
      ) as HTMLElement | undefined;
      expect(zipItem).toBeTruthy();
      expect(zipItem!.getAttribute("aria-disabled")).toBe("true");

      tracker.expectNoDomRaces();
      expectNoOctaneOverlayInDocument();
    } finally {
      tracker.dispose();
    }
  }, 30_000);
});
