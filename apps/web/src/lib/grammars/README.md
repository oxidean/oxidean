# Vendored TextMate grammars

In-repo grammars for Shiki blob highlighting (D-19). Do not alias `.tsrx` / `.ripple` to TS/JS.

| File                     | Source                                                                                                                  | Scope           |
| ------------------------ | ----------------------------------------------------------------------------------------------------------------------- | --------------- |
| `tsrx.tmLanguage.json`   | [tsrx-org/tsrx](https://github.com/tsrx-org/tsrx/blob/main/grammars/textmate/tsrx.tmLanguage.json)                      | `source.tsrx`   |
| `ripple.tmLanguage.json` | [stackblitz/textmate](https://github.com/stackblitz/textmate/blob/main/grammars/ripple/syntaxes/ripple.tmLanguage.json) | `source.ripple` |
| `oxl.tmLanguage.json`    | [oxidean/oxl](https://github.com/oxidean/oxl/tree/main/editors/grammars) `editors/grammars/`                              | `source.oxl`    |

The tsrx/ripple grammars are TypeScriptReact forks (Microsoft TypeScript-TmLanguage lineage); `oxl` is authored in oxidean/oxl — refresh it from that repo when highlighting drifts. Refresh the TypeScriptReact forks by re-downloading their URLs.
