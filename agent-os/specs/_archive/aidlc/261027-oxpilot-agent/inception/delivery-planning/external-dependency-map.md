# External Dependency Map — `/oxpilot`

Items outside this team that could hold up a Bolt, mapped to what they
block. None are blocking in this build — the table records them as
informational.

## Gated items

| Dependency | Owner | Lead time | Blocks | If it slips |
|---|---|---|---|---|
| TanStack AI (`ai-octane` patterns) | Upstream OSS | none — used as design reference; integration is in-repo code | B4 (runtime loop shape) | Already mitigated: our own `LlmProvider`/turn loop owns semantics; the framework informs patterns, it is not a build gate |
| OpenAI / Anthropic API | Third-party SaaS | none — wiremock fixtures pin the wire shape; live keys only for manual dogfood | B3 verification only for fixtures | Adapter contract tests still pass; provider marked unverified |
| Ollama | Self-hosted by end user | none | B3 | Same — fixture coverage |
| Maintainer PR review + CodeRabbit | Repo maintainer | per-PR | Final merge, not any Bolt | The standing delegation routes all ratification there; the PR stays open until green |

## Notes

No data-availability windows, approval lead times, or external-team
hand-offs apply to any Bolt. Consumed inputs per upstream coverage:
requirements (fixture-only provider testing, NFR deferrals),
components (unit ownership drives which Bolt an item lands in), and
the unit-of-work + dependency DAG (Bolt-to-item mapping).
