# Rust crates

Crate-level conventions live in `.cursor/rules/rust-crates.mdc` and the
in-repo skills `rust-best-practices` / `rust-async-patterns`. Core points:

- Workspace layout: `oxidean-web` (edge) → `oxidean-api` (router + RPC
  dispatch + domain handlers) → `oxidean-db` `Database` façade +
  `oxidean-git` `GitBackend`; `oxidean-core` is types only, no I/O.
- No `services/` layer — domain internals are named structs on
  `AppState`/`RpcCtx`.
- New subsystems get a trait+adapter seam (the `GitBackend`/`CliGitBackend`
  and `EmailSender` precedent), not inline code in `oxidean-api`.
- REST `/api/v1` is a pure translation façade over the same dispatch;
  protocol surfaces (smart HTTP, LFS, SSH, registries, OAuth, MCP, actions)
  are sibling route stacks sharing ACL/audit.
- Gates: `cargo fmt`, `clippy --workspace --all-targets -D warnings
  -D dead_code`, `cargo machete`, `RUSTDOCFLAGS="-D warnings"`.
