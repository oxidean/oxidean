# Cross-Unit Traceability — agentic-vcs-protocol

Final coverage gate over every `FR`/`NFR` from requirements.md and every
three-segment `AC` from stories.md, resolved across all 10 unit
`traceability.json` files plus the stage-level
`construction/code-generation/traceability.json` (which carries IDs whose
implementations span units — the owning target files are listed, not a unit).

**Verdict: PASS — 160/160 enumerated IDs covered; every cited target file
exists on disk.** No uncovered elements.

| ID | Owning unit | Target | Verdict |
|----|-------------|--------|---------|
| AC1.1.1 | u10-vcs-web-surfaces | apps/web/src/routes/$owner.$repo.settings.tsrx, apps/web/src/components/repo/vcs-settings-panel.tsrx | PASS |
| AC1.1.2 | u1-vcs-wire-foundation | VcsErrorCode::Disabled + stub arms | PASS |
| AC1.1.3 | stage | crates/oxidean-db/src/vcs.rs, crates/oxidean-vcs/src/api.rs | PASS |
| AC1.2.1 | u7-vcs-api-surface | crates/oxidean-vcs/src/api.rs | PASS |
| AC1.2.2 | u3-vcs-object-store | crates/oxidean-vcs/src/object_store/mod.rs | PASS |
| AC1.2.3 | u10-vcs-web-surfaces | apps/web/src/components/repo/vcs-settings-panel.tsrx | PASS |
| AC1.3.1 | u3-vcs-object-store | crates/oxidean-vcs/src/object_store/gc.rs | PASS |
| AC1.3.2 | u3-vcs-object-store | crates/oxidean-db/src/vcs.rs | PASS |
| AC1.3.3 | u2-vcs-persistence | vcs_objects global + vcs_object_links attribution | PASS |
| AC2.1.1 | u4-vcs-changeset-core | implementation + unit tests (code-generation-plan.md deliverables + unit-test-instructions.md matrix) | PASS |
| AC2.1.2 | u4-vcs-changeset-core | implementation + unit tests (code-generation-plan.md deliverables + unit-test-instructions.md matrix) | PASS |
| AC2.1.3 | u4-vcs-changeset-core | implementation + unit tests (code-generation-plan.md deliverables + unit-test-instructions.md matrix) | PASS |
| AC2.1.4 | u4-vcs-changeset-core | implementation + unit tests (code-generation-plan.md deliverables + unit-test-instructions.md matrix) | PASS |
| AC2.1.5 | u4-vcs-changeset-core | implementation + unit tests (code-generation-plan.md deliverables + unit-test-instructions.md matrix) | PASS |
| AC2.2.1 | u4-vcs-changeset-core | implementation + unit tests (code-generation-plan.md deliverables + unit-test-instructions.md matrix) | PASS |
| AC2.2.2 | u4-vcs-changeset-core | implementation + unit tests (code-generation-plan.md deliverables + unit-test-instructions.md matrix) | PASS |
| AC2.2.3 | u4-vcs-changeset-core | implementation + unit tests (code-generation-plan.md deliverables + unit-test-instructions.md matrix) | PASS |
| AC2.3.1 | u4-vcs-changeset-core | implementation + unit tests (code-generation-plan.md deliverables + unit-test-instructions.md matrix) | PASS |
| AC2.3.2 | u4-vcs-changeset-core | implementation + unit tests (code-generation-plan.md deliverables + unit-test-instructions.md matrix) | PASS |
| AC2.4.1 | u4-vcs-changeset-core | implementation + unit tests (code-generation-plan.md deliverables + unit-test-instructions.md matrix) | PASS |
| AC2.4.2 | stage | crates/oxidean-vcs/src/merge_engine.rs | PASS |
| AC2.4.3 | stage | crates/oxidean-vcs/src/merge_engine.rs | PASS |
| AC2.5.1 | stage | crates/oxidean-vcs/src/merge_engine.rs | PASS |
| AC2.5.2 | stage | crates/oxidean-vcs/src/merge_engine.rs | PASS |
| AC2.6.1 | u1-vcs-wire-foundation | VCS_ACTIVITY_* constants + RepoActivityItem widening | PASS |
| AC2.6.2 | u4-vcs-changeset-core | implementation + unit tests (code-generation-plan.md deliverables + unit-test-instructions.md matrix) | PASS |
| AC2.7.1 | u1-vcs-wire-foundation | error taxonomy + stub envelope | PASS |
| AC2.7.2 | u10-vcs-web-surfaces | apps/web/src/components/repo/repo-activity-row.tsrx | PASS |
| AC2.7.3 | u2-vcs-persistence | feeds.rs/admin/users.rs Option-safe consumers | PASS |
| AC3.1.1 | u3-vcs-object-store | crates/oxidean-vcs/src/object_store/mod.rs | PASS |
| AC3.1.2 | u3-vcs-object-store | crates/oxidean-vcs/src/object_store/mod.rs | PASS |
| AC3.1.3 | u3-vcs-object-store | crates/oxidean-vcs/src/object_store/mod.rs | PASS |
| AC3.2.1 | u3-vcs-object-store | crates/oxidean-vcs/src/object_store/mod.rs | PASS |
| AC3.2.2 | u3-vcs-object-store | crates/oxidean-vcs/src/object_store/mod.rs | PASS |
| AC3.3.1 | u3-vcs-object-store | crates/oxidean-vcs/src/object_store/mod.rs | PASS |
| AC3.3.2 | u3-vcs-object-store | crates/oxidean-vcs/src/object_store/mod.rs | PASS |
| AC3.3.3 | u3-vcs-object-store | crates/oxidean-vcs/src/object_store/mod.rs | PASS |
| AC3.4.1 | u3-vcs-object-store | crates/oxidean-vcs/src/object_store/mod.rs | PASS |
| AC3.4.2 | u3-vcs-object-store | crates/oxidean-vcs/src/object_store/mod.rs | PASS |
| AC3.5.1 | u3-vcs-object-store | crates/oxidean-vcs/src/object_store/gc.rs | PASS |
| AC3.5.2 | u4-vcs-changeset-core | implementation + unit tests (code-generation-plan.md deliverables + unit-test-instructions.md matrix) | PASS |
| AC3.5.3 | u3-vcs-object-store | crates/oxidean-vcs/src/object_store/mod.rs | PASS |
| AC4.1.1 | u4-vcs-changeset-core | implementation + unit tests (code-generation-plan.md deliverables + unit-test-instructions.md matrix) | PASS |
| AC4.1.2 | u4-vcs-changeset-core | implementation + unit tests (code-generation-plan.md deliverables + unit-test-instructions.md matrix) | PASS |
| AC4.1.3 | u4-vcs-changeset-core | implementation + unit tests (code-generation-plan.md deliverables + unit-test-instructions.md matrix) | PASS |
| AC4.2.1 | u4-vcs-changeset-core | implementation + unit tests (code-generation-plan.md deliverables + unit-test-instructions.md matrix) | PASS |
| AC4.2.2 | u4-vcs-changeset-core | implementation + unit tests (code-generation-plan.md deliverables + unit-test-instructions.md matrix) | PASS |
| AC4.3.1 | u4-vcs-changeset-core | implementation + unit tests (code-generation-plan.md deliverables + unit-test-instructions.md matrix) | PASS |
| AC4.3.2 | u4-vcs-changeset-core | implementation + unit tests (code-generation-plan.md deliverables + unit-test-instructions.md matrix) | PASS |
| AC4.4.1 | u4-vcs-changeset-core | implementation + unit tests (code-generation-plan.md deliverables + unit-test-instructions.md matrix) | PASS |
| AC4.5.1 | u4-vcs-changeset-core | implementation + unit tests (code-generation-plan.md deliverables + unit-test-instructions.md matrix) | PASS |
| AC4.5.2 | u4-vcs-changeset-core | implementation + unit tests (code-generation-plan.md deliverables + unit-test-instructions.md matrix) | PASS |
| AC5.1.1 | u1-vcs-wire-foundation | Nullable<T> CAS fields on VcsRefUpdateRequest | PASS |
| AC5.1.2 | u4-vcs-changeset-core | implementation + unit tests (code-generation-plan.md deliverables + unit-test-instructions.md matrix) | PASS |
| AC5.1.3 | u7-vcs-api-surface | crates/oxidean-vcs/src/api.rs | PASS |
| AC5.1.4 | u4-vcs-changeset-core | implementation + unit tests (code-generation-plan.md deliverables + unit-test-instructions.md matrix) | PASS |
| AC5.2.1 | u4-vcs-changeset-core | implementation + unit tests (code-generation-plan.md deliverables + unit-test-instructions.md matrix) | PASS |
| AC5.2.2 | u4-vcs-changeset-core | implementation + unit tests (code-generation-plan.md deliverables + unit-test-instructions.md matrix) | PASS |
| AC5.2.3 | u4-vcs-changeset-core | implementation + unit tests (code-generation-plan.md deliverables + unit-test-instructions.md matrix) | PASS |
| AC6.1.1 | u8-vcs-cli | crates/oxidean-vcs/src/client/working_copy.rs | PASS |
| AC6.1.2 | u8-vcs-cli | crates/oxidean-vcs/src/client/working_copy.rs | PASS |
| AC6.1.3 | u8-vcs-cli | crates/oxidean-vcs/src/client/working_copy.rs | PASS |
| AC6.2.1 | u8-vcs-cli | crates/oxidean-vcs/src/client/working_copy.rs | PASS |
| AC6.2.2 | u8-vcs-cli | crates/oxidean-vcs/src/client/working_copy.rs | PASS |
| AC6.3.1 | u8-vcs-cli | crates/oxidean-vcs/src/client/working_copy.rs | PASS |
| AC7.1.1 | stage | crates/oxidean-vcs/src/git_interop.rs | PASS |
| AC7.1.2 | u7-vcs-api-surface | crates/oxidean-vcs/src/api.rs | PASS |
| AC7.1.3 | stage | crates/oxidean-vcs/src/git_interop.rs | PASS |
| AC7.2.1 | stage | crates/oxidean-vcs/src/git_interop.rs | PASS |
| AC7.2.2 | stage | crates/oxidean-vcs/src/git_interop.rs | PASS |
| AC7.2.3 | stage | crates/oxidean-vcs/src/git_interop.rs | PASS |
| AC7.2.4 | stage | crates/oxidean-vcs/src/git_interop.rs | PASS |
| AC7.4.1 | stage | crates/oxidean-vcs/src/git_interop.rs | PASS |
| AC7.4.2 | u4-vcs-changeset-core | implementation + unit tests (code-generation-plan.md deliverables + unit-test-instructions.md matrix) | PASS |
| AC8.1.1 | u1-vcs-wire-foundation | vcs_types.rs DTOs + PROCEDURES + dispatch arms | PASS |
| AC8.1.2 | u1-vcs-wire-foundation | PROCEDURES + manifest_shape | PASS |
| AC8.1.3 | u1-vcs-wire-foundation | /api/vcs/* alias classification | PASS |
| AC8.1.4 | u1-vcs-wire-foundation | VcsErrorCode closed enum | PASS |
| AC8.2.1 | u1-vcs-wire-foundation | rpc_gen.rs template + regenerated index.ts | PASS |
| AC8.2.2 | u1-vcs-wire-foundation | crates/oxidean-vcs skeleton + versioning.toml [[component]] + proc_classes + rpc-gen | PASS |
| AC8.3.1 | u9-vcs-mcp | crates/oxidean-vcs/src/client/mcp.rs | PASS |
| AC8.3.2 | u9-vcs-mcp | crates/oxidean-api/src/mcp.rs | PASS |
| AC8.3.3 | u9-vcs-mcp | crates/oxidean-vcs/src/client/mcp.rs | PASS |
| AC8.4.1 | u8-vcs-cli | crates/oxidean-cli/src/commands.rs, crates/oxidean-cli/src/rpc.rs, crates/oxidean-cli/src/main.rs, crates/oxidean-cli/src/http.rs | PASS |
| AC8.4.2 | u8-vcs-cli | crates/oxidean-cli/src/commands.rs, crates/oxidean-cli/src/rpc.rs, crates/oxidean-cli/src/main.rs, crates/oxidean-cli/src/http.rs | PASS |
| AC8.4.3 | u8-vcs-cli | crates/oxidean-cli/src/commands.rs, crates/oxidean-cli/src/rpc.rs, crates/oxidean-cli/src/main.rs, crates/oxidean-cli/src/http.rs | PASS |
| AC8.5.1 | u1-vcs-wire-foundation | VcsErrorCode::Unauthenticated/Forbidden + object-route alias classification | PASS |
| AC8.5.2 | stage | crates/oxidean-vcs/src/api.rs | PASS |
| AC8.6.1 | u1-vcs-wire-foundation | proc_classes vcs.* block + coverage test | PASS |
| AC8.6.2 | stage | crates/oxidean-api/src/lib.rs | PASS |
| AC8.6.3 | stage | crates/oxidean-vcs/src/api.rs | PASS |
| FR1.1 | u10-vcs-web-surfaces | apps/web/src/components/repo/vcs-settings-panel.tsrx, apps/web/src/routes/$owner.$repo.settings.tsrx, apps/web/src/components/repo/repo-settings-nav.tsrx | PASS |
| FR1.2 | stage | crates/oxidean-db/src/vcs.rs, crates/oxidean-vcs/src/api.rs | PASS |
| FR1.3 | stage | crates/oxidean-vcs/src/api.rs, crates/oxidean-db/src/vcs.rs | PASS |
| FR1.4 | u10-vcs-web-surfaces | apps/web/src/components/repo/vcs-settings-panel.tsrx | PASS |
| FR1.5 | u3-vcs-object-store | crates/oxidean-vcs/src/object_store/mod.rs | PASS |
| FR2.1 | u4-vcs-changeset-core | implementation + unit tests (code-generation-plan.md deliverables + unit-test-instructions.md matrix) | PASS |
| FR2.2 | u4-vcs-changeset-core | implementation + unit tests (code-generation-plan.md deliverables + unit-test-instructions.md matrix) | PASS |
| FR2.3 | u4-vcs-changeset-core | implementation + unit tests (code-generation-plan.md deliverables + unit-test-instructions.md matrix) | PASS |
| FR2.4 | u4-vcs-changeset-core | implementation + unit tests (code-generation-plan.md deliverables + unit-test-instructions.md matrix) | PASS |
| FR2.5 | u4-vcs-changeset-core | implementation + unit tests (code-generation-plan.md deliverables + unit-test-instructions.md matrix) | PASS |
| FR3.1 | u3-vcs-object-store | crates/oxidean-vcs/src/object_store/mod.rs | PASS |
| FR3.2 | u3-vcs-object-store | crates/oxidean-vcs/src/object_store/mod.rs | PASS |
| FR3.3 | u3-vcs-object-store | crates/oxidean-vcs/src/object_store/gc.rs | PASS |
| FR3.4 | u3-vcs-object-store | crates/oxidean-vcs/src/object_store/mod.rs | PASS |
| FR4.1 | stage | crates/oxidean-vcs/src/client/working_copy.rs, crates/oxidean-vcs/src/client/object_cache.rs | PASS |
| FR4.2 | stage | crates/oxidean-vcs/src/client/working_copy.rs, crates/oxidean-vcs/src/client/object_cache.rs | PASS |
| FR4.3 | stage | crates/oxidean-vcs/src/client/working_copy.rs, crates/oxidean-vcs/src/object_store/mod.rs | PASS |
| FR4.4 | stage | crates/oxidean-vcs/src/client/working_copy.rs | PASS |
| FR5.1 | u4-vcs-changeset-core | implementation + unit tests (code-generation-plan.md deliverables + unit-test-instructions.md matrix) | PASS |
| FR5.2 | u4-vcs-changeset-core | implementation + unit tests (code-generation-plan.md deliverables + unit-test-instructions.md matrix) | PASS |
| FR5.3 | u4-vcs-changeset-core | implementation + unit tests (code-generation-plan.md deliverables + unit-test-instructions.md matrix) | PASS |
| FR5.4 | u4-vcs-changeset-core | implementation + unit tests (code-generation-plan.md deliverables + unit-test-instructions.md matrix) | PASS |
| FR6.1 | u4-vcs-changeset-core | implementation + unit tests (code-generation-plan.md deliverables + unit-test-instructions.md matrix) | PASS |
| FR6.2 | u4-vcs-changeset-core | implementation + unit tests (code-generation-plan.md deliverables + unit-test-instructions.md matrix) | PASS |
| FR6.3 | u4-vcs-changeset-core | implementation + unit tests (code-generation-plan.md deliverables + unit-test-instructions.md matrix) | PASS |
| FR7.1 | stage | crates/oxidean-vcs/src/changeset_store.rs, crates/oxidean-vcs/src/api.rs | PASS |
| FR7.2 | stage | crates/oxidean-vcs/src/merge_engine.rs | PASS |
| FR7.3 | stage | crates/oxidean-vcs/src/merge_engine.rs | PASS |
| FR7.4 | u4-vcs-changeset-core | implementation + unit tests (code-generation-plan.md deliverables + unit-test-instructions.md matrix) | PASS |
| FR8.1 | u8-vcs-cli | crates/oxidean-vcs/src/client/working_copy.rs | PASS |
| FR8.2 | u8-vcs-cli | crates/oxidean-vcs/src/client/object_cache.rs | PASS |
| FR8.3 | u8-vcs-cli | crates/oxidean-vcs/src/client/cli.rs | PASS |
| FR9.1 | stage | crates/oxidean-vcs/src/git_interop.rs | PASS |
| FR9.2 | stage | crates/oxidean-vcs/src/git_interop.rs | PASS |
| FR9.3 | stage | crates/oxidean-vcs/src/git_interop.rs | PASS |
| FR9.4 | u4-vcs-changeset-core | implementation + unit tests (code-generation-plan.md deliverables + unit-test-instructions.md matrix) | PASS |
| FR10.1 | u4-vcs-changeset-core | implementation + unit tests (code-generation-plan.md deliverables + unit-test-instructions.md matrix) | PASS |
| FR10.2 | stage | crates/oxidean-vcs/src/lib.rs, crates/oxidean-cli/src/commands.rs, packages/api-client/src/index.ts | PASS |
| FR10.3 | stage | crates/oxidean-api/src/lib.rs | PASS |
| FR11.1 | u3-vcs-object-store | crates/oxidean-vcs/src/object_store/mod.rs | PASS |
| FR11.2 | u3-vcs-object-store | crates/oxidean-vcs/src/object_store/mod.rs | PASS |
| FR11.3 | stage | crates/oxidean-api/src/app.rs | PASS |
| FR12.1 | u8-vcs-cli | crates/oxidean-vcs/src/client/cli.rs, crates/oxidean-cli/src/args.rs | PASS |
| FR12.2 | stage | crates/oxidean-vcs/src/client/cli.rs, crates/oxidean-cli/src/commands.rs, crates/oxidean-cli/src/args.rs | PASS |
| FR12.3 | stage | crates/oxidean-cli/src/args.rs, packages/api-client/src/index.ts | PASS |
| FR13.1 | u9-vcs-mcp | crates/oxidean-vcs/src/client/mcp.rs | PASS |
| FR13.2 | u9-vcs-mcp | crates/oxidean-api/src/mcp.rs | PASS |
| FR13.3 | u9-vcs-mcp | crates/oxidean-vcs/src/client/mcp.rs, crates/oxidean-api/src/mcp.rs | PASS |
| FR14.1 | stage | crates/oxidean-vcs/src/api.rs | PASS |
| FR14.2 | stage | crates/oxidean-api/src/lib.rs | PASS |
| FR14.3 | stage | crates/oxidean-vcs/src/api.rs | PASS |
| FR14.4 | stage | crates/oxidean-vcs/src/api.rs, crates/oxidean-vcs/src/object_store/config.rs | PASS |
| FR15.1 | stage | crates/oxidean-vcs/src/lib.rs | PASS |
| FR15.2 | stage | crates/oxidean-api/src/app.rs, crates/oxidean-api/src/lib.rs | PASS |
| FR15.3 | stage | crates/oxidean-cli/src/commands.rs, crates/oxidean-api/src/mcp.rs | PASS |
| FR15.4 | stage | versioning.toml, docs/ARCHITECTURE.md | PASS |
| FR15.5 | u4-vcs-changeset-core | implementation + unit tests (code-generation-plan.md deliverables + unit-test-instructions.md matrix) | PASS |
| FR16.1 | u10-vcs-web-surfaces | apps/web/src/components/repo/repo-activity-row.tsrx, apps/web/src/routes/$owner.$repo.activity.tsrx | PASS |
| FR16.2 | stage | crates/oxidean-vcs/src/api.rs, crates/oxidean-vcs/src/activity.rs | PASS |
| NFR1 | u4-vcs-changeset-core | implementation + unit tests (code-generation-plan.md deliverables + unit-test-instructions.md matrix) | PASS |
| NFR2 | stage | crates/oxidean-vcs/src/changeset_store.rs, crates/oxidean-vcs/src/ref_registry.rs | PASS |
| NFR3 | stage | crates/oxidean-vcs/src/object_store/mod.rs, crates/oxidean-vcs/src/changeset_store.rs | PASS |
| NFR4 | stage | crates/oxidean-vcs/src/api.rs | PASS |
| NFR5 | stage | crates/oxidean-api/src/lib.rs, crates/oxidean-vcs/src/lib.rs | PASS |
| NFR6 | u4-vcs-changeset-core | implementation + unit tests (code-generation-plan.md deliverables + unit-test-instructions.md matrix) | PASS |
| NFR7 | u4-vcs-changeset-core | implementation + unit tests (code-generation-plan.md deliverables + unit-test-instructions.md matrix) | PASS |
| NFR8 | u4-vcs-changeset-core | implementation + unit tests (code-generation-plan.md deliverables + unit-test-instructions.md matrix) | PASS |
| NFR9 | u4-vcs-changeset-core | implementation + unit tests (code-generation-plan.md deliverables + unit-test-instructions.md matrix) | PASS |
| NFR10 | u4-vcs-changeset-core | implementation + unit tests (code-generation-plan.md deliverables + unit-test-instructions.md matrix) | PASS |
