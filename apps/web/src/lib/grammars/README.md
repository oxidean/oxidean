# Vendored TextMate grammars

In-repo grammars for Shiki blob highlighting (D-19). Do not alias `.tsrx` / `.ripple` to TS/JS.

| File                     | Source                                                                                                                  | Scope           |
| ------------------------ | ----------------------------------------------------------------------------------------------------------------------- | --------------- |
| `tsrx.tmLanguage.json`   | [tsrx-org/tsrx](https://github.com/tsrx-org/tsrx/blob/main/grammars/textmate/tsrx.tmLanguage.json)                      | `source.tsrx`   |
| `ripple.tmLanguage.json` | [stackblitz/textmate](https://github.com/stackblitz/textmate/blob/main/grammars/ripple/syntaxes/ripple.tmLanguage.json) | `source.ripple` |

The tsrx/ripple grammars are TypeScriptReact forks (Microsoft TypeScript-TmLanguage lineage); refresh them by re-downloading their URLs.

## oxl — packaged, not vendored

The `oxl` grammar is **not** vendored here — it ships as the
[`@oxidean/shiki-oxl`](https://github.com/oxidean/oxl/tree/main/editors/grammars)
package, published per oxl release (`@oxidean/shiki-oxl@<ver>` = "the grammar
as of oxl `v<ver>`"). `apps/web` consumes the raw grammar via the package's
`./grammar.json` export in `src/lib/highlight.ts`.

Bump cadence: **manual**. When oxl ships grammar changes, point the dependency
at the newer release tarball:

```bash
cd apps/web
bun add "@oxidean/shiki-oxl@https://github.com/oxidean/oxl/releases/download/v<ver>/oxidean-shiki-oxl-<ver>.tgz"
```

bun.lock records the URL and integrity, so the bump lands as a normal diff.
Polling oxl releases automatically (e.g. a scheduled job that opens the bump
PR) is a possible follow-up — not implemented.
