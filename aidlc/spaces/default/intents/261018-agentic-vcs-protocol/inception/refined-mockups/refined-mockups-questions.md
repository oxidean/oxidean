# Refined Mockups — Interview

## Q1. What is the "UI surface" for this feature?

The stories span machine and human surfaces. Rough-mockups was skipped by scope (classic has no Ideation phase), so this stage designs directly from `stories.md` + `requirements.md`. What does the refined-design surface actually cover?

**Options:**
- A. Machine-facing UX only — `ox vcs` CLI flows, MCP tool call/result shapes, RPC request/response envelopes; zero Octane screens (web-UI history views are Won't-band, O5)
- B. Machine surfaces plus the minimal repo-settings toggle — one Octane control on the existing `[owner]/[repo]/settings` page so Sana can enable/disable VCS without a CLI
- C. Machine surfaces plus a read-only VCS history section in repo pages

[Answer]: B — the enable/disable control is a real operator affordance and the settings page already exists (AC1.1/AC1.2); everything else is machine-facing per requirements (O5 excludes web history views; history browsing happens through `vcs.log`/`vcs.diff` via CLI/MCP/RPC). Option C would reintroduce the excluded web-UI surface.

## Q2. CLI interaction pattern for `ox vcs`?

The AC8.4.1 subcommand set: `enable`, `status`, `commit`, `log`, `diff`, `show`/`cat`, `blame`, `history`, `merge-base`, `merge`, `rebase`, `checkout`, `ref`, `import`, `export`.

**Options:**
- A. Noun-verb mirroring `ox`'s existing conventions (`ox vcs <verb>`), human `text` output + `--json` verbatim `{ok}` envelope per `Out::Rpc`
- B. Flat single-word commands (`ox vcs-commit`, …)

[Answer]: A — `ox vcs <subcommand>` is already ratified by FR12.1/AC8.4.1; output follows the `Out::Rpc {envelope, pretty}` idiom where `--json` prints the RPC envelope verbatim and `{ok:false}` drives the exit code via `rpc::error_exit_code`. Errors surface the typed `vcs.*` code + message on stderr (`ox: <code>: <message>`), exit non-zero.

## Q3. How do machine users perceive state — the DX contract?

For API-first consumers (Ava, Kit), the "interface" is response shape + error semantics + discoverability.

**Options:**
- A. Typed `{ok:true,data}` / `{ok:false,error:{code,message}}` envelopes; `system.manifest` advertises the family; every procedure's input/output schema generated into `@oxidean/api-client`
- B. Prose/CLI-first only

[Answer]: A — matches the existing RPC envelope and NFR5/NFR8 wire-stability requirements; discoverability via `system.manifest` (AC8.1.2), raw escape hatch `ox api vcs.*` (AC8.4.3).

## Q4. What states must each surface handle?

**Options:**
- A. Full state matrix per surface: CLI (success/conflict/not-found/disabled + non-zero exits), MCP (`isError` tool content for domain denials per AC8.3.2), settings toggle (enabled/disabled/saving/error), RPC (typed error codes from the AC8.1.4 taxonomy)
- B. Happy path only

[Answer]: A — the error taxonomy (AC8.1.4) and the fail-closed negative paths (AC1.2.x, AC8.5.x, AC8.6.x) are already behavioral requirements; the mockups must show them.

## Q5. Design-system alignment for the settings toggle?

**Options:**
- A. Reuse existing `components/settings/` section pattern + standard checkbox/confirm affordances; no new visual system
- B. New component treatment for VCS

[Answer]: A — one small control inside an existing settings section; Octane `.tsrx`, existing form/query helpers, no new framework. The toggle is the only Octane element in this feature.

## Q6. Accessibility requirements?

**Options:**
- A. WCAG 2.1 AA for the settings control (labelled, keyboard-operable, focus-visible, error announced); terminal surfaces stay screen-reader-friendly (no color-only semantics, stable text output)
- B. None

[Answer]: A — AA matches the web app's existing posture; machine surfaces get "no color-only meaning / deterministic plain-text output" as the accessibility analogue.

## Q7. Responsive breakpoints?

[Answer]: Not applicable to CLI/MCP/RPC; the settings control inherits the existing settings page's responsive layout (no new breakpoint behavior introduced).

## Q8. Pagination/streaming affordances?

`vcs.log`, `vcs.tree`, `vcs.path_history`, `vcs.blame` are paginated (AC4.1.2); CLI `--limit`/`--cursor` flags and `--json` cursor passthrough?

[Answer]: Yes — CLI exposes `--limit`/`--cursor` (and `--all` with an explicit "may page many times" note); `--json` echoes the cursor so scripts can loop; open question R-14 (whether `vcs.diff` joins the pagination contract) is parked for contract-design.

## Assumption Confirmation

`[Answer]: Looks correct`
