import { describe, expect, it } from "vitest";
import {
  httpsCloneUrl,
  resolvePublicOriginFromEnv,
  resolveSshAdvertiseHost,
  resolveSshAdvertisePort,
  sshCloneUrl,
  sshNeedsPortHint,
} from "./public-origin";

describe("public-origin", () => {
  it("builds absolute HTTPS clone URLs", () => {
    expect(httpsCloneUrl("http://localhost", "ada", "hello")).toBe(
      "http://localhost/ada/hello.git",
    );
    expect(httpsCloneUrl("http://localhost/", "ada", "hello")).toBe(
      "http://localhost/ada/hello.git",
    );
  });

  it("reads OXIDEAN_PUBLIC_ORIGIN when set", () => {
    const prev = process.env.OXIDEAN_PUBLIC_ORIGIN;
    const prevGw = process.env.RAILWAY_SERVICE_GATEWAY_URL;
    const prevDom = process.env.RAILWAY_PUBLIC_DOMAIN;
    delete process.env.RAILWAY_SERVICE_GATEWAY_URL;
    delete process.env.RAILWAY_PUBLIC_DOMAIN;
    process.env.OXIDEAN_PUBLIC_ORIGIN = "https://git.example/";
    expect(resolvePublicOriginFromEnv()).toBe("https://git.example");
    if (prev === undefined) delete process.env.OXIDEAN_PUBLIC_ORIGIN;
    else process.env.OXIDEAN_PUBLIC_ORIGIN = prev;
    if (prevGw === undefined) delete process.env.RAILWAY_SERVICE_GATEWAY_URL;
    else process.env.RAILWAY_SERVICE_GATEWAY_URL = prevGw;
    if (prevDom === undefined) delete process.env.RAILWAY_PUBLIC_DOMAIN;
    else process.env.RAILWAY_PUBLIC_DOMAIN = prevDom;
  });

  it("prefers Railway gateway when OXIDEAN_PUBLIC_ORIGIN host is stale", () => {
    const prev = process.env.OXIDEAN_PUBLIC_ORIGIN;
    const prevGw = process.env.RAILWAY_SERVICE_GATEWAY_URL;
    const prevDom = process.env.RAILWAY_PUBLIC_DOMAIN;
    delete process.env.RAILWAY_PUBLIC_DOMAIN;
    process.env.OXIDEAN_PUBLIC_ORIGIN = "https://gateway-preview-4893.up.railway.app";
    process.env.RAILWAY_SERVICE_GATEWAY_URL = "gateway-oxidean-pr-31.up.railway.app";
    expect(resolvePublicOriginFromEnv()).toBe("https://gateway-oxidean-pr-31.up.railway.app");
    if (prev === undefined) delete process.env.OXIDEAN_PUBLIC_ORIGIN;
    else process.env.OXIDEAN_PUBLIC_ORIGIN = prev;
    if (prevGw === undefined) delete process.env.RAILWAY_SERVICE_GATEWAY_URL;
    else process.env.RAILWAY_SERVICE_GATEWAY_URL = prevGw;
    if (prevDom === undefined) delete process.env.RAILWAY_PUBLIC_DOMAIN;
    else process.env.RAILWAY_PUBLIC_DOMAIN = prevDom;
  });

  it("keeps custom domain when Railway gateway host differs", () => {
    const prev = process.env.OXIDEAN_PUBLIC_ORIGIN;
    const prevGw = process.env.RAILWAY_SERVICE_GATEWAY_URL;
    const prevDom = process.env.RAILWAY_PUBLIC_DOMAIN;
    delete process.env.RAILWAY_SERVICE_GATEWAY_URL;
    process.env.OXIDEAN_PUBLIC_ORIGIN = "https://app.oxidean.dev";
    process.env.RAILWAY_PUBLIC_DOMAIN = "gateway-production.up.railway.app";
    expect(resolvePublicOriginFromEnv()).toBe("https://app.oxidean.dev");
    if (prev === undefined) delete process.env.OXIDEAN_PUBLIC_ORIGIN;
    else process.env.OXIDEAN_PUBLIC_ORIGIN = prev;
    if (prevGw === undefined) delete process.env.RAILWAY_SERVICE_GATEWAY_URL;
    else process.env.RAILWAY_SERVICE_GATEWAY_URL = prevGw;
    if (prevDom === undefined) delete process.env.RAILWAY_PUBLIC_DOMAIN;
    else process.env.RAILWAY_PUBLIC_DOMAIN = prevDom;
  });

  it("builds scp-style SSH URLs for port 22 and 2222 (never ssh://)", () => {
    expect(sshCloneUrl("git.example", 22, "ada", "hello")).toBe("git@git.example:ada/hello.git");
    expect(sshCloneUrl("localhost/", 2222, "ada", "hello")).toBe("git@localhost:ada/hello.git");
    expect(sshCloneUrl("127.0.0.1", 2222, "ada", "hello")).not.toMatch(/^ssh:\/\//);
  });

  it("sshNeedsPortHint only when port !== 22", () => {
    expect(sshNeedsPortHint(22)).toBe(false);
    expect(sshNeedsPortHint(2222)).toBe(true);
  });

  it("resolveSshAdvertiseHost falls back to the public origin hostname", () => {
    // No middleware metas exist in this environment.
    expect(resolveSshAdvertiseHost("http://127.0.0.1:3000")).toBe("127.0.0.1");
  });

  it("resolveSshAdvertisePort defaults to 2222", () => {
    expect(resolveSshAdvertisePort()).toBe(2222);
  });
});
