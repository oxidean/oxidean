import { unified } from "unified";
import remarkParse from "remark-parse";
import remarkGfm from "remark-gfm";
import remarkGithub from "remark-github";
import remarkRehype from "remark-rehype";
import rehypeSanitize from "rehype-sanitize";
import rehypeStringify from "rehype-stringify";
import type { Element, ElementContent, Root } from "hast";

export type RenderGfmOptions = {
  /** Default owner for bare `#N` refs (ISS-04 / D-ISS-13). */
  owner?: string;
  /** Default repo for bare `#N` refs (ISS-04 / D-ISS-13). */
  repo?: string;
};

/** True for paths that should default to rendered GFM (blob preview / README). */
export function isMarkdownPath(path: string): boolean {
  const base = path.split("/").pop()?.toLowerCase() ?? "";
  return (
    base.endsWith(".md") ||
    base.endsWith(".markdown") ||
    base.endsWith(".mdown") ||
    base.endsWith(".mkd") ||
    base === "readme"
  );
}

/**
 * Post-sanitize pass: the GitHub sanitize schema strips `loading`/`decoding`,
 * so re-attach them to `img` after sanitizing. Values are fixed literals —
 * no user input reaches the attributes.
 */
function rehypeLazyImages() {
  return (tree: Root) => {
    const walk = (node: Root | Element) => {
      if (node.type === "element" && node.tagName === "img") {
        node.properties.loading = "lazy";
        node.properties.decoding = "async";
      }
      for (const child of node.children ?? []) walk(child as Element);
    };
    walk(tree);
  };
}

/** Language id from a fenced `code` element's `language-*` class, if present. */
function fenceLanguage(codeEl: Element): string | undefined {
  const cls = codeEl.properties.className;
  const names = Array.isArray(cls) ? cls : typeof cls === "string" ? [cls] : [];
  for (const n of names) {
    if (typeof n === "string" && n.startsWith("language-")) {
      const lang = n.slice("language-".length).trim().toLowerCase();
      if (lang) return lang;
    }
  }
  return undefined;
}

/** Concatenated text of a hast subtree — fence source lives in text nodes. */
function textOf(node: Element): string {
  let out = "";
  const walk = (n: Element | ElementContent) => {
    if (n.type === "text") {
      out += n.value;
      return;
    }
    if ("children" in n) for (const c of n.children) walk(c);
  };
  walk(node);
  return out;
}

/**
 * Post-sanitize pass: ````suggestion` fences (COL-08) become a labeled
 * suggested-change block rather than an ordinary code fence. Runs BEFORE
 * the highlighter so `language-suggestion` never reaches Shiki. The block
 * body reuses the fence's text children — escaped text nodes, no markup —
 * same trust posture as the rest of this post-sanitize phase. Diff-file
 * comments render the richer `-`/`+` widget instead; this covers every
 * other markdown surface (PR conversation, comments panel, issues).
 */
function rehypeSuggestionFences() {
  return (tree: Root) => {
    const walk = (node: Root | Element) => {
      const children = node.children ?? [];
      for (let index = 0; index < children.length; index++) {
        const child = children[index];
        if (child.type !== "element") continue;
        if (child.tagName === "pre") {
          const codeEl = child.children.find(
            (c): c is Element => c.type === "element" && c.tagName === "code",
          );
          if (codeEl && fenceLanguage(codeEl) === "suggestion") {
            children[index] = {
              type: "element",
              tagName: "div",
              properties: { className: ["md-suggestion"] },
              children: [
                {
                  type: "element",
                  tagName: "div",
                  properties: { className: ["md-suggestion-label"] },
                  children: [{ type: "text", value: "Suggested change" }],
                },
                {
                  type: "element",
                  tagName: "pre",
                  properties: { className: ["md-suggestion-code"] },
                  children: codeEl.children,
                },
              ],
            };
            continue; // already replaced — don't descend into it
          }
        }
        walk(child);
      }
    };
    walk(tree);
  };
}

/**
 * Post-sanitize pass: swap `pre > code.language-*` fences for Shiki
 * highlighted markup. Runs AFTER rehype-sanitize — the schema would strip
 * Shiki's `style` attributes, and inserting our generated hast afterwards
 * can't reintroduce user markup (fence text re-enters as escaped text
 * nodes). Unknown langs fall back to plaintext inside `highlightCodeToHast`;
 * langless fences keep the plain `pre > code` shape. `./highlight` loads
 * lazily so fence-free pages never pull Shiki into the bundle path.
 */
function rehypeHighlightFences() {
  return async (tree: Root) => {
    const fences: { parent: Root | Element; index: number; lang: string; code: string }[] = [];
    const walk = (node: Root | Element) => {
      (node.children ?? []).forEach((child, index) => {
        if (child.type !== "element") return;
        if (child.tagName === "pre") {
          const codeEl = child.children.find(
            (c): c is Element => c.type === "element" && c.tagName === "code",
          );
          const lang = codeEl ? fenceLanguage(codeEl) : undefined;
          if (codeEl && lang) {
            fences.push({ parent: node, index, lang, code: textOf(codeEl) });
            return;
          }
        }
        walk(child);
      });
    };
    walk(tree);
    if (fences.length === 0) return;
    const { highlightCodeToHast } = await import("./highlight");
    await Promise.all(
      fences.map(async (fence) => {
        try {
          fence.parent.children[fence.index] = await highlightCodeToHast(fence.code, {
            lang: fence.lang,
          });
        } catch {
          // Highlight failure leaves the sanitized plaintext fence in place.
        }
      }),
    );
  };
}

/**
 * Render GitHub-flavored Markdown to HTML. rehype-sanitize runs before the
 * generated-markup passes (fence highlighting, lazy images) — raw user HTML
 * is stripped first (D-18 / D-ISS-10), so nothing afterwards can reintroduce
 * it; Shiki output is trusted generator markup, not user input.
 *
 * With owner/repo context, `remark-github` autolinks `#N` and `owner/repo#N` to
 * `/{owner}/{repo}/issues/{n}`. Mentions and commits are not linked (Q1 / D-ISS-13).
 */
export async function renderGfm(markdown: string, opts?: RenderGfmOptions): Promise<string> {
  const owner = opts?.owner?.trim() || "owner";
  const repo = opts?.repo?.trim() || "repo";

  const file = await unified()
    .use(remarkParse)
    .use(remarkGfm)
    .use(remarkGithub, {
      repository: `${owner}/${repo}`,
      buildUrl(values) {
        if (values.type === "issue") {
          return `/${values.user}/${values.project}/issues/${values.no}`;
        }
        // Mentions / commits / compare: disabled until a later social phase (Q1).
        return false;
      },
    })
    .use(remarkRehype)
    .use(rehypeSanitize)
    .use(rehypeSuggestionFences)
    .use(rehypeHighlightFences)
    .use(rehypeLazyImages)
    .use(rehypeStringify)
    .process(markdown);
  return String(file);
}
