/**
 * Chromium gate for the issue-#111 early-navigation bridge: clicks on
 * internal anchors that hydration hasn't wired (`$$click` absent) are routed
 * through router.navigate; hydrated anchors and non-route hrefs keep native
 * semantics. Covers the mobile dead-window where taps used to fall through
 * to full-document reloads.
 */
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  EARLY_NAV_BOOT_SCRIPT,
  earlyNavHrefForTarget,
  installEarlyNavBridge,
} from "@/lib/early-nav";

const disposers: Array<() => void> = [];

afterEach(() => {
  for (const dispose of disposers.splice(0)) dispose();
  delete window.__oxideanEarlyNav;
  delete window.__oxideanEarlyNavReady;
  document.body.innerHTML = "";
});

function anchor(html: string): HTMLAnchorElement {
  const host = document.createElement("div");
  host.innerHTML = html;
  const a = host.querySelector("a");
  if (!a) throw new Error("bad fixture");
  document.body.appendChild(host);
  return a;
}

function clickOn(target: Element, init: MouseEventInit = {}): MouseEvent {
  const event = new MouseEvent("click", {
    bubbles: true,
    cancelable: true,
    button: 0,
    ...init,
  });
  target.dispatchEvent(event);
  return event;
}

describe("earlyNavHrefForTarget", () => {
  it("returns href for an unwired internal anchor", () => {
    const a = anchor('<a href="/owner/repo">x</a>');
    expect(earlyNavHrefForTarget(a)).toBe("/owner/repo");
  });

  it("resolves through child elements (icon/text nodes)", () => {
    const a = anchor('<a href="/owner/repo/issues"><span data-x>Issues</span></a>');
    const span = a.querySelector("span");
    expect(earlyNavHrefForTarget(span ?? null)).toBe("/owner/repo/issues");
  });

  it("skips anchors hydration already wired", () => {
    const a = anchor('<a href="/x">x</a>');
    (a as unknown as Record<string, unknown>).$$click = () => {};
    expect(earlyNavHrefForTarget(a)).toBeUndefined();
    (a as unknown as Record<string, unknown>)["$$capture:click"] = () => {};
    delete (a as unknown as Record<string, unknown>).$$click;
    expect(earlyNavHrefForTarget(a)).toBeUndefined();
  });

  it("skips /api endpoints, protocol-relative, and external hrefs", () => {
    expect(earlyNavHrefForTarget(anchor('<a href="/api/releases/assets/1">x</a>'))).toBeUndefined();
    expect(earlyNavHrefForTarget(anchor('<a href="/api">x</a>'))).toBeUndefined();
    expect(earlyNavHrefForTarget(anchor('<a href="//cdn.example/x">x</a>'))).toBeUndefined();
    expect(earlyNavHrefForTarget(anchor('<a href="https://example.com/x">x</a>'))).toBeUndefined();
  });

  it("skips target=_blank, download, and data-no-early-nav opt-outs", () => {
    expect(earlyNavHrefForTarget(anchor('<a href="/x" target="_blank">x</a>'))).toBeUndefined();
    expect(earlyNavHrefForTarget(anchor('<a href="/x" download>x</a>'))).toBeUndefined();
    expect(earlyNavHrefForTarget(anchor('<a href="/x" data-no-early-nav>x</a>'))).toBeUndefined();
  });

  it("ignores non-anchor targets", () => {
    const div = document.createElement("div");
    document.body.appendChild(div);
    expect(earlyNavHrefForTarget(div)).toBeUndefined();
    expect(earlyNavHrefForTarget(null)).toBeUndefined();
  });
});

describe("installEarlyNavBridge", () => {
  it("routes unwired internal clicks through navigate and prevents default", () => {
    const navigate = vi.fn();
    disposers.push(installEarlyNavBridge(navigate));
    const a = anchor('<a href="/owner/repo/issues">Issues</a>');
    const event = clickOn(a);
    expect(navigate).toHaveBeenCalledWith({ href: "/owner/repo/issues" });
    expect(event.defaultPrevented).toBe(true);
  });

  it("does not intercept hydrated anchors", () => {
    const navigate = vi.fn();
    disposers.push(installEarlyNavBridge(navigate));
    const a = anchor('<a href="/owner/repo">repo</a>');
    (a as unknown as Record<string, unknown>).$$click = () => {};
    // A hydrated router link owns the click: its handler preventDefaults and
    // drives SPA navigation — the bridge must stay out of the way.
    a.addEventListener("click", (e) => e.preventDefault());
    clickOn(a);
    expect(navigate).not.toHaveBeenCalled();
  });

  it("ignores modifier clicks and non-primary buttons", () => {
    const navigate = vi.fn();
    disposers.push(installEarlyNavBridge(navigate));
    const a = anchor('<a href="/x">x</a>');
    a.addEventListener("click", (e) => e.preventDefault());
    clickOn(a, { ctrlKey: true });
    clickOn(a, { metaKey: true });
    clickOn(a, { shiftKey: true });
    clickOn(a, { button: 1 });
    expect(navigate).not.toHaveBeenCalled();
  });

  it("respects clicks already prevented upstream", () => {
    const navigate = vi.fn();
    disposers.push(installEarlyNavBridge(navigate));
    const a = anchor('<a href="/x">x</a>');
    const event = new MouseEvent("click", { bubbles: true, cancelable: true, button: 0 });
    event.preventDefault();
    a.dispatchEvent(event);
    expect(navigate).not.toHaveBeenCalled();
  });

  it("detaches on dispose", () => {
    const navigate = vi.fn();
    const dispose = installEarlyNavBridge(navigate);
    dispose();
    const a = anchor('<a href="/x">x</a>');
    a.addEventListener("click", (e) => e.preventDefault());
    clickOn(a);
    expect(navigate).not.toHaveBeenCalled();
  });
});

describe("EARLY_NAV_BOOT_SCRIPT", () => {
  /** Parse the boot script like the inline <head> tag does. */
  function boot(): void {
    // oxlint-disable-next-line no-eval
    (0, eval)(EARLY_NAV_BOOT_SCRIPT);
    disposers.push(() => {
      delete window.__oxideanEarlyNav;
      delete window.__oxideanEarlyNavReady;
    });
  }

  it("queues a pre-router tap and flushes it when the router registers", () => {
    boot();
    expect(typeof window.__oxideanEarlyNavReady).toBe("function");
    const a = anchor('<a href="/owner/repo/issues">Issues</a>');
    const event = clickOn(a);
    expect(event.defaultPrevented).toBe(true);
    const navigate = vi.fn();
    disposers.push(installEarlyNavBridge(navigate));
    expect(navigate).toHaveBeenCalledWith({ href: "/owner/repo/issues" });
  });

  it("routes directly once the router has registered", () => {
    boot();
    const navigate = vi.fn();
    disposers.push(installEarlyNavBridge(navigate));
    navigate.mockClear();
    const a = anchor('<a href="/owner/repo/pulls">Pulls</a>');
    clickOn(a);
    expect(navigate).toHaveBeenCalledWith({ href: "/owner/repo/pulls" });
  });
});
