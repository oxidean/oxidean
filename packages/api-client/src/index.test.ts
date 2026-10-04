import { describe, expect, it } from "vitest";
import { RPC_VERSION, RPC_VERSION_HEADER, createClient } from "./index";

describe("api-client", () => {
  it("exposes protocol version 1", () => {
    expect(RPC_VERSION).toBe(1);
    expect(RPC_VERSION_HEADER).toBe("Oxidean-RPC-Version");
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
    expect(headers.get("Oxidean-RPC-Version")).toBe("1");
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
    expect(headers.get("Oxidean-RPC-Version")).toBe("1");
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
              capabilities: { mcp: false, rest: false, oauth: false },
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
