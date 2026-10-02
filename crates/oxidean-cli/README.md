# oxidean-cli

`ox` — companion command-line client for Oxidean forge instances: auth login,
repo/issue/PR/actions/packages operations, scriptable `--json` output, and a
raw `ox api` escape hatch over the typed `/api/rpc` surface.

| | |
|--|--|
| **Crate** | `oxidean-cli` |
| **Version** | `0.1.0` (workspace) |
| **License** | [MIT](../../LICENSE) |
| **Bin** | `ox` |

## Role

- Speaks `POST {instance}/api/rpc` with the same `{procedure, input}` envelope
  and `Oxidean-RPC-Version` header as `@oxidean/api-client`
- Authenticates with `Authorization: Bearer <pat>` (PAT on RPC lands with
  API-02; cookie-only instances return `auth.unauthenticated`)
- Stores instance URL + token under `~/.config/ox/config.json`
  (`$OXIDEAN_CONFIG_DIR` / `$XDG_CONFIG_HOME` honored)
- Depends on [`oxidean-core`](../oxidean-core/README.md) for the shared
  envelope types — no server-side crates

## Run

```bash
cargo build -p oxidean-cli          # target/debug/ox
./target/debug/ox auth login --instance https://forge.example.com --token oxidean_pat_…
./target/debug/ox repo list
```

User documentation: [docs/CLI.md](../../docs/CLI.md).

## Exit codes

`0` ok · `1` api/transport error · `2` usage · `3` auth.
