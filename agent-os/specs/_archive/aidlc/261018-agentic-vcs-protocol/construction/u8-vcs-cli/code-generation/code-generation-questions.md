# Code Generation — Questions (u8-vcs-cli)

## Plan Approval

Approve the code-generation plan and unit-test instructions for `u8-vcs-cli`: the `oxidean-vcs` `client` feature (`VcsTransport` injected seam, `CliSurface` 16-command `ox vcs` implementations returning `CliOutcome{envelope,pretty}`, `WorkingCopy` sparse checkout over paged `vcs.tree` with `PathFilter` + non-empty-target refusal, `ObjectCache` bounded sha256-verified shared cache, `CheckoutResult`), the `oxidean-cli` thin glue (`Command::Vcs { sub: VcsSub }` + USAGE/help rows + `required_procedures` + `CLI_PROCS` + `CliVcsTransport` over `Client`/`HttpClient`), the `error_exit_code` additions mapping `vcs.unauthenticated`/`vcs.forbidden` to exit 3, and the Cargo.toml reshuffle moving `oxidean-core`/`serde`/`serde_json`/`base64`/`sha2`/`hex`/`async-trait` to shared deps so the client feature carries zero `oxidean-db`/`oxidean-git`/`sqlx` edges.

[Approval Fingerprint]: sha256:v3:7f5a15e320e098feca3065a0f79b5943761be3a30ee7199a8d1c4cd9127e1c35
[Planned Source]: 36082e8df398fbad741f503d5e240f647f60664d02c1a8ab8ff4838686fd2744

1. Approve Plan — proceed to code generation
2. Request Changes — revise the plan

[Answer]: Approve Plan
