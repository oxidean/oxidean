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
 *
 * The reference is grep -E, which is line-based: `\s` and `[^>]` there
 * cannot cross newlines, so the JS port uses `[^\S\n]` (horizontal
 * whitespace) and `[^>\n]` to keep the same matching domain.
 */

const AI_COAUTHOR_LINE = /^\s*co-?authored-by:/i;
// Unambiguous machine signals anywhere in the trailer: [bot] tags,
// bot@ addresses, *-ai-integration logins, AI-vendor domains.
const AI_COAUTHOR_VENDOR =
  /\[bot\]|(^|[<.\s_-])bot@|ai-integration|noreply@(anthropic|openai)\.|@(anthropic|openai|cognition|devin|codeium|windsurf|claude-code)\.(com|ai|dev|io)>/i;
// A bare vendor identity as the display name — "Co-authored-by:
// Claude <noreply@anthropic.com>" hits the domain rule above; this
// catches no-email and generic-mailbox forms. Human names that merely
// CONTAIN a vendor token ("Claude Martin <claude.martin@…>") stay
// legal per .cursor/rules/no-ai-attribution.mdc.
const AI_COAUTHOR_NAME =
  /^(claude|cursor|copilot|chatgpt|codex|gemini|devin|openai|anthropic|codeium|windsurf|swe-)([ \t-]ai)?$/i;
// "Generated with <tool>", "Made with: X", "Authored via X" — the tool
// names that sign output, plus the trailer forms regardless of vendor.
const GENERATOR_FOOTER =
  /(generated|made|created|authored|written|produced)[^\S\n]+(with|by|via)[^\S\n]*\[?[^\S\n]*(claude|cursor|copilot|chatgpt|codex|gemini|devin|openai|anthropic|codeium|windsurf|swe-)/i;
const GENERATOR_TRAILER =
  /^[^\S\n]*(generated|made|authored)[- ](with|by|via)[^\S\n]*:/im;
// standalone "via <tool>" / "by <tool>" lines — the footer regex
// needs a leading verb, so a lone via-line slips past it.
const VIA_LINE =
  /^[^\S\n]*(via|by|using|with)[^\S\n]+(claude|cursor|copilot|chatgpt|codex|gemini|devin|openai|anthropic|codeium|windsurf|swe-[0-9a-z.-]*)([^\S\n-]ai)?[^\S\n.!]*$/im;
const BOT_EMOJI = /🤖/;
// AI named as the author/agent — not as the subject of the change
// ("add Copilot webhook" stays legal).
const AI_SELF_ATTRIBUTION =
  /\b(as an ai|i generated this|authored by (claude|cursor|copilot|chatgpt|codex|gemini|devin|openai|swe-))\b/i;
// Vendor-stamped HTML comments smuggled into the message.
const ATTRIBUTION_HTML_COMMENT =
  /<!--[^>\n]*(claude|cursor|copilot|chatgpt|codex|gemini|devin|openai|anthropic|generated|🤖)/i;
// Zero-width / invisible markers: U+200B..U+200D, U+2060, U+FEFF.
const ZERO_WIDTH_MARKER = /[\u200B-\u200D\u2060\uFEFF]/;

/** Scan one text surface; returns violation descriptions ([] = clean). */
export function scanAttribution(text: string): string[] {
  const violations: string[] = [];
  const lines = text.split("\n");

  if (
    lines.some((line) => {
      if (!AI_COAUTHOR_LINE.test(line)) return false;
      if (AI_COAUTHOR_VENDOR.test(line)) return true;
      const name = line
        .split(":")
        .slice(1)
        .join(":")
        .replace(/<.*$/, "")
        .trim();
      return AI_COAUTHOR_NAME.test(name);
    })
  ) {
    violations.push("AI Co-authored-by trailer");
  }
  if (GENERATOR_FOOTER.test(text)) {
    violations.push("AI generator footer");
  }
  if (GENERATOR_TRAILER.test(text)) {
    violations.push("generator trailer");
  }
  if (VIA_LINE.test(text)) {
    violations.push("attribution via-line");
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
