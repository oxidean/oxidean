import { describe, expect, it, vi } from "vitest";
import {
  resolveSwBuildId,
  sanitizeSwBuildId,
  stampSwSource,
  SW_BUILD_PLACEHOLDER,
} from "./sw-build-id";
import { buildSwRegisterScript } from "./sw-register";

describe("sw-build-id", () => {
  it("sanitizes to URL-safe short ids", () => {
    expect(sanitizeSwBuildId(" abc/def!0123456789abcdef ")).toBe("abcdef0123456789abcdef");
    expect(sanitizeSwBuildId("")).toBe("unknown");
  });

  it("prefers Railway / CI commit env over local fallback", () => {
    expect(
      resolveSwBuildId({
        RAILWAY_GIT_COMMIT_SHA: "deadbeefcafebabe",
      }),
    ).toBe("deadbeefcafebabe");
    expect(
      resolveSwBuildId({
        GITHUB_SHA: "1111222233334444",
        RAILWAY_GIT_COMMIT_SHA: "aaaabbbbccccdddd",
      }),
    ).toBe("aaaabbbbccccdddd");
  });

  it("stamps CACHE_NAME placeholder", () => {
    const src = `const CACHE_NAME = "oxidean-shell-${SW_BUILD_PLACEHOLDER}";\n`;
    expect(stampSwSource(src, "abc123")).toContain('const CACHE_NAME = "oxidean-shell-abc123";');
    expect(stampSwSource(src, "abc123")).not.toContain(SW_BUILD_PLACEHOLDER);
  });
});

describe("buildSwRegisterScript", () => {
  it("embeds build id and update/skipWaiting hooks", () => {
    const script = buildSwRegisterScript("deploy-1");
    expect(script).toContain('BUILD="deploy-1"');
    expect(script).toContain("/sw.js?v=");
    expect(script).toContain("reg.update()");
    expect(script).toContain("SKIP_WAITING");
    expect(script).toContain("controllerchange");
  });

  it("strips unsafe characters from build id", () => {
    const script = buildSwRegisterScript('x";alert(1)//');
    expect(script).toContain('BUILD="xalert1"');
    expect(script).not.toContain("alert(1)");
  });

  /**
   * Execute the emitted registration script against stubbed window/navigator
   * globals and return the captured listeners + spies. The script is an IIFE,
   * so it runs on eval.
   */
  function runRegisterScript(controller: unknown) {
    const events: Record<string, Array<() => void>> = {
      load: [],
      controllerchange: [],
    };
    const reload = vi.fn();
    const register = vi.fn().mockResolvedValue({
      waiting: null,
      installing: null,
      addEventListener: () => {},
      update: () => Promise.resolve(),
    });
    const sandbox = {
      window: {
        addEventListener: (ev: string, fn: () => void) => events[ev].push(fn),
        location: { reload },
      },
      navigator: {
        serviceWorker: {
          controller,
          register,
          addEventListener: (ev: string, fn: () => void) => events[ev].push(fn),
        },
      },
      sessionStorage: {
        getItem: () => null,
        setItem: () => {},
      },
    };
    // oxlint-disable-next-line no-eval — executes the emitted boot script
    // verbatim against stubbed globals; that is the test.
    const run = (0, eval)(
      "(function(window,navigator,sessionStorage){" + buildSwRegisterScript("b1") + "})",
    ) as (w: unknown, n: unknown, s: unknown) => void;
    run(sandbox.window, sandbox.navigator, sandbox.sessionStorage);
    const fire = (ev: string) => events[ev].forEach((fn) => fn());
    return { events, fire, reload, register };
  }

  it("does not reload on the first-ever controllerchange (fresh install)", async () => {
    const { fire, reload, register } = runRegisterScript(null);
    fire("load");
    await vi.waitFor(() => expect(register).toHaveBeenCalled());
    fire("controllerchange"); // fresh install claims via clients.claim()
    expect(reload).not.toHaveBeenCalled();
  });

  it("reloads once when an updated worker takes over (had a controller)", async () => {
    const { fire, reload, register } = runRegisterScript({ active: true });
    fire("load");
    await vi.waitFor(() => expect(register).toHaveBeenCalled());
    fire("controllerchange");
    expect(reload).toHaveBeenCalledTimes(1);
    fire("controllerchange"); // second claim in same session — still once
    expect(reload).toHaveBeenCalledTimes(1);
  });

  it("arms the update-reload after the fresh-install claim", async () => {
    const { fire, reload, register } = runRegisterScript(null);
    fire("load");
    await vi.waitFor(() => expect(register).toHaveBeenCalled());
    fire("controllerchange"); // fresh install — skipped
    expect(reload).not.toHaveBeenCalled();
    fire("controllerchange"); // a later deploy's worker — reloads
    expect(reload).toHaveBeenCalledTimes(1);
  });
});
