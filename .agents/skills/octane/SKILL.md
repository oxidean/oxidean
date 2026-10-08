---
name: octane
description: >-
  Octane UI authoring for Oxidean (`.tsrx`, Rivet templates, native events,
  TanStack Query via @octanejs). Use when editing apps/web components, routes,
  SSR loaders, or when tempted to write React JSX patterns. Prefer this over
  treating the UI as React.
---

# Octane (Oxidean)

Official reference: [octanejs.dev/llms.txt](https://octanejs.dev/llms.txt) · [Differences from React](https://octanejs.dev/docs/differences-from-react) · [TSRX vs TSX](https://octanejs.dev/docs/tsrx-vs-tsx)

Octane is Inferno’s successor with a React-*shaped* programming model (hooks, Suspense, transitions) but **AOT compilation**, **no virtual DOM**, and **native DOM events**. Oxidean ships UI almost entirely as **`.tsrx`**.

## Project defaults

| Do | Don’t |
|----|--------|
| Author UI in `.tsrx` with `function X() @{ … }` | Treat files as React and `return (` mixed with Rivet |
| Use `@if` / `@else`, `@for (…; key …)`, `@switch` | Invent `@else if` (unsupported — nest `@if` or use `@switch`) |
| Text inputs: `onInput` + controlled `value` | React synthetic `onChange` for per-edit text |
| Session/server data via `@octanejs/tanstack-query` | New global stores for `auth.me` / admin settings |
| Forms via `@octanejs/tanstack-form` (`useForm`, `onInput` + `field.handleChange`) | Parallel `useState` per field for multi-field forms |
| File pickers via `@octanejs/dropzone` / `FileDropzone` | Ad-hoc hidden `<input type="file">` without dropzone |
| Internal links via `AppLink` (`@/components/ui/app-link`) — client nav + intent preload, plain `<a>` fallback without a router | Raw `<a href="/…">` for app routes (full reload = theme flicker); `scripts/check-internal-anchors.ts` enforces |
| Forms: `method="post" action="#"` + `type="button"` where needed | Rely on GET navigations from submit |
| Anonymous auth pages: SSR loaders, no form skeletons | Skeleton-first anonymous login/signup |

Shared session helpers: `apps/web/src/lib/session-queries.ts`, `use-chrome-account.tsrx`, `query-client.ts`. Test with `apps/web/src/test/render-with-query.ts`.

Before committing web UI changes: `make web-lint` and `make web-format-check` (`@tsrx/oxc` — type-aware oxlint + oxfmt). See [AGENTS.md](../../../AGENTS.md).

## Authoring `.tsrx`

```tsrx
export function Example(props: { title: string }) {
  const [open, setOpen] = useState(false);

  @{
    <section>
      <h1>{props.title as string}</h1>
      @if (open) {
        <p>Visible</p>
      } @else {
        <p>Hidden</p>
      }
      <button type="button" onClick={() => setOpen(!open)}>Toggle</button>
    </section>
  }
}
```

- `@{ … }` is the template return shorthand — **one** output node (element or `<>…</>`).
- Dynamic text often needs `{expr as string}` when not provably a string.
- Hooks may be conditional (compiler call-site slots). Do **not** put a hook in a plain JS `for` loop — use `@for` or a child component.
- **Hook-calling helpers must live in `.tsrx`/`.tsx` files.** Slots are compiler-injected per call site — a helper in plain `.ts` (e.g. `useX() { const a = useQuery(...); const b = useQuery(...); }`) gives every hook call the same slot → shared observer/state → options rotation → refetch/remount loop. Renaming `foo.ts` → `foo.tsrx` is the fix; extensionless imports don't change.
- `useState` / `useReducer` expose a stable third tuple member `[state, set, getState]` when observed — prefer that over a ref for latest async state.
- Omit dependency arrays on `useEffect` / `useMemo` / `useCallback` when the compiler can infer them; explicit arrays keep React semantics; `null` means every render.
- Refs are props (`ref={…}`); no `forwardRef`.
- Use `class` / `className` (clsx-style). Prefer project Tailwind / existing chrome styles over new design systems.

## Events (native, not synthetic)

- Per-edit text: `onInput`.
- Commit-on-blur with native `change`: keep `onChange` and add `suppressNativeChangeWarning` — never add a noop `onInput` to silence warnings.
- Checkbox/radio: cancel activation in `onClick` if needed; native `change` is not cancelable the React way.

## Control flow

Template directives only inside `@{ }` / directive bodies:

- `@if (c) { } @else { }` — nest for else-if
- `@for (const x of xs; key x.id) { } @empty { }`
- `@switch (v) { @case (a) { } @default { } }`
- `@try { } @pending { } @catch (e) { }`
- **Each directive arm renders a single node** — `@if`/`@else`/`@for` bodies emitting multiple sibling elements must wrap them in `<>…</>` (octane ≥0.10 rejects multi-node arms that older compilers tolerated).
- **No `/*` in element text** — inside template text `/*` opens a block comment and swallows the closing tag to EOF ("Unclosed tag '<x>'"). Write such strings as expressions instead: `{"/api/actions/*" as string}`.

Plain JS control flow belongs in setup (above `@{`), not mixed as React ternary soup inside broken `return (` templates.

## Mixing React (rare)

Only if required: `ReactCompat` / `OctaneCompat` from `octane/react`. Do **not** alias `react` to Octane. Prefer `@octanejs/*` bindings (e.g. `@octanejs/tanstack-query`, `@octanejs/tanstack-router`) already used in-repo.

## Failure modes seen in this repo

1. **Export undefined / no hydration** — component used `return (` with `@if` / `@{` fragments → Vite import protection. Fix: full Rivet `@{` body.
2. **GET form submits** — missing `method="post" action="#"` or button `type="button"` on SPA forms.
3. **Duplicate `auth.me`** — bypass shared Query helpers; always go through session query options / cache helpers.
4. **`insertBefore` / HierarchyRequestError (“Something went wrong!”)** — Base UI controls that **mount/unmount** DOM on click (notably `Radio.Indicator` with default `keepMounted={false}`, and `<X.Portal>` on Dialog / AlertDialog / Menu) while Octane re-renders siblings in the same interaction. Portals return a fragment whose root count changes (`null` → `portalSubtree + createPortal`), so Octane computes a stale `insertBefore` anchor for the siblings next to it.
   - **Fix (prefer keeping Base UI):** set `keepMounted` on `Radio.Indicator` (our `RadioGroupItem` does this) and on every `<X.Portal>` — `DialogPortal`/`AlertDialogPortal`/`MenuPrimitive.Portal` in `components/ui/*` pass it so the portal stays mounted and toggles `hidden` instead of remounting. Also keep large auth/detail panels always mounted and toggle with `hidden` / `className` — do **not** `@if`/`@else` them next to the radio. Avoid replacing Base UI with plain buttons unless a primitive has no keepMounted-style escape hatch (e.g. `Select.Portal` — document with `octane-dom-race-ok`).
   - **Multi-root `@if`:** wrap multiple siblings in `<>…</>` — a bare `@if` with two root nodes also breaks reconciliation.
   - **Lint:** `scripts/check-octane-dom-races.ts` (part of `make web-lint`) flags `<X.Portal>`/`<X.X.Portal>` without `keepMounted`, raw `createPortal(`, `@if`/`@for` inside `form.Subscribe` / `*.Subscribe` bodies, and `onCheckedChange`/`onValueChange` + `form.setFieldValue` inside a broad `Subscribe` of `s.values`. Prefer `form.Field` per control, keep panels mounted with `hidden`, or extract a child component. Opt out documented cases with `// octane-dom-race-ok`.
   - **Checkbox / Radio:** our `Checkbox` and `RadioGroupItem` pass `keepMounted` on Indicators so check/uncheck does not remount the indicator span mid-reconciliation.
   - **Tests:** happy-dom `trackDomErrors()` (global in `setup-integration.ts`) is **necessary but not sufficient** — happy-dom often never throws the Chromium `insertBefore` path for Base UI + Octane. Prefer Vitest **browser** project (`*.browser.test.tsx`, `make test-web-browser`) for component click races, plus stack-browser `newGuardedPage()` / `pageerror` / `assertNoOctaneOverlay` for full-stack flows (e.g. `expectPatMintClickThroughFlow`, `expectMirrorAuthToggleFlow`). Opt out of the global assert with `allowDomRacesInThisTest()` (rare).
   - **Coverage gate:** high-risk surfaces (`Checkbox` / `RadioGroup` / Select / Switch / Dialog·Menu portals / `form.Subscribe`) must appear in `apps/web/src/test/browser-coverage.manifest.ts` — `make browser-coverage-check-pr` (CI `browser-coverage`). New or changed UI of this class needs a `*.browser.test.tsx` (use `pickSelectOptionByTestId` for Base UI Select); skip-only is rejected for touched surfaces (see [docs/TESTING.md](../../../docs/TESTING.md)).

## When stuck

1. Re-read https://octanejs.dev/llms.txt
2. Mirror an existing route under `apps/web/src/routes/*.tsrx` or `components/*.tsrx`
3. Run the matching Vitest project (`*.integration.test.ts` / e2e) before claiming UI done
