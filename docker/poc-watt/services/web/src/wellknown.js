// Port of crates/oxidean-web/src/wellknown.rs — /.well-known/{webmcp,mcp}
// discovery documents.

export const WEBMCP_PATH = '/.well-known/webmcp'
export const MCP_PATH = '/.well-known/mcp'
const MCP_ENDPOINT_PATH = '/api/mcp'

// crates/oxidean-core/src/lib.rs MCP_PROTOCOL_VERSIONS
const MCP_PROTOCOL_VERSIONS = ['2024-11-05', '2025-03-26', '2025-06-18']

function mcpDocument() {
  return {
    mcp_version: '1.0',
    server_name: 'Oxidean',
    endpoints: { streamable_http: MCP_ENDPOINT_PATH },
    transport: 'streamable-http (JSON responses; no standalone SSE stream)',
    protocol_versions: MCP_PROTOCOL_VERSIONS,
    capabilities: { tools: true, resources: true, prompts: false },
    methods: [
      'initialize', 'ping', 'tools/list', 'tools/call',
      'resources/list', 'resources/templates/list', 'resources/read',
      'prompts/list',
    ],
    authentication: {
      required: false,
      anonymous: 'public-readable data only; private or write tools answer isError',
      methods: [
        'session-cookie (oxidean_session; same-origin browser agents)',
        'bearer-pat (oxidean_pat_… classic / oxidean_fg_… fine-grained)',
      ],
    },
  }
}

export function webmcpDocument() {
  return {
    schema_version: 1,
    site: {
      name: 'Oxidean',
      description:
        'Self-hostable code forge — repositories, issues, pull requests, Actions runs, packages, and search.',
    },
    webmcp: {
      api: 'document.modelContext (navigator.modelContext fallback)',
      registration:
        'each page registers the instance tools/list catalog as WebMCP tools; execute() proxies tools/call to the MCP endpoint',
      declarative: 'search_site form tool via toolname/tooldescription attributes',
    },
    mcp: mcpDocument(),
    tools: {
      list: `${MCP_ENDPOINT_PATH} tools/list`,
      call: `${MCP_ENDPOINT_PATH} tools/call`,
    },
    links: {
      self: WEBMCP_PATH,
      mcp_endpoint: MCP_ENDPOINT_PATH,
      mcp_metadata: MCP_PATH,
    },
  }
}

export { mcpDocument }
