/**
 * Stray-template-text matcher (issue #165).
 *
 * A `;` typed into JSX children position compiles to a text node and ships a
 * literal semicolon on the page ("Get started ; Explore"). The source-level
 * gate is `scripts/check-template-semicolons.ts`; this matcher is the
 * rendered-output half — run against `astro build` output
 * (scripts/check-rendered-semicolons.ts) and the live e2e DOM
 * (e2e/stack-browser/dom-race-guard.ts's assertNoOctaneOverlay).
 *
 * Precision: a `;`-only text node is flagged only when its parent is a
 * layout/parentage element — the bug class emits the stray text between
 * sibling components inside flex/grid wrappers. Text inside semantic carriers
 * (`p`, `span`, `a`, `td`, `code`, `h1`–`h6`, `button`, `label`, …) and inside
 * `script`/`style`/`pre`/`code`/`svg`/`textarea`/`template`/`noscript` bodies
 * is left alone — code tokens, prose, and seeded content may legitimately be
 * `;`. Deliberate visible semicolons belong in a carrier element
 * (`<span>{";"}</span>`), never bare between layout siblings.
 */

export type StrayTemplateText = {
  /** Tag name of the element the text node sits inside ("" at document level). */
  parent: string;
  /** The stray text, trimmed. */
  text: string;
  /** Byte offset into the (container-stripped) HTML for diagnostics. */
  offset: number;
};

const SKIP_BLOCKS =
  /<(script|style|pre|code|svg|textarea|template|noscript)\b[^>]*>[\s\S]*?<\/\1>/gi;

const TOKEN_RE = /<!--[\s\S]*?-->|<![^>]*>|<\/?[a-zA-Z][a-zA-Z0-9-]*[^>]*>|[^<]+/g;

const VOID_TAGS = new Set([
  "area",
  "base",
  "br",
  "col",
  "embed",
  "hr",
  "img",
  "input",
  "link",
  "meta",
  "param",
  "source",
  "track",
  "wbr",
]);

const TEXT_CARRIERS = new Set([
  "a",
  "abbr",
  "b",
  "bdi",
  "bdo",
  "blockquote",
  "button",
  "caption",
  "cite",
  "code",
  "data",
  "del",
  "dfn",
  "em",
  "figcaption",
  "h1",
  "h2",
  "h3",
  "h4",
  "h5",
  "h6",
  "i",
  "ins",
  "kbd",
  "label",
  "legend",
  "mark",
  "option",
  "output",
  "p",
  "pre",
  "q",
  "ruby",
  "s",
  "samp",
  "small",
  "span",
  "strong",
  "sub",
  "summary",
  "sup",
  "td",
  "th",
  "time",
  "title",
  "u",
  "var",
]);

const TAG_NAME_RE = /^<\/?([a-zA-Z][a-zA-Z0-9-]*)/;
const STRAY_RE = /^\s*;+\s*$/;

/** Find `;`-only text nodes inside layout elements in serialized HTML. */
export function findStrayTemplateText(html: string): StrayTemplateText[] {
  const stripped = html.replace(SKIP_BLOCKS, "");
  const stack: string[] = [];
  const hits: StrayTemplateText[] = [];
  for (const m of stripped.matchAll(TOKEN_RE)) {
    const tok = m[0];
    if (tok.startsWith("<")) {
      if (tok.startsWith("<!--") || tok.startsWith("<!")) continue;
      const name = TAG_NAME_RE.exec(tok)?.[1]?.toLowerCase() ?? "";
      if (tok.startsWith("</")) {
        // Pop through the matching open tag (tolerates malformed nesting).
        for (let i = stack.length - 1; i >= 0; i--) {
          if (stack[i] === name) {
            stack.length = i;
            break;
          }
        }
      } else if (name && !VOID_TAGS.has(name) && !tok.endsWith("/>")) {
        stack.push(name);
      }
      continue;
    }
    const text = tok.trim();
    if (STRAY_RE.test(text)) {
      const parent = stack[stack.length - 1] ?? "";
      if (!TEXT_CARRIERS.has(parent)) {
        hits.push({ parent, text, offset: m.index ?? 0 });
      }
    }
  }
  return hits;
}
