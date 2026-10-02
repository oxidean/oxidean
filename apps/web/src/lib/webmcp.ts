/**
 * WebMCP surface (AGT-02): advertise the instance MCP endpoint to browser-side
 * agents and bridge the tools a page registers to `POST /api/mcp`.
 *
 * WebMCP (W3C Web Machine Learning CG draft, surfaced by Chrome DevTools MCP
 * via `list_webmcp_tools` / `call_webmcp_tool`) lets a page expose JS-callable
 * tools to in-browser agents. The current spec surface is
 * `document.modelContext`; earlier drafts and some shipping implementations
 * expose `navigator.modelContext`, so registration feature-detects both.
 *
 * Instead of duplicating forge logic in page script, the page registers the
 * instance's live `tools/list` catalog as WebMCP tools whose `execute`
 * callbacks forward `tools/call` to `/api/mcp`. Same-origin fetches carry the
 * `oxidean_session` cookie, so an agent acts with the signed-in user's ACLs;
 * anonymous viewers get the public-data tool behavior the endpoint already
 * defines. Browsers without the API run none of this (no polyfill shipped).
 */

/** Same-origin instance MCP endpoint (AGT-01). */
export const MCP_ENDPOINT_PATH = "/api/mcp";

/**
 * WebMCP discovery document (`rel="webmcp"` link target). Served by the web
 * app — see `vite-plugins/webmcp-well-known.ts`.
 */
export const WEBMCP_WELL_KNOWN_PATH = "/.well-known/webmcp";

/**
 * MCP server metadata document (`.well-known` URI in the style of the
 * discovery proposals circulating for MCP servers). Same plugin.
 */
export const MCP_WELL_KNOWN_PATH = "/.well-known/mcp";

/** Protocol versions the endpoint negotiates — keep in sync with mcp.rs. */
export const MCP_PROTOCOL_VERSIONS = ["2024-11-05", "2025-03-26", "2025-06-18"];

/**
 * Inline registration snippet for `dangerouslySetInnerHTML` in the root shell.
 * Fully static — no interpolation, so nothing user-controlled reaches the
 * markup (same contract as the SW register script, T-03-19).
 *
 * Behavior: detect a WebMCP surface (`document.modelContext`, falling back to
 * `navigator.modelContext`), fetch the instance `tools/list` once, and expose
 * each entry as a WebMCP tool that proxies `tools/call` to `/api/mcp`. Prefers
 * per-tool `registerTool` (current spec shape); falls back to bulk
 * `provideContext` (earlier draft). Detection retries once on `window.load`
 * for surfaces injected late by extensions; a failed `tools/list` also retries
 * there once. Any API absence or failure is a silent no-op.
 */
export function buildWebMcpRegisterScript(): string {
  return (
    "(function(){" +
    'if(typeof window==="undefined"||typeof fetch!=="function")return;' +
    'var ENDPOINT="/api/mcp";' +
    "var seq=0,started=false;" +
    "function detect(){" +
    'var mc=(typeof document!=="undefined"&&document.modelContext)||(typeof navigator!=="undefined"&&navigator.modelContext);' +
    "if(!mc)return null;" +
    'if(typeof mc.registerTool==="function")return{ctx:mc,mode:"tool"};' +
    'if(typeof mc.provideContext==="function")return{ctx:mc,mode:"context"};' +
    "return null;" +
    "}" +
    "function rpc(method,params){" +
    'return fetch(ENDPOINT,{method:"POST",credentials:"same-origin",' +
    'headers:{"content-type":"application/json",accept:"application/json"},' +
    'body:JSON.stringify({jsonrpc:"2.0",id:++seq,method:method,params:params||{}})' +
    "}).then(function(res){" +
    'if(!res.ok)throw new Error("mcp http "+res.status);' +
    "return res.json();" +
    "}).then(function(msg){" +
    'if(msg&&msg.error)throw new Error(msg.error.message||"mcp error");' +
    "return msg?msg.result:null;" +
    "});" +
    "}" +
    "function bindExecute(name){" +
    "return function(args){" +
    'return rpc("tools/call",{name:name,arguments:args||{}});' +
    "};" +
    "}" +
    "function start(surface){" +
    "if(started)return;" +
    "started=true;" +
    'rpc("tools/list").then(function(result){' +
    "var tools=result&&result.tools?result.tools:[];" +
    "var descriptors=tools.map(function(t){" +
    "return{" +
    'name:String(t.name||""),' +
    'description:String(t.description||""),' +
    'inputSchema:t.inputSchema||{type:"object",properties:{}},' +
    "execute:bindExecute(t.name)" +
    "};" +
    '}).filter(function(d){return d.name!=="";});' +
    "if(!descriptors.length)return;" +
    'if(surface.mode==="tool"){' +
    "descriptors.forEach(function(d){" +
    "try{" +
    "var p=surface.ctx.registerTool(d);" +
    'if(p&&typeof p.catch==="function")p.catch(function(){});' +
    "}catch(e){}" +
    "});" +
    "}else{" +
    "try{surface.ctx.provideContext({tools:descriptors});}catch(e){}" +
    "}" +
    "}).catch(function(){started=false;});" +
    "}" +
    "function boot(){var s=detect();if(s)start(s);}" +
    "boot();" +
    'window.addEventListener("load",boot);' +
    "})();"
  );
}

/**
 * `/.well-known/mcp` body — instance MCP server metadata. Fields follow the
 * well-known discovery proposal circulating for MCP servers (endpoint map,
 * capabilities, auth); the authoritative catalog stays `tools/list` on the
 * endpoint so this document cannot drift on tool names or schemas.
 */
export function buildMcpWellKnownDocument(): Record<string, unknown> {
  return {
    mcp_version: "1.0",
    server_name: "Oxidean",
    endpoints: {
      streamable_http: MCP_ENDPOINT_PATH,
    },
    transport: "streamable-http (JSON responses; no standalone SSE stream)",
    protocol_versions: [...MCP_PROTOCOL_VERSIONS],
    capabilities: {
      tools: true,
      resources: true,
      prompts: false,
    },
    methods: [
      "initialize",
      "ping",
      "tools/list",
      "tools/call",
      "resources/list",
      "resources/templates/list",
      "resources/read",
      "prompts/list",
    ],
    authentication: {
      required: false,
      anonymous: "public-readable data only; private or write tools answer isError",
      methods: [
        "session-cookie (oxidean_session; same-origin browser agents)",
        "bearer-pat (oxidean_pat_… classic / oxidean_fg_… fine-grained)",
      ],
    },
  };
}

/**
 * `/.well-known/webmcp` body — the `rel="webmcp"` discovery document for
 * browser agents: how the site surfaces tools (in-page `modelContext`
 * registration) and where the backing MCP endpoint lives.
 */
export function buildWebMcpWellKnownDocument(): Record<string, unknown> {
  return {
    schema_version: 1,
    site: {
      name: "Oxidean",
      description:
        "Self-hostable code forge — repositories, issues, pull requests, Actions runs, packages, and search.",
    },
    webmcp: {
      api: "document.modelContext (navigator.modelContext fallback)",
      registration:
        "each page registers the instance tools/list catalog as WebMCP tools; execute() proxies tools/call to the MCP endpoint",
    },
    mcp: buildMcpWellKnownDocument(),
    tools: {
      list: `${MCP_ENDPOINT_PATH} tools/list`,
      call: `${MCP_ENDPOINT_PATH} tools/call`,
    },
    links: {
      self: WEBMCP_WELL_KNOWN_PATH,
      mcp_endpoint: MCP_ENDPOINT_PATH,
      mcp_metadata: MCP_WELL_KNOWN_PATH,
    },
  };
}
