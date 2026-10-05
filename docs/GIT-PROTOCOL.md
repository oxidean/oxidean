# Git protocol surface

Audit record for GIT-27: what the forge supports on the Git wire protocol, on
both transports. Behavior is delegated to the system `git-upload-pack` /
`git-receive-pack` binaries; Oxidean owns only the transport plumbing that
delivers client intent to those binaries.

## Feature × transport matrix

| Feature | Smart HTTP | SSH | Notes |
| --- | --- | --- | --- |
| Protocol v2 | Supported | Supported | Client opt-in via `git -c protocol.version=2 …` or `protocol.version=2` config. |
| Protocol v0 | Supported | Supported | Default when the client sends no version request. No forced upgrade. |
| Partial clone (`--filter=blob:none`) | Supported | Supported | `uploadpack.allowFilter=true` is injected per spawn; see below. |
| Partial clone (`--filter=tree:0`) | Supported | Supported | Same code path as `blob:none`; verified locally. |
| Partial clone (other filters) | Supported | Supported | `blob:limit`, sparse `combine`, etc. share the `filter` capability. |
| Lazy backfill (promisor fetch) | Supported | Supported | Missing blobs are fetched on demand over the same remote. |
| Shallow clone (`--depth`) | Supported | Supported | `fetch=shallow` capability is advertised by upload-pack. |
| Shallow deepen (`--deepen`, `--unshallow`) | Supported | Supported | Same shallow capability covers deepen requests. |
| Push from a shallow clone | Supported | Supported | Receive-pack accepts pushes whose history the server can verify; pushing a commit whose parent is beyond the client's shallow boundary is fine because the server holds full history. |
| Push with `--depth` (shallow receive) | Unsupported | Unsupported | `receive-pack` does not advertise shallow receive; Git itself does not implement receiving into a shallow boundary. This is upstream Git behavior, not a forge restriction. |

## Smart HTTP plumbing

`crates/oxidean-api/src/routes/git_smart_http.rs` bridges requests to
`git-http-backend` as a CGI process (`git/http_backend.rs`). Relevant CGI env:

| CGI env | Source |
| --- | --- |
| `GIT_PROTOCOL` | Value of the client's `Git-Protocol` request header, forwarded verbatim when present. |
| `GIT_PROJECT_ROOT` | `OXIDEAN_REPOS_DIR` |
| `GIT_HTTP_EXPORT_ALL` | `1` (repos authorize at the route layer, not via `git-daemon-export-ok`) |
| `PATH_INFO` / `QUERY_STRING` / `REQUEST_METHOD` | Request |
| `CONTENT_TYPE` / `CONTENT_LENGTH` | Request headers / body |
| `REMOTE_USER` | Authenticated username (Basic PAT) |
| `GIT_CONFIG_COUNT` + `GIT_CONFIG_KEY_0=uploadpack.allowFilter` + `GIT_CONFIG_VALUE_0=true` | Fixed at spawn; enables the `filter` capability for every repository, including ones created before the flag existed. |

## SSH plumbing

`crates/oxidean-api/src/ssh/` runs an in-process `russh` listener.
`ssh/pack.rs` spawns `git-upload-pack` / `git-receive-pack` with argv only (no
shell). Channel `env` requests are filtered by an allowlist:

- `GIT_PROTOCOL` is accepted (nonempty, ≤256 bytes, printable ASCII) and
  forwarded to the spawned service. The last accepted value wins, matching
  OpenSSH `setenv` semantics.
- Every other name — `LD_PRELOAD`, `GIT_DIR`, etc. — is refused with
  `channel_failure`.

Upload-pack additionally gets `GIT_CONFIG_*` for `uploadpack.allowFilter`;
receive-pack gets the `OXIDEAN_*` protection-hook pairs (D-PKG-01).

## Design notes

- The forge relies on system Git's own promisor/shallow machinery; there is no
  custom object-serving layer. If a filter shape fails, it fails the same way
  stock `git upload-pack` would.
- Receiving pushes *into* a shallow boundary is not part of the Git protocol
  on any transport; documenting rather than fixing.
- `GIT_CONFIG_*` env injection was chosen over writing `uploadpack.allowFilter`
  into each bare repo's `config` so existing repositories need no migration.

## Test coverage

`crates/oxidean-api/tests/git_protocol.rs` exercises both transports with real
`git` and `ssh` client binaries:

- Smart HTTP: v2 `ls-remote`, `blob:none` clone + lazy blob backfill,
  `--depth=1` clone + `--deepen`, and push from a shallow clone.
- SSH (russh client): env-request accept for `GIT_PROTOCOL`, protocol v0
  control without it, refusal of non-allowlisted names.
- SSH (OpenSSH client e2e): v2 clone, `blob:none` clone, `--depth=1` clone +
  `--deepen`.
