# Support Contribution — aidlc-aws-platform-agent
**Stage:** infrastructure-design | **Unit:** u1-vcs-wire-foundation | **Verdict:** NEEDS CHANGES

All claimed lanes exist and cover the claims; core mechanics (`globs=[]`, scope attribution, BR1.2 atomicity, literal-version carrier) verified. Two factual errors in the wire-bump recipe + gate-honesty notes; all integrated.

1. **[MINOR] `feat!(wire)` is grammar-invalid** — `HEADER_RE` puts `!` after the scope paren: `feat(wire)!:`. **Fixed.**
2. **[MINOR] `BREAKING CHANGE:` footer can't attribute to `wire`** — `globs=[]` means attribution is scope-only; `!`/footer marks level only. Reworded: bump fires on `wire`/`rpc` scope; `!` marks intent.
3. **[NOTE] The wire-scope pin is convention-enforced, not CI** — commit-check validates grammar only; unmapped scopes warn+pass. Named honestly in the Gate column.
4. **[NOTE] Atomicity is per landed-tree, not per-commit; Cargo.lock is in the atomic set** — `check` validates the checked-out tree; within one PR commits can span, but one commit is the safe form. **Fixed.**
5. **[NOTE] Third WIRE_VERSION pin site: `docs/openapi.yaml` `info.version`** — `bump.ts` doesn't rewrite it; `openapi-sync-check` gates; `make openapi-gen` at release. **Pinned.**
6. **[NOTE] ci-image `-latest` fallback will annotate this PR — benign** (Cargo.toml/lockfile are hash inputs; ci-base publishes the new tag on merge). Recorded.
7. **[NOTE] `dead-code` job also runs `cargo doc -D warnings`; `rust-fmt`/`web-format-check` gate new files.** **Mapped.**
8. **[NOTE] Rollback wrinkle** — default `Revert "…"` subjects are attribution-exempt; a wire-visible revert needs `revert(wire):`. **Pinned.**

Verified: all Makefile lanes exist (test:237, rpc-sync-check:104, version-check:122, dead-code-check:402, web-lint:386, web-format-check:392, test-web-browser:412); CI jobs exist (commit-lint:114, api-rust:269, rpc-sync:563, dead-code:307, web-octane:279, web-browser:337); scope→component map at commit.ts:127→plan.ts:83; BR1.2 check.ts:110-120; literal-version workspace.ts:92-104; parity tests rpc.rs:1852-1910; api-client not a knip workspace; repo.activity.list absent from REST table (no openapi mapping needed for the DTO widening).
