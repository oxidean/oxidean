# WebMCP surface (AGT-02)

WebMCP is a W3C Web Machine Learning Community Group draft that lets a web page
register JavaScript-callable tools for browser-side AI agents. Chrome surfaces
registered tools through Chrome DevTools MCP (`list_webmcp_tools` /
`call_webmcp_tool`), and extensions such as MCP-B do the same for other agents.
The current spec surface is `document.modelContext`; earlier drafts and some
shipping builds expose `navigator.modelContext`.

Oxidean advertises its instance MCP endpoint ([MCP.md](MCP.md), AGT-01) to those
agents and bridges every registered WebMCP tool to it, so a browser agent on an
Oxidean page can work with repositories, issues, pull requests, Actions runs,
packages, and search without DOM scraping.

## What the app publishes

Every page carries:

1. **A `<link rel="webmcp">` advertisement** — points at
   `/.well-known/webmcp`, a JSON document describing the WebMCP surface and the
   backing endpoint. `/.well-known/mcp` additionally publishes MCP server
   metadata (endpoint, protocol versions, capabilities, auth methods) in the
   style of the well-known discovery proposals for MCP servers. Both are
   served by the web app, not the API.
2. **An inline registration script** — feature-detects
   `document.modelContext`, then `navigator.modelContext`, and exits silently
   when neither exists (no polyfill is shipped; unsupporting browsers are
   unaffected). When present, the script fetches `tools/list` from `/api/mcp`
   and registers each entry as a WebMCP tool via `registerTool` (with a
   `provideContext` fallback for earlier-draft implementations).

## How tool calls work

A WebMCP tool's `execute` callback sends one JSON-RPC `tools/call` to
`POST /api/mcp` and returns the endpoint's result unchanged — `content[]`,
`structuredContent`, and `isError` included. The page never embeds tool
implementations or credentials; it is a thin bridge, so tool behavior, ACLs,
and error shapes match the instance endpoint exactly. The registered catalog
is whatever the instance advertises at page-load time — new server-side tools
appear without web changes.

### Sequence

```
agent ── list_webmcp_tools ──▶ browser ──▶ document.modelContext (per tab)
agent ── call_webmcp_tool ───▶ tool.execute(args)
                                    │
                                    ▼ fetch POST /api/mcp {tools/call}
                              instance MCP endpoint ──▶ typed RPC handlers
```

## Authentication

| Caller | Credential | What they get |
| --- | --- | --- |
| Browser agent on an Oxidean page | `oxidean_session` cookie (same-origin fetch; nothing to configure) | The signed-in user's ACLs — private repos the user can read, write tools where the user can write |
| Browser agent, signed-out viewer | none | Anonymous context: public-readable tools still run; private/auth-required calls return `isError` content |
| Remote / non-browser MCP client | `Authorization: Bearer oxidean_pat_…` / `oxidean_fg_…` | PAT scope mapping per [MCP.md](MCP.md#authentication) |

The session cookie is HttpOnly — the bridge never reads it, it just rides the
same-origin request. PATs are for remote clients talking to `/api/mcp`
directly; do not put tokens into page script.

## Security notes

- Tools are document-scoped: each tab exposes its own set, and cross-origin
  documents cannot see or call them by default.
- Write tools (`issue_create`, `issue_comment`) run with the viewer's ACL —
  nothing is elevated by the bridge.
- The browser mediates tool calls and can ask the user for consent; failed
  calls surface as tool errors, not protocol failures, matching the endpoint's
  `isError` convention (private repos answer `repo.not_found`).

## Trying it

1. Run Chrome 155+ with `--enable-experimental-web-platform-features` (or a
   browser/extension that implements the draft, such as MCP-B).
2. Open any page on the instance and sign in for full access.
3. From Chrome DevTools MCP: `list_webmcp_tools` shows `repo_list`,
   `issue_get`, `search_code`, and the rest of the instance catalog;
   `call_webmcp_tool` invokes them.

The same checks work without a browser agent:

```bash
curl -s https://forge.example.com/.well-known/webmcp
curl -s https://forge.example.com/.well-known/mcp
curl -s https://forge.example.com/api/mcp \
  -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}'
```

In local development the Vite dev server proxies `/api/mcp` to the API (see
`vite.config.ts`), so the bridge works on `localhost:3000` too.
