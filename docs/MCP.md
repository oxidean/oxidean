# MCP endpoint (AGT-01)

Oxidean exposes a **Model Context Protocol** server so MCP clients (agentic
editors, CLIs, orchestrators) can browse repositories, issues, pull requests,
Actions runs, and packages, and run instance search — backed by the same typed
RPC handlers that power the web app, so ACLs and visibility rules are identical.

## Endpoint

| Method | Path | Purpose |
| --- | --- | --- |
| `POST` | `/api/mcp` | Single JSON-RPC 2.0 message → `application/json` response |
| `GET` | `/api/mcp` | Standalone SSE stream — **not supported** (`405`) |

This is the streamable-HTTP transport in its JSON mode: each `POST` carries one
JSON-RPC message and returns one JSON-RPC response (or `202 Accepted` with an
empty body for notifications and client responses). Batch arrays are rejected;
send one message per request. There are no `Mcp-Session-Id` sessions — every
request is independent.

Supported protocol versions: `2024-11-05`, `2025-03-26`, `2025-06-18`
(unknown versions negotiate to the newest supported).

### Methods

`initialize` · `ping` · `tools/list` · `tools/call` · `resources/list` ·
`resources/templates/list` · `resources/read` · `prompts/list` (empty) ·
`notifications/*` (acknowledged with `202`).

### Example

```bash
curl -s https://forge.example.com/api/mcp \
  -H 'content-type: application/json' \
  -H 'authorization: Bearer oxidean_pat_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx' \
  -d '{
    "jsonrpc": "2.0", "id": 1, "method": "initialize",
    "params": {"protocolVersion": "2025-06-18", "capabilities": {},
               "clientInfo": {"name": "my-agent", "version": "0.1"}}
  }'
```

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "protocolVersion": "2025-06-18",
    "capabilities": {"tools": {}, "resources": {}, "prompts": {}},
    "serverInfo": {"name": "oxidean", "version": "…"},
    "instructions": "…"
  }
}
```

## Authentication

| Credential | How | Notes |
| --- | --- | --- |
| Session cookie | `Cookie: oxidean_session=…` | Browser-grade access; full ACL |
| Classic PAT | `Authorization: Bearer oxidean_pat_…` | Requires the `repo` scope for repo tools; `package:read`/`package:write` for `packages_list` |
| Fine-grained PAT | `Authorization: Bearer oxidean_fg_…` | `contents:read`/`contents:write` + repository selection gate repo tools; `package:read`/`package:write` for packages |

Send the token as a **Bearer** header — never as a URL parameter or RPC input.
Mint tokens under **Settings → Developer → Personal access tokens**.

A request that *presents* an invalid, expired, or unknown credential fails with
HTTP `401` and `WWW-Authenticate: Bearer` (failed attempts share the Smart HTTP
IP rate limiter — 20 failures / 15 min). A request with **no** credential runs
as anonymous: public data stays reachable, while tools that need a user or a
private repository return `isError` content instead of failing the protocol.

### Scope mapping for fine-grained tokens

- **Read tools** pass on public repositories and on repositories inside the
  token's selection; private repositories outside the grant answer
  `repo.not_found` — the same shape as a missing repo, so token probing cannot
  enumerate private names.
- **Write tools** (`issue_create`, `issue_comment`) additionally require
  `contents:write` and the repository inside the grant.
- `repo_list` results are filtered to the granted selection for `selected`
  tokens.

## Tools

All tools are thin wrappers over the typed RPC layer — authorization,
validation, and error codes are identical to `/api/rpc`.

| Tool | Wraps | Notes |
| --- | --- | --- |
| `repo_list` | `repo.listMine` / `repo.listByOwner` | `owner` arg lists a user/org; omit for your own repos |
| `repo_get` | `repo.get` | Metadata, visibility, default branch, stats |
| `repo_tree` | `repo.tree` | Directory listing at `ref`/`path` |
| `issue_list` / `issue_get` | `issue.list` / `issue.get` | `state`: open\|closed\|all |
| `issue_create` / `issue_comment` | `issue.create` / `issue.comments.create` | Write permission required |
| `issue_list_comments` | `issue.comments.list` | |
| `pull_list` / `pull_get` | `pull.list` / `pull.get` | `state`: open\|closed\|merged\|all |
| `actions_list_runs` / `actions_get_run` | `repo.actions.listRuns` / `repo.actions.getRun` | |
| `packages_list` | `packages.list` | `owner` slug or `repository_id` |
| `search_repos` | `repo.explore` | Public repo search/browse |
| `search_issues` / `search_pulls` / `search_code` | `repo.search` | `type` fixed per tool; `q` required |

`tools/call` results carry the handler payload twice: `content[]` holds the JSON
document as text, and `structuredContent` holds the parsed object.

### Tool errors vs protocol errors

Domain failures — `repo.not_found`, `auth.required`, `issue.not_found`, scope
denials — are **tool errors**: a normal JSON-RPC result with
`"isError": true` and `"<code>: <message>"` text in `content[]`. Protocol-level
`error` objects are reserved for malformed JSON (`-32700`), bad envelopes
(`-32600`), unknown methods (`-32601`), and invalid params (`-32602`).

## Resources

`resources/list` is empty (per-repo resources would explode the list); discover
shapes via `resources/templates/list`:

| URI template | Contents |
| --- | --- |
| `oxidean://repo/{owner}/{name}/blob/{path}?ref={ref}` | File text (`text/plain`) or base64 blob for binary files; `ref` defaults to the repo default branch |
| `oxidean://repo/{owner}/{name}/issue/{number}` | Issue title/state/author/body as Markdown |
| `oxidean://repo/{owner}/{name}/pull/{number}` | Pull request title/state/author/body as Markdown |

Reads go through the same ACL + token-scope seam as the matching tools.

## Browser agents (WebMCP)

The web app advertises this endpoint to browser-side agents: every page
registers the live `tools/list` catalog as WebMCP tools (`document.modelContext`
/ `navigator.modelContext`) that proxy `tools/call` here with the viewer's
session cookie, and a `<link rel="webmcp">` tag points at
`/.well-known/webmcp` for pre-navigation discovery. See
[WEBMCP.md](WEBMCP.md).

## Error envelope

```json
{"jsonrpc": "2.0", "id": 1, "error": {"code": -32601, "message": "method not found: bogus/method"}}
```

HTTP `401` responses include `WWW-Authenticate: Bearer realm="Oxidean MCP"`;
rate-limited auth attempts return `429` with `Retry-After`.
