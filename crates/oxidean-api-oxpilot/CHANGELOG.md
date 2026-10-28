# Changelog

All notable changes to this component are documented here.

## [0.1.0] - 2026-10-27

### Added

- `oxpilot.status` / `oxpilot.session_create` / `oxpilot.session_list` —
  SessionOnly RPC surface for the `/oxpilot` agentic-chat walking skeleton.
- `POST /api/oxpilot/sessions/{id}/chat` + `GET
  /api/oxpilot/sessions/{id}/chat/stream` SSE routes: turn claim, redacted
  persistence, buffered replay, snapshot resume, terminal semantics.
