/**
 * No-AI-attribution scan — the enforcement half of
 * `.cursor/rules/no-ai-attribution.mdc`, ported from the sibling repos'
 * `scripts/commit-check.sh` (`check_attribution`). Runs on every surface
 * `commit-check` lints: commit messages (subject + body), PR titles, and
 * PR bodies via `--scan-file`.
 *
 * Returns one human-readable violation per matched rule; an empty array
 * means clean. Grammar stays in `parseCommit` — this module only answers
 * "does this text carry AI/tool attribution or invisible markers".
 */

const AI_COAUTHOR_LINE = /^\s*co-?authored-by:/i;
// AI vendors, bots, and *-ai-* / [bot] addresses. A human
// Co-authored-by is fine; the ban is on machine attribution.
const AI_COAUTHOR_VENDOR =
  /claude|cursor|copilot|chatgpt|codex|gemini|devin-ai|cognition\.ai|devin\.ai[\s>]|openai|anthropic|codeium|windsurf|swe-|\[bot\]|[a-z0-9-]*bot@|ai-integration/i;
// "Generated with <tool>", "Made with: X", "Authored via X" — the tool
// names that sign output, plus the trailer forms regardless of vendor.
const GENERATOR_FOOTER =
  /(generated|made|created|authored|written|produced)\s+(with|by|via)\s*\[?\s*(claude|cursor|copilot|chatgpt|codex|gemini|devin|openai|anthropic|codeium|windsurf|swe-)/i;
const GENERATOR_TRAILER = /^\s*(generated|made|authored)[- ](with|by|via)\s*:/im;
const BOT_EMOJI = /🤖/;
// AI named as the author/agent — not as the subject of the change
// ("add Copilot webhook" stays legal).
const AI_SELF_ATTRIBUTION =
  /\b(as an ai|i generated this|authored by (claude|cursor|copilot|chatgpt|codex|gemini|devin|openai|swe-))\b/i;
// Vendor-stamped HTML comments smuggled into the message.
const ATTRIBUTION_HTML_COMMENT =
  /<!--[^>]*(claude|cursor|copilot|chatgpt|codex|gemini|devin|openai|anthropic|generated|🤖)/i;
// Zero-width / invisible markers: U+200B..U+200D, U+2060, U+FEFF.
const ZERO_WIDTH_MARKER = /[\u200B-\u200D\u2060\uFEFF]/;

/** Scan one text surface; returns violation descriptions ([] = clean). */
export function scanAttribution(text: string): string[] {
  const violations: string[] = [];
  const lines = text.split("\n");

  if (
    lines.some(
      (line) => AI_COAUTHOR_LINE.test(line) && AI_COAUTHOR_VENDOR.test(line),
    )
  ) {
    violations.push("AI Co-authored-by trailer");
  }
  if (GENERATOR_FOOTER.test(text)) {
    violations.push("AI generator footer");
  }
  if (GENERATOR_TRAILER.test(text)) {
    violations.push("generator trailer");
  }
  if (BOT_EMOJI.test(text)) {
    violations.push("bot-emoji attribution marker");
  }
  if (AI_SELF_ATTRIBUTION.test(text)) {
    violations.push("AI self-attribution");
  }
  if (ATTRIBUTION_HTML_COMMENT.test(text)) {
    violations.push("attribution HTML comment");
  }
  if (ZERO_WIDTH_MARKER.test(text)) {
    violations.push("zero-width/invisible attribution marker");
  }
  return violations;
}
