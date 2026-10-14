import { describe, expect, it } from "vitest";
import {
  LEGACY_WIRE_VERSION,
  MCP_PROTOCOL_VERSIONS,
  RPC_VERSION,
  RPC_VERSION_HEADER,
  WIRE_VERSION,
  createClient,
  wireCompatWarning,
  wireCompatible,
} from "./index";

describe("api-client", () => {
  it("exposes the wire version surface", () => {
    expect(WIRE_VERSION).toBe("0.1.0");
    expect(RPC_VERSION).toBe(1);
    expect(LEGACY_WIRE_VERSION).toBe("0.1.0");
    expect(RPC_VERSION_HEADER).toBe("Oxidean-RPC-Version");
    expect(MCP_PROTOCOL_VERSIONS.at(-1)).toBe("2025-06-18");
  });

  it("wireCompatible mirrors the shared caret rule (BR6.1)", () => {
    expect(wireCompatible("0.1.0", "0.1.5")).toBe(true);
    expect(wireCompatible("0.1.0", "0.2.0")).toBe(false);
    expect(wireCompatible("1.0.0", "1.4.2")).toBe(true);
    expect(wireCompatible("1.9.0", "2.0.0")).toBe(false);
    expect(wireCompatible("0.9.9", "1.0.0")).toBe(false);
    expect(wireCompatible("garbage", "0.1.0")).toBe(false);
    expect(wireCompatible("0.1.0-beta.1", "0.1.0")).toBe(true);
  });

  it("wireCompatWarning is advisory per BR6.4", () => {
    expect(wireCompatWarning({ wire_version: "0.1.9" })).toBeNull();
    expect(wireCompatWarning({ wire_version: "0.2.0" })).toContain("0.2.0");
    // protocol_version 1 without wire_version → legacy 0.1.x → compatible.
    expect(wireCompatWarning({ protocol_version: 1 })).toBeNull();
    expect(wireCompatWarning({})).toContain("did not advertise");
  });

  it("sends version header on health", async () => {
    const calls: RequestInit[] = [];
    const client = createClient({
      baseUrl: "http://example.test",
      fetch: async (_url, init) => {
        calls.push(init ?? {});
        return new Response(
          JSON.stringify({
            ok: true,
            data: { status: "ok", version: "0.1.0", database: "skipped" },
          }),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      },
    });
    const res = await client.system.health();
    expect(res.ok).toBe(true);
    const headers = new Headers(calls[0]?.headers);
    expect(headers.get("Oxidean-RPC-Version")).toBe("0.1.0");
  });

  it("sends version header on dbProbe", async () => {
    const calls: { url: string; init: RequestInit }[] = [];
    const client = createClient({
      baseUrl: "http://example.test",
      fetch: async (url, init) => {
        calls.push({ url: String(url), init: init ?? {} });
        return new Response(
          JSON.stringify({
            ok: true,
            data: { dialect: "sqlite", probe_count: 1, probed_at: "2026-01-01T00:00:00Z" },
          }),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      },
    });
    const res = await client.system.dbProbe();
    expect(res.ok).toBe(true);
    const body = JSON.parse(String(calls[0]?.init.body));
    expect(body.procedure).toBe("system.db_probe");
    const headers = new Headers(calls[0]?.init.headers);
    expect(headers.get("Oxidean-RPC-Version")).toBe("0.1.0");
  });

  it("calls system.manifest with an empty input", async () => {
    const calls: { url: string; init: RequestInit }[] = [];
    const client = createClient({
      baseUrl: "http://example.test",
      fetch: async (url, init) => {
        calls.push({ url: String(url), init: init ?? {} });
        return new Response(
          JSON.stringify({
            ok: true,
            data: {
              protocol_version: 1,
              server_version: "0.1.0",
              procedures: { "system.manifest": true },
              capabilities: { mcp: false, rest: false, oauth: false, cli_dist: false },
              min_cli_version: "0.1.0",
            },
          }),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      },
    });
    const res = await client.system.manifest();
    expect(res.ok).toBe(true);
    if (res.ok) {
      expect(res.data.procedures["system.manifest"]).toBe(true);
    }
    const body = JSON.parse(String(calls[0]?.init.body));
    expect(body.procedure).toBe("system.manifest");
  });
});
