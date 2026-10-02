import { Script } from "node:vm";
import { describe, expect, it } from "vitest";
import {
  buildMcpWellKnownDocument,
  buildWebMcpRegisterScript,
  buildWebMcpWellKnownDocument,
  MCP_ENDPOINT_PATH,
  MCP_WELL_KNOWN_PATH,
  WEBMCP_WELL_KNOWN_PATH,
} from "./webmcp";

describe("buildWebMcpRegisterScript", () => {
  it("is a syntactically valid self-contained script", () => {
    const script = buildWebMcpRegisterScript();
    // Compiles without executing — catches unbalanced quotes/braces in the
    // string-concatenated snippet.
    expect(() => new Script(script)).not.toThrow();
    expect(script.startsWith("(function(){")).toBe(true);
    expect(script.endsWith("})();")).toBe(true);
    expect(script).not.toContain("</script");
  });

  it("feature-detects both document and navigator modelContext surfaces", () => {
    const script = buildWebMcpRegisterScript();
    expect(script).toContain("document.modelContext");
    expect(script).toContain("navigator.modelContext");
    expect(script).toContain("registerTool");
    expect(script).toContain("provideContext");
  });

  it("bridges tools/list + tools/call to the instance endpoint same-origin", () => {
    const script = buildWebMcpRegisterScript();
    expect(script).toContain('"/api/mcp"');
    expect(script).toContain('"tools/list"');
    expect(script).toContain('"tools/call"');
    expect(script).toContain('credentials:"same-origin"');
    expect(script).toContain('jsonrpc:"2.0"');
  });
});

describe("well-known discovery documents", () => {
  it("advertises the instance streamable-HTTP endpoint", () => {
    const doc = buildMcpWellKnownDocument() as Record<string, Record<string, unknown>>;
    expect(doc.endpoints.streamable_http).toBe(MCP_ENDPOINT_PATH);
    expect(doc.capabilities.tools).toBe(true);
    expect(doc.protocol_versions as unknown as string[]).toContain("2025-06-18");
  });

  it("webmcp document links the endpoint and metadata document", () => {
    const doc = buildWebMcpWellKnownDocument() as Record<string, Record<string, unknown>>;
    expect(doc.links.self).toBe(WEBMCP_WELL_KNOWN_PATH);
    expect(doc.links.mcp_endpoint).toBe(MCP_ENDPOINT_PATH);
    expect(doc.links.mcp_metadata).toBe(MCP_WELL_KNOWN_PATH);
    expect(doc.webmcp.api).toContain("document.modelContext");
  });
});
