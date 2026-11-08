# Web UI

The web app is **Octane** — `.tsrx` islands on Astro static shells — never
React JSX. Load `.agents/skills/octane/SKILL.md` before editing `apps/web`;
`.cursor/rules/octane-ui.mdc` encodes the same rules for Cursor.

Non-negotiables:

- Author `.tsrx` with Rivet templates (`@{`, `@if`/`@else`, `@for`,
  `function Comp() @{ … }`); never mix `return (` JSX with Rivet directives
  in one component.
- Server/session data: TanStack Query via `apps/web/src/lib/session-queries.ts`;
  forms via `@octanejs/tanstack-form`; uploads via `@octanejs/dropzone`.
- Text fields use native `onInput`; `useState` for local UI state only — no
  Zustand for server data.
- DOM races: don't `@if`/`@else`-swap sibling trees next to Base UI
  RadioGroup/Select or nest `@if`/`@for` inside `form.Subscribe` bodies;
  keep panels mounted and toggle `hidden` — see `docs/CODE_PRACTICES.md`
  § Octane UI.
- Gates before committing `apps/web`: `make web-lint` + `make web-format-check`;
  high-risk interactive UI needs `*.browser.test.tsx` + manifest row.
